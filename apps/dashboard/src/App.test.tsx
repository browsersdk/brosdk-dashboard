import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { DashboardSnapshot, OperationRecord } from "./types";

const api = vi.hoisted(() => ({
  eventsSince: vi.fn(),
  getSnapshot: vi.fn(),
  installKernel: vi.fn(),
  isDesktopRuntime: vi.fn(),
}));

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    eventsSince: api.eventsSince,
    getSnapshot: api.getSnapshot,
    installKernel: api.installKernel,
    isDesktopRuntime: api.isDesktopRuntime,
  };
});

afterEach(cleanup);

beforeEach(() => {
  window.history.replaceState(null, "", "/?page=kernels");
  api.eventsSince.mockReset().mockResolvedValue([]);
  api.getSnapshot.mockReset();
  api.installKernel.mockReset();
  api.isDesktopRuntime.mockReset().mockReturnValue(true);
});

describe("App kernel page", () => {
  it("shows immediate feedback before the SDK install operation resolves", async () => {
    let currentSnapshot = snapshot();
    api.getSnapshot.mockImplementation(() => Promise.resolve(currentSnapshot));
    const installOperation = operation({
      id: "op-install-142",
      status: "running",
      message: "SDK 已受理安装，等待下载进度回调（3 分钟无回调将标记失败）",
    });
    const installRequest = deferred<OperationRecord>();
    api.installKernel.mockReturnValue(installRequest.promise);

    render(<App />);
    await screen.findByRole("heading", { name: "内核" });

    fireEvent.click(screen.getByRole("button", { name: "安装 Chrome" }));

    const pendingPanel = await screen.findByLabelText("内核安装进度");
    expect(pendingPanel.textContent).toContain("Chrome 142");
    expect(pendingPanel.textContent).toContain("已发送安装请求，等待 SDK 受理");
    expect(pendingPanel.textContent).toContain("排队中");
    const installButton = screen.getByRole("button", { name: "安装 Chrome" }) as HTMLButtonElement;
    expect(installButton.disabled).toBe(true);
    expect(screen.getByRole("row", { name: /Chrome.*已发送安装请求，等待 SDK 受理/ })).toBeTruthy();

    currentSnapshot = snapshot({
      operations: [operation({
        id: installOperation.id,
        status: "running",
        message: "browser-install · Downloading · 42%",
      })],
    });
    installRequest.resolve(installOperation);

    await waitFor(() => {
      expect(screen.getByLabelText("内核安装进度").textContent).toContain("browser-install · Downloading · 42%");
    });
  });

  it("filters platform matrix kernels and installs by kernel id", async () => {
    const currentSnapshot = snapshot({
      kernels: [
        kernel("chrome-146-linux-x86_64", "YunBrowser", "linux", "x86_64", 146),
        kernel("chrome-146-macos-arm64", "YunBrowser.app", "macos", "arm64", 146),
        kernel("chrome-146-windows-x86_64", "YunBrowser.exe", "windows", "x86_64", 146),
      ],
    });
    api.getSnapshot.mockResolvedValue(currentSnapshot);
    api.installKernel.mockResolvedValue(operation({
      request: {
        kernelId: "chrome-146-windows-x86_64",
        platform: "windows",
        arch: "x86_64",
        cores: [{ major: 146, type: "chrome" }],
      },
    }));

    render(<App />);
    await screen.findByRole("heading", { name: "内核" });

    expect(screen.queryByText("YunBrowser")).toBeNull();
    expect(screen.queryByText("YunBrowser.app")).toBeNull();
    const installButton = screen.getByRole("button", { name: "安装 YunBrowser.exe" });
    fireEvent.click(installButton);

    await waitFor(() => {
      expect(api.installKernel).toHaveBeenCalledWith(expect.objectContaining({
        id: "chrome-146-windows-x86_64",
        platform: "windows",
        arch: "x86_64",
      }));
    });
  });
});

function snapshot(overrides: Partial<DashboardSnapshot> = {}): DashboardSnapshot {
  const now = "2026-07-28T00:00:00.000Z";
  return {
    sdk: {
      state: "host-running",
      runtime: {
        state: "running",
        pid: 100,
        generation: 1,
        endpoint: "pipe",
        lastError: null,
      },
      initialized: true,
      apiKey: { source: "secure-store", present: true },
      hostPath: "sdk-host.exe",
      dllPath: "libs/windows_x64/brosdk.dll",
      workDir: "runtime/sdk-work",
      lastSmoke: null,
    },
    capabilities: {
      platform: "windows",
      arch: "x86_64",
      supportStatus: "available",
      unsupportedReason: null,
      cAbi: true,
      embeddedWebApi: true,
      embeddedMcp: true,
      supportsInitPort: true,
      callbacks: [],
      syncCalls: [],
      asyncCalls: [],
      cdpCalls: [],
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
      configured: true,
      active: true,
      allowedTools: [],
      managerRoute: "Manager",
      endpointHint: "127.0.0.1:39000",
      notes: [],
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
    environments: [],
    environmentCache: {
      source: "sdk-server",
      state: "fresh",
      count: 0,
      lastSuccessAt: now,
      lastAttemptAt: now,
      lastError: null,
    },
    environmentBindings: [],
    fingerprints: [],
    proxies: [],
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
      updatedAt: now,
    }],
    operations: [],
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
    ...overrides,
  };
}

function kernel(id: string, name: string, platform: string, arch: string, major: number) {
  return {
    id,
    kernelType: "chrome",
    name,
    major,
    version: null,
    latestVersion: String(major),
    platform,
    arch,
    status: "available",
    installPath: null,
    downloadAvailable: true,
    updatedAt: "2026-07-28T00:00:00.000Z",
  };
}

function operation(overrides: Partial<OperationRecord> = {}): OperationRecord {
  return {
    id: "op-install",
    kind: "kernel.install",
    envId: null,
    label: "安装或更新内核",
    status: "running",
    message: "browser-install · Downloading · 42%",
    requestId: 42,
    generation: 0,
    errorCode: null,
    request: { cores: [{ major: 142, type: "chrome" }] },
    createdAt: "2026-07-28T00:00:00.000Z",
    updatedAt: "2026-07-28T00:00:00.000Z",
    ...overrides,
  };
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  let reject: (reason?: unknown) => void = () => undefined;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}
