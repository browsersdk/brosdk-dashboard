param(
  [switch]$PortableOnly,
  [string]$TargetTriple = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path $root "dist\release"
$portableRoot = Join-Path $releaseRoot "BroSDK-Dashboard-portable"
$sidecarScript = Join-Path $PSScriptRoot "prepare-windows-sidecar.ps1"

function Get-Sha256([string]$Path) {
  $sha256 = [Security.Cryptography.SHA256]::Create()
  $stream = [IO.File]::OpenRead($Path)
  try {
    return ([BitConverter]::ToString($sha256.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
  }
  finally {
    $stream.Dispose()
    $sha256.Dispose()
  }
}

Push-Location $root
try {
  & $sidecarScript -TargetTriple $TargetTriple
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  if ($PortableOnly) {
    npm run build --workspace apps/desktop -- --no-bundle --target $TargetTriple
  }
  else {
    npm run build --workspace apps/desktop -- --target $TargetTriple
  }
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  $targetRelease = Join-Path $root "target\$TargetTriple\release"
  if (Test-Path -LiteralPath $portableRoot) {
    Remove-Item -LiteralPath $portableRoot -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path (Join-Path $portableRoot "brosdk") | Out-Null
  Copy-Item -LiteralPath (Join-Path $targetRelease "brosdk-desktop.exe") -Destination (Join-Path $portableRoot "BroSDK Dashboard.exe")
  Copy-Item -LiteralPath (Join-Path $targetRelease "sdk-host.exe") -Destination (Join-Path $portableRoot "sdk-host.exe")
  Copy-Item -LiteralPath (Join-Path $root "libs\windows_x64\brosdk.dll") -Destination (Join-Path $portableRoot "brosdk\brosdk.dll")

  $version = (Get-Content -Raw (Join-Path $root "apps\desktop\src-tauri\tauri.conf.json") | ConvertFrom-Json).version
  $portablePrefix = $portableRoot.TrimEnd('\') + '\'
  $files = Get-ChildItem -LiteralPath $portableRoot -File -Recurse | ForEach-Object {
    $relativePath = $_.FullName.Substring($portablePrefix.Length).Replace("\", "/")
    [ordered]@{
      path = $relativePath
      sha256 = Get-Sha256 $_.FullName
      size = $_.Length
    }
  }
  [ordered]@{
    product = "BroSDK Dashboard"
    version = $version
    target = $TargetTriple
    generatedAt = [DateTime]::UtcNow.ToString("o")
    files = $files
  } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $portableRoot "RELEASE-MANIFEST.json") -Encoding utf8

  $zipPath = Join-Path $releaseRoot "BroSDK-Dashboard-$version-windows-x64-portable.zip"
  if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [IO.Compression.ZipFile]::CreateFromDirectory(
    $portableRoot,
    $zipPath,
    [IO.Compression.CompressionLevel]::Optimal,
    $false
  )
  if (-not (Test-Path -LiteralPath $zipPath -PathType Leaf)) {
    throw "Portable ZIP was not created: $zipPath"
  }
  Write-Output "Portable release: $zipPath"
}
finally {
  Pop-Location
}
