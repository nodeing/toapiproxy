param(
    [string]$Target,
    [string[]]$Bundles = @(),
    [switch]$CopyInstaller
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$buildDir = Join-Path $repoRoot "build"
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$targetSegment = if ($Target) { $Target } else { "" }
$bundleRoot = if ($targetSegment) {
    Join-Path $repoRoot ("src-tauri\target\" + $targetSegment + "\release\bundle")
} else {
    Join-Path $repoRoot "src-tauri\target\release\bundle"
}
$bundleDir = Join-Path $bundleRoot "nsis"

function Get-ArtifactPlatformLabel {
    param(
        [string]$TargetTriple
    )

    switch ($TargetTriple) {
        "x86_64-pc-windows-msvc" { return "windows_x64" }
        "aarch64-pc-windows-msvc" { return "windows_arm64" }
        default { return "windows_host" }
    }
}

function Get-SafeArtifactBaseName {
    param(
        [string]$ProductName,
        [string]$Version,
        [string]$PlatformLabel
    )

    $safeProductName = ($ProductName -replace '[^A-Za-z0-9._-]+', '_').Trim('_')
    if (-not $safeProductName) {
        $safeProductName = "app"
    }

    return "$safeProductName" + "_" + "$Version" + "_" + "$PlatformLabel"
}

function Assert-Arm64ToolchainPrerequisites {
    param(
        [string]$TargetTriple
    )

    if ($TargetTriple -ne "aarch64-pc-windows-msvc") {
        return
    }

    $clang = Get-Command clang -ErrorAction SilentlyContinue
    if (-not $clang) {
        $defaultLlvmBin = "C:\Program Files\LLVM\bin"
        $defaultClang = Join-Path $defaultLlvmBin "clang.exe"
        if (Test-Path $defaultClang) {
            if ($env:Path -notlike "*$defaultLlvmBin*") {
                $env:Path = "$defaultLlvmBin;$env:Path"
            }
            $clang = Get-Command clang -ErrorAction SilentlyContinue
        }
    }

    if (-not $clang) {
        throw @"
Windows ARM64 builds in this project require LLVM Clang because the ring crate
invokes `clang` for aarch64-pc-windows-msvc.

Install LLVM/Clang first, then reopen PowerShell and retry. Example:
  winget install --source winget --exact --id LLVM.LLVM

After installation, verify:
  clang --version
"@
    }
}

function Assert-BundledConfigHasNoRealSecrets {
    param(
        [string]$ConfigPath
    )

    if (-not (Test-Path $ConfigPath)) {
        throw "Bundled config template not found at $ConfigPath"
    }

    $content = Get-Content $ConfigPath -Raw
    $hasBundledProviderKey = [regex]::IsMatch(
        $content,
        '(?m)^(?!\s*#)\s*-\s*api-key:\s*["'']?[^"''\r\n\[\]#][^\r\n#]*$'
    )
    $hasBundledClientKeys = [regex]::IsMatch(
        $content,
        '(?m)^(?!\s*#)\s*api-keys:\s*\[(?!\s*\])'
    ) -or [regex]::IsMatch(
        $content,
        '(?ms)^(?!\s*#)\s*api-keys:\s*$\s*-\s*\S'
    )

    if ($hasBundledProviderKey -or $hasBundledClientKeys) {
        throw @"
Bundled config template contains non-empty API keys or tokens:
  $ConfigPath

Move real credentials to the user's local runtime config after first launch.
The packaged config.yaml must remain a secret-free template.
"@
    }
}

if (-not (Test-Path $tauriConfigPath)) {
    throw "tauri.conf.json not found at $tauriConfigPath"
}

$tauriConfig = Get-Content $tauriConfigPath -Raw | ConvertFrom-Json
$productName = [string]$tauriConfig.productName
$version = [string]$tauriConfig.version
if (-not $productName) {
    $productName = "app"
}
if (-not $version) {
    $version = "0.0.0"
}

Assert-BundledConfigHasNoRealSecrets -ConfigPath (Join-Path $repoRoot "src-tauri\resources\config.yaml")
Assert-Arm64ToolchainPrerequisites -TargetTriple $Target

& (Join-Path $PSScriptRoot "build-cli-proxy.ps1") -Target $Target

Push-Location $repoRoot
try {
    $cargoArgs = @("tauri", "build", "--ci")
    if ($Target) {
        $cargoArgs += "--target"
        $cargoArgs += $Target
    }
    if ($Bundles.Count -gt 0) {
        $cargoArgs += "--bundles"
        $cargoArgs += $Bundles
    }
    Write-Host ("Running: cargo " + ($cargoArgs -join " "))
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri build failed."
    }
} finally {
    Pop-Location
}

if (-not $CopyInstaller) {
    return
}

New-Item -ItemType Directory -Force $buildDir | Out-Null
$artifactDir = Join-Path $buildDir "dist"
New-Item -ItemType Directory -Force $artifactDir | Out-Null
$installer = Get-ChildItem (Join-Path $bundleDir "*-setup.exe") |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $installer) {
    throw "NSIS installer not found after build."
}

$platformLabel = Get-ArtifactPlatformLabel -TargetTriple $Target
$artifactBaseName = Get-SafeArtifactBaseName -ProductName $productName -Version $version -PlatformLabel $platformLabel
$dest = Join-Path $artifactDir ($artifactBaseName + "_setup.exe")
Copy-Item $installer.FullName $dest -Force
Write-Host "Installer copied to $dest"
