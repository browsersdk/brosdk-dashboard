param(
  [string]$PortableRoot = "dist\release\BroSDK-Dashboard-portable"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$portableRoot = [IO.Path]::GetFullPath((Join-Path $root $PortableRoot))
$manifestPath = Join-Path $portableRoot "RELEASE-MANIFEST.json"

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

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "Portable release manifest not found: $manifestPath"
}

$requiredFiles = @(
  "BroSDK Dashboard.exe",
  "sdk-host.exe",
  "brosdk/brosdk.dll"
)
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$manifestPaths = @($manifest.files | ForEach-Object { $_.path })

foreach ($relativePath in $requiredFiles) {
  $path = Join-Path $portableRoot ($relativePath.Replace("/", "\"))
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Required portable release file is missing: $relativePath"
  }
  if ($manifestPaths -notcontains $relativePath) {
    throw "Required file is missing from RELEASE-MANIFEST.json: $relativePath"
  }
}

foreach ($entry in $manifest.files) {
  $path = Join-Path $portableRoot ($entry.path.Replace("/", "\"))
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Manifest file is missing: $($entry.path)"
  }
  if ((Get-Sha256 $path) -ne $entry.sha256) {
    throw "Manifest hash mismatch: $($entry.path)"
  }
  if ((Get-Item -LiteralPath $path).Length -ne [int64]$entry.size) {
    throw "Manifest size mismatch: $($entry.path)"
  }
}

Write-Output "Portable release verified: $portableRoot"
Write-Output "Files: $($manifest.files.Count)"
