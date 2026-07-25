param(
  [string]$TargetTriple = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root "target\release\sdk-host.exe"
$destinationDir = Join-Path $root "apps\desktop\src-tauri\bin"
$destination = Join-Path $destinationDir "sdk-host-$TargetTriple.exe"

Push-Location $root
try {
  cargo build -p sdk-host --release --target $TargetTriple
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  $source = Join-Path $root "target\$TargetTriple\release\sdk-host.exe"
  New-Item -ItemType Directory -Force -Path $destinationDir | Out-Null
  Copy-Item -LiteralPath $source -Destination $destination -Force
  Write-Output "Prepared Tauri sidecar: $destination"
}
finally {
  Pop-Location
}
