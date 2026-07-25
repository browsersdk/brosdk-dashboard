import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { KernelRecord, ProxyProfile } from "../../types";
import { EnvironmentCreatePanel, usableEnvironmentKernels } from "./EnvironmentCreatePanel";

afterEach(cleanup);

const proxy: ProxyProfile = {
  id: "proxy-1",
  name: "Office proxy",
  scheme: "socks5",
  host: "127.0.0.1",
  port: 1080,
  username: "alice",
  passwordPresent: true,
  boundEnvIds: [],
  updatedAt: "2026-07-26T00:00:00Z",
};

function kernel(id: string, major: number, overrides: Partial<KernelRecord> = {}): KernelRecord {
  return {
    id,
    kernelType: "chrome",
    name: `Chrome ${major}`,
    major,
    version: String(major),
    latestVersion: null,
    platform: "windows",
    arch: "x86_64",
    status: "installed",
    installPath: `cores/${id}`,
    downloadAvailable: false,
    updatedAt: "2026-07-26T00:00:00Z",
    ...overrides,
  };
}

function renderPanel(overrides: Partial<React.ComponentProps<typeof EnvironmentCreatePanel>> = {}) {
  const props: React.ComponentProps<typeof EnvironmentCreatePanel> = {
    proxies: [proxy],
    kernels: [kernel("chrome-134", 134), kernel("chrome-141", 141)],
    platform: "windows",
    busy: false,
    desktop: true,
    onCancel: vi.fn(),
    onOpenKernels: vi.fn(),
    onCreate: vi.fn(),
    ...overrides,
  };
  render(<EnvironmentCreatePanel {...props} />);
  return props;
}

describe("EnvironmentCreatePanel", () => {
  it("shows only proxy and kernel business fields and selects the newest core", () => {
    renderPanel();
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    expect(screen.getByLabelText("代理")).toBeTruthy();
    expect((screen.getByLabelText("内核版本") as HTMLSelectElement).value).toBe("chrome-141");
    expect(screen.queryByText("环境名称")).toBeNull();
    expect(screen.queryByText("customerId")).toBeNull();
  });

  it("submits profile and kernel ids without proxy credentials", () => {
    const onCreate = vi.fn();
    renderPanel({ onCreate });
    fireEvent.change(screen.getByLabelText("代理"), { target: { value: "proxy-1" } });
    fireEvent.change(screen.getByLabelText("内核版本"), { target: { value: "chrome-134" } });
    fireEvent.click(screen.getByRole("button", { name: "创建环境" }));
    expect(onCreate).toHaveBeenCalledWith({ proxyProfileId: "proxy-1", kernelId: "chrome-134" });
    expect(JSON.stringify(onCreate.mock.calls)).not.toContain("alice");
    expect(JSON.stringify(onCreate.mock.calls)).not.toContain("1080");
  });

  it("routes to kernel management when no local core is usable", () => {
    const onOpenKernels = vi.fn();
    renderPanel({
      kernels: [
        kernel("remote", 142, { status: "available", installPath: null }),
        kernel("wrong-platform", 141, { platform: "linux" }),
      ],
      onOpenKernels,
    });
    expect((screen.getByLabelText("内核版本") as HTMLSelectElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "前往内核" }));
    expect(onOpenKernels).toHaveBeenCalledOnce();
  });

  it("supports explicit and keyboard cancellation", () => {
    const onCancel = vi.fn();
    renderPanel({ onCancel });
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    fireEvent.keyDown(screen.getByRole("form", { name: "创建环境" }), { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("filters and sorts usable cores deterministically", () => {
    const result = usableEnvironmentKernels([
      kernel("older", 131),
      kernel("newer", 141, { status: "update-available" }),
      kernel("remote", 142, { status: "available", installPath: null }),
      kernel("unsupported", 150, { kernelType: "yun" }),
    ], "win32");
    expect(result.map((item) => item.id)).toEqual(["newer", "older"]);
  });
});
