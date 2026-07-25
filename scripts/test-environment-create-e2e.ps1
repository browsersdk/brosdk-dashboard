param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$dataDir = Join-Path $tempRoot ("brosdk-dashboard-create-e2e-" + [guid]::NewGuid().ToString("N"))
$resolvedDataDir = [IO.Path]::GetFullPath($dataDir)
$leaf = Split-Path -Leaf $resolvedDataDir
if (-not $resolvedDataDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not $leaf.StartsWith("brosdk-dashboard-create-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use an E2E data directory outside the system temporary directory"
}

$originalDataDir = $env:BROSDK_DATA_DIR
$originalWorkDir = $env:BROSDK_WORK_DIR

try {
    $env:BROSDK_DATA_DIR = $resolvedDataDir
    if ([string]::IsNullOrWhiteSpace($env:BROSDK_WORK_DIR)) {
        $env:BROSDK_WORK_DIR = Join-Path $repoRoot "runtime\sdk-work"
    }
    Set-Location -LiteralPath $repoRoot

    & cargo build -p sdk-host
    if ($LASTEXITCODE -ne 0) {
        throw "sdk-host build failed with exit code $LASTEXITCODE"
    }
    & cargo run -p manager --bin environment-create-e2e
    if ($LASTEXITCODE -ne 0) {
        throw "environment create E2E failed with exit code $LASTEXITCODE"
    }
}
finally {
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
    if (Test-Path -LiteralPath $resolvedDataDir) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}
