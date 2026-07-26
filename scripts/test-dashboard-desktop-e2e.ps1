param(
    [int]$TimeoutSeconds = 120
)

$ErrorActionPreference = "Stop"

if (-not $IsWindows -and $env:OS -ne "Windows_NT") {
    throw "Dashboard desktop E2E currently requires Windows"
}

Add-Type -AssemblyName UIAutomationClient

$initializeLabel = -join ([char[]](0x521D, 0x59CB, 0x5316))
$environmentLabel = -join ([char[]](0x73AF, 0x5883))
$operationsLabel = -join ([char[]](0x64CD, 0x4F5C))
$reconcileLabel = -join ([char[]](0x5BF9, 0x8D26))
$aiLabel = "AI " + (-join ([char[]](0x52A9, 0x624B)))
$aiProviderSettingsLabel = "AI Provider " + (-join ([char[]](0x8BBE, 0x7F6E)))
$runningLabel = -join ([char[]](0x8FD0, 0x884C, 0x4E2D))
$cdpUnavailableLabel = (-join ([char[]](0x672A, 0x66B4, 0x9732))) + " TCP " + (-join ([char[]](0x5730, 0x5740)))
$internalCdpLabel = "DLL " + (-join ([char[]](0x5185, 0x90E8))) + " CDP / MCP"
$startPattern = (-join ([char[]](0x542F, 0x52A8))) + " *"
$stopPattern = (-join ([char[]](0x505C, 0x6B62))) + " *"

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$dataDir = Join-Path $tempRoot ("brosdk-dashboard-ui-e2e-" + [guid]::NewGuid().ToString("N"))
$resolvedDataDir = [IO.Path]::GetFullPath($dataDir)
$leaf = Split-Path -Leaf $resolvedDataDir
if (-not $resolvedDataDir.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not $leaf.StartsWith("brosdk-dashboard-ui-e2e-", [StringComparison]::Ordinal)) {
    throw "Refusing to use an E2E data directory outside the system temporary directory"
}

$originalDataDir = $env:BROSDK_DATA_DIR
$originalWorkDir = $env:BROSDK_WORK_DIR
$desktopProcess = $null
$viteProcess = $null
$ownsDesktop = $false
$ownsVite = $false
$window = $null
$initializedThroughUi = $false
$startInvoked = $false
$readyObserved = $false
$stopInvoked = $false
$stoppedObserved = $false
$operationIdentityObserved = $false
$aiEnvironmentContextObserved = $false
$cdpEndpointObserved = $false
$aiProviderSettingsObserved = $false
$targetEnvId = $null

function Get-DashboardWindow([int]$AppProcessId) {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    return @($root.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        [System.Windows.Automation.Condition]::TrueCondition
    )) | Where-Object {
        $_.Current.ProcessId -eq $AppProcessId -and $_.Current.Name -eq "BroSDK Dashboard"
    } | Select-Object -First 1
}

function Get-DashboardElements($DashboardWindow) {
    return $DashboardWindow.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
    )
}

