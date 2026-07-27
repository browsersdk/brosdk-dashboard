param(
    [int]$TimeoutSeconds = 120,
    [string]$DesktopExecutable = "",
    [switch]$FirstRunOnly,
    [switch]$AgentLifecycle,
    [switch]$AgentStatusQuery,
    [string]$TargetEnvironmentId = ""
)

$ErrorActionPreference = "Stop"

if (-not $IsWindows -and $env:OS -ne "Windows_NT") {
    throw "Dashboard desktop E2E currently requires Windows"
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Windows.Forms

$initializeLabel = -join ([char[]](0x521D, 0x59CB, 0x5316))
$environmentLabel = -join ([char[]](0x73AF, 0x5883))
$operationsLabel = -join ([char[]](0x64CD, 0x4F5C))
$settingsLabel = -join ([char[]](0x8BBE, 0x7F6E))
$runSdkSelfCheckLabel = (-join ([char[]](0x8FD0, 0x884C))) + " SDK " + (-join ([char[]](0x81EA, 0x68C0)))
$recentSelfCheckLabel = -join ([char[]](0x6700, 0x8FD1, 0x81EA, 0x68C0))
$reconcileLabel = -join ([char[]](0x5BF9, 0x8D26))
$aiLabel = "AI " + (-join ([char[]](0x52A9, 0x624B)))
$aiProviderSettingsLabel = "AI Provider " + (-join ([char[]](0x8BBE, 0x7F6E)))
$aiRequestLabel = "AI " + (-join ([char[]](0x8BF7, 0x6C42)))
$aiReplyLabel = "AI " + (-join ([char[]](0x56DE, 0x590D)))
$aiErrorLabel = "AI " + (-join ([char[]](0x9519, 0x8BEF)))
$generatePlanLabel = -join ([char[]](0x751F, 0x6210, 0x8BA1, 0x5212))
$approvePlanLabel = -join ([char[]](0x6279, 0x51C6, 0x5E76, 0x6267, 0x884C))
$automaticExecutionLabel = -join ([char[]](0x81EA, 0x52A8, 0x6267, 0x884C))
$runAgentLabel = (-join ([char[]](0x8FD0, 0x884C))) + " Agent"
$newConversationLabel = -join ([char[]](0x65B0, 0x5EFA, 0x4F1A, 0x8BDD))
$singleEnvironmentLabel = -join ([char[]](0x5355, 0x73AF, 0x5883))
$newConversationEnvironmentLabel = -join ([char[]](0x65B0, 0x4F1A, 0x8BDD, 0x5173, 0x8054, 0x73AF, 0x5883))
$createLabel = -join ([char[]](0x521B, 0x5EFA))
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
$originalAiBaseUrl = $env:BROSDK_AI_BASE_URL
$originalAiModel = $env:BROSDK_AI_MODEL
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
$sdkSelfCheckObserved = $false
$agentPlanObserved = $false
$agentApprovalInvoked = $false
$agentOperationObserved = $false
$chatEnterReplyObserved = $false
$agentStoppedStatusObserved = $false
$agentStatusLeftEnvironmentStopped = $false
$targetEnvId = $null
$environmentConversationCreated = $false

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

function Find-DashboardEdit($DashboardWindow, [string]$Name, [switch]$Enabled) {
    $elements = Get-DashboardElements $DashboardWindow
    return @($elements) | Where-Object {
        $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit -and
            $_.Current.Name -eq $Name -and
            (-not $Enabled -or $_.Current.IsEnabled)
    } | Select-Object -First 1
}

function Find-ReadyEnvironmentStopButton($DashboardWindow, [string]$NamePattern) {
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $buttons = @(Get-DashboardElements $DashboardWindow) | Where-Object {
        $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
            $_.Current.Name -like $NamePattern -and
            $_.Current.IsEnabled
    }
    foreach ($button in $buttons) {
        $ancestor = $button
        for ($depth = 0; $depth -lt 8 -and $ancestor; $depth++) {
            if ($ancestor.Current.ControlType -eq [System.Windows.Automation.ControlType]::DataItem) {
                $rowElements = $ancestor.FindAll(
                    [System.Windows.Automation.TreeScope]::Descendants,
                    [System.Windows.Automation.Condition]::TrueCondition
                )
                if (@($rowElements) | Where-Object { $_.Current.Name -eq $runningLabel } | Select-Object -First 1) {
                    return $button
                }
            }
            $ancestor = $walker.GetParent($ancestor)
        }
    }
    return $null
}

function Invoke-DashboardButton($Button) {
    $Button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Select-DashboardComboBoxOption($DashboardWindow, [string]$ComboName, [string]$OptionPattern) {
    $combo = Wait-ForDashboardValue {
        @(Get-DashboardElements $DashboardWindow) | Where-Object {
            $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::ComboBox -and
                $_.Current.Name -eq $ComboName -and
                $_.Current.IsEnabled
        } | Select-Object -First 1
    } "combo box $ComboName"
    try {
        $selection = $combo.GetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern).Current.GetSelection()
        if (@($selection) | Where-Object { $_.Current.Name -like $OptionPattern } | Select-Object -First 1) {
            return
        }
    }
    catch {
    }
    $expand = $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
    $expand.Expand()
    try {
        $option = Wait-ForDashboardValue {
            $root = [System.Windows.Automation.AutomationElement]::RootElement
            $matches = @($root.FindAll(
                [System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.Condition]::TrueCondition
            )) | Where-Object {
                $_.Current.Name -like $OptionPattern
            }
            foreach ($match in $matches) {
                try {
                    [void]$match.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
                    return $match
                }
                catch {
                }
            }
            return $null
        } "combo box option $OptionPattern" 15
        $option.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    }
    finally {
        if ($expand.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Expanded) {
            $expand.Collapse()
        }
    }
}

function New-DashboardConversation($DashboardWindow, [string]$EnvironmentId = "") {
    $newConversationButton = Wait-ForDashboardValue {
        Find-DashboardButton $DashboardWindow $newConversationLabel -Enabled
    } "new AI conversation"
    Invoke-DashboardButton $newConversationButton
    if (-not [string]::IsNullOrWhiteSpace($EnvironmentId)) {
        $singleEnvironmentButton = Wait-ForDashboardValue {
            Find-DashboardButton $DashboardWindow $singleEnvironmentLabel -Enabled
        } "single-environment conversation scope"
        Invoke-DashboardButton $singleEnvironmentButton
        Select-DashboardComboBoxOption $DashboardWindow $newConversationEnvironmentLabel "*$EnvironmentId*"
    }
    $createButton = Wait-ForDashboardValue {
        Find-DashboardButton $DashboardWindow $createLabel -Enabled
    } "create AI conversation"
    Invoke-DashboardButton $createButton
}

try {
    if ($AgentStatusQuery -and [string]::IsNullOrWhiteSpace($TargetEnvironmentId) -and
        -not [string]::IsNullOrWhiteSpace($env:BROSDK_E2E_ENV_ID)) {
        $TargetEnvironmentId = $env:BROSDK_E2E_ENV_ID
    }

    if ([string]::IsNullOrWhiteSpace($DesktopExecutable)) {
        $desktopProcess = Get-Process -Name "brosdk-desktop" -ErrorAction SilentlyContinue |
            Sort-Object StartTime -Descending |
            Select-Object -First 1
    }

    if (-not $desktopProcess) {
        if ([string]::IsNullOrWhiteSpace($DesktopExecutable)) {
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
            $DesktopExecutable = Join-Path $repoRoot "target\debug\brosdk-desktop.exe"
        }
        else {
            $DesktopExecutable = [IO.Path]::GetFullPath($DesktopExecutable)
            if (-not (Test-Path -LiteralPath $DesktopExecutable -PathType Leaf)) {
                throw "Desktop executable not found: $DesktopExecutable"
            }
            $existingDesktop = Get-Process -Name "brosdk-desktop", "BroSDK Dashboard" -ErrorAction SilentlyContinue
            if ($existingDesktop) {
                throw "Refusing to start installed desktop E2E while another brosdk-desktop process is running"
            }
        }

        if (-not $FirstRunOnly) {
            $configuredDataDir = $null
            $configPath = Join-Path $env:LOCALAPPDATA "BroSDK Dashboard\config\data-dir.json"
            if (Test-Path -LiteralPath $configPath) {
                $configuredDataDir = (Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json).dataDir
            }
            $sourceDataDir = if ([string]::IsNullOrWhiteSpace($configuredDataDir)) {
                Join-Path $env:LOCALAPPDATA "BroSDK Dashboard"
            }
            else {
                [IO.Path]::GetFullPath($configuredDataDir)
            }
            $targetSecrets = Join-Path $resolvedDataDir "secrets"
            New-Item -ItemType Directory -Path $targetSecrets -Force | Out-Null
            foreach ($secretName in @("sdk-api-key.bin", "ai-api-key.bin")) {
                $sourceSecret = Join-Path (Join-Path $sourceDataDir "secrets") $secretName
                if (Test-Path -LiteralPath $sourceSecret) {
                    Copy-Item -LiteralPath $sourceSecret -Destination (Join-Path $targetSecrets $secretName)
                }
            }
        }

        $env:BROSDK_DATA_DIR = $resolvedDataDir
        if ([string]::IsNullOrWhiteSpace($env:BROSDK_AI_BASE_URL)) {
            $env:BROSDK_AI_BASE_URL = "https://api.deepseek.com"
        }
        if ([string]::IsNullOrWhiteSpace($env:BROSDK_AI_MODEL)) {
            $env:BROSDK_AI_MODEL = "deepseek-v4-flash"
        }
        if ([string]::IsNullOrWhiteSpace($env:BROSDK_WORK_DIR)) {
            $env:BROSDK_WORK_DIR = Join-Path $repoRoot "runtime\sdk-work"
        }
        $desktopProcess = Start-Process -FilePath $DesktopExecutable -PassThru
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

    if ($FirstRunOnly) {
        if ($startupState.Mode -ne "first-run") {
            throw "Expected installed Dashboard to show first-run API Key initialization"
        }
        [ordered]@{
            status = "passed"
            desktopLaunchedByTest = $ownsDesktop
            firstRunInitializationVisible = $true
        } | ConvertTo-Json
        return
    }

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

    $settingsButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $settingsLabel -Enabled
    } "settings navigation before SDK self-check"
    Invoke-DashboardButton $settingsButton
    $selfCheckButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $runSdkSelfCheckLabel -Enabled
    } "enabled SDK self-check button"
    Invoke-DashboardButton $selfCheckButton
    Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        $report = @($elements) | Where-Object {
            $_.Current.Name -eq $recentSelfCheckLabel
        } | Select-Object -First 1
        $enabledButton = Find-DashboardButton $window $runSdkSelfCheckLabel -Enabled
        if ($report -and $enabledButton) { return $true }
        return $false
    } "completed SDK self-check" | Out-Null
    $sdkSelfCheckObserved = $true

    $environmentButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $environmentLabel -Enabled
    } "environment navigation"
    Invoke-DashboardButton $environmentButton

    $targetStartPattern = if ([string]::IsNullOrWhiteSpace($TargetEnvironmentId)) {
        $startPattern
    }
    else {
        "$startPattern($TargetEnvironmentId)"
    }
    $targetStopPattern = if ([string]::IsNullOrWhiteSpace($TargetEnvironmentId)) {
        $stopPattern
    }
    else {
        "$stopPattern($TargetEnvironmentId)"
    }

    $baselineControl = Wait-ForDashboardValue {
        $start = Find-DashboardButton $window $targetStartPattern -Like -Enabled
        if ($start) {
            return [pscustomobject]@{ Mode = "stopped"; Button = $start }
        }
        $stop = Find-ReadyEnvironmentStopButton $window $targetStopPattern
        if ($stop) {
            return [pscustomobject]@{ Mode = "ready"; Button = $stop }
        }
        return $false
    } "stable target environment control"

    if ($baselineControl.Mode -eq "ready") {
        Invoke-DashboardButton $baselineControl.Button
        $startButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $targetStartPattern -Like -Enabled
        } "stopped baseline environment"
    }
    else {
        $startButton = $baselineControl.Button
    }
    if ($startButton.Current.Name -match "\(([^)]+)\)$") {
        $targetEnvId = $Matches[1]
    }
    if ([string]::IsNullOrWhiteSpace($targetEnvId)) {
        throw "Could not extract target envId from the environment control"
    }
    if (-not [string]::IsNullOrWhiteSpace($TargetEnvironmentId) -and $targetEnvId -ne $TargetEnvironmentId) {
        throw "Resolved environment $targetEnvId does not match requested target"
    }
    $targetStartPattern = "$startPattern($targetEnvId)"
    $targetStopPattern = "$stopPattern($targetEnvId)"

    if ($AgentStatusQuery) {
        $aiButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $aiLabel -Enabled
        } "AI navigation for stopped-status query"
        Invoke-DashboardButton $aiButton

        New-DashboardConversation $window
        $agentModeButton = Wait-ForDashboardValue {
            Find-DashboardButton $window "Agent" -Enabled
        } "Agent mode for stopped-status query"
        Invoke-DashboardButton $agentModeButton
        $automaticExecutionButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $automaticExecutionLabel -Enabled
        } "automatic Agent execution mode"
        Invoke-DashboardButton $automaticExecutionButton
        $agentRequest = Wait-ForDashboardValue {
            Find-DashboardEdit $window $aiRequestLabel -Enabled
        } "Agent stopped-status request input"
        $statusPrompt = (-join ([char[]](0x53EA, 0x67E5, 0x8BE2, 0x73AF, 0x5883))) +
            " $targetEnvId " +
            (-join ([char[]](0x662F, 0x5426, 0x5DF2, 0x7ECF, 0x542F, 0x52A8, 0xFF0C,
                0x4E0D, 0x6267, 0x884C, 0x542F, 0x52A8, 0x3001, 0x505C, 0x6B62, 0x6216,
                0x5176, 0x5B83, 0x5199, 0x64CD, 0x4F5C, 0x3002, 0x8BF7, 0x6839, 0x636E,
                0x5F53, 0x524D, 0x5B9E, 0x65F6, 0x72B6, 0x6001, 0x56DE, 0x7B54, 0x3002)))
        $agentRequest.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($statusPrompt)
        $runAgentButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $runAgentLabel -Enabled
        } "enabled automatic Agent button"
        Invoke-DashboardButton $runAgentButton

        Wait-ForDashboardValue {
            $elements = Get-DashboardElements $window
            $errorReply = @($elements) | Where-Object {
                $_.Current.Name -eq $aiErrorLabel
            } | Select-Object -Last 1
            if ($errorReply) {
                $errorText = @($errorReply.FindAll(
                    [System.Windows.Automation.TreeScope]::Descendants,
                    [System.Windows.Automation.Condition]::TrueCondition
                )) | ForEach-Object { $_.Current.Name } | Where-Object { $_ }
                throw "Agent stopped-status query returned an AI error: $($errorText -join ' ')"
            }
            $reply = @($elements) | Where-Object {
                $_.Current.Name -eq $aiReplyLabel
            } | Select-Object -Last 1
            if (-not $reply) {
                return $false
            }
            $replyText = (@($reply.FindAll(
                [System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.Condition]::TrueCondition
            )) | ForEach-Object { $_.Current.Name } | Where-Object { $_ }) -join " "
            $normalizedReply = $replyText.ToLowerInvariant()
            $claimsReady = @(
                "ready",
                (-join ([char[]](0x5DF2, 0x7ECF, 0x542F, 0x52A8))),
                (-join ([char[]](0x5DF2, 0x542F, 0x52A8, 0x6210, 0x529F)))
            ) |
                Where-Object { $normalizedReply.Contains($_) } |
                Select-Object -First 1
            $negatedRunning = @(
                ((-join ([char[]](0x4E0D, 0x5728))) + $runningLabel),
                (-join ([char[]](0x672A, 0x8FD0, 0x884C))),
                (-join ([char[]](0x672A, 0x542F, 0x52A8)))
            ) |
                Where-Object { $normalizedReply.Contains($_) } |
                Select-Object -First 1
            if (-not $claimsReady -and $normalizedReply.Contains($runningLabel) -and -not $negatedRunning) {
                $claimsReady = $runningLabel
            }
            if ($claimsReady) {
                throw "Agent incorrectly reported a stopped environment as running: $replyText"
            }
            $reportsStopped = @(
                "stopped",
                (-join ([char[]](0x672A, 0x542F, 0x52A8))),
                (-join ([char[]](0x6CA1, 0x6709, 0x542F, 0x52A8))),
                (-join ([char[]](0x672A, 0x8FD0, 0x884C))),
                (-join ([char[]](0x5DF2, 0x505C, 0x6B62)))
            ) |
                Where-Object { $normalizedReply.Contains($_) } |
                Select-Object -First 1
            if ($reportsStopped) {
                return $true
            }
            return $false
        } "automatic Agent stopped-status reply" | Out-Null
        $agentStoppedStatusObserved = $true

        $environmentButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $environmentLabel -Enabled
        } "environment navigation after stopped-status query"
        Invoke-DashboardButton $environmentButton
        Wait-ForDashboardValue {
            Find-DashboardButton $window $targetStartPattern -Like -Enabled
        } "environment left stopped by Agent status query" | Out-Null
        $agentStatusLeftEnvironmentStopped = $true
    }

    $startButton = Wait-ForDashboardValue {
        Find-DashboardButton $window $targetStartPattern -Like -Enabled
    } "enabled environment start button after AI status verification"

    if ($AgentLifecycle) {
        $aiButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $aiLabel -Enabled
        } "AI navigation for Agent lifecycle"
        Invoke-DashboardButton $aiButton

        New-DashboardConversation $window $targetEnvId
        $environmentConversationCreated = $true
        $agentModeButton = Wait-ForDashboardValue {
            Find-DashboardButton $window "Agent" -Enabled
        } "Agent mode"
        Invoke-DashboardButton $agentModeButton
        $manualExecutionButton = Wait-ForDashboardValue {
            Find-DashboardButton $window (-join ([char[]](0x6BCF, 0x6B21, 0x6279, 0x51C6))) -Enabled
        } "manual Agent execution mode"
        Invoke-DashboardButton $manualExecutionButton
        $agentRequest = Wait-ForDashboardValue {
            Find-DashboardEdit $window $aiRequestLabel -Enabled
        } "Agent request input"
        $startEnvironmentPrompt = (-join ([char[]](0x542F, 0x52A8, 0x73AF, 0x5883))) + " " + $targetEnvId
        $agentRequest.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($startEnvironmentPrompt)
        $generatePlanButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $generatePlanLabel -Enabled
        } "enabled Agent plan button"
        Invoke-DashboardButton $generatePlanButton

        $approvePlanButton = Wait-ForDashboardValue {
            $approve = Find-DashboardButton $window $approvePlanLabel -Enabled
            $elements = Get-DashboardElements $window
            $stoppedPrecondition = @($elements) | Where-Object {
                $_.Current.Name -eq "stopped"
            } | Select-Object -First 1
            if ($approve -and $stoppedPrecondition) { return $approve }
            return $null
        } "Agent plan with stopped precondition"
        $agentPlanObserved = $true
        Invoke-DashboardButton $approvePlanButton
        $agentApprovalInvoked = $true
        $startInvoked = $true
        Wait-ForDashboardValue {
            $elements = Get-DashboardElements $window
            @($elements) | Where-Object {
                $_.Current.Name -like "Operation *"
            } | Select-Object -First 1
        } "Agent operation result" | Out-Null
        $agentOperationObserved = $true

        $environmentButton = Wait-ForDashboardValue {
            Find-DashboardButton $window $environmentLabel -Enabled
        } "environment navigation after Agent approval"
        Invoke-DashboardButton $environmentButton
    }
    else {
        Invoke-DashboardButton $startButton
        $startInvoked = $true
    }

    $stopButton = Wait-ForDashboardValue {
        Find-ReadyEnvironmentStopButton $window $targetStopPattern
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
    if (-not $environmentConversationCreated) {
        New-DashboardConversation $window $targetEnvId
        $environmentConversationCreated = $true
    }
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

    New-DashboardConversation $window
    $chatModeButton = Wait-ForDashboardValue {
        Find-DashboardButton $window "Chat" -Enabled
    } "Chat mode"
    Invoke-DashboardButton $chatModeButton
    $chatRequest = Wait-ForDashboardValue {
        Find-DashboardEdit $window $aiRequestLabel -Enabled
    } "Chat request input"
    $chatMutationPrompt = (-join ([char[]](0x542F, 0x52A8, 0x73AF, 0x5883))) + " " + $targetEnvId
    $chatRequest.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($chatMutationPrompt)
    $chatRequest.SetFocus()
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Wait-ForDashboardValue {
        $elements = Get-DashboardElements $window
        $reply = @($elements) | Where-Object {
            $_.Current.Name -eq $aiReplyLabel
        } | Select-Object -First 1
        $agentGuidance = @($elements) | Where-Object {
            $_.Current.Name -like "*Agent*" -and $_.Current.Name -like "*Chat*"
        } | Select-Object -First 1
        if ($reply -and $agentGuidance) { return $true }
        return $false
    } "Chat Enter submission and visible read-only reply" | Out-Null
    $chatEnterReplyObserved = $true

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
        Find-DashboardButton $window $targetStopPattern -Like -Enabled
    } "ready environment stop button after AI verification"
    Invoke-DashboardButton $stopButton
    $stopInvoked = $true

    Wait-ForDashboardValue {
        Find-DashboardButton $window $targetStartPattern -Like -Enabled
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
        sdkSelfCheckObserved = $sdkSelfCheckObserved
        agentLifecycleRequested = [bool]$AgentLifecycle
        agentPlanObserved = $agentPlanObserved
        agentApprovalInvoked = $agentApprovalInvoked
        agentOperationObserved = $agentOperationObserved
        chatEnterReplyObserved = $chatEnterReplyObserved
        agentStatusQueryRequested = [bool]$AgentStatusQuery
        agentStoppedStatusObserved = $agentStoppedStatusObserved
        agentStatusLeftEnvironmentStopped = $agentStatusLeftEnvironmentStopped
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
            $cleanupPattern = if ([string]::IsNullOrWhiteSpace($targetEnvId)) { $stopPattern } else { "$stopPattern($targetEnvId)" }
            $cleanupStop = Find-DashboardButton $window $cleanupPattern -Like -Enabled
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
    if ($null -eq $originalAiBaseUrl) {
        Remove-Item Env:BROSDK_AI_BASE_URL -ErrorAction SilentlyContinue
    }
    else {
        $env:BROSDK_AI_BASE_URL = $originalAiBaseUrl
    }
    if ($null -eq $originalAiModel) {
        Remove-Item Env:BROSDK_AI_MODEL -ErrorAction SilentlyContinue
    }
    else {
        $env:BROSDK_AI_MODEL = $originalAiModel
    }
    if ($ownsDesktop -and (Test-Path -LiteralPath $resolvedDataDir)) {
        Remove-Item -LiteralPath $resolvedDataDir -Recurse -Force
    }
}
