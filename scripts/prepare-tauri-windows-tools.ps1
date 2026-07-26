param(
  [switch]$IncludeWix
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
  throw "Tauri Windows bundle tools can only be prepared on Windows"
}

$cacheRoot = Join-Path $env:LOCALAPPDATA "tauri"
$downloadRoot = Join-Path $env:TEMP "brosdk-dashboard-tauri-tools"

function Assert-ChildPath([string]$Path, [string]$Parent) {
  $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  $fullParent = [IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
  if (-not $fullPath.StartsWith($fullParent, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to modify path outside managed root: $fullPath"
  }
}

function Test-ExpectedHash(
  [string]$Path,
  [string]$Algorithm,
  [string]$ExpectedHash
) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $false
  }
  $hasher = switch ($Algorithm.ToUpperInvariant()) {
    "SHA1" { [Security.Cryptography.SHA1]::Create(); break }
    "SHA256" { [Security.Cryptography.SHA256]::Create(); break }
    default { throw "Unsupported hash algorithm: $Algorithm" }
  }
  $stream = [IO.File]::OpenRead($Path)
  try {
    $actualHash = ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace("-", "")
    return $actualHash -eq $ExpectedHash
  }
  finally {
    $stream.Dispose()
    $hasher.Dispose()
  }
}

function Get-VerifiedDownload(
  [string]$Url,
  [string]$Path,
  [string]$Algorithm,
  [string]$ExpectedHash,
  [long]$ExpectedSize
) {
  if (Test-ExpectedHash $Path $Algorithm $ExpectedHash) {
    return
  }
  if ((Test-Path -LiteralPath $Path -PathType Leaf) -and
      (Get-Item -LiteralPath $Path).Length -ge $ExpectedSize) {
    Remove-Item -LiteralPath $Path -Force
  }

  Write-Output "Downloading verified Tauri bundle tool: $Url"
  & curl.exe @(
    "--ssl-no-revoke",
    "--location",
    "--retry", "6",
    "--retry-all-errors",
    "--retry-delay", "3",
    "--connect-timeout", "30",
    "--max-time", "1800",
    "--continue-at", "-",
    "--output", $Path,
    $Url
  )
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to download Tauri bundle tool; rerun the command to resume: $Url"
  }
  if (-not (Test-ExpectedHash $Path $Algorithm $ExpectedHash)) {
    Remove-Item -LiteralPath $Path -Force
    throw "Downloaded Tauri bundle tool failed $Algorithm verification: $Path"
  }
}

function Test-RequiredFiles([string]$Root, [string[]]$RelativePaths) {
  foreach ($relativePath in $RelativePaths) {
    if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath) -PathType Leaf)) {
      return $false
    }
  }
  return $true
}

New-Item -ItemType Directory -Force -Path $cacheRoot, $downloadRoot | Out-Null

$nsisCache = Join-Path $cacheRoot "NSIS"
$nsisRequired = @(
  "makensis.exe",
  "Stubs\lzma-x86-unicode",
  "Stubs\lzma_solid-x86-unicode",
  "Include\MUI2.nsh",
  "Include\FileFunc.nsh",
  "Include\x64.nsh",
  "Include\nsDialogs.nsh",
  "Include\WinMessages.nsh",
  "Include\Win\COM.nsh",
  "Include\Win\Propkey.nsh",
  "Include\Win\RestartManager.nsh",
  "Plugins\x86-unicode\additional\nsis_tauri_utils.dll"
)
$nsisPlugin = Join-Path $nsisCache "Plugins\x86-unicode\additional\nsis_tauri_utils.dll"
$nsisReady = (Test-RequiredFiles $nsisCache $nsisRequired) -and
  (Test-ExpectedHash $nsisPlugin "SHA1" "75197FEE3C6A814FE035788D1C34EAD39349B860")

