import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../../types";
import { AiPage } from "./AiPage";

const api = vi.hoisted(() => ({
  chat: vi.fn(),
  plan: vi.fn(),
  execute: vi.fn(),
}));

vi.mock("../../api", () => ({
  aiChat: api.chat,
  aiPlanAgent: api.plan,
  aiExecuteAgent: api.execute,
  isDesktopRuntime: () => true,
}));

const snapshot = {
  ai: {
    provider: "openai-compatible",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-v4-flash",
    apiKeyPresent: true,
    apiKeySource: "secure-storage",
    baseUrlSource: "settings",
    modelSource: "settings",
  },
  environments: [{
    envId: "env-1",
    name: "共享环境",
    status: "ready",
    cdp: "ws://127.0.0.1:9222/devtools/browser/private-one?token=secret",
    lastEvent: "browser-open-success",
    generation: 2,
    requestId: 21,
    currentOperationId: "op-1",
    updatedAt: "2026-07-26T00:00:00Z",
  }, {
    envId: "env-2",
    name: "共享环境",
    status: "ready",
    cdp: "ws://127.0.0.1:9333/devtools/browser/private-two",
    lastEvent: "browser-open-success",
    generation: 3,
    requestId: 22,
    currentOperationId: "op-2",
    updatedAt: "2026-07-26T00:00:00Z",
  }],
} as DashboardSnapshot;

afterEach(cleanup);

beforeEach(() => {
  api.chat.mockReset();
  api.plan.mockReset();
  api.execute.mockReset();
});

describe("AiPage", () => {
  it("disambiguates duplicate names by envId and shows the full local CDP", () => {
    render(<AiPage snapshot={snapshot} onRefresh={vi.fn()} onError={vi.fn()} onOpenSettings={vi.fn()} />);
    const select = screen.getByLabelText("AI 环境上下文") as HTMLSelectElement;
    expect(Array.from(select.options).map((option) => option.text)).toEqual([
      "共享环境 · env-1",
      "共享环境 · env-2",
    ]);
    expect(screen.getByText(snapshot.environments[0].cdp)).toBeTruthy();

    fireEvent.change(select, { target: { value: "env-2" } });
    expect(screen.getByText(snapshot.environments[1].cdp)).toBeTruthy();
    expect(screen.getByText("op-2")).toBeTruthy();
  });

  it("passes the selected envId to chat and opens provider settings", async () => {
    api.chat.mockResolvedValue({ answer: "ok", model: "deepseek-v4-flash", readOnly: true });
    const onOpenSettings = vi.fn();
    render(<AiPage snapshot={snapshot} onRefresh={vi.fn()} onError={vi.fn()} onOpenSettings={onOpenSettings} />);

    fireEvent.change(screen.getByLabelText("AI 环境上下文"), { target: { value: "env-2" } });
    fireEvent.change(screen.getByLabelText("AI 请求"), { target: { value: "查看当前环境" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));
    await waitFor(() => expect(api.chat).toHaveBeenCalledWith("查看当前环境", "env-2"));
    expect(screen.getByText("ok")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "AI Provider 设置" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
  });

  it("shows the DLL control channel when no TCP CDP address is exposed", () => {
    const pipeSnapshot = {
      ...snapshot,
      environments: [{ ...snapshot.environments[0], cdp: "-" }],
    } as DashboardSnapshot;
    render(<AiPage snapshot={pipeSnapshot} onRefresh={vi.fn()} onError={vi.fn()} onOpenSettings={vi.fn()} />);
    expect(screen.getByText("未暴露 TCP 地址")).toBeTruthy();
    expect(screen.getByText("DLL 内部 CDP / MCP")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "复制 CDP 地址" })).toBeNull();
  });
});
