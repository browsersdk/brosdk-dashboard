import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot, OperationRecord } from "../../types";
import { fingerprintDetailGroups } from "../environments/remoteDetails";
import { FingerprintPage } from "./FingerprintPage";

afterEach(cleanup);

const now = "2026-07-26T00:00:00Z";

function environment(envId: string, name: string, status: string) {
  return {
    envId,
    name,
    status,
    cdp: status === "ready" ? "ws://127.0.0.1/devtools/browser/1" : "-",
    lastEvent: "synced",
    generation: 0,
    requestId: null,
    currentOperationId: null,
    updatedAt: now,
  };
}

function operation(envId: string): OperationRecord {
  return {
    id: "op-1",
    kind: "environment.detail.refresh",
    envId,
    label: "刷新环境详情",
    status: "succeeded",
    message: "environment detail refreshed",
    requestId: null,
    generation: 0,
    errorCode: null,
    request: { envId },
    createdAt: now,
    updatedAt: now,
  };
}

const snapshot = {
  environments: [environment("env-1", "上海办公", "stopped"), environment("env-2", "东京运营", "ready")],
  environmentBindings: [{
    envId: "env-1",
    fingerprintProfileId: null,
    proxyProfileId: null,
    remoteFingerprint: {
      ua: "Mozilla/5.0 Chrome/141",
      language: ["zh-CN", "zh"],
      zone: "Asia/Shanghai",
      canvas: 1,
    },
    remoteProxy: { displayUrl: "socks5://alice:***@127.0.0.1:1080" },
    remoteKernel: { kernel: "yun", version: "141", system: "All Windows" },
    remoteMetadata: { serial: "CN-001" },
    refreshedAt: now,
  }],
} as DashboardSnapshot;

describe("FingerprintPage", () => {
  it("shows the selected server environment fingerprint as structured fields", () => {
    render(<FingerprintPage snapshot={snapshot} desktop={false} onRefresh={vi.fn()} onError={vi.fn()} />);
    expect(screen.getByText("User Agent")).toBeTruthy();
    expect(screen.getByText("Mozilla/5.0 Chrome/141")).toBeTruthy();
    expect(screen.getByText("socks5://alice:***@127.0.0.1:1080")).toBeTruthy();
    expect((screen.getByRole("button", { name: "检查页" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByText("Profile JSON")).toBeNull();
  });

  it("refreshes only the newly selected environment and enables checks only when ready", async () => {
    const refreshDetail = vi.fn(async (envId: string) => operation(envId));
    const openCheck = vi.fn(async () => undefined);
    render(
      <FingerprintPage
        snapshot={snapshot}
        desktop
        onRefresh={vi.fn(async () => undefined)}
        onError={vi.fn()}
        onRefreshDetail={refreshDetail}
        onOpenCheck={openCheck}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /东京运营/ }));
    await waitFor(() => expect(refreshDetail).toHaveBeenCalledWith("env-2"));
    const checkButton = screen.getByRole("button", { name: "检查页" }) as HTMLButtonElement;
    await waitFor(() => expect(checkButton.disabled).toBe(false));
    fireEvent.click(checkButton);
    await waitFor(() => expect(openCheck).toHaveBeenCalledWith("env-2"));
  });

  it("keeps unknown fingerprint fields visible but filters sensitive keys", () => {
    const groups = fingerprintDetailGroups({
      customSurface: { mode: 2 },
      cookieSeed: "hidden",
      nested: { value: true },
    });
    const text = JSON.stringify(groups);
    expect(text).toContain("customSurface");
    expect(text).toContain("nested");
    expect(text).not.toContain("cookieSeed");
    expect(text).not.toContain("hidden");
  });
});
