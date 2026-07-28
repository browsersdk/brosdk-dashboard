import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { assertEnvironmentIdentity } from "./environmentIdentity";
import type {
  AiAgentExecution,
  AiAgentRun,
  AiAgentPlan,
  AiChatResponse,
  AiConversationTurn,
  AiProviderConfigInput,
  AiProviderStatus,
  ApiKeyInitializationResult,
  DashboardSnapshot,
  EnvironmentBatchAction,
  EnvironmentBatchResult,
  EnvironmentCreateInput,
  EnvironmentMetadataUpdateInput,
  FingerprintProfile,
  FingerprintProfileInput,
  ManagerEvent,
  ManagerSettings,
  McpToolCallExecution,
  McpToolDiscovery,
  McpToolScope,
  OperationExecution,
  OperationRecord,
  ProxyParseResult,
  ProxyProfile,
  ProxyProfileInput,
  SmokeReport,
} from "./types";

export async function getSnapshot(): Promise<DashboardSnapshot> {
  const snapshot = isTauri()
    ? await invoke<DashboardSnapshot>("manager_snapshot")
    : demoSnapshot();
  return assertEnvironmentIdentity(snapshot);
}

export async function configureApiKey(apiKey: string): Promise<ApiKeyInitializationResult> {
  if (!isTauri()) {
    throw new Error("请在桌面客户端中完成初始化");
  }
  return invoke<ApiKeyInitializationResult>("manager_configure_api_key", { apiKey });
}

export async function clearApiKey(): Promise<void> {
  return invoke("manager_clear_api_key");
}

export async function configureAiProvider(input: AiProviderConfigInput): Promise<AiProviderStatus> {
  return invoke<AiProviderStatus>("manager_configure_ai_provider", { input });
}

export async function clearAiApiKey(): Promise<AiProviderStatus> {
  return invoke<AiProviderStatus>("manager_clear_ai_api_key");
}

export async function runSmoke(): Promise<SmokeReport> {
  if (!isTauri()) {
    throw new Error("请在 Tauri 桌面窗口中运行 SDK smoke");
  }
  return invoke<SmokeReport>("run_sdk_smoke");
}

export async function syncEnvironments() {
  return invoke("manager_sync_environments");
}

export async function createEnvironment(input: EnvironmentCreateInput): Promise<OperationRecord> {
  return invoke<OperationRecord>("manager_create_environment", { input });
}

export async function reconcileRuntimes() {
  return invoke("manager_reconcile_runtimes");
}

export async function startEnvironment(envId: string) {
  return invoke("manager_start_environment", { envId });
}

export async function stopEnvironment(envId: string) {
  return invoke("manager_stop_environment", { envId });
}

export async function batchEnvironmentAction(
  action: EnvironmentBatchAction,
  envIds: string[],
): Promise<EnvironmentBatchResult> {
  return invoke<EnvironmentBatchResult>("manager_batch_environment_action", {
    input: { action, envIds },
  });
}

export async function updateEnvironmentMetadata(
  input: EnvironmentMetadataUpdateInput,
): Promise<OperationRecord> {
  return invoke<OperationRecord>("manager_update_environment_metadata", { input });
}

export async function destroyEnvironment(envId: string): Promise<OperationRecord> {
  return invoke<OperationRecord>("manager_destroy_environment", { envId });
}

export async function cleanupEnvironmentLocalData(envId: string): Promise<OperationExecution> {
  return invoke<OperationExecution>("manager_cleanup_environment_local_data", { envId });
}

export async function captureEnvironmentDiagnostic(envId: string): Promise<OperationExecution> {
  return invoke<OperationExecution>("manager_capture_environment_diagnostic", { envId });
}

export function isDesktopRuntime() {
  return isTauri();
}

export async function eventsSince(sequence: number): Promise<ManagerEvent[]> {
  return invoke<ManagerEvent[]>("manager_events_since", { sequence });
}

export function onManagerEvent(
  callback: (event: ManagerEvent) => void,
): Promise<UnlistenFn> {
  return listen<ManagerEvent>("manager-event", (event) => callback(event.payload));
}

