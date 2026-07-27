param(
    [int]$TimeoutSeconds = 60,
    [string]$DesktopExecutable = ""
)

$ErrorActionPreference = "Stop"

if (-not $IsWindows -and $env:OS -ne "Windows_NT") {
    throw "Desktop tray E2E currently requires Windows"
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class BroSdkTrayNative {
    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X; public int Y; }

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out Point point);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
}
'@

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$dataDir = [IO.Path]::Combine($tempRoot, "brosdk-dashboard-tray-e2e-" + [guid]::NewGuid().ToString("N"))
$desktopProcess = $null
$secondProcess = $null
$viteProcess = $null
$ownsVite = $false
$originalDataDir = $env:BROSDK_DATA_DIR
$originalWorkDir = $env:BROSDK_WORK_DIR
$cursor = [BroSdkTrayNative+Point]::new()
$cursorCaptured = [BroSdkTrayNative]::GetCursorPos([ref]$cursor)
$notificationIconPattern = "*" + (-join ([char[]](0x56FE, 0x6807))) + "*"
$exitLabel = -join ([char[]](0x9000, 0x51FA))

function Wait-ForValue([scriptblock]$Probe, [string]$Description, [int]$Seconds = $TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $value = & $Probe
        if ($null -ne $value -and $false -ne $value) {
            return $value
        }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for $Description"
}

function Get-DesktopElements {
    try {
        return [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition
        )
    }
    catch {
        return @()
    }
}

function Find-TrayButton([string]$Name) {
    return @(Get-DesktopElements) | Where-Object {
        $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
        $_.Current.ClassName -eq "SystemTray.NormalButton" -and
        $_.Current.Name -eq $Name -and
        -not $_.Current.IsOffscreen
    } | Select-Object -First 1
}

function Get-BroSdkTrayButton {
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        $tray = Find-TrayButton "BroSDK Dashboard"
        if ($tray) {
            return $tray
        }
        $chevron = @(Get-DesktopElements) | Where-Object {
            $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
            $_.Current.ClassName -eq "SystemTray.NormalButton" -and
            $_.Current.AutomationId -eq "SystemTrayIcon" -and
            $_.Current.Name -like $notificationIconPattern
        } | Select-Object -First 1
        if (-not $chevron) {
            Start-Sleep -Milliseconds 500
            continue
        }
        $chevron.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $deadline = (Get-Date).AddSeconds(4)
        do {
            Start-Sleep -Milliseconds 200
            $tray = Find-TrayButton "BroSDK Dashboard"
            if ($tray) {
                return $tray
            }
        } while ((Get-Date) -lt $deadline)
    }
    throw "Timed out waiting for BroSDK tray icon"
}

