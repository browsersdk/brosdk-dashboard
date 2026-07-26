param()

$ErrorActionPreference = "Stop"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("brosdk-credential-e2e-" + [guid]::NewGuid().ToString("N"))
$previousDataDir = $env:BROSDK_DATA_DIR
$previousApiKey = $env:BROSDK_API_KEY
$secretAllocated = $false

try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    if (-not $env:BROSDK_E2E_API_KEY) {
        $secure = Read-Host "BroSDK API Key" -AsSecureString
        $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
        try {
            $env:BROSDK_E2E_API_KEY = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
            $secretAllocated = $true
        }
        finally {
            [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
        }
    }
    Remove-Item Env:BROSDK_API_KEY -ErrorAction SilentlyContinue
    $env:BROSDK_DATA_DIR = $tempRoot
    $env:BROSDK_E2E_CREDENTIAL = "1"
    cargo build -p sdk-host
    if ($LASTEXITCODE -ne 0) { throw "sdk-host build failed" }
    cargo run -p manager --bin credential-e2e
    if ($LASTEXITCODE -ne 0) { throw "credential E2E failed" }
}
finally {
    Remove-Item Env:BROSDK_E2E_CREDENTIAL -ErrorAction SilentlyContinue
    if ($secretAllocated) {
        Remove-Item Env:BROSDK_E2E_API_KEY -ErrorAction SilentlyContinue
    }
    if ($null -eq $previousDataDir) { Remove-Item Env:BROSDK_DATA_DIR -ErrorAction SilentlyContinue } else { $env:BROSDK_DATA_DIR = $previousDataDir }
    if ($null -eq $previousApiKey) { Remove-Item Env:BROSDK_API_KEY -ErrorAction SilentlyContinue } else { $env:BROSDK_API_KEY = $previousApiKey }
    $resolvedTemp = [System.IO.Path]::GetFullPath($tempRoot)
    $resolvedSystemTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedTemp.StartsWith($resolvedSystemTemp, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedTemp)) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
    }
}
