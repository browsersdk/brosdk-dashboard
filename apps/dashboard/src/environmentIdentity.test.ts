import { describe, expect, it } from "vitest";
import type { DashboardSnapshot } from "./types";
import { assertEnvironmentIdentity, environmentControlLabel, environmentLabel } from "./environmentIdentity";

const snapshot = {
  environments: [environment("env-1", "共享环境"), environment("env-2", "共享环境")],
  environmentBindings: [binding("env-1"), binding("env-2")],
} as DashboardSnapshot;

describe("environment identity", () => {
  it("allows duplicate names while labels remain distinguishable by envId", () => {
    expect(assertEnvironmentIdentity(snapshot)).toBe(snapshot);
    expect(environmentLabel(snapshot.environments[0])).toBe("共享环境 · env-1");
    expect(environmentControlLabel("选择", snapshot.environments[1])).toBe("选择 共享环境 (env-2)");
  });

  it("rejects empty or duplicate environment ids", () => {
    expect(() => assertEnvironmentIdentity({
      ...snapshot,
      environments: [environment("env-1", "A"), environment("env-1", "B")],
      environmentBindings: [],
    })).toThrow("环境数据包含重复 envId: env-1");
    expect(() => assertEnvironmentIdentity({
      ...snapshot,
      environments: [environment(" ", "A")],
      environmentBindings: [],
    })).toThrow("环境数据缺少 envId");
  });

  it("rejects duplicate and orphan environment detail bindings", () => {
    expect(() => assertEnvironmentIdentity({
      ...snapshot,
      environmentBindings: [binding("env-1"), binding("env-1")],
    })).toThrow("环境详情数据包含重复 envId: env-1");
    expect(() => assertEnvironmentIdentity({
      ...snapshot,
      environmentBindings: [binding("env-3")],
    })).toThrow("环境详情引用了不存在的 envId: env-3");
  });
});

function environment(envId: string, name: string): DashboardSnapshot["environments"][number] {
  return {
    envId,
    name,
    status: "stopped",
    cdp: "-",
    lastEvent: "synced",
    generation: 0,
    requestId: null,
    currentOperationId: null,
    updatedAt: "2026-07-26T00:00:00Z",
  };
}

function binding(envId: string): DashboardSnapshot["environmentBindings"][number] {
  return {
    envId,
    fingerprintProfileId: null,
    proxyProfileId: null,
    remoteFingerprint: {},
    remoteProxy: null,
    remoteKernel: {},
    remoteMetadata: {},
    refreshedAt: null,
  };
}
