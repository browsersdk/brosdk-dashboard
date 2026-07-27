import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../../types";
import { AiPage } from "./AiPage";

const api = vi.hoisted(() => ({
  chat: vi.fn(),
  plan: vi.fn(),
  execute: vi.fn(),
  run: vi.fn(),
}));

vi.mock("../../api", () => ({
  aiChat: api.chat,
  aiPlanAgent: api.plan,
  aiExecuteAgent: api.execute,
  aiRunAgent: api.run,
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
    envId: "2044366881367789568",
    name: "共享环境",
    status: "stopped",
    cdp: "-",
    lastEvent: "browser-close-success",
    generation: 3,
    requestId: null,
    currentOperationId: null,
    updatedAt: "2026-07-26T00:00:00Z",
  }],
} as DashboardSnapshot;

afterEach(cleanup);

beforeEach(() => {
  localStorage.clear();
  api.chat.mockReset();
  api.plan.mockReset();
  api.execute.mockReset();
  api.run.mockReset();
});

describe("AiPage", () => {
  it("fixes the MCP scope when each conversation is created", () => {
    renderPage();
    expect(screen.getByLabelText("AI 会话作用域").textContent).toContain("全局");
    expect(screen.getByLabelText("AI 会话作用域").textContent).toContain("全部环境");
    expect(screen.queryByLabelText("AI 关联环境")).toBeNull();
    expect(screen.getByLabelText("AI 会话历史")).toBeTruthy();
    expect(screen.getByRole("button", { name: "新建会话" })).toBeTruthy();
    expect((screen.getByRole("button", { name: "清空当前会话" }) as HTMLButtonElement).disabled).toBe(true);

    createEnvironmentConversation("env-1");
    expect(screen.getByLabelText("AI 会话作用域").textContent).toContain("单环境");
    expect(screen.getByLabelText("AI 会话作用域").textContent).toContain("共享环境 · env-1");
    expect(screen.queryByLabelText("新会话关联环境")).toBeNull();

    createGlobalConversation();
    expect(screen.getByLabelText("AI 会话作用域").textContent).toContain("全局");
    fireEvent.click(document.querySelectorAll<HTMLButtonElement>(".ai-conversation-select")[1]);
    expect(screen.getByLabelText("AI 会话作用域").textContent).toContain("共享环境 · env-1");
  });

  it("persists messages and sends bounded conversation history on later turns", async () => {
    api.chat
      .mockResolvedValueOnce({ answer: "第一轮回答", model: "deepseek-v4-flash", readOnly: true })
      .mockResolvedValueOnce({ answer: "第二轮回答", model: "deepseek-v4-flash", readOnly: true });
    const view = renderPage();

    submitPrompt("第一轮问题");
    await screen.findByText("第一轮回答");
    expect(api.chat).toHaveBeenNthCalledWith(1, "第一轮问题", null, []);

    submitPrompt("第二轮问题");
    await screen.findByText("第二轮回答");
    expect(api.chat).toHaveBeenNthCalledWith(2, "第二轮问题", null, [
      { role: "user", content: "第一轮问题" },
      { role: "assistant", content: "第一轮回答" },
    ]);

    view.unmount();
    renderPage();
    expect(screen.getAllByText("第一轮问题").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("第二轮回答")).toBeTruthy();
  });

  it("creates, switches, clears, and deletes local conversations", async () => {
    api.chat.mockResolvedValue({ answer: "保留的回答", model: "deepseek-v4-flash", readOnly: true });
    renderPage();
    submitPrompt("历史会话标题");
    await screen.findByText("保留的回答");

    createGlobalConversation();
    expect(screen.getByText("当前会话为空")).toBeTruthy();
    expect(screen.getAllByText("新会话").length).toBeGreaterThan(0);

    fireEvent.click(screen.getByText("历史会话标题").closest("button")!);
    expect(screen.getByText("保留的回答")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "清空当前会话" }));
    expect(screen.queryByText("保留的回答")).toBeNull();
    expect(screen.getByText("当前会话为空")).toBeTruthy();

    const deleteButtons = screen.getAllByRole("button", { name: /删除会话/ });
    fireEvent.click(deleteButtons[0]);
    expect(screen.getAllByRole("button", { name: /删除会话/ })).toHaveLength(1);
  });

  it("executes a reviewed Agent plan and keeps it in conversation history", async () => {
    const plan = {
      summary: "启动目标环境",
      action: "environment.start",
      envId: "2044366881367789568",
      expectedState: "stopped",
      idempotencyKey: "manager-key",
      arguments: {},
    };
    api.plan.mockResolvedValue(plan);
    api.execute.mockResolvedValue({
      action: "environment.start",
      operation: { id: "operation-1", status: "running" },
      response: null,
      statusSemantics: "等待 browser-open-success",
      replayed: false,
    });
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    renderPage({ onRefresh });

    fireEvent.click(screen.getByRole("button", { name: "Agent" }));
    submitPrompt("启动环境 2044366881367789568");
    await screen.findByText("启动目标环境");
    expect(api.plan).toHaveBeenCalledWith("启动环境 2044366881367789568", null, []);

    fireEvent.click(screen.getByRole("button", { name: "批准并执行" }));
    await screen.findByText("Operation operation-1");
    expect(api.execute).toHaveBeenCalledWith(plan, false);
    expect(onRefresh).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "批准并执行" })).toBeNull();
  });

  it("automatically executes Agent plans when the conversation opts in", async () => {
    const plan = {
      summary: "停止目标环境",
      action: "environment.stop",
      envId: "env-1",
      expectedState: "ready",
      idempotencyKey: "automatic-key",
      arguments: {},
    };
    api.run.mockResolvedValue({
      answer: "环境已重启并恢复运行",
      model: "deepseek-v4-flash",
      maxToolRounds: 4,
      steps: [{
        plan,
        execution: {
          action: "environment.stop",
          operation: { id: "operation-auto-stop", status: "succeeded" },
          response: null,
          statusSemantics: "已停止",
          replayed: false,
        },
      }, {
        plan: { ...plan, action: "environment.start", idempotencyKey: "automatic-start-key" },
        execution: {
          action: "environment.start",
          operation: { id: "operation-auto-start", status: "succeeded" },
          response: null,
          statusSemantics: "已运行",
          replayed: false,
        },
      }],
    });
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: "Agent" }));
    fireEvent.click(screen.getByRole("button", { name: "自动执行" }));
    submitPrompt("重启环境 env-1");

    await screen.findByText("环境已重启并恢复运行");
    expect(api.run).toHaveBeenCalledWith("重启环境 env-1", null, []);
    expect(screen.getByText("2 个步骤已执行")).toBeTruthy();
    expect(api.plan).not.toHaveBeenCalled();
    expect(api.execute).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "批准并执行" })).toBeNull();
  });

  it("submits with Enter and keeps Shift+Enter for a newline", async () => {
    api.chat.mockResolvedValue({ answer: "已回复", model: "deepseek-v4-flash", readOnly: true });
    renderPage();
    const input = screen.getByLabelText("AI 请求");

    fireEvent.change(input, { target: { value: "第一条" } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
    await screen.findByText("已回复");
    expect(api.chat).toHaveBeenCalledWith("第一条", null, []);

    fireEvent.change(input, { target: { value: "保留换行" } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter", shiftKey: true });
    expect(api.chat).toHaveBeenCalledTimes(1);
    expect((input as HTMLTextAreaElement).value).toBe("保留换行");
  });

  it("does not offer an unsafe retry after an execution attempt fails", async () => {
    api.plan.mockResolvedValue({
      summary: "启动目标环境",
      action: "environment.start",
      envId: "2044366881367789568",
      expectedState: "stopped",
      idempotencyKey: "failed-key",
      arguments: {},
    });
    api.execute.mockRejectedValue("执行状态不确定");
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: "Agent" }));
    submitPrompt("启动环境 2044366881367789568");
    await screen.findByText("启动目标环境");
    fireEvent.click(screen.getByRole("button", { name: "批准并执行" }));

    await screen.findByText("执行未完成");
    expect(screen.queryByRole("button", { name: "批准并执行" })).toBeNull();
  });

  it("shows string errors returned by Tauri instead of a generic fallback", async () => {
    api.plan.mockRejectedValue("AI agent expected environment state ready, but current state is stopped");
    const onError = vi.fn();
    renderPage({ onError });

    fireEvent.click(screen.getByRole("button", { name: "Agent" }));
    submitPrompt("启动环境");
    await screen.findByText("AI agent expected environment state ready, but current state is stopped");
    expect(onError).toHaveBeenLastCalledWith("AI agent expected environment state ready, but current state is stopped");
  });

  it("opens provider settings and exposes a real CDP address only when available", () => {
    const onOpenSettings = vi.fn();
    renderPage({ onOpenSettings });
    expect(screen.queryByRole("button", { name: "复制 CDP 地址" })).toBeNull();
    createEnvironmentConversation("env-1");
    expect(screen.getByText(snapshot.environments[0].cdp)).toBeTruthy();
    expect(screen.getByRole("button", { name: "复制 CDP 地址" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "AI Provider 设置" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
  });
});

function renderPage(overrides: Partial<Parameters<typeof AiPage>[0]> = {}) {
  return render(<AiPage
    snapshot={snapshot}
    onRefresh={vi.fn().mockResolvedValue(undefined)}
    onError={vi.fn()}
    onOpenSettings={vi.fn()}
    {...overrides}
  />);
}

function submitPrompt(prompt: string) {
  fireEvent.change(screen.getByLabelText("AI 请求"), { target: { value: prompt } });
  fireEvent.click(screen.getByRole("button", { name: /发送|生成计划|运行 Agent/ }));
}

function createGlobalConversation() {
  fireEvent.click(screen.getByRole("button", { name: "新建会话" }));
  const dialog = screen.getByRole("dialog", { name: "新建 AI 会话" });
  fireEvent.click(within(dialog).getByRole("button", { name: "创建" }));
}

function createEnvironmentConversation(envId: string) {
  fireEvent.click(screen.getByRole("button", { name: "新建会话" }));
  const dialog = screen.getByRole("dialog", { name: "新建 AI 会话" });
  fireEvent.click(within(dialog).getByRole("button", { name: "单环境" }));
  fireEvent.change(within(dialog).getByLabelText("新会话关联环境"), {
    target: { value: envId },
  });
  fireEvent.click(within(dialog).getByRole("button", { name: "创建" }));
}
