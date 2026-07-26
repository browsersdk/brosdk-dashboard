param(
  [switch]$RequireMsi,
  [switch]$RequireSignature,
  [string]$ReleaseRoot = "dist\release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$releaseRoot = if ([IO.Path]::IsPathRooted($ReleaseRoot)) {
  [IO.Path]::GetFullPath($ReleaseRoot)
}
else {
  [IO.Path]::GetFullPath((Join-Path $root $ReleaseRoot))
}
$manifestPath = Join-Path $releaseRoot "WINDOWS-RELEASE-MANIFEST.json"
$portableVerifier = Join-Path $PSScriptRoot "verify-windows-portable.ps1"

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

function Get-SignatureStatus([string]$Path) {
  try {
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate]::CreateFromSignedFile($Path)
    $certificate.Reset()
  }
  catch {
    return "NotSigned"
  }

  $signTool = Get-ChildItem -LiteralPath "${env:ProgramFiles(x86)}\Windows Kits\10\bin" `
    -Filter "signtool.exe" -File -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.DirectoryName.EndsWith("\x64", [StringComparison]::OrdinalIgnoreCase) } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
  if (-not $signTool) {
    return "Present"
  }
  & $signTool.FullName verify /pa /q $Path 2>$null
  return $(if ($LASTEXITCODE -eq 0) { "Valid" } else { "Invalid" })
}

function Resolve-ArtifactPath([string]$RelativePath) {
  $path = [IO.Path]::GetFullPath((Join-Path $releaseRoot ($RelativePath.Replace("/", "\"))))
  $prefix = $releaseRoot.TrimEnd('\') + '\'
  if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Release manifest path escapes release root: $RelativePath"
  }
  return $path
}

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "Windows release manifest not found: $manifestPath"
}

& $portableVerifier -PortableRoot (Join-Path $releaseRoot "BroSDK-Dashboard-portable")
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$kinds = @($manifest.artifacts | ForEach-Object { $_.kind })
foreach ($requiredKind in @("portable", "nsis")) {
  if ($kinds -notcontains $requiredKind) {
    throw "Windows release manifest is missing required artifact kind: $requiredKind"
  }
}
if ($RequireMsi -and $kinds -notcontains "msi") {
  throw "Windows release manifest is missing required artifact kind: msi"
}

foreach ($artifact in $manifest.artifacts) {
  $path = Resolve-ArtifactPath $artifact.path
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Release artifact is missing: $($artifact.path)"
  }
  $item = Get-Item -LiteralPath $path
  if ($item.Length -ne [int64]$artifact.size) {
    throw "Release artifact size mismatch: $($artifact.path)"
  }
  if ((Get-Sha256 $path) -ne $artifact.sha256) {
    throw "Release artifact hash mismatch: $($artifact.path)"
  }
  if ($artifact.kind -eq "nsis") {
    if ($item.Length -lt 1MB) {
      throw "NSIS installer is unexpectedly small: $($artifact.path)"
    }
    if ($item.VersionInfo.ProductVersion -ne $manifest.version) {
      throw "NSIS installer version does not match release manifest: $($artifact.path)"
    }
  }
  if ($artifact.kind -in @("nsis", "msi")) {
    $signatureStatus = Get-SignatureStatus $path
    if ($RequireSignature -and $signatureStatus -ne "Valid") {
      throw "Release artifact is not validly signed: $($artifact.path) ($signatureStatus)"
    }
  }
}

$portable = $manifest.artifacts | Where-Object { $_.kind -eq "portable" } | Select-Object -First 1
$portablePath = Resolve-ArtifactPath $portable.path
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($portablePath)
try {
  $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace("\", "/") })
  foreach ($requiredEntry in @("BroSDK Dashboard.exe", "sdk-host.exe", "brosdk/brosdk.dll", "RELEASE-MANIFEST.json")) {
    if ($entries -notcontains $requiredEntry) {
      throw "Portable ZIP is missing required entry: $requiredEntry"
    }
  }
}
finally {
  $archive.Dispose()
}

Write-Output "Windows release verified: $releaseRoot"
Write-Output "Artifacts: $($manifest.artifacts.Count)"
Write-Output "Signature policy: $(if ($RequireSignature) { 'required' } else { 'reported only' })"
