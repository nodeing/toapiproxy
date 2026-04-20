param(
    [string]$Version,
    [string]$Remote = "origin",
    [string]$Branch,
    [switch]$SkipBuild,
    [switch]$SkipCommit,
    [switch]$SkipTag,
    [switch]$SkipPush,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $repoRoot "package.json"
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$cargoTomlPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$historyPath = Join-Path $repoRoot "release-history.json"
$buildDir = Join-Path $repoRoot "build"
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Write-TextFile {
    param(
        [string]$Path,
        [string]$Content
    )

    [System.IO.File]::WriteAllText($Path, $Content, $utf8NoBom)
}

function Invoke-GitCommand {
    param(
        [string[]]$GitArgs,
        [switch]$AllowFailure
    )

    $output = & git @GitArgs 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw "git $($GitArgs -join ' ') failed: $($output -join [Environment]::NewLine)"
    }

    return ($output -join [Environment]::NewLine).Trim()
}

function Assert-SemVer {
    param([string]$InputVersion)

    if ($InputVersion -notmatch '^\d+\.\d+\.\d+$') {
        throw "Version '$InputVersion' is invalid. Use semantic version format like 1.0.1."
    }
}

function Get-NextPatchVersion {
    param([string]$InputVersion)

    Assert-SemVer -InputVersion $InputVersion
    $parts = $InputVersion.Split('.')
    return "{0}.{1}.{2}" -f $parts[0], $parts[1], ([int]$parts[2] + 1)
}

function ConvertTo-PrettyJson {
    param($Value)

    return ($Value | ConvertTo-Json -Depth 10)
}