function Invoke-Element($Element) {
    $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Find-TrayExitItem([int]$ProcessId) {
    return @(Get-DesktopElements) | Where-Object {
        $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::MenuItem -and
        $_.Current.ProcessId -eq $ProcessId -and
        $_.Current.Name -eq $exitLabel -and
        -not $_.Current.IsOffscreen
    } | Select-Object -First 1
}

try {
    [IO.Directory]::CreateDirectory($dataDir) | Out-Null
    $env:BROSDK_DATA_DIR = $dataDir
    $env:BROSDK_WORK_DIR = Join-Path $repoRoot "runtime\sdk-work"

    $listener = Get-NetTCPConnection -State Listen -LocalPort 1420 -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $listener) {
        $viteOut = Join-Path $tempRoot ("brosdk-tray-vite-" + [guid]::NewGuid().ToString("N") + ".log")
        $viteError = Join-Path $tempRoot ("brosdk-tray-vite-" + [guid]::NewGuid().ToString("N") + ".error.log")
        $viteProcess = Start-Process -FilePath (Get-Command node).Source -ArgumentList @(
            (Join-Path $repoRoot "node_modules\vite\bin\vite.js"),
            "--host", "127.0.0.1", "--port", "1420", "--strictPort"
        ) -WorkingDirectory (Join-Path $repoRoot "apps\dashboard") -WindowStyle Hidden -PassThru `
            -RedirectStandardOutput $viteOut -RedirectStandardError $viteError
        $ownsVite = $true
        Wait-ForValue {
            try {
                (Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:1420/" -TimeoutSec 2).StatusCode -eq 200
            }
            catch {
                $false
            }
        } "Dashboard Vite server" 30 | Out-Null
    }

    if ([string]::IsNullOrWhiteSpace($DesktopExecutable)) {
        Push-Location $repoRoot
        try {
            & cargo build -p sdk-host -p brosdk-desktop
            if ($LASTEXITCODE -ne 0) {
                throw "desktop build failed with exit code $LASTEXITCODE"
            }
        }
        finally {
            Pop-Location
        }
        $DesktopExecutable = Join-Path $repoRoot "target\debug\brosdk-desktop.exe"
    }
    else {
        $DesktopExecutable = [IO.Path]::GetFullPath($DesktopExecutable)
    }

    $desktopProcess = Start-Process -FilePath $DesktopExecutable -PassThru
    $window = Wait-ForValue {
        @(Get-DesktopElements) | Where-Object {
            $_.Current.ProcessId -eq $desktopProcess.Id -and
            $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Window -and
            $_.Current.Name -eq "BroSDK Dashboard"
        } | Select-Object -First 1
    } "Dashboard window"
    $windowHandle = [IntPtr]$window.Current.NativeWindowHandle
    Wait-ForValue { [BroSdkTrayNative]::IsWindowVisible($windowHandle) } "visible Dashboard window" | Out-Null
    Start-Sleep -Seconds 2

    [BroSdkTrayNative]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-ForValue { -not [BroSdkTrayNative]::IsWindowVisible($windowHandle) } "hidden Dashboard window" | Out-Null
    if ($desktopProcess.HasExited) {
        throw "Dashboard process exited after the main window was closed"
    }

    $secondProcess = Start-Process -FilePath $DesktopExecutable -PassThru
    Wait-ForValue {
        $secondProcess.Refresh()
        $secondProcess.HasExited
    } "second Dashboard instance exit" 15 | Out-Null
    if ($secondProcess.ExitCode -ne 0) {
        throw "Second Dashboard instance exited with code $($secondProcess.ExitCode)"
    }
    Wait-ForValue { [BroSdkTrayNative]::IsWindowVisible($windowHandle) } "existing Dashboard restored by second launch" | Out-Null

    [BroSdkTrayNative]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-ForValue { -not [BroSdkTrayNative]::IsWindowVisible($windowHandle) } "Dashboard hidden before tray restore" | Out-Null

    $trayButton = Get-BroSdkTrayButton
    Invoke-Element $trayButton
    Wait-ForValue { [BroSdkTrayNative]::IsWindowVisible($windowHandle) } "Dashboard restored from tray" | Out-Null

    [BroSdkTrayNative]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-ForValue { -not [BroSdkTrayNative]::IsWindowVisible($windowHandle) } "Dashboard hidden before tray exit" | Out-Null
    $exitItem = $null
    for ($attempt = 1; $attempt -le 3 -and -not $exitItem; $attempt++) {
        $trayButton = Get-BroSdkTrayButton
        $bounds = $trayButton.Current.BoundingRectangle
        if ($bounds.Width -le 0 -or $bounds.Height -le 0) {
            throw "BroSDK tray icon did not expose clickable bounds"
        }
        [BroSdkTrayNative]::SetCursorPos(
            [int]($bounds.X + ($bounds.Width / 2)),
            [int]($bounds.Y + ($bounds.Height / 2))
        ) | Out-Null
        Start-Sleep -Milliseconds 100
        [BroSdkTrayNative]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
        [BroSdkTrayNative]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $deadline = (Get-Date).AddSeconds(3)
        do {
            Start-Sleep -Milliseconds 200
            $exitItem = Find-TrayExitItem $desktopProcess.Id
        } while (-not $exitItem -and (Get-Date) -lt $deadline)
    }
    if (-not $exitItem) {
        throw "Timed out waiting for tray exit menu item"
    }
    $exitBounds = $exitItem.Current.BoundingRectangle
    [BroSdkTrayNative]::SetCursorPos(
        [int]($exitBounds.X + ($exitBounds.Width / 2)),
        [int]($exitBounds.Y + ($exitBounds.Height / 2))
    ) | Out-Null
    [BroSdkTrayNative]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [BroSdkTrayNative]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Wait-ForValue {
        $desktopProcess.Refresh()
        $desktopProcess.HasExited
    } "Dashboard process exit" 15 | Out-Null

    [ordered]@{
        status = "passed"
        closeHidWindow = $true
        processStayedRunning = $true
        secondInstanceRedirected = $true
        trayRestoredWindow = $true
        trayMenuExitedProcess = $true
    } | ConvertTo-Json
}
finally {
    if ($cursorCaptured) {
        [BroSdkTrayNative]::SetCursorPos($cursor.X, $cursor.Y) | Out-Null
    }
    if ($desktopProcess -and -not $desktopProcess.HasExited) {
        Stop-Process -Id $desktopProcess.Id -Force -ErrorAction SilentlyContinue
        $desktopProcess.WaitForExit()
    }
    if ($secondProcess -and -not $secondProcess.HasExited) {
        Stop-Process -Id $secondProcess.Id -Force -ErrorAction SilentlyContinue
        $secondProcess.WaitForExit()
    }
    if ($ownsVite -and $viteProcess -and -not $viteProcess.HasExited) {
        Stop-Process -Id $viteProcess.Id -Force -ErrorAction SilentlyContinue
        $viteProcess.WaitForExit()
    }
    if ($null -eq $originalDataDir) {
        Remove-Item Env:BROSDK_DATA_DIR -ErrorAction SilentlyContinue
    }
    else {
        $env:BROSDK_DATA_DIR = $originalDataDir
    }
    if ($null -eq $originalWorkDir) {
        Remove-Item Env:BROSDK_WORK_DIR -ErrorAction SilentlyContinue
    }
    else {
        $env:BROSDK_WORK_DIR = $originalWorkDir
    }
    $resolvedDataDir = [IO.Path]::GetFullPath($dataDir)
    if ($resolvedDataDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($resolvedDataDir).StartsWith("brosdk-dashboard-tray-e2e-") -and
        [IO.Directory]::Exists($resolvedDataDir)) {
        [IO.Directory]::Delete($resolvedDataDir, $true)
    }
}
