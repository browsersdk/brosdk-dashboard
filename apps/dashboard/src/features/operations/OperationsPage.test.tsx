import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { OperationsPage } from "./OperationsPage";
import type { DashboardSnapshot } from "../../types";

vi.mock("../../api", () => ({
  cancelOperation: vi.fn(),
  isDesktopRuntime: () => false,
  retryOperation: vi.fn(),
}));

afterEach(cleanup);

function snapshot(): DashboardSnapshot {
  const timestamp = "2026-07-26T00:00:00.000Z";
  return {
    environments: [
      { envId: "env-01", name: "共享环境", status: "ready", cdp: "-", lastEvent: "ready", generation: 1, requestId: null, currentOperationId: null, updatedAt: timestamp },
      { envId: "env-02", name: "共享环境", status: "stopped", cdp: "-", lastEvent: "stopped", generation: 1, requestId: null, currentOperationId: null, updatedAt: timestamp },
    ],
    operations: [
      { id: "op-start", kind: "environment.start", envId: "env-01", label: "启动共享环境", status: "failed", message: "callback timeout", requestId: null, generation: 1, errorCode: "SDK_TIMEOUT", request: { envId: "env-01" }, createdAt: timestamp, updatedAt: timestamp },
      { id: "op-refresh", kind: "environment.refresh-detail", envId: "env-02", label: "刷新共享环境指纹", status: "failed", message: "详情读取失败", requestId: null, generation: 1, errorCode: "SDK_ERROR", request: { envId: "env-02" }, createdAt: timestamp, updatedAt: timestamp },
      { id: "op-stop", kind: "environment.stop", envId: "env-02", label: "停止共享环境", status: "queued", message: "等待 SDK 执行", requestId: null, generation: 1, errorCode: null, request: { envId: "env-02" }, createdAt: timestamp, updatedAt: timestamp },
      { id: "op-sync", kind: "environment.sync", envId: null, label: "同步远端环境", status: "succeeded", message: "synced", requestId: null, generation: 0, errorCode: null, request: null, createdAt: timestamp, updatedAt: timestamp },
    ],
  } as DashboardSnapshot;
}

describe("OperationsPage", () => {
  it("filters by envId while keeping duplicate environment names distinguishable", () => {
    render(<OperationsPage snapshot={snapshot()} onRefresh={vi.fn()} onError={vi.fn()} />);

    expect(screen.getByLabelText("操作摘要").textContent).toContain("显示 4/4");
    expect((screen.getByLabelText("环境筛选") as HTMLSelectElement).value).toBe("all");
    expect(screen.getByRole("option", { name: "共享环境 · env-01" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "共享环境 · env-02" })).toBeTruthy();

    fireEvent.change(screen.getByLabelText("环境筛选"), { target: { value: "env-02" } });

    expect(screen.getByLabelText("操作摘要").textContent).toContain("显示 2/4");
    expect(screen.getByRole("row", { name: /刷新共享环境指纹/ })).toBeTruthy();
    expect(screen.getByRole("row", { name: /停止共享环境/ })).toBeTruthy();
    expect(screen.queryByRole("row", { name: /启动共享环境/ })).toBeNull();
  });

  it("only exposes safe queued cancellation and supported retries", () => {
    render(<OperationsPage snapshot={snapshot()} onRefresh={vi.fn()} onError={vi.fn()} />);

    expect((screen.getByRole("button", { name: /重试 启动共享环境 env-01/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: /重试 刷新共享环境指纹 env-02/ })).toBeNull();
    expect((screen.getByRole("button", { name: /取消 停止共享环境 env-02/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: /取消 启动共享环境 env-01/ })).toBeNull();
  });

  it("shows the selected operation with the environment identity", () => {
    render(<OperationsPage snapshot={snapshot()} onRefresh={vi.fn()} onError={vi.fn()} />);

    fireEvent.click(screen.getByRole("row", { name: /启动共享环境/ }));

    expect(screen.getByRole("complementary").textContent).toContain("共享环境 · env-01");
    expect(screen.getByRole("complementary").textContent).toContain("SDK_TIMEOUT");
  });
});