if (-not $nsisReady) {
  $nsisArchive = Join-Path $downloadRoot "nsis-3.11.zip"
  $pluginDownload = Join-Path $downloadRoot "nsis_tauri_utils-v0.5.3.dll"
  Get-VerifiedDownload `
    "https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip" `
    $nsisArchive `
    "SHA1" `
    "EF7FF767E5CBD9EDD22ADD3A32C9B8F4500BB10D" `
    2361546
  Get-VerifiedDownload `
    "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll" `
    $pluginDownload `
    "SHA1" `
    "75197FEE3C6A814FE035788D1C34EAD39349B860" `
    34304

  $extractRoot = Join-Path $env:TEMP ("brosdk-dashboard-nsis-" + [Guid]::NewGuid().ToString("N"))
  Assert-ChildPath $extractRoot $env:TEMP
  try {
    Expand-Archive -LiteralPath $nsisArchive -DestinationPath $extractRoot -Force
    $sourceRoot = Join-Path $extractRoot "nsis-3.11"
    if (-not (Test-Path -LiteralPath $sourceRoot -PathType Container)) {
      throw "Unexpected NSIS archive structure"
    }
    if (Test-Path -LiteralPath $nsisCache) {
      Assert-ChildPath $nsisCache $cacheRoot
      Remove-Item -LiteralPath $nsisCache -Recurse -Force
    }
    Copy-Item -LiteralPath $sourceRoot -Destination $nsisCache -Recurse -Force
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $nsisPlugin) | Out-Null
    Copy-Item -LiteralPath $pluginDownload -Destination $nsisPlugin -Force
  }
  finally {
    if (Test-Path -LiteralPath $extractRoot) {
      Assert-ChildPath $extractRoot $env:TEMP
      Remove-Item -LiteralPath $extractRoot -Recurse -Force
    }
  }
}

if (-not ((Test-RequiredFiles $nsisCache $nsisRequired) -and
    (Test-ExpectedHash $nsisPlugin "SHA1" "75197FEE3C6A814FE035788D1C34EAD39349B860"))) {
  throw "Tauri NSIS tool cache is incomplete: $nsisCache"
}
Write-Output "Tauri NSIS tools ready: $nsisCache"

if ($IncludeWix) {
  $wixCache = Join-Path $cacheRoot "WixTools314"
  $wixRequired = @(
    "candle.exe",
    "candle.exe.config",
    "darice.cub",
    "light.exe",
    "light.exe.config",
    "wconsole.dll",
    "winterop.dll",
    "wix.dll",
    "WixUIExtension.dll",
    "WixUtilExtension.dll"
  )
  if (-not (Test-RequiredFiles $wixCache $wixRequired)) {
    $wixArchive = Join-Path $downloadRoot "wix314-binaries.zip"
    Get-VerifiedDownload `
      "https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip" `
      $wixArchive `
      "SHA256" `
      "6AC824E1642D6F7277D0ED7EA09411A508F6116BA6FAE0AA5F2C7DAA2FF43D31" `
      41297555

    $extractRoot = Join-Path $env:TEMP ("brosdk-dashboard-wix-" + [Guid]::NewGuid().ToString("N"))
    Assert-ChildPath $extractRoot $env:TEMP
    try {
      Expand-Archive -LiteralPath $wixArchive -DestinationPath $extractRoot -Force
      if (Test-Path -LiteralPath $wixCache) {
        Assert-ChildPath $wixCache $cacheRoot
        Remove-Item -LiteralPath $wixCache -Recurse -Force
      }
      New-Item -ItemType Directory -Force -Path $wixCache | Out-Null
      Copy-Item -Path (Join-Path $extractRoot "*") -Destination $wixCache -Recurse -Force
    }
    finally {
      if (Test-Path -LiteralPath $extractRoot) {
        Assert-ChildPath $extractRoot $env:TEMP
        Remove-Item -LiteralPath $extractRoot -Recurse -Force
      }
    }
  }
  if (-not (Test-RequiredFiles $wixCache $wixRequired)) {
    throw "Tauri WiX tool cache is incomplete: $wixCache"
  }
  Write-Output "Tauri WiX tools ready: $wixCache"
}
