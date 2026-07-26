import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../../types";
import { EnvironmentBatchBar, environmentActionIds } from "./EnvironmentBatchBar";

afterEach(cleanup);

const environments = [
  environment("env-1", "stopped"),
  environment("env-2", "ready"),
  environment("env-3", "stopping"),
];

function environment(envId: string, status: string): DashboardSnapshot["environments"][number] {
  return {
    envId,
    name: envId,
    status,
    cdp: "-",
    lastEvent: "synced",
    generation: 0,
    requestId: null,
    currentOperationId: null,
    updatedAt: "2026-07-26T00:00:00Z",
  };
}

describe("EnvironmentBatchBar", () => {
  it("splits selected environments by valid lifecycle transition", () => {
    expect(environmentActionIds(environments, ["env-1", "env-2", "env-3"], "start")).toEqual(["env-1"]);
    expect(environmentActionIds(environments, ["env-1", "env-2", "env-3"], "stop")).toEqual(["env-2"]);
  });

  it("submits only eligible environment ids", () => {
    const onAction = vi.fn();
    render(<EnvironmentBatchBar environments={environments} selectedIds={["env-1", "env-2", "env-3"]} desktop busy={false} onAction={onAction} onClear={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "启动 1" }));
    fireEvent.click(screen.getByRole("button", { name: "停止 1" }));
    expect(onAction).toHaveBeenNthCalledWith(1, "start", ["env-1"]);
    expect(onAction).toHaveBeenNthCalledWith(2, "stop", ["env-2"]);
  });

  it("keeps lifecycle actions disabled outside the desktop runtime", () => {
    render(<EnvironmentBatchBar environments={environments} selectedIds={["env-1", "env-2"]} desktop={false} busy={false} onAction={vi.fn()} onClear={vi.fn()} />);
    expect((screen.getByRole("button", { name: "启动 1" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "停止 1" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
