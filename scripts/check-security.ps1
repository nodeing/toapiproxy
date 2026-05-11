$ErrorActionPreference = "Stop"

$RootDir = (& git rev-parse --show-toplevel).Trim()
Set-Location $RootDir

$script:Failures = 0
$script:Warnings = 0

function Add-Failure {
    param([string]$Message)
    Write-Host "ERROR: $Message" -ForegroundColor Red
    $script:Failures += 1
}

function Add-Warning {
    param([string]$Message)
    Write-Host "WARN: $Message" -ForegroundColor Yellow
    $script:Warnings += 1
}

function Test-EnvExample {
    param([string]$Path)
    return $Path -match '(\.env\.example|\.env\.sample|\.env\.template|\.example\.env|\.sample\.env)$'
}

function Test-ForbiddenPath {
    param([string]$Path)
    $normalized = $Path -replace '\\', '/'

    if ($normalized -match '(^|/)\.env($|\.)' -and -not (Test-EnvExample $normalized)) {
        Add-Failure "$normalized is a local environment file. Do not commit it."
    }

    if ($normalized -match '\.(pem|key|p12|pfx|jks|keystore|mobileprovision)$') {
        Add-Failure "$normalized looks like a private key, certificate, or signing credential."
    }

    if ($normalized -match '(^|/)auths/.*\.(json|ya?ml|toml)$') {
        Add-Failure "$normalized is an auth data file."
    }

    if ($normalized -match '(^|/)(auth|token|tokens|credential|credentials|secret|secrets)\.(json|ya?ml|toml)$') {
        Add-Failure "$normalized looks like a saved credential file."
    }

    if ($normalized -match '(^|/)(config\.local|.*\.local)\.(json|ya?ml|toml)$') {
        Add-Failure "$normalized looks like a local private config file."
    }

    if ($normalized -in @(
        "config.yaml",
        "config.yml",
        "codex-config-profiles.json",
        "claude-config-profiles.json",
        "backend-usage.json",
        "usage-statistics/backend-usage.json"
    )) {
        Add-Failure "$normalized looks like local runtime data or a private config file."
    }
}

$KnownSecretRegex = '-----BEGIN (RSA |DSA |EC |OPENSSH |PGP )?PRIVATE KEY-----|(^|[^A-Z0-9])(AKIA|ASIA)[0-9A-Z]{16}([^A-Z0-9]|$)|(^|[^A-Za-z0-9_-])sk-(proj-)?[A-Za-z0-9_-]{20,}([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9_-])sk-ant-[A-Za-z0-9_-]{20,}([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9_])(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{30,}([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_])github_pat_[A-Za-z0-9_]{20,}([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_-])AIza[0-9A-Za-z_-]{35}([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9-])xox[baprs]-[A-Za-z0-9-]{20,}([^A-Za-z0-9-]|$)|(^|[^A-Za-z0-9_-])eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}([^A-Za-z0-9_-]|$)'

function Test-TextFile {
    param([string]$Path)
    try {
        $bytes = [System.IO.File]::ReadAllBytes($Path)
        $limit = [Math]::Min($bytes.Length, 4096)
        for ($i = 0; $i -lt $limit; $i++) {
            if ($bytes[$i] -eq 0) {
                return $false
            }
        }
        return $true
    } catch {
        return $false
    }
}

function Scan-FileContent {
    param(
        [string]$Path,
        [string]$Label
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return
    }

    if (-not (Test-TextFile $Path)) {
        return
    }

    $lineNumber = 0
    foreach ($line in [System.IO.File]::ReadLines((Resolve-Path -LiteralPath $Path))) {
        $lineNumber += 1
        if ($line -match $KnownSecretRegex) {
            Add-Failure "${Label}:$lineNumber contains a known secret pattern."
        }
    }
}

function Test-BundledConfig {
    param(
        [string]$Path,
        [string]$Label
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Add-Failure "$Label is missing."
        return
    }

    $inKeyList = $false
    foreach ($rawLine in [System.IO.File]::ReadLines((Resolve-Path -LiteralPath $Path))) {
        $line = $rawLine -replace '\s+#.*$', ''
        if ($line -match '^\s*#') {
            continue
        }

        if ($inKeyList) {
            if ($line -match '^\s*-\s*\S') {
                Add-Failure "$Label contains a non-empty key list."
                return
            }
            if ($line -notmatch '^\s+') {
                $inKeyList = $false
            }
        }

        if ($line -match '^\s*-\s*api-key:\s*(.+)$') {
            $value = $Matches[1].Trim()
            if ($value -ne "" -and $value -ne '""' -and $value -ne "''") {
                Add-Failure "$Label contains a non-empty upstream api-key value."
                return
            }
        }

        if ($line -match '^\s*(api-keys|codex-api-key):\s*$') {
            $inKeyList = $true
        } elseif ($line -match '^\s*(api-keys|codex-api-key):\s*(.+)$') {
            $value = ($Matches[2] -replace '\s', '')
            if ($value -ne '[]') {
                Add-Failure "$Label contains non-empty api-keys or codex-api-key values."
                return
            }
        }
    }
}

$paths = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($path in (& git ls-files)) {
    if ($path) { [void]$paths.Add($path) }
}
foreach ($path in (& git ls-files --others --exclude-standard)) {
    if ($path) { [void]$paths.Add($path) }
}
foreach ($path in (& git diff --cached --name-only --diff-filter=ACMR)) {
    if ($path) { [void]$paths.Add($path) }
}

foreach ($path in $paths) {
    Test-ForbiddenPath $path
    Scan-FileContent $path "working:$path"

    if (($path -replace '\\', '/') -match '^docs/img/.*\.(png|jpg|jpeg|webp)$') {
        Add-Warning "$path is a documentation image. Manually confirm it does not show personal account data."
    }
}

Test-BundledConfig "src-tauri/resources/config.yaml" "src-tauri/resources/config.yaml"

$stagedPaths = & git diff --cached --name-only --diff-filter=ACMR
foreach ($stagedPath in $stagedPaths) {
    if (-not $stagedPath) { continue }

    Test-ForbiddenPath $stagedPath
    $tempFile = New-TemporaryFile
    try {
        & git show ":$stagedPath" | Set-Content -LiteralPath $tempFile -NoNewline
        Scan-FileContent $tempFile "staged:$stagedPath"
        if ($stagedPath -eq "src-tauri/resources/config.yaml") {
            Test-BundledConfig $tempFile "staged:src-tauri/resources/config.yaml"
        }
    } finally {
        Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
    }
}

if ($script:Failures -gt 0) {
    Write-Host ""
    Write-Host "Security check failed with $script:Failures problem(s)." -ForegroundColor Red
    exit 1
}

if ($script:Warnings -gt 0) {
    Write-Host "Security check passed with $script:Warnings warning(s)."
} else {
    Write-Host "Security check passed."
}
