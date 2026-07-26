import type { DashboardSnapshot } from "./types";

export type EnvironmentSummary = DashboardSnapshot["environments"][number];

export function environmentLabel(environment: EnvironmentSummary) {
  return `${environment.name} · ${environment.envId}`;
}

export function environmentControlLabel(action: string, environment: EnvironmentSummary) {
  return `${action} ${environment.name} (${environment.envId})`;
}

export function assertEnvironmentIdentity(snapshot: DashboardSnapshot): DashboardSnapshot {
  const environmentIds = uniqueNonEmptyIds(
    snapshot.environments.map((environment) => environment.envId),
    "环境",
  );
  const bindingIds = uniqueNonEmptyIds(
    snapshot.environmentBindings.map((binding) => binding.envId),
    "环境详情",
  );
  for (const envId of bindingIds) {
    if (!environmentIds.has(envId)) {
      throw new Error(`环境详情引用了不存在的 envId: ${envId}`);
    }
  }
  return snapshot;
}

function uniqueNonEmptyIds(values: string[], subject: string) {
  const ids = new Set<string>();
  for (const value of values) {
    if (value.trim().length === 0) {
      throw new Error(`${subject}数据缺少 envId`);
    }
    if (ids.has(value)) {
      throw new Error(`${subject}数据包含重复 envId: ${value}`);
    }
    ids.add(value);
  }
  return ids;
}
