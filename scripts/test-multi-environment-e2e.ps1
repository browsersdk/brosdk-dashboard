param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$dataDir = Join-Path $tempRoot ("brosdk-dashboard-multi-e2e-" + [guid]::NewGuid().ToString("N"))
$resolvedDataDir = [IO.Path]::GetFullPath($dataDir)
$leaf = Split-Path -Leaf $resolvedDataDir
if (-not $resolvedDataDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not $leaf.StartsWith("brosdk-dashboard-multi-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use an E2E data directory outside the system temporary directory"
}

$originalDataDir = $env:BROSDK_DATA_DIR
$originalWorkDir = $env:BROSDK_WORK_DIR
$originalMutation = $env:BROSDK_E2E_ALLOW_MUTATION
$originalEmbeddedPort = $env:BROSDK_EMBEDDED_PORT
$originalAiBaseUrl = $env:BROSDK_AI_BASE_URL
$originalAiModel = $env:BROSDK_AI_MODEL

try {
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
    $sourceSecrets = Join-Path $sourceDataDir "secrets"
    $targetSecrets = Join-Path $resolvedDataDir "secrets"
    New-Item -ItemType Directory -Path $targetSecrets -Force | Out-Null
    foreach ($secretName in @("sdk-api-key.bin", "ai-api-key.bin")) {
        $sourceSecret = Join-Path $sourceSecrets $secretName
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
    & cargo run -p manager --bin multi-environment-e2e
    if ($LASTEXITCODE -ne 0) {
        throw "multi-environment E2E failed with exit code $LASTEXITCODE"
    }
}
finally {
    if ($null -eq $originalMutation) {
        Remove-Item Env:BROSDK_E2E_ALLOW_MUTATION -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_E2E_ALLOW_MUTATION = $originalMutation
    }
    if ($null -eq $originalDataDir) {
        Remove-Item Env:BROSDK_DATA_DIR -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_DATA_DIR = $originalDataDir
    }
    if ($null -eq $originalWorkDir) {
        Remove-Item Env:BROSDK_WORK_DIR -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_WORK_DIR = $originalWorkDir
    }
    if ($null -eq $originalEmbeddedPort) {
        Remove-Item Env:BROSDK_EMBEDDED_PORT -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_EMBEDDED_PORT = $originalEmbeddedPort
    }
    if ($null -eq $originalAiBaseUrl) {
        Remove-Item Env:BROSDK_AI_BASE_URL -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_AI_BASE_URL = $originalAiBaseUrl
    }
    if ($null -eq $originalAiModel) {
        Remove-Item Env:BROSDK_AI_MODEL -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_AI_MODEL = $originalAiModel
    }
    if (Test-Path -LiteralPath $resolvedDataDir) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}
