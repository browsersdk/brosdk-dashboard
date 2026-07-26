import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot, McpToolCallExecution, McpToolDiscovery } from "../../types";
import { McpPage } from "./McpPage";

const api = vi.hoisted(() => ({
  call: vi.fn(),
  discover: vi.fn(),
}));

vi.mock("../../api", () => ({
  callEmbeddedMcp: api.call,
  discoverEmbeddedMcpTools: api.discover,
}));

afterEach(cleanup);

beforeEach(() => {
  api.call.mockReset();
  api.discover.mockReset();
});

const operation = {
  id: "op-mcp",
  kind: "mcp.global-tool-call",
  envId: null,
  label: "MCP",
  status: "succeeded",
  message: "completed",
  requestId: null,
  generation: 0,
  errorCode: null,
  request: null,
  createdAt: "2026-07-26T00:00:00Z",
  updatedAt: "2026-07-26T00:00:00Z",
};

const snapshot = {
  mcp: {
    mode: "manager-routed",
    embeddedAvailable: true,
    configured: true,
    active: true,
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
      "environment:browser_state",
      "environment:tabs",
      "environment:snapshot",
      "environment:diff",
      "environment:read",
      "environment:grep",
      "environment:screenshot",
    ],
  },
  environments: [{
    envId: "10001",
    name: "Primary",
    status: "ready",
    cdp: "http://127.0.0.1:9222",
    lastEvent: "ready",
    generation: 1,
    requestId: null,
    currentOperationId: null,
    updatedAt: "2026-07-26T00:00:00Z",
  }],
} as DashboardSnapshot;

function renderPage(overrides: Partial<React.ComponentProps<typeof McpPage>> = {}) {
  const props: React.ComponentProps<typeof McpPage> = {
    snapshot,
    desktop: true,
    onRefresh: vi.fn().mockResolvedValue(undefined),
    onError: vi.fn(),
    ...overrides,
  };
  render(<McpPage {...props} />);
  return props;
}

describe("McpPage", () => {
  it("switches between global and ready-environment scopes", () => {
    renderPage();
    expect(screen.queryByLabelText("运行环境")).toBeNull();
    expect((screen.getByLabelText("工具") as HTMLSelectElement).value).toBe("sdk.health");

    fireEvent.click(screen.getByRole("button", { name: "单环境" }));
    expect((screen.getByLabelText("运行环境") as HTMLSelectElement).value).toBe("10001");
    expect((screen.getByLabelText("工具") as HTMLSelectElement).value).toBe("browser_state");
  });

  it("shows the discovered Manager intersection and protects mutation tools", async () => {
    api.discover.mockResolvedValue({
      operation,
      scope: "global",
      envId: null,
      protocolVersion: "2025-11-25",
      advertisedTools: [
        { name: "sdk.health", description: null, readOnlyHint: true, destructiveHint: false },
        { name: "env.create", description: null, readOnlyHint: false, destructiveHint: false },
        { name: "env.list", description: null, readOnlyHint: true, destructiveHint: false },
      ],
      allowedTools: ["sdk.health", "env.list"],
    } satisfies McpToolDiscovery);
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: "发现工具" }));
    await waitFor(() => expect(api.discover).toHaveBeenCalledWith("global", null));
    expect(screen.getByText("env.create")).toBeTruthy();
    expect(screen.getByText("策略保护")).toBeTruthy();
    expect((screen.getByLabelText("工具") as HTMLSelectElement).options).toHaveLength(2);
  });

  it("builds a global environment detail call without a routed envId", async () => {
    api.call.mockResolvedValue(execution("global", null, "env.get"));
    renderPage();
    fireEvent.change(screen.getByLabelText("工具"), { target: { value: "env.get" } });
    fireEvent.click(screen.getByRole("button", { name: "运行工具" }));

    await waitFor(() => expect(api.call).toHaveBeenCalledWith(
      "global",
      null,
      "env.get",
      { envId: "10001" },
    ));
    expect(screen.getByText("脱敏响应")).toBeTruthy();
  });

  it("builds a bounded environment grep call from dedicated controls", async () => {
    api.call.mockResolvedValue(execution("environment", "10001", "grep"));
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: "单环境" }));
    fireEvent.change(screen.getByLabelText("工具"), { target: { value: "grep" } });
    fireEvent.change(screen.getByLabelText("Page"), { target: { value: "7" } });
    fireEvent.change(screen.getByLabelText("搜索范围"), { target: { value: "content" } });
    fireEvent.change(screen.getByLabelText("搜索文本"), { target: { value: "invoice" } });
    fireEvent.click(screen.getByRole("button", { name: "运行工具" }));

    await waitFor(() => expect(api.call).toHaveBeenCalledWith(
      "environment",
      "10001",
      "grep",
      { page: 7, pattern: "invoice", over: "content" },
    ));
  });

  it("uses runtime discovery and JSON arguments for every environment tool", async () => {
    api.discover.mockResolvedValue({
      operation,
      scope: "environment",
      envId: "10001",
      protocolVersion: "2025-11-25",
      advertisedTools: [
        { name: "browser_state", description: null, readOnlyHint: true, destructiveHint: false },
        { name: "navigate", description: null, readOnlyHint: false, destructiveHint: false },
      ],
      allowedTools: ["browser_state", "navigate"],
    } satisfies McpToolDiscovery);
    api.call.mockResolvedValue(execution("environment", "10001", "navigate"));
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: "单环境" }));
    fireEvent.click(screen.getByRole("button", { name: "发现工具" }));
    await waitFor(() => expect(api.discover).toHaveBeenCalledWith("environment", "10001"));
    fireEvent.change(screen.getByLabelText("工具"), { target: { value: "navigate" } });
    fireEvent.change(screen.getByLabelText("MCP JSON 参数"), {
      target: { value: '{"url":"https://example.com"}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "运行工具" }));

    await waitFor(() => expect(api.call).toHaveBeenCalledWith(
      "environment",
      "10001",
      "navigate",
      { url: "https://example.com" },
    ));
  });

  it("disables environment calls when no environment is ready", () => {
    renderPage({
      snapshot: {
        ...snapshot,
        environments: snapshot.environments.map((environment) => ({ ...environment, status: "stopped" })),
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "单环境" }));
    expect((screen.getByRole("button", { name: "发现工具" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "运行工具" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("distinguishes same-name environments by envId in every picker", () => {
    renderPage({
      snapshot: {
        ...snapshot,
        environments: [
          { ...snapshot.environments[0], envId: "10001", name: "共享环境" },
          { ...snapshot.environments[0], envId: "10002", name: "共享环境" },
        ],
      },
    });

    fireEvent.click(screen.getByRole("button", { name: "单环境" }));
    const environmentSelect = screen.getByLabelText("运行环境") as HTMLSelectElement;
    expect(Array.from(environmentSelect.options).map((option) => option.text)).toEqual([
      "共享环境 · 10001",
      "共享环境 · 10002",
    ]);
    fireEvent.change(environmentSelect, { target: { value: "10002" } });
    expect(environmentSelect.value).toBe("10002");
  });
});

function execution(
  scope: "global" | "environment",
  envId: string | null,
  tool: string,
): McpToolCallExecution {
  return {
    operation,
    scope,
    envId,
    tool,
    protocolVersion: "2025-11-25",
    advertisedTools: [tool],
    response: { content: [{ type: "text", text: "ok" }] },
  };
}
