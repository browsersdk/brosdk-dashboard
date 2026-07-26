param(
  [string]$ReleaseRoot = "dist\release"
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
  throw "Windows MSI smoke test requires Windows"
}

$root = Split-Path -Parent $PSScriptRoot
$releaseRoot = if ([IO.Path]::IsPathRooted($ReleaseRoot)) {
  [IO.Path]::GetFullPath($ReleaseRoot)
}
else {
  [IO.Path]::GetFullPath((Join-Path $root $ReleaseRoot))
}
$manifestPath = Join-Path $releaseRoot "WINDOWS-RELEASE-MANIFEST.json"
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())

function Assert-TestPath([string]$Path) {
  $fullPath = [IO.Path]::GetFullPath($Path)
  $prefix = $tempRoot.TrimEnd('\') + '\'
  if (-not $fullPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
      -not (Split-Path -Leaf $fullPath).StartsWith("brosdk-dashboard-msi-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use MSI test path outside the managed temp root: $fullPath"
  }
}

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
  throw "Windows release manifest not found: $manifestPath"
}

$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$msiArtifacts = @($manifest.artifacts | Where-Object { $_.kind -eq "msi" })
if ($msiArtifacts.Count -eq 0) {
  throw "Windows release manifest does not contain MSI artifacts"
}

$verified = 0
foreach ($artifact in $msiArtifacts) {
  $msiPath = [IO.Path]::GetFullPath((Join-Path $releaseRoot ($artifact.path.Replace("/", "\"))))
  $extractRoot = Join-Path $tempRoot ("brosdk-dashboard-msi-e2e-" + [Guid]::NewGuid().ToString("N"))
  Assert-TestPath $extractRoot
  try {
    New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null
    $arguments = @(
      "/a",
      ('"' + $msiPath + '"'),
      "/qn",
      "TARGETDIR=$extractRoot"
    )
    $process = Start-Process -FilePath "msiexec.exe" -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) {
      throw "MSI administrative extraction failed for $($artifact.path) with code $($process.ExitCode)"
    }

    $app = Get-ChildItem -LiteralPath $extractRoot -File -Recurse -Filter "*.exe" |
      Where-Object { $_.Name -in @("BroSDK Dashboard.exe", "brosdk-desktop.exe") } |
      Select-Object -First 1
    $sidecar = Get-ChildItem -LiteralPath $extractRoot -File -Recurse -Filter "sdk-host.exe" | Select-Object -First 1
    $sdkDll = Get-ChildItem -LiteralPath $extractRoot -File -Recurse -Filter "brosdk.dll" | Select-Object -First 1
    if (-not $app -or -not $sidecar -or -not $sdkDll) {
      throw "MSI administrative image is missing Dashboard, sdk-host.exe, or brosdk.dll: $($artifact.path)"
    }
    $verified++
  }
  finally {
    if (Test-Path -LiteralPath $extractRoot) {
      Assert-TestPath $extractRoot
      Remove-Item -LiteralPath $extractRoot -Recurse -Force
    }
  }
}

[ordered]@{
  status = "passed"
  msiPackages = $verified
  administrativeExtraction = $true
  requiredFilesPresent = $true
} | ConvertTo-Json
