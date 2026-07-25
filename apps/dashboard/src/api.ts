import { invoke } from "@tauri-apps/api/core";
import type { DashboardSnapshot, SmokeReport } from "./types";

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
        status: "stopped",
        cdp: "-",
        lastEvent: "桌面命令不可用",
      },
    ],
    operations: [],
  };
}
