import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../../types";
import { AiProviderSettings } from "./AiProviderSettings";

const api = vi.hoisted(() => ({
  configure: vi.fn(),
  clear: vi.fn(),
}));

vi.mock("../../api", () => ({
  configureAiProvider: api.configure,
  clearAiApiKey: api.clear,
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
  settings: {
    aiBaseUrl: "https://api.deepseek.com",
    aiModel: "deepseek-v4-flash",
  },
} as DashboardSnapshot;

afterEach(cleanup);

beforeEach(() => {
  api.configure.mockReset().mockResolvedValue(snapshot.ai);
  api.clear.mockReset().mockResolvedValue({ ...snapshot.ai, apiKeyPresent: false, apiKeySource: "none" });
});

describe("AiProviderSettings", () => {
  it("saves provider settings without reflecting the secret", async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    render(<AiProviderSettings snapshot={snapshot} onRefresh={onRefresh} onError={vi.fn()} />);
    const key = screen.getByLabelText("AI API Key") as HTMLInputElement;
    expect(key.type).toBe("password");
    fireEvent.change(key, { target: { value: "secret-test-key" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(api.configure).toHaveBeenCalledWith({
      baseUrl: "https://api.deepseek.com",
      model: "deepseek-v4-flash",
      apiKey: "secret-test-key",
    }));
    expect(key.value).toBe("");
    expect(document.body.textContent).not.toContain("secret-test-key");
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("locks environment-managed values and credentials", () => {
    render(<AiProviderSettings snapshot={{
      ...snapshot,
      ai: {
        ...snapshot.ai,
        apiKeySource: "environment",
        baseUrlSource: "environment",
        modelSource: "environment",
      },
    }} onRefresh={vi.fn()} onError={vi.fn()} />);
    expect((screen.getByLabelText("OpenAI-compatible Base URL") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText("AI Model") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText("AI API Key") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "清除 API Key" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