function Wait-ForDashboardValue([scriptblock]$Probe, [string]$Description, [int]$Seconds = $TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    do {
        $value = & $Probe
        if ($null -ne $value -and $false -ne $value) {
            return $value
        }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for $Description"
}

function Find-DashboardButton($DashboardWindow, [string]$Name, [switch]$Like, [switch]$Enabled) {
    $elements = Get-DashboardElements $DashboardWindow
    return @($elements) | Where-Object {
        $matchesName = if ($Like) { $_.Current.Name -like $Name } else { $_.Current.Name -eq $Name }
        $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
            $matchesName -and
            (-not $Enabled -or $_.Current.IsEnabled)
    } | Select-Object -First 1
}

function Invoke-DashboardButton($Button) {
    $Button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

try {
    $desktopProcess = Get-Process -Name "brosdk-desktop" -ErrorAction SilentlyContinue |
        Sort-Object StartTime -Descending |
        Select-Object -First 1

    if (-not $desktopProcess) {
        $listener = Get-NetTCPConnection -State Listen -LocalPort 1420 -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $listener) {
            $viteOut = Join-Path $tempRoot ("brosdk-dashboard-vite-" + [guid]::NewGuid().ToString("N") + ".log")
            $viteError = Join-Path $tempRoot ("brosdk-dashboard-vite-" + [guid]::NewGuid().ToString("N") + ".error.log")
            $viteProcess = Start-Process -FilePath (Get-Command node).Source -ArgumentList @(
                (Join-Path $repoRoot "node_modules\vite\bin\vite.js"),
                "--host", "127.0.0.1",
                "--port", "1420",
                "--strictPort"
            ) -WorkingDirectory (Join-Path $repoRoot "apps\dashboard") -WindowStyle Hidden -PassThru `
                -RedirectStandardOutput $viteOut -RedirectStandardError $viteError
            $ownsVite = $true
            Wait-ForDashboardValue {
                try {
                    $response = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:1420/" -TimeoutSec 2
                    return $response.StatusCode -eq 200
                }
                catch {
                    return $false
                }
            } "Dashboard Vite server" 30 | Out-Null
        }

        Set-Location -LiteralPath $repoRoot
        & cargo build -p sdk-host -p brosdk-desktop
        if ($LASTEXITCODE -ne 0) {
            throw "desktop build failed with exit code $LASTEXITCODE"
        }

        $env:BROSDK_DATA_DIR = $resolvedDataDir
        if ([string]::IsNullOrWhiteSpace($env:BROSDK_WORK_DIR)) {
            $env:BROSDK_WORK_DIR = Join-Path $repoRoot "runtime\sdk-work"
        }
        $desktopProcess = Start-Process -FilePath (Join-Path $repoRoot "target\debug\brosdk-desktop.exe") -PassThru
        $ownsDesktop = $true
    }

    $window = Wait-ForDashboardValue { Get-DashboardWindow $desktopProcess.Id } "Dashboard desktop window" 60
    $startupState = Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        $inputElement = @($elements) | Where-Object {
            $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit -and $_.Current.Name -eq "API Key"
        } | Select-Object -First 1
        if ($inputElement) {
            return [pscustomobject]@{ Mode = "first-run"; Element = $inputElement }
        }
        $environmentElement = @($elements) | Where-Object {
            $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
                $_.Current.Name -eq $environmentLabel
        } | Select-Object -First 1
        if ($environmentElement) {
            return [pscustomobject]@{ Mode = "initialized"; Element = $environmentElement }
        }
        return $null
    } "Dashboard startup state" 60
    $apiInput = if ($startupState.Mode -eq "first-run") { $startupState.Element } else { $null }

    if ($apiInput) {
        $apiKey = $env:BROSDK_E2E_API_KEY
        if ([string]::IsNullOrWhiteSpace($apiKey)) {
            $secure = Read-Host "BroSDK API Key" -AsSecureString
            $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
            try {
                $apiKey = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
            }
            finally {
                [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
            }
        }
        if ([string]::IsNullOrWhiteSpace($apiKey)) {
            throw "Dashboard desktop E2E requires an API Key"
        }
        $apiInput.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($apiKey)
        $apiKey = $null
        $initializeButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $initializeLabel -Enabled
        } "enabled initialization button" 10
        Invoke-DashboardButton $initializeButton
        Wait-ForDashboardValue {
            Find-DashboardButton $window $environmentLabel -Enabled
        } "initialized Dashboard workspace" | Out-Null
        $initializedThroughUi = $true
    }

    $environmentButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $environmentLabel -Enabled
    } "environment navigation"
    Invoke-DashboardButton $environmentButton

    $existingStop = Find-DashboardButton $window $stopPattern -Like -Enabled
    if ($existingStop) {
        Invoke-DashboardButton $existingStop
        Wait-ForDashboardValue {
            Find-DashboardButton $window $startPattern -Like -Enabled
        } "stopped baseline environment" | Out-Null
    }

    $startButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $startPattern -Like -Enabled
    } "enabled environment start button"
    if ($startButton.Current.Name -match "\(([^)]+)\)$") {
        $targetEnvId = $Matches[1]
    }
    if ([string]::IsNullOrWhiteSpace($targetEnvId)) {
        throw "Could not extract target envId from the environment control"
    }
    Invoke-DashboardButton $startButton
    $startInvoked = $true

    $stopButton = Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        $readyStatus = @($elements) | Where-Object {
            $_.Current.Name -eq $runningLabel
        } | Select-Object -First 1
        $enabledStop = @($elements) | Where-Object {
            $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
                $_.Current.Name -like $stopPattern -and
                $_.Current.IsEnabled
        } | Select-Object -First 1
        if ($readyStatus -and $enabledStop) { return $enabledStop }
        return $null
    } "ready environment state"
    $readyObserved = $true

    $reconcileButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $reconcileLabel -Enabled
    } "runtime reconcile button"
    Invoke-DashboardButton $reconcileButton
    Start-Sleep -Milliseconds 500
    Wait-ForDashboardValue {
        Find-DashboardButton $window $reconcileLabel -Enabled
    } "completed runtime reconciliation" | Out-Null

    $aiButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $aiLabel -Enabled
    } "AI navigation"
    Invoke-DashboardButton $aiButton
    $cdpContext = Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        $identity = @($elements) | Where-Object {
            $_.Current.Name -eq $targetEnvId
        } | Select-Object -First 1
        $cdp = @($elements) | Where-Object {
            $_.Current.Name -eq $cdpUnavailableLabel -or
                $_.Current.Name -eq $internalCdpLabel -or
                $_.Current.Name -match "^(wss?|https?)://" -or
                $_.Current.Name -match "^(localhost|\d{1,3}(\.\d{1,3}){3}|\[[0-9a-fA-F:]+\]):\d+$"
        } | Select-Object -First 1
        if ($identity -and $cdp) {
            return [pscustomobject]@{
                Concrete = $cdp.Current.Name -match "^(wss?|https?)://" -or
                    $cdp.Current.Name -match "^(localhost|\d{1,3}(\.\d{1,3}){3}|\[[0-9a-fA-F:]+\]):\d+$"
            }
        }
        return $false
    } "AI environment identity and CDP"
    $aiEnvironmentContextObserved = $true
    $cdpEndpointObserved = $cdpContext.Concrete

    $aiSettingsButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $aiProviderSettingsLabel -Enabled
    } "AI Provider settings entry"
    Invoke-DashboardButton $aiSettingsButton
    Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        @($elements) | Where-Object {
            $_.Current.Name -eq "OpenAI-compatible Base URL"
        } | Select-Object -First 1
    } "AI Provider settings" | Out-Null
    $aiProviderSettingsObserved = $true

    $environmentButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $environmentLabel -Enabled
    } "environment navigation after AI verification"
    Invoke-DashboardButton $environmentButton
    $stopButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $stopPattern -Like -Enabled
    } "ready environment stop button after AI verification"
    Invoke-DashboardButton $stopButton
    $stopInvoked = $true

    Wait-ForDashboardValue {
        Find-DashboardButton $window $startPattern -Like -Enabled
    } "restored stopped environment" | Out-Null
    $stoppedObserved = $true

    $operationsButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $operationsLabel -Enabled
    } "operations navigation"
    Invoke-DashboardButton $operationsButton
    Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        @($elements) | Where-Object {
            $_.Current.Name -like "*$targetEnvId*"
        } | Select-Object -First 1
    } "operation center environment identity" | Out-Null
    $operationIdentityObserved = $true

    [ordered]@{
        status = "passed"
        desktopLaunchedByTest = $ownsDesktop
        initializedThroughUi = $initializedThroughUi
        startInvokedFromDashboard = $startInvoked
        readyObservedInDashboard = $readyObserved
        stopInvokedFromDashboard = $stopInvoked
        stoppedObservedInDashboard = $stoppedObserved
        operationIdentityObserved = $operationIdentityObserved
        aiEnvironmentContextObserved = $aiEnvironmentContextObserved
        cdpEndpointObserved = $cdpEndpointObserved
        aiProviderSettingsObserved = $aiProviderSettingsObserved
    } | ConvertTo-Json
}
finally {
    if ($window -and $startInvoked -and -not $stoppedObserved) {
        try {
            $cleanupEnvironment = Find-DashboardButton $window $environmentLabel -Enabled
            if ($cleanupEnvironment) {
                Invoke-DashboardButton $cleanupEnvironment
                Start-Sleep -Seconds 1
            }
            $cleanupStop = Find-DashboardButton $window $stopPattern -Like -Enabled
            if ($cleanupStop) {
                Invoke-DashboardButton $cleanupStop
                Start-Sleep -Seconds 2
            }
        }
        catch {
        }
    }
    if ($ownsDesktop -and $desktopProcess -and -not $desktopProcess.HasExited) {
        $desktopProcess.CloseMainWindow() | Out-Null
        if (-not $desktopProcess.WaitForExit(10000)) {
            Stop-Process -Id $desktopProcess.Id -Force
        }
    }
    if ($ownsVite -and $viteProcess -and -not $viteProcess.HasExited) {
        Stop-Process -Id $viteProcess.Id -Force
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
    if ($ownsDesktop -and (Test-Path -LiteralPath $resolvedDataDir)) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}
