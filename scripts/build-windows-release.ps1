param(
  [switch]$PortableOnly,
  [switch]$Msi,
  [string]$TargetTriple = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path $root "dist\release"
$portableRoot = Join-Path $releaseRoot "BroSDK-Dashboard-portable"
$sidecarScript = Join-Path $PSScriptRoot "prepare-windows-sidecar.ps1"
$toolsScript = Join-Path $PSScriptRoot "prepare-tauri-windows-tools.ps1"

if ($PortableOnly -and $Msi) {
  throw "PortableOnly and Msi cannot be used together"
}

function Assert-ChildPath([string]$Path, [string]$Parent) {
  $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  $fullParent = [IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
  if (-not $fullPath.StartsWith($fullParent, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to modify path outside release root: $fullPath"
  }
}

function Remove-ManagedDirectory([string]$Path, [string]$Parent) {
  Assert-ChildPath $Path $Parent
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
      Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction SilentlyContinue |
        ForEach-Object { $_.Attributes = [IO.FileAttributes]::Normal }
      Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
      return
    }
    catch {
      if ($attempt -eq 3) {
        throw
      }
      Start-Sleep -Seconds $attempt
    }
  }
}

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

function Get-ReleaseArtifact([string]$Path, [string]$Kind) {
  $item = Get-Item -LiteralPath $Path
  return [ordered]@{
    kind = $Kind
    path = $item.FullName.Substring($releaseRoot.TrimEnd('\').Length + 1).Replace("\", "/")
    sha256 = Get-Sha256 $item.FullName
    size = $item.Length
    signatureStatus = Get-SignatureStatus $Path
  }
}

function New-PortableRelease([string]$TargetRelease, [string]$Version, [string]$Architecture) {
  if (Test-Path -LiteralPath $portableRoot) {
    Remove-ManagedDirectory $portableRoot $releaseRoot
  }
  New-Item -ItemType Directory -Force -Path (Join-Path $portableRoot "brosdk") | Out-Null
  Copy-Item -LiteralPath (Join-Path $TargetRelease "brosdk-desktop.exe") -Destination (Join-Path $portableRoot "BroSDK Dashboard.exe")
  Copy-Item -LiteralPath (Join-Path $TargetRelease "sdk-host.exe") -Destination (Join-Path $portableRoot "sdk-host.exe")
  Copy-Item -LiteralPath (Join-Path $root "libs\windows_x64\brosdk.dll") -Destination (Join-Path $portableRoot "brosdk\brosdk.dll")

  $portablePrefix = $portableRoot.TrimEnd('\') + '\'
  $files = Get-ChildItem -LiteralPath $portableRoot -File -Recurse | ForEach-Object {
    [ordered]@{
      path = $_.FullName.Substring($portablePrefix.Length).Replace("\", "/")
      sha256 = Get-Sha256 $_.FullName
      size = $_.Length
    }
  }
  [ordered]@{
    product = "BroSDK Dashboard"
    version = $Version
    target = $TargetTriple
    generatedAt = [DateTime]::UtcNow.ToString("o")
    files = $files
  } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $portableRoot "RELEASE-MANIFEST.json") -Encoding utf8

  $zipPath = Join-Path $releaseRoot "BroSDK-Dashboard-$Version-windows-$Architecture-portable.zip"
  if (Test-Path -LiteralPath $zipPath) {
    Assert-ChildPath $zipPath $releaseRoot
    Remove-Item -LiteralPath $zipPath -Force
  }
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
  return $zipPath
}

Push-Location $root
try {
  New-Item -ItemType Directory -Force -Path $releaseRoot | Out-Null
  & $sidecarScript -TargetTriple $TargetTriple
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  npm run build --workspace apps/desktop -- --no-bundle --target $TargetTriple --ci
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  $config = Get-Content -Raw (Join-Path $root "apps\desktop\src-tauri\tauri.conf.json") | ConvertFrom-Json
  $version = $config.version
  $architecture = if ($TargetTriple.StartsWith("x86_64-")) { "x64" } else { $TargetTriple.Split('-')[0] }
  $targetRelease = Join-Path $root "target\$TargetTriple\release"
  $staleInstallerPatterns = @(
    "BroSDK-Dashboard-$version-windows-$architecture-setup.exe",
    "BroSDK-Dashboard-$version-windows-$architecture*.msi"
  )
  foreach ($pattern in $staleInstallerPatterns) {
    Get-ChildItem -LiteralPath $releaseRoot -File -Filter $pattern -ErrorAction SilentlyContinue |
      ForEach-Object {
        Assert-ChildPath $_.FullName $releaseRoot
        Remove-Item -LiteralPath $_.FullName -Force
      }
  }
  $portableZip = New-PortableRelease $targetRelease $version $architecture
  $artifacts = @((Get-ReleaseArtifact $portableZip "portable"))

  if (-not $PortableOnly) {
    $bundleTypes = if ($Msi) { @("nsis", "msi") } else { @("nsis") }
    & $toolsScript -IncludeWix:$Msi
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    if ($Msi) {
      npm run build --workspace apps/desktop -- --target $TargetTriple --bundles nsis msi --ci
    }
    else {
      npm run build --workspace apps/desktop -- --target $TargetTriple --bundles nsis --ci
    }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    foreach ($bundleType in $bundleTypes) {
      $bundleRoot = Join-Path $targetRelease "bundle\$bundleType"
      $extension = if ($bundleType -eq "msi") { ".msi" } else { ".exe" }
      $bundleArtifacts = @(Get-ChildItem -LiteralPath $bundleRoot -File | Where-Object { $_.Extension -eq $extension })
      if ($bundleArtifacts.Count -eq 0) {
        throw "No $bundleType bundle was created under $bundleRoot"
      }

      for ($index = 0; $index -lt $bundleArtifacts.Count; $index++) {
        $suffix = ""
        if ($bundleArtifacts.Count -gt 1) {
          $localeMatch = [regex]::Match($bundleArtifacts[$index].BaseName, "_([a-z]{2}-[A-Z]{2})$")
          $suffix = if ($localeMatch.Success) { "-$($localeMatch.Groups[1].Value)" } else { "-$($index + 1)" }
        }
        $destinationName = if ($bundleType -eq "msi") {
          "BroSDK-Dashboard-$version-windows-$architecture$suffix.msi"
        }
        else {
          "BroSDK-Dashboard-$version-windows-$architecture-setup.exe"
        }
        $destination = Join-Path $releaseRoot $destinationName
        Copy-Item -LiteralPath $bundleArtifacts[$index].FullName -Destination $destination -Force
        $artifacts += Get-ReleaseArtifact $destination $bundleType
        Write-Output "$($bundleType.ToUpperInvariant()) release: $destination"
      }
    }
  }

  [ordered]@{
    product = "BroSDK Dashboard"
    version = $version
    target = $TargetTriple
    generatedAt = [DateTime]::UtcNow.ToString("o")
    artifacts = $artifacts
  } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $releaseRoot "WINDOWS-RELEASE-MANIFEST.json") -Encoding utf8

  Write-Output "Portable release: $portableZip"
  Write-Output "Release manifest: $(Join-Path $releaseRoot 'WINDOWS-RELEASE-MANIFEST.json')"
}
finally {
  Pop-Location
}
