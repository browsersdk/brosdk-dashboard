param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$dataDir = Join-Path $tempRoot ("brosdk-dashboard-lifecycle-e2e-" + [guid]::NewGuid().ToString("N"))
$resolvedDataDir = [IO.Path]::GetFullPath($dataDir)
$leaf = Split-Path -Leaf $resolvedDataDir
if (-not $resolvedDataDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not $leaf.StartsWith("brosdk-dashboard-lifecycle-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use an E2E data directory outside the system temporary directory"
}

$originalDataDir = $env:BROSDK_DATA_DIR
$originalWorkDir = $env:BROSDK_WORK_DIR
$originalUseOnlyEnv = $env:BROSDK_E2E_USE_ONLY_ENV
$originalEmbeddedPort = $env:BROSDK_EMBEDDED_PORT
$secretAllocated = $false
$portAllocated = $false

try {
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_API_KEY)) {
        $secure = Read-Host "BroSDK API Key" -AsSecureString
        $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
        try {
            $env:BROSDK_API_KEY = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
            $secretAllocated = $true
        }
        finally {
            [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
        }
    }
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_E2E_ENV_ID) -and
        [string]::IsNullOrWhiteSpace($env:BROSDK_E2E_USE_ONLY_ENV)) {
        $env:BROSDK_E2E_USE_ONLY_ENV = "1"
    }
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_EMBEDDED_PORT)) {
        $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
        $listener.Start()
        try {
            $env:BROSDK_EMBEDDED_PORT = [string]$listener.LocalEndpoint.Port
            $portAllocated = $true
        }
        finally {
            $listener.Stop()
        }
    }
    $env:BROSDK_DATA_DIR = $resolvedDataDir
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_WORK_DIR)) {
        $env:BROSDK_WORK_DIR = Join-Path $repoRoot "runtime\sdk-work"
    }
    Set-Location -LiteralPath $repoRoot

    & cargo build -p sdk-host
    if ($LASTEXITCODE -ne 0) {
        throw "sdk-host build failed with exit code $LASTEXITCODE"
    }
    & cargo run -p manager --bin environment-e2e
    if ($LASTEXITCODE -ne 0) {
        throw "environment lifecycle E2E failed with exit code $LASTEXITCODE"
    }
}
finally {
    if ($secretAllocated) {
        Remove-Item Env:BROSDK_API_KEY -ErrorAction SilentlyContinue
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
    if ($null -eq $originalUseOnlyEnv) {
        Remove-Item Env:BROSDK_E2E_USE_ONLY_ENV -ErrorAction SilentlyContinue
    } else {
        $env:BROSDK_E2E_USE_ONLY_ENV = $originalUseOnlyEnv
    }
    if ($portAllocated) {
        if ($null -eq $originalEmbeddedPort) {
            Remove-Item Env:BROSDK_EMBEDDED_PORT -ErrorAction SilentlyContinue
        } else {
            $env:BROSDK_EMBEDDED_PORT = $originalEmbeddedPort
        }
    }
    if (Test-Path -LiteralPath $resolvedDataDir) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}
