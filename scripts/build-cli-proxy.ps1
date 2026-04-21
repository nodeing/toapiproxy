param(
    [string]$Target
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$cliProjectDir = Join-Path $repoRoot "third_party\CLIProxyAPIPlus"
$binaryName = "cli-proxy-api-plus.exe"
$outputPath = Join-Path $repoRoot ("src-tauri\resources\" + $binaryName)
$tempOutputPath = Join-Path $repoRoot ("src-tauri\resources\cli-proxy-api-plus.build.exe")
$goCacheRoot = Join-Path $repoRoot ".cache\go"
$goBuildCache = Join-Path $goCacheRoot "build"
$goModCache = Join-Path $goCacheRoot "mod"
$binaryProcessName = "cli-proxy-api-plus"
$artifactDir = Join-Path $repoRoot "build\backend"

function Resolve-DefaultTargetTriple {
    $rustHost = $null
    if (Get-Command rustc -ErrorAction SilentlyContinue) {
        try {
            $rustHost = (& rustc -Vv 2>$null | Select-String '^host:' | ForEach-Object {
                ($_ -split ':\s+', 2)[1].Trim()
            } | Select-Object -First 1)
        } catch {
        }
    }

    switch ($rustHost) {
        "x86_64-pc-windows-msvc" { return $rustHost }
        "aarch64-pc-windows-msvc" { return $rustHost }
    }

    $arch = $env:PROCESSOR_ARCHITEW6432
    if (-not $arch) {
        $arch = $env:PROCESSOR_ARCHITECTURE
    }

    switch ($arch.ToUpperInvariant()) {
        "ARM64" { return "aarch64-pc-windows-msvc" }
        "AMD64" { return "x86_64-pc-windows-msvc" }
        default { return "x86_64-pc-windows-msvc" }
    }
}

$resolvedTarget = if ($Target) { $Target } else { Resolve-DefaultTargetTriple }

function Resolve-GoTarget {
    param(
        [string]$TargetTriple
    )

    switch ($TargetTriple) {
        "x86_64-pc-windows-msvc" {
            return @{
                GOOS = "windows"
                GOARCH = "amd64"
            }
        }
        "aarch64-pc-windows-msvc" {
            return @{
                GOOS = "windows"
                GOARCH = "arm64"
            }
        }
        default {
            throw "Unsupported target triple: $TargetTriple"
        }
    }
}

function Stop-RunningBinaryProcesses {
    param(
        [string]$ProcessName
    )

    $running = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
    if ($running.Count -eq 0) {
        return
    }

    $processIds = $running | Select-Object -ExpandProperty Id
    Write-Host "Stopping running $ProcessName processes: $($processIds -join ', ')"
    $running | Stop-Process -Force -ErrorAction Stop

    foreach ($processId in $processIds) {
        try {
            Wait-Process -Id $processId -Timeout 10 -ErrorAction Stop
        } catch {
        }
    }
}

function Get-CLIProxyAPIPlusCommit {
    param(
        [string]$RepoRoot
    )

    $subtreePath = "third_party/CLIProxyAPIPlus"

    try {
        $subtreeMessage = git -C $RepoRoot log `
            --format=%B `
            -n 1 `
            --grep "git-subtree-dir: $subtreePath" `
            --fixed-strings 2>$null

        foreach ($line in ($subtreeMessage -split "`r?`n")) {
            if ($line -like "git-subtree-split:*") {
                $splitCommit = ($line -replace '^git-subtree-split:\s*', "").Trim()
                if ($splitCommit) {
                    return $splitCommit.Substring(0, [Math]::Min(8, $splitCommit.Length))
                }
            }
        }
    } catch {
    }

    try {
        $pathCommit = (git -C $RepoRoot log -1 --format=%h -- $subtreePath 2>$null).Trim()
        if ($pathCommit) {
            return $pathCommit
        }
    } catch {
    }

    return "workspace"
}

if (-not (Get-Command go -ErrorAction SilentlyContinue)) {
    throw "go command not found. Install Go before building CLIProxyAPIPlus."
}

if (-not (Test-Path $cliProjectDir)) {
    throw "CLIProxyAPIPlus project not found at $cliProjectDir"
}

New-Item -ItemType Directory -Force (Split-Path -Parent $outputPath) | Out-Null

$buildDate = Get-Date -Format "yyyy-MM-ddTHH:mm:ssK"
$commit = Get-CLIProxyAPIPlusCommit -RepoRoot $repoRoot

$env:CGO_ENABLED = "0"
$env:GOTOOLCHAIN = "local"
$env:GOCACHE = $goBuildCache
$env:GOMODCACHE = $goModCache

New-Item -ItemType Directory -Force $goBuildCache | Out-Null
New-Item -ItemType Directory -Force $goModCache | Out-Null
New-Item -ItemType Directory -Force $artifactDir | Out-Null

$goTarget = Resolve-GoTarget -TargetTriple $resolvedTarget
$artifactPath = Join-Path $artifactDir ("cli-proxy-api-plus-" + $resolvedTarget + ".exe")

Write-Host "Building CLIProxyAPIPlus from $cliProjectDir for $resolvedTarget"
if (Test-Path $tempOutputPath) {
    Remove-Item $tempOutputPath -Force
}

Stop-RunningBinaryProcesses -ProcessName $binaryProcessName

Push-Location $cliProjectDir
try {
    $env:GOOS = $goTarget.GOOS
    $env:GOARCH = $goTarget.GOARCH
    & go build `
        -buildvcs=false `
        -trimpath `
        -ldflags "-s -w -X main.Version=workspace -X main.Commit=$commit -X main.BuildDate=$buildDate" `
        -o $tempOutputPath `
        .\cmd\server
    if ($LASTEXITCODE -ne 0) {
        throw "CLIProxyAPIPlus build failed. Ensure the installed Go toolchain satisfies third_party/CLIProxyAPIPlus/go.mod."
    }
} finally {
    Pop-Location
}

if (-not (Test-Path $tempOutputPath)) {
    throw "CLIProxyAPIPlus build did not produce $tempOutputPath"
}

if (Test-Path $outputPath) {
    try {
        Remove-Item $outputPath -Force
    } catch {
        throw "Failed to replace $outputPath because the existing file is still in use. Close TOAPIPROXY or stop the local backend, then try again. $($_.Exception.Message)"
    }
}

try {
    Move-Item $tempOutputPath $outputPath
} catch {
    throw "Failed to move the new CLIProxyAPIPlus binary into place. $($_.Exception.Message)"
}

Write-Host "CLIProxyAPIPlus binary updated at $outputPath"
Copy-Item $outputPath $artifactPath -Force
Write-Host "Target artifact cached at $artifactPath"
