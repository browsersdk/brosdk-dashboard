param(
  [string]$InstallerPath = "dist\release\BroSDK-Dashboard-0.1.0-windows-x64-setup.exe",
  [int]$TimeoutSeconds = 180,
  [switch]$FullDashboardE2e
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
  throw "Windows installer smoke test requires Windows"
}

$root = Split-Path -Parent $PSScriptRoot
$installerPath = if ([IO.Path]::IsPathRooted($InstallerPath)) {
  [IO.Path]::GetFullPath($InstallerPath)
}
else {
  [IO.Path]::GetFullPath((Join-Path $root $InstallerPath))
}
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$installRoot = Join-Path $tempRoot ("brosdk-dashboard-installer-e2e-" + [Guid]::NewGuid().ToString("N"))
$uninstaller = $null
$installed = $false

function Assert-TestPath([string]$Path) {
  $fullPath = [IO.Path]::GetFullPath($Path)
  $prefix = $tempRoot.TrimEnd('\') + '\'
  if (-not $fullPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
      -not (Split-Path -Leaf $fullPath).StartsWith("brosdk-dashboard-installer-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use installer test path outside the managed temp root: $fullPath"
  }
}

function Get-BroSdkInstallations {
  $keys = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
  )
  foreach ($key in $keys) {
    if (-not (Test-Path -LiteralPath $key)) { continue }
    Get-ChildItem -LiteralPath $key | ForEach-Object {
      $item = Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue
      if ($item.DisplayName -eq "BroSDK Dashboard") {
        $item
      }
    }
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

if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
  throw "NSIS installer not found: $installerPath"
}
if (@(Get-BroSdkInstallations).Count -gt 0) {
  throw "Refusing installer smoke test because BroSDK Dashboard is already installed"
}
Assert-TestPath $installRoot

try {
  $installProcess = Start-Process -FilePath $installerPath -ArgumentList @(
    "/S",
    "/D=$installRoot"
  ) -WindowStyle Hidden -Wait -PassThru
  if ($installProcess.ExitCode -ne 0) {
    throw "NSIS installer exited with code $($installProcess.ExitCode)"
  }
  $installed = $true

  $desktopExecutable = Get-ChildItem -LiteralPath $installRoot -File -Recurse -Filter "*.exe" |
    Where-Object { $_.Name -notin @("sdk-host.exe", "uninstall.exe") } |
    Sort-Object FullName |
    Select-Object -First 1
  $sidecar = Get-ChildItem -LiteralPath $installRoot -File -Recurse -Filter "sdk-host.exe" | Select-Object -First 1
  $sdkDll = Get-ChildItem -LiteralPath $installRoot -File -Recurse -Filter "brosdk.dll" | Select-Object -First 1
  $uninstaller = Get-ChildItem -LiteralPath $installRoot -File -Recurse -Filter "uninstall.exe" | Select-Object -First 1

  foreach ($required in @($desktopExecutable, $sidecar, $sdkDll, $uninstaller)) {
    if (-not $required) {
      throw "Installed release is missing a required executable or SDK file"
    }
  }
  foreach ($executable in @($desktopExecutable, $sidecar)) {
    if ((Get-PeSubsystem $executable.FullName) -ne 2) {
      throw "Installed executable is not linked as a Windows GUI subsystem application: $($executable.Name)"
    }
  }

  $desktopTest = Join-Path $PSScriptRoot "test-dashboard-desktop-e2e.ps1"
  if ([string]::IsNullOrWhiteSpace($env:BROSDK_E2E_API_KEY) -and -not $FullDashboardE2e) {
    & $desktopTest -TimeoutSeconds $TimeoutSeconds -DesktopExecutable $desktopExecutable.FullName -FirstRunOnly
  }
  else {
    & $desktopTest -TimeoutSeconds $TimeoutSeconds -DesktopExecutable $desktopExecutable.FullName
  }

  $uninstallProcess = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -WindowStyle Hidden -Wait -PassThru
  if ($uninstallProcess.ExitCode -ne 0) {
    throw "NSIS uninstaller exited with code $($uninstallProcess.ExitCode)"
  }
  $installed = $false
  Start-Sleep -Seconds 2

  if (@(Get-BroSdkInstallations).Count -gt 0) {
    throw "BroSDK Dashboard uninstall registration remains after silent uninstall"
  }
  if (Test-Path -LiteralPath $desktopExecutable.FullName -PathType Leaf) {
    throw "Installed Dashboard executable remains after silent uninstall"
  }

  [ordered]@{
    status = "passed"
    installer = Split-Path -Leaf $installerPath
    requiredFilesPresent = $true
    desktopE2e = if ([string]::IsNullOrWhiteSpace($env:BROSDK_E2E_API_KEY) -and -not $FullDashboardE2e) { "first-run" } else { "full" }
    silentUninstall = $true
  } | ConvertTo-Json
}
finally {
  if ($installed -and $uninstaller -and (Test-Path -LiteralPath $uninstaller.FullName -PathType Leaf)) {
    try {
      Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -WindowStyle Hidden -Wait | Out-Null
      Start-Sleep -Seconds 1
    }
    catch {
    }
  }
  if (Test-Path -LiteralPath $installRoot) {
    Assert-TestPath $installRoot
    Remove-Item -LiteralPath $installRoot -Recurse -Force
  }
}
