param(
    [int]$Port = 1431
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$targetDir = Join-Path $repoRoot "target\readme-screenshots"
$viteProcess = $null

if (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) {
    throw "Port $Port is already in use"
}

New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
$viteOut = Join-Path $targetDir "vite.log"
$viteError = Join-Path $targetDir "vite.error.log"

try {
    $viteProcess = Start-Process -FilePath (Get-Command node).Source -ArgumentList @(
        (Join-Path $repoRoot "node_modules\vite\bin\vite.js"),
        "--host", "127.0.0.1",
        "--port", "$Port",
        "--strictPort"
    ) -WorkingDirectory (Join-Path $repoRoot "apps\dashboard") -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput $viteOut -RedirectStandardError $viteError

    $deadline = (Get-Date).AddSeconds(30)
    do {
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/" -TimeoutSec 2
            if ($response.StatusCode -eq 200) { break }
        }
        catch {
        }
        Start-Sleep -Milliseconds 300
    } while ((Get-Date) -lt $deadline)

    if (-not $response -or $response.StatusCode -ne 200) {
        throw "Dashboard preview did not start on port $Port"
    }

    & node (Join-Path $PSScriptRoot "capture-readme-screenshots.mjs") "http://127.0.0.1:$Port"
    if ($LASTEXITCODE -ne 0) {
        throw "README screenshot capture failed with exit code $LASTEXITCODE"
    }
}
finally {
    if ($viteProcess -and -not $viteProcess.HasExited) {
        Stop-Process -Id $viteProcess.Id -Force
    }
}
