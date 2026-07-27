param(
  [string]$PortableRoot = "dist\release\BroSDK-Dashboard-portable"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$portableRoot = if ([IO.Path]::IsPathRooted($PortableRoot)) {
  [IO.Path]::GetFullPath($PortableRoot)
}
else {
  [IO.Path]::GetFullPath((Join-Path $root $PortableRoot))
}
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

function Get-PeSubsystem([string]$Path) {
  $bytes = [IO.File]::ReadAllBytes($Path)
  if ($bytes.Length -lt 256 -or [BitConverter]::ToUInt16($bytes, 0) -ne 0x5A4D) {
    throw "File is not a valid PE executable: $Path"
  }
  $peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
  if ($peOffset -lt 0 -or $peOffset + 94 -gt $bytes.Length -or
      [BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550) {
    throw "File has an invalid PE header: $Path"
  }
  return [BitConverter]::ToUInt16($bytes, $peOffset + 24 + 68)
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

foreach ($relativePath in @("BroSDK Dashboard.exe", "sdk-host.exe")) {
  $path = Join-Path $portableRoot $relativePath
  if ((Get-PeSubsystem $path) -ne 2) {
    throw "Windows executable is not linked as a GUI subsystem application: $relativePath"
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
