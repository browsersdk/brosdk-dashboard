param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$dataDir = Join-Path $tempRoot ("brosdk-dashboard-ai-e2e-" + [guid]::NewGuid().ToString("N"))
$resolvedDataDir = [IO.Path]::GetFullPath($dataDir)
$leaf = Split-Path -Leaf $resolvedDataDir
if (-not $resolvedDataDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not $leaf.StartsWith("brosdk-dashboard-ai-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use an AI E2E data directory outside the system temporary directory"
}

$originalDataDir = $env:BROSDK_DATA_DIR
$originalWorkDir = $env:BROSDK_WORK_DIR
$originalMutation = $env:BROSDK_E2E_ALLOW_MUTATION
$originalEmbeddedPort = $env:BROSDK_EMBEDDED_PORT
$originalAiBaseUrl = $env:BROSDK_AI_BASE_URL
$originalAiModel = $env:BROSDK_AI_MODEL

try {
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_E2E_ENV_ID)) {
        throw "BROSDK_E2E_ENV_ID is required"
    }
    if (Get-Process -Name "sdk-host" -ErrorAction SilentlyContinue) {
        throw "An sdk-host process is already running; close the desktop runtime before this isolated E2E"
    }

    $configuredDataDir = $null
    $configPath = Join-Path $env:LOCALAPPDATA "BroSDK Dashboard\config\data-dir.json"
    if (Test-Path -LiteralPath $configPath) {
        $configuredDataDir = (Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json).dataDir
    }
    $sourceDataDir = if ([string]::IsNullOrWhiteSpace($configuredDataDir)) {
        Join-Path $env:LOCALAPPDATA "BroSDK Dashboard"
    } else {
        [IO.Path]::GetFullPath($configuredDataDir)
    }
    $targetSecrets = Join-Path $resolvedDataDir "secrets"
    New-Item -ItemType Directory -Path $targetSecrets -Force | Out-Null
    foreach ($secretName in @("sdk-api-key.bin", "ai-api-key.bin")) {
        $sourceSecret = Join-Path (Join-Path $sourceDataDir "secrets") $secretName
        if (Test-Path -LiteralPath $sourceSecret) {
            Copy-Item -LiteralPath $sourceSecret -Destination (Join-Path $targetSecrets $secretName)
        }
    }
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_API_KEY) -and
        -not (Test-Path -LiteralPath (Join-Path $targetSecrets "sdk-api-key.bin"))) {
        throw "BroSDK API key is unavailable from the environment and secure storage"
    }
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_AI_API_KEY) -and
        -not (Test-Path -LiteralPath (Join-Path $targetSecrets "ai-api-key.bin"))) {
        throw "AI API key is unavailable from the environment and secure storage"
    }

    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try {
        $env:BROSDK_EMBEDDED_PORT = [string]$listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
    $env:BROSDK_E2E_ALLOW_MUTATION = "1"
    $env:BROSDK_DATA_DIR = $resolvedDataDir
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_AI_BASE_URL)) {
        $env:BROSDK_AI_BASE_URL = "https://api.deepseek.com"
    }
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_AI_MODEL)) {
        $env:BROSDK_AI_MODEL = "deepseek-v4-flash"
    }
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_WORK_DIR)) {
        $env:BROSDK_WORK_DIR = Join-Path $repoRoot "runtime\sdk-work"
    }
    Set-Location -LiteralPath $repoRoot
    & cargo build -p sdk-host
    if ($LASTEXITCODE -ne 0) {
        throw "sdk-host build failed with exit code $LASTEXITCODE"
    }
    & cargo run -p manager --bin ai-assistant-e2e
    if ($LASTEXITCODE -ne 0) {
        throw "AI assistant E2E failed with exit code $LASTEXITCODE"
    }
}
finally {
    foreach ($entry in @(
        @{ Name = "BROSDK_DATA_DIR"; Value = $originalDataDir },
        @{ Name = "BROSDK_WORK_DIR"; Value = $originalWorkDir },
        @{ Name = "BROSDK_E2E_ALLOW_MUTATION"; Value = $originalMutation },
        @{ Name = "BROSDK_EMBEDDED_PORT"; Value = $originalEmbeddedPort },
        @{ Name = "BROSDK_AI_BASE_URL"; Value = $originalAiBaseUrl },
        @{ Name = "BROSDK_AI_MODEL"; Value = $originalAiModel }
    )) {
        if ($null -eq $entry.Value) {
            Remove-Item ("Env:" + $entry.Name) -ErrorAction SilentlyContinue
        } else {
            Set-Item ("Env:" + $entry.Name) $entry.Value
        }
    }
    if (Test-Path -LiteralPath $resolvedDataDir) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}