export async function updateSettings(settings: ManagerSettings): Promise<void> {
  return invoke("manager_update_settings", { settings });
}

export async function refreshEnvironmentDetails() {
  return invoke("manager_refresh_environment_details");
}

export async function refreshEnvironmentDetail(envId: string): Promise<OperationRecord> {
  return invoke<OperationRecord>("manager_refresh_environment_detail", { envId });
}

export async function parseProxyUrl(url: string): Promise<ProxyParseResult> {
  return invoke("manager_parse_proxy_url", { url });
}

export async function saveProxyProfile(input: ProxyProfileInput): Promise<ProxyProfile> {
  return invoke("manager_save_proxy_profile", { input });
}

export async function deleteProxyProfile(profileId: string): Promise<void> {
  return invoke("manager_delete_proxy_profile", { profileId });
}

export async function diagnoseProxy(profileId: string | null, url: string): Promise<OperationExecution> {
  return invoke("manager_diagnose_proxy", { profileId, url });
}

export async function systemProxyDiagnostics(): Promise<OperationExecution> {
  return invoke("manager_system_proxy_diagnostics");
}

export async function saveFingerprintProfile(input: FingerprintProfileInput): Promise<FingerprintProfile> {
  return invoke("manager_save_fingerprint_profile", { input });
}

export async function importFingerprintProfile(path: string): Promise<FingerprintProfile> {
  return invoke("manager_import_fingerprint_profile", { path });
}

export async function exportFingerprintProfile(profileId: string, path: string): Promise<void> {
  return invoke("manager_export_fingerprint_profile", { profileId, path });
}

export async function deleteFingerprintProfile(profileId: string): Promise<void> {
  return invoke("manager_delete_fingerprint_profile", { profileId });
}

export async function openFingerprintCheck(envId: string): Promise<OperationExecution> {
  return invoke("manager_open_fingerprint_check", { envId });
}

export async function refreshKernels() {
  return invoke("manager_refresh_kernels");
}

export async function installKernel(major: number, kernelType: string | null) {
  return invoke("manager_install_kernel", { input: { major, kernelType } });
}

export async function cleanupKernelCache(major: number | null): Promise<OperationExecution> {
  return invoke("manager_cleanup_kernel_cache", { major });
}

export async function uninstallKernel(kernelId: string) {
  return invoke("manager_uninstall_kernel", { kernelId });
}

export async function cancelOperation(operationId: string) {
  return invoke("manager_cancel_operation", { operationId });
}

export async function retryOperation(operationId: string) {
  return invoke("manager_retry_operation", { operationId });
}

export async function createDiagnosticBundle(outputPath: string) {
  return invoke("manager_create_diagnostic_bundle", { outputPath });
}

export async function aiChat(
  prompt: string,
  contextEnvId: string | null,
  history: AiConversationTurn[] = [],
): Promise<AiChatResponse> {
  return invoke<AiChatResponse>("manager_ai_chat", { request: { prompt, contextEnvId, history } });
}

export async function aiPlanAgent(
  prompt: string,
  contextEnvId: string | null,
  history: AiConversationTurn[] = [],
): Promise<AiAgentPlan> {
  return invoke<AiAgentPlan>("manager_ai_plan_agent", { request: { prompt, contextEnvId, history } });
}

export async function aiExecuteAgent(
  plan: AiAgentPlan,
  automatic = false,
): Promise<AiAgentExecution> {
  return invoke<AiAgentExecution>("manager_ai_execute_agent", {
    request: { plan, approved: true, automatic },
  });
}

export async function aiRunAgent(
  prompt: string,
  contextEnvId: string | null,
  history: AiConversationTurn[] = [],
): Promise<AiAgentRun> {
  return invoke<AiAgentRun>("manager_ai_run_agent", {
    request: { prompt, contextEnvId, history, approved: true },
  });
}

