import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot, EnvironmentBindingSummary } from "../../types";
import { EnvironmentDetail } from "./EnvironmentDetail";

afterEach(cleanup);

const binding: EnvironmentBindingSummary = {
  envId: "env-1",
  fingerprintProfileId: null,
  proxyProfileId: null,
  remoteFingerprint: { language: ["zh-CN"], zone: "Asia/Shanghai", dpi: "1920x1080" },
  remoteProxy: { displayUrl: "socks5://alice:***@127.0.0.1:1080" },
  remoteKernel: { kernel: "yun", version: "141", system: "All Windows" },
  remoteMetadata: { serial: "CN-001" },
  refreshedAt: "2026-07-26T00:00:00Z",
};

function environment(status: string): DashboardSnapshot["environments"][number] {
  return {
    envId: "env-1",
    name: "上海办公",
    status,
    cdp: status === "ready" ? "ws://127.0.0.1/devtools/browser/1" : "-",
    lastEvent: "synced",
    generation: 1,
    requestId: null,
    currentOperationId: null,
    updatedAt: "2026-07-26T00:00:00Z",
  };
}

function renderDetail(status: string, overrides: Partial<React.ComponentProps<typeof EnvironmentDetail>> = {}) {
  const props: React.ComponentProps<typeof EnvironmentDetail> = {
    environment: environment(status),
    binding,
    busy: false,
    desktop: true,
    diagnostic: null,
    onClose: vi.fn(),
    onStart: vi.fn(),
    onStop: vi.fn(),
    onRefresh: vi.fn(),
    onOpenCheck: vi.fn(),
    onCaptureDiagnostic: vi.fn(),
    onCleanupLocalData: vi.fn(),
    onDelete: vi.fn(),
    ...overrides,
  };
  render(<EnvironmentDetail {...props} />);
  return props;
}

describe("EnvironmentDetail", () => {
  it("requires inline confirmation for destructive stopped-environment actions", () => {
    const props = renderDetail("stopped");
    expect((screen.getByRole("button", { name: "页面诊断" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "删除环境" }));
    expect(screen.getByRole("alertdialog", { name: "确认删除环境" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    expect(props.onDelete).toHaveBeenCalledOnce();
  });

  it("exposes running diagnostics but keeps cleanup and deletion disabled", () => {
    const onCaptureDiagnostic = vi.fn();
    renderDetail("ready", {
      onCaptureDiagnostic,
      diagnostic: {
        pageCount: 2,
        failedPages: 0,
        pages: [{ origin: "https://example.com" }],
      },
    });
    expect((screen.getByRole("button", { name: "清理本地数据" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "删除环境" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "页面诊断" }));
    expect(onCaptureDiagnostic).toHaveBeenCalledOnce();
    expect(screen.getByText("2 页 · 0 失败")).toBeTruthy();
    expect(screen.getByText("https://example.com")).toBeTruthy();
  });

  it("confirms local cleanup independently from server deletion", () => {
    const props = renderDetail("stopped");
    fireEvent.click(screen.getByRole("button", { name: "清理本地数据" }));
    expect(screen.getByRole("alertdialog", { name: "确认清理本地数据" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    expect(props.onCleanupLocalData).toHaveBeenCalledOnce();
    expect(props.onDelete).not.toHaveBeenCalled();
  });
});
