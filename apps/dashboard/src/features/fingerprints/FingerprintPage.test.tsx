import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
  }, {
    envId: "env-2",
    fingerprintProfileId: null,
    proxyProfileId: null,
    remoteFingerprint: {
      ua: "Mozilla/5.0 Chrome/142",
      language: ["ja-JP", "ja"],
      zone: "Asia/Tokyo",
      canvas: 1,
    },
    remoteProxy: null,
    remoteKernel: { kernel: "yun", version: "141", system: "All Windows" },
    remoteMetadata: { serial: "" },
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
    const staleSnapshot = {
      ...snapshot,
      environmentBindings: snapshot.environmentBindings.map((binding) => binding.envId === "env-2"
        ? { ...binding, refreshedAt: null }
        : binding),
    };
    render(
      <FingerprintPage
        snapshot={staleSnapshot}
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

  it("compares selected remote summaries as same, different, or unknown", () => {
    render(<FingerprintPage snapshot={snapshot} desktop={false} onRefresh={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "对比" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "对比 东京运营 (env-2)" }));

    const kernelRow = screen.getByRole("row", { name: /内核/ });
    expect(within(kernelRow).getByText("相同")).toBeTruthy();
    const userAgentRow = screen.getByRole("row", { name: /User Agent/ });
    expect(within(userAgentRow).getByText("不同")).toBeTruthy();
    expect(within(userAgentRow).getByText("Mozilla/5.0 Chrome/141")).toBeTruthy();
    expect(within(userAgentRow).getByText("Mozilla/5.0 Chrome/142")).toBeTruthy();
    const serialRow = screen.getByRole("row", { name: /序列号/ });
    expect(within(serialRow).getByText("未知")).toBeTruthy();
  });

  it("limits comparison selection to four environments", () => {
    const environments = Array.from({ length: 5 }, (_, index) => environment(`env-${index + 1}`, `环境 ${index + 1}`, "stopped"));
    render(<FingerprintPage snapshot={{ ...snapshot, environments }} desktop={false} onRefresh={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "对比" }));
    for (const index of [2, 3, 4]) fireEvent.click(screen.getByRole("checkbox", { name: `对比 环境 ${index} (env-${index})` }));
    expect((screen.getByRole("checkbox", { name: "对比 环境 5 (env-5)" }) as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByText("4/4")).toBeTruthy();
  });

  it("refreshes each selected comparison environment before one snapshot reload", async () => {
    const refreshDetail = vi.fn(async (envId: string) => operation(envId));
    const onRefresh = vi.fn(async () => undefined);
    render(<FingerprintPage snapshot={snapshot} desktop onRefresh={onRefresh} onError={vi.fn()} onRefreshDetail={refreshDetail} />);
    fireEvent.click(screen.getByRole("button", { name: "对比" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "对比 东京运营 (env-2)" }));
    fireEvent.click(screen.getByRole("button", { name: "刷新所选" }));
    await waitFor(() => expect(refreshDetail.mock.calls.map(([envId]) => envId)).toEqual(["env-1", "env-2"]));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("keeps same-name environments distinct by envId in details and comparison", () => {
    const sameNameSnapshot = {
      ...snapshot,
      environments: snapshot.environments.map((item) => ({ ...item, name: "共享环境" })),
    };
    const { container } = render(
      <FingerprintPage snapshot={sameNameSnapshot} desktop={false} onRefresh={vi.fn()} onError={vi.fn()} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "查看 共享环境 (env-2)" }));
    expect(within(container.querySelector(".fingerprint-heading") as HTMLElement).getByText("env-2")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "对比" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "对比 共享环境 (env-1)" }));
    const columns = Array.from(container.querySelectorAll(".fingerprint-comparison th[data-env-id]"));
    expect(columns.map((column) => column.getAttribute("data-env-id"))).toEqual(["env-1", "env-2"]);
  });
});