export async function callEmbeddedMcp(
  scope: McpToolScope,
  envId: string | null,
  tool: string,
  arguments_: Record<string, unknown>,
): Promise<McpToolCallExecution> {
  return invoke<McpToolCallExecution>("manager_call_embedded_mcp", {
    request: { scope, envId, tool, arguments: arguments_ },
  });
}

export async function discoverEmbeddedMcpTools(
  scope: McpToolScope,
  envId: string | null,
): Promise<McpToolDiscovery> {
  return invoke<McpToolDiscovery>("manager_discover_embedded_mcp_tools", {
    request: { scope, envId },
  });
}

export async function pickDirectory(defaultPath?: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selection = await open({ directory: true, multiple: false, defaultPath });
  return typeof selection === "string" ? selection : null;
}

export async function pickJsonFile(): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selection = await open({
    multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  return typeof selection === "string" ? selection : null;
}

export async function saveFile(defaultPath: string, extension: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath, filters: [{ name: extension.toUpperCase(), extensions: [extension] }] });
}

function isTauri() {
  return "__TAURI_INTERNALS__" in window;
}

function demoSnapshot(): DashboardSnapshot {
  const search = new URLSearchParams(window.location.search);
  const workspacePreview = search.get("preview") === "workspace";
  const duplicateNames = search.get("scenario") === "duplicate-names";
  const startingProgress = search.get("scenario") === "starting-progress";
  const firstEnvironmentName = duplicateNames ? "共享工作环境" : "Marketing CN";
  const secondEnvironmentName = duplicateNames ? "共享工作环境" : "Operations JP";
  const firstEnvironmentStatus = duplicateNames ? "ready" : startingProgress ? "starting" : "stopped";
  const secondEnvironmentStatus = duplicateNames ? "ready" : "stopped";
  const previewNow = new Date().toISOString();
  const previewOperations = duplicateNames ? [
    {
      id: "op-preview-start-01",
      kind: "environment.start",
      envId: "env-demo-01",
      label: `启动${firstEnvironmentName}`,
      status: "failed",
      message: "callback timeout",
      requestId: null,
      generation: 2,
      errorCode: "SDK_TIMEOUT",
      request: { envId: "env-demo-01" },
      createdAt: previewNow,
      updatedAt: previewNow,
    },
    {
      id: "op-preview-refresh-02",
      kind: "environment.refresh-detail",
      envId: "env-demo-02",
      label: `刷新${secondEnvironmentName}指纹`,
      status: "failed",
      message: "详情读取失败",
      requestId: null,
      generation: 0,
      errorCode: "SDK_ERROR",
      request: { envId: "env-demo-02" },
      createdAt: previewNow,
      updatedAt: previewNow,
    },
    {
      id: "op-preview-stop-02",
      kind: "environment.stop",
      envId: "env-demo-02",
      label: `停止${secondEnvironmentName}`,
      status: "queued",
      message: "等待 SDK 执行",
      requestId: null,
      generation: 1,
      errorCode: null,
      request: { envId: "env-demo-02" },
      createdAt: previewNow,
      updatedAt: previewNow,
    },
    {
      id: "op-preview-sync",
      kind: "environment.sync",
      envId: null,
      label: "同步远端环境",
      status: "succeeded",
      message: "synced 2 environments",
      requestId: null,
      generation: 0,
      errorCode: null,
      request: null,
      createdAt: previewNow,
      updatedAt: previewNow,
    },
  ] : [
    {
      id: "op-demo",
      kind: "environment.sync",
      envId: null,
      label: "同步远端环境",
      status: "succeeded",
      message: "synced 1 environments",
      requestId: null,
      generation: 0,
      errorCode: null,
      request: null,
      createdAt: previewNow,
      updatedAt: previewNow,
    },
  ];
  return {
    sdk: {
      state: "browser-preview",
      runtime: {
        state: "stopped",
        pid: null,
        generation: 0,
        endpoint: null,
        lastError: null,
      },
      initialized: workspacePreview,
      apiKey: { source: workspacePreview ? "preview" : "none", present: workspacePreview },
      hostPath: null,
      dllPath: "libs/windows_x64/brosdk.dll",
      workDir: "runtime/sdk-work",
      lastSmoke: null,
    },
    capabilities: {
      platform: "windows",
      supportStatus: "available",
      unsupportedReason: null,
      cAbi: true,
      embeddedWebApi: true,
      embeddedMcp: true,
      supportsInitPort: true,
      callbacks: ["result", "log"],
      syncCalls: ["sdk_get_user_sig", "sdk_init", "sdk_info", "sdk_env_create", "sdk_env_update", "sdk_env_destroy", "sdk_env_page", "sdk_env_getinfo"],
      asyncCalls: ["sdk_browser_open", "sdk_browser_close"],
      cdpCalls: ["sdk_browser_command", "sdk_browser_snapshot"],
      dllPath: "libs/windows_x64/brosdk.dll",
      dllExists: true,
      libraryDir: "windows_x64",
      libraryFilename: "brosdk.dll",
      secretBackend: "windows-dpapi",
      ipcTransport: "named-pipe",
    },
    mcp: {
      mode: "manager-routed",
      embeddedAvailable: true,
      configured: false,
      active: false,
      allowedTools: [
        "global:sdk.health",
        "global:sdk.info",
        "global:env.list",
        "global:env.resolve",
        "global:env.get",
        "global:browser.status",
        "global:task.list",
        "global:task.get",
        "global:mcp.endpoint",
        "environment:env.browser_state",
        "environment:env.tabs",
        "environment:env.bookmarks",
        "environment:env.history",
        "environment:env.tab_groups",
        "environment:env.navigate",
        "environment:env.snapshot",
        "environment:env.diff",
        "environment:env.act",
        "environment:env.download",
        "environment:env.upload",
        "environment:env.read",
        "environment:env.grep",
        "environment:env.screenshot",
        "environment:env.pdf",
        "environment:env.wait",
        "environment:env.windows",
        "environment:env.evaluate",
      ],
      managerRoute: "Manager routes envId and operation state; DLL embedded MCP remains a capability.",
      endpointHint: "not enabled",
      notes: ["Browser preview mode"],
    },
    ai: {
      provider: "openai-compatible",
      baseUrl: "https://api.deepseek.com",
      model: "deepseek-v4-flash",
      apiKeyPresent: false,
      apiKeySource: "none",
      baseUrlSource: "default",
      modelSource: "default",
    },
    environments: [
      {
        envId: "env-demo-01",
        name: firstEnvironmentName,
        status: firstEnvironmentStatus,
        cdp: duplicateNames ? "ws://127.0.0.1/preview/env-demo-01" : "-",
        lastEvent: startingProgress ? "browser-open · Downloading · 37%" : "browser-close-success",
        generation: 0,
        requestId: null,
        currentOperationId: null,
        updatedAt: new Date(0).toISOString(),
      },
      {
        envId: "env-demo-02",
        name: secondEnvironmentName,
        status: secondEnvironmentStatus,
        cdp: duplicateNames ? "ws://127.0.0.1/preview/env-demo-02" : "-",
        lastEvent: "env_page sync",
        generation: 0,
        requestId: null,
        currentOperationId: null,
        updatedAt: new Date(0).toISOString(),
      },
    ],
    environmentCache: {
      source: "sdk-server",
      state: "fresh",
      count: 2,
      lastSuccessAt: new Date().toISOString(),
      lastAttemptAt: new Date().toISOString(),
      lastError: null,
    },
    environmentBindings: [{
      envId: "env-demo-01",
      fingerprintProfileId: "fp-demo",
      proxyProfileId: "proxy-demo",
      remoteFingerprint: {
        system: "All Windows",
        platform: "Win32",
        ua: "Mozilla/5.0 Chrome/141.0.0.0",
        language: ["zh-CN", "zh"],
        zone: "Asia/Shanghai",
        dpi: "1920x1080",
        canvas: 1,
        webGl: 1,
        webRTC: 1,
        cpu: 8,
        mem: 8,
      },
      remoteProxy: {
        source: "proxy",
        scheme: "socks5",
        host: "127.0.0.1",
        port: 1080,
        username: "demo",
        passwordPresent: true,
        displayUrl: "socks5://demo:***@127.0.0.1:1080",
      },
      remoteKernel: { kernel: "yun", version: "141", system: "All Windows" },
      remoteMetadata: { envName: firstEnvironmentName, serial: "DEMO-01", enableDevtools: 1 },
      refreshedAt: new Date().toISOString(),
    }, {
      envId: "env-demo-02",
      fingerprintProfileId: null,
      proxyProfileId: null,
      remoteFingerprint: {
        system: "All Windows",
        platform: "Win32",
        ua: "Mozilla/5.0 Chrome/141.0.0.0",
        language: ["ja-JP", "ja"],
        zone: "Asia/Tokyo",
        dpi: "1440x900",
        canvas: 2,
        webGl: 1,
        webRTC: 1,
        cpu: 8,
        mem: 16,
      },
      remoteProxy: null,
      remoteKernel: { kernel: "yun", version: "141", system: "All Windows" },
      remoteMetadata: { envName: secondEnvironmentName, serial: "JP-02", enableDevtools: 1 },
      refreshedAt: new Date().toISOString(),
    }],
    fingerprints: [{
      id: "fp-demo",
      name: "Windows 中文办公",
      source: "local",
      profile: { platform: "Win32", language: "zh-CN", timezone: "Asia/Shanghai", screen: "1920x1080" },
      boundEnvIds: ["env-demo-01"],
      updatedAt: new Date().toISOString(),
    }],
    proxies: [{
      id: "proxy-demo",
      name: "本地 SOCKS5",
      scheme: "socks5",
      host: "127.0.0.1",
      port: 1080,
      username: "demo",
      passwordPresent: true,
      boundEnvIds: ["env-demo-01"],
      updatedAt: new Date().toISOString(),
    }],
    kernels: [{
      id: "chrome-142-windows-x86_64",
      kernelType: "chrome",
      name: "Chrome",
      major: 142,
      version: null,
      latestVersion: "142001",
      platform: "windows",
      arch: "x86_64",
      status: "available",
      installPath: null,
      downloadAvailable: true,
      updatedAt: previewNow,
    }, {
      id: "chrome-141-windows-x86_64",
      kernelType: "chrome",
      name: "Chrome",
      major: 141,
      version: "141.0.7390.0",
      latestVersion: "142001",
      platform: "windows",
      arch: "x86_64",
      status: "update-available",
      installPath: "runtime/sdk-work/cores/chrome-141-windows-x86_64",
      downloadAvailable: true,
      updatedAt: previewNow,
    }, {
      id: "firefox-140-windows-x86_64",
      kernelType: "firefox",
      name: "Firefox",
      major: 140,
      version: null,
      latestVersion: "140012",
      platform: "windows",
      arch: "x86_64",
      status: "available",
      installPath: null,
      downloadAvailable: true,
      updatedAt: previewNow,
    }, {
      id: "chromium-139-windows-x86_64",
      kernelType: "chromium",
      name: "Chromium",
      major: 139,
      version: null,
      latestVersion: "139009",
      platform: "windows",
      arch: "x86_64",
      status: "available",
      installPath: null,
      downloadAvailable: true,
      updatedAt: previewNow,
    }, {
      id: "broium-138-windows-x86_64",
      kernelType: "broium",
      name: "Broium",
      major: 138,
      version: null,
      latestVersion: null,
      platform: "windows",
      arch: "x86_64",
      status: "available",
      installPath: null,
      downloadAvailable: false,
      updatedAt: previewNow,
    }],
    operations: previewOperations,
    settings: {
      dataDir: "runtime/data",
      workDir: "runtime/sdk-work",
      extensionDir: "runtime/data/extensions",
      logDir: "runtime/data/logs",
      sdkApiUrl: null,
      debug: false,
      startupPolicy: "restore-none",
      embeddedMcpPort: null,
      aiBaseUrl: null,
      aiModel: null,
    },
    latestEventSequence: 0,
    databasePath: "runtime/data/manager.sqlite3",
  };
}