function Get-RelativeRepoPath {
    param([string]$Path)

    $resolvedPath = (Resolve-Path $Path).Path
    if ($resolvedPath.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $resolvedPath.Substring($repoRoot.Length).TrimStart('\').Replace('\', '/')
    }

    return $resolvedPath
}

if (-not (Test-Path $packageJsonPath)) {
    throw "package.json not found at $packageJsonPath"
}

if (-not (Test-Path $tauriConfigPath)) {
    throw "tauri.conf.json not found at $tauriConfigPath"
}

if (-not (Test-Path $cargoTomlPath)) {
    throw "Cargo.toml not found at $cargoTomlPath"
}

$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
$tauriConfig = Get-Content $tauriConfigPath -Raw | ConvertFrom-Json
$cargoToml = Get-Content $cargoTomlPath -Raw

$currentVersion = [string]$tauriConfig.version
if (-not $currentVersion) {
    throw "Unable to determine current version from $tauriConfigPath"
}

$packageVersion = [string]$packageJson.version
$cargoVersionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
$cargoVersion = if ($cargoVersionMatch.Success) { $cargoVersionMatch.Groups[1].Value } else { "" }

if ($packageVersion -and $packageVersion -ne $currentVersion) {
    Write-Warning "package.json version ($packageVersion) differs from tauri.conf.json version ($currentVersion). Release will use $currentVersion as the current baseline."
}

if ($cargoVersion -and $cargoVersion -ne $currentVersion) {
    Write-Warning "Cargo.toml version ($cargoVersion) differs from tauri.conf.json version ($currentVersion). Release will sync it."
}

$targetVersion = if ($Version) { $Version } else { Get-NextPatchVersion -InputVersion $currentVersion }
Assert-SemVer -InputVersion $targetVersion

$releaseMode = if ($Version) { "manual" } else { "auto-patch" }
$tagName = "v$targetVersion"

$resolvedBranch = $Branch
if (-not $resolvedBranch) {
    $resolvedBranch = Invoke-GitCommand -GitArgs @("branch", "--show-current") -AllowFailure
    if (-not $resolvedBranch) {
        $resolvedBranch = "master"
    }
}

$remoteUrl = Invoke-GitCommand -GitArgs @("remote", "get-url", $Remote)
if (-not $remoteUrl) {
    throw "Remote '$Remote' not found."
}

if ($remoteUrl -notmatch 'gitee\.com') {
    Write-Warning "Remote '$Remote' does not look like a Gitee remote: $remoteUrl"
}

$tagExists = Invoke-GitCommand -GitArgs @("tag", "--list", $tagName) -AllowFailure
if ($tagExists -eq $tagName) {
    throw "Tag $tagName already exists. Choose another version."
}

Write-Host "Preparing release"
Write-Host "  Current version : $currentVersion"
Write-Host "  Target version  : $targetVersion"
Write-Host "  Release mode    : $releaseMode"
Write-Host "  Remote          : $Remote ($remoteUrl)"
Write-Host "  Branch          : $resolvedBranch"
Write-Host "  Build enabled   : $(-not $SkipBuild)"
Write-Host "  Commit enabled  : $(-not $SkipCommit)"
Write-Host "  Tag enabled     : $(-not $SkipTag)"
Write-Host "  Push enabled    : $(-not $SkipPush)"

if ($DryRun) {
    Write-Host "Dry run complete. No files were modified."
    return
}

$packageJson.version = $targetVersion
$tauriConfig.version = $targetVersion

$updatedCargoToml = [regex]::Replace(
    $cargoToml,
    '(?m)^version\s*=\s*"[^"]+"',
    "version = `"$targetVersion`"",
    1
)

if ($updatedCargoToml -eq $cargoToml) {
    throw "Failed to update version in $cargoTomlPath"
}

Write-TextFile -Path $packageJsonPath -Content ((ConvertTo-PrettyJson -Value $packageJson) + [Environment]::NewLine)
Write-TextFile -Path $tauriConfigPath -Content ((ConvertTo-PrettyJson -Value $tauriConfig) + [Environment]::NewLine)
Write-TextFile -Path $cargoTomlPath -Content $updatedCargoToml

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot "build-app.ps1") -Bundles nsis -CopyInstaller
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed."
    }
}

$installer = Get-ChildItem $buildDir -Filter "*$targetVersion*-setup.exe" -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

$installerPath = if ($installer) { Get-RelativeRepoPath -Path $installer.FullName } else { $null }

$history = @()
if (Test-Path $historyPath) {
    $rawHistory = Get-Content $historyPath -Raw
    if ($rawHistory.Trim()) {
        $parsedHistory = $rawHistory | ConvertFrom-Json
        if ($parsedHistory -is [System.Array]) {
            $history = @($parsedHistory)
        } else {
            $history = @($parsedHistory)
        }
    }
}

$historyEntry = [ordered]@{
    version = $targetVersion
    releasedAt = (Get-Date).ToString("yyyy-MM-ddTHH:mm:ssK")
    releaseMode = $releaseMode
    channel = "gitee"
    remote = [ordered]@{
        name = $Remote
        url = $remoteUrl
    }
    branch = $resolvedBranch
    tag = $tagName
    installer = $installerPath
}

$updatedHistory = @($historyEntry) + @($history)
Write-TextFile -Path $historyPath -Content ((ConvertTo-PrettyJson -Value $updatedHistory) + [Environment]::NewLine)

$managedFiles = @(
    "package.json",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml",
    "CHANGELOG.md",
    "release-history.json"
)

if (-not $SkipCommit) {
    $gitAddArgs = @("add", "--") + $managedFiles
    $gitCommitArgs = @("commit", "-m", "release: $tagName", "--") + $managedFiles
    Invoke-GitCommand -GitArgs $gitAddArgs | Out-Null
    Invoke-GitCommand -GitArgs $gitCommitArgs | Out-Null
}

if (-not $SkipTag) {
    Invoke-GitCommand -GitArgs @("tag", "-a", $tagName, "-m", "Release $tagName") | Out-Null
}

if (-not $SkipPush) {
    Invoke-GitCommand -GitArgs @("push", $Remote, $resolvedBranch) | Out-Null
    if (-not $SkipTag) {
        Invoke-GitCommand -GitArgs @("push", $Remote, $tagName) | Out-Null
    }
}

Write-Host ""
Write-Host "Release completed successfully."
Write-Host "  Version  : $targetVersion"
Write-Host "  History  : $historyPath"
if ($installerPath) {
    Write-Host "  Installer: $installerPath"
}
