import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiKeySetup } from "./ApiKeySetup";

describe("ApiKeySetup", () => {
  afterEach(cleanup);

  it("submits a trimmed key without rendering it as plain text", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<ApiKeySetup mode="first-run" desktop source="none" busy={false} onSubmit={onSubmit} />);
    const input = screen.getByLabelText("API Key") as HTMLInputElement;
    expect(input.type).toBe("password");
    fireEvent.change(input, { target: { value: "  test-key  " } });
    fireEvent.click(screen.getByRole("button", { name: "初始化" }));
    expect(onSubmit).toHaveBeenCalledWith("test-key");
  });

  it("keeps environment-managed credentials read only", () => {
    const view = render(<ApiKeySetup mode="settings" desktop source="environment" busy={false} onSubmit={vi.fn()} onClear={vi.fn()} />);
    expect((within(view.container).getByLabelText("API Key") as HTMLInputElement).disabled).toBe(true);
    expect((within(view.container).getByRole("button", { name: "移除" }) as HTMLButtonElement).disabled).toBe(true);
    expect(view.container.textContent).toContain("由系统环境管理");
  });

  it("disables initialization in browser preview", () => {
    const view = render(<ApiKeySetup mode="first-run" desktop={false} source="none" busy={false} onSubmit={vi.fn()} />);
    fireEvent.change(within(view.container).getByLabelText("API Key"), { target: { value: "test-key" } });
    expect((within(view.container).getByRole("button", { name: "初始化" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
