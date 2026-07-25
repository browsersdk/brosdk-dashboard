import { invoke } from "@tauri-apps/api/core";
import type { DashboardSnapshot, ManagerEvent, ManagerSettings, SmokeReport } from "./types";

export async function getSnapshot(): Promise<DashboardSnapshot> {
  if (isTauri()) {
    return invoke<DashboardSnapshot>("manager_snapshot");
  }
  return demoSnapshot();
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

export async function reconcileRuntimes() {
  return invoke("manager_reconcile_runtimes");
}

export async function startEnvironment(envId: string) {
  return invoke("manager_start_environment", { envId });
}

export async function stopEnvironment(envId: string) {
  return invoke("manager_stop_environment", { envId });
}

export function isDesktopRuntime() {
  return isTauri();
}

export async function eventsSince(sequence: number): Promise<ManagerEvent[]> {
  return invoke<ManagerEvent[]>("manager_events_since", { sequence });
}

export async function updateSettings(settings: ManagerSettings): Promise<void> {
  return invoke("manager_update_settings", { settings });
}

function isTauri() {
  return "__TAURI_INTERNALS__" in window;
}

function demoSnapshot(): DashboardSnapshot {
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
      apiKey: { source: "BROSDK_API_KEY", present: false },
      hostPath: null,
      dllPath: "libs/windows_x64/brosdk.dll",
      workDir: "runtime/sdk-work",
      lastSmoke: null,
    },
    capabilities: {
      platform: "windows",
      cAbi: true,
      embeddedWebApi: true,
      embeddedMcp: true,
      supportsInitPort: true,
      callbacks: ["result", "log", "cookies-storage", "security-decision"],
      syncCalls: ["sdk_get_user_sig", "sdk_init", "sdk_info", "sdk_env_page"],
      asyncCalls: ["sdk_browser_open", "sdk_browser_close"],
      cdpCalls: ["sdk_browser_command", "sdk_browser_snapshot"],
      dllPath: "libs/windows_x64/brosdk.dll",
      dllExists: true,
    },
    mcp: {
      mode: "manager-routed",
      embeddedAvailable: true,
      managerRoute: "Manager routes envId and operation state; DLL embedded MCP remains a capability.",
      endpointHint: "not enabled",
      notes: ["Browser preview mode"],
    },
    environments: [
      {
        envId: "-",
        name: "等待 env_page 同步",
        localLabel: "",
        tags: [],
        status: "stopped",
        cdp: "-",
        lastEvent: "桌面命令不可用",
        generation: 0,
        requestId: null,
        currentOperationId: null,
        updatedAt: new Date(0).toISOString(),
      },
    ],
    operations: [],
    settings: {
      workDir: "runtime/sdk-work",
      extensionDir: "runtime/data/extensions",
      logDir: "runtime/data/logs",
      sdkApiUrl: null,
      debug: false,
    },
    latestEventSequence: 0,
    databasePath: "runtime/data/manager.sqlite3",
  };
}
