import { LoaderCircle, Play, Square, X } from "lucide-react";
import { actionTitle, desktopActionReason } from "../../actionTitles";
import type { DashboardSnapshot, EnvironmentBatchAction } from "../../types";

type Environment = DashboardSnapshot["environments"][number];

interface EnvironmentBatchBarProps {
  environments: Environment[];
  selectedIds: string[];
  desktop: boolean;
  busy: boolean;
  onAction: (action: EnvironmentBatchAction, envIds: string[]) => void;
  onClear: () => void;
}

export function environmentActionIds(
  environments: Environment[],
  selectedIds: string[],
  action: EnvironmentBatchAction,
) {
  const selected = new Set(selectedIds);
  return environments
    .filter((environment) => selected.has(environment.envId))
    .filter((environment) => action === "start"
      ? ["stopped", "failed"].includes(environment.status)
      : ["ready", "starting"].includes(environment.status))
    .map((environment) => environment.envId);
}

export function EnvironmentBatchBar({
  environments,
  selectedIds,
  desktop,
  busy,
  onAction,
  onClear,
}: EnvironmentBatchBarProps) {
  if (selectedIds.length === 0) return null;
  const startIds = environmentActionIds(environments, selectedIds, "start");
  const stopIds = environmentActionIds(environments, selectedIds, "stop");
  const batchActionReason = desktopActionReason(desktop, busy, "批量操作正在执行");
  const startReason = batchActionReason || (startIds.length === 0 ? "当前选择没有可启动环境" : "");
  const stopReason = batchActionReason || (stopIds.length === 0 ? "当前选择没有可停止环境" : "");

  return (
    <div className="environment-batch-bar" role="toolbar" aria-label="批量环境操作">
      <strong>{selectedIds.length} 个已选择</strong>
      <span>{startIds.length} 可启动 · {stopIds.length} 可停止</span>
      <div>
        <button className="button secondary compact" type="button" title={actionTitle("批量启动", startReason)} disabled={!desktop || busy || startIds.length === 0} onClick={() => onAction("start", startIds)}>
          {busy ? <LoaderCircle className="spin" size={14} /> : <Play size={14} />}启动 {startIds.length}
        </button>
        <button className="button secondary compact" type="button" title={actionTitle("批量停止", stopReason)} disabled={!desktop || busy || stopIds.length === 0} onClick={() => onAction("stop", stopIds)}>
          <Square size={14} />停止 {stopIds.length}
        </button>
        <button className="icon-button" type="button" title={actionTitle("清除选择", busy ? "批量操作正在执行" : "")} aria-label="清除选择" disabled={busy} onClick={onClear}><X size={15} /></button>
      </div>
    </div>
  );
}
