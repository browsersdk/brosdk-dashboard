import { Globe2, LoaderCircle, RefreshCw, X } from "lucide-react";
import type { DashboardSnapshot, EnvironmentBindingSummary } from "../../types";
import { formatRemoteValue, readRemoteValue, remoteProxyLabel } from "./remoteDetails";

interface EnvironmentDetailProps {
  environment: DashboardSnapshot["environments"][number];
  binding: EnvironmentBindingSummary | null;
  busy: boolean;
  desktop: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onOpenCheck: () => void;
}

const runtimeStatus: Record<string, string> = {
  stopped: "已停止",
  starting: "启动中",
  ready: "运行中",
  stopping: "停止中",
  failed: "失败",
  unknown: "未知",
};

export function EnvironmentDetail({
  environment,
  binding,
  busy,
  desktop,
  onClose,
  onRefresh,
  onOpenCheck,
}: EnvironmentDetailProps) {
  const kernel = binding?.remoteKernel;
  const metadata = binding?.remoteMetadata;
  const fingerprint = binding?.remoteFingerprint;
  const rows = [
    ["内核", [readRemoteValue(kernel, ["kernel"]), readRemoteValue(kernel, ["version"])].filter(Boolean).join(" ") || "-"],
    ["系统", formatRemoteValue(readRemoteValue(kernel, ["system"]) ?? readRemoteValue(fingerprint, ["system", "platform"]))],
    ["代理", binding ? remoteProxyLabel(binding.remoteProxy) : "-"],
    ["语言", formatRemoteValue(readRemoteValue(fingerprint, ["language", "languages"]))],
    ["时区", formatRemoteValue(readRemoteValue(fingerprint, ["zone", "timezone", "timeZone"]))],
    ["屏幕", formatRemoteValue(readRemoteValue(fingerprint, ["dpi", "screen", "screenResolution"]))],
    ["序列号", formatRemoteValue(readRemoteValue(metadata, ["serial"]))],
    ["CDP", environment.cdp],
    ["最后事件", environment.lastEvent],
    ["详情刷新", binding?.refreshedAt ? new Date(binding.refreshedAt).toLocaleString("zh-CN") : "未读取"],
  ];

  return (
    <aside className="environment-detail" aria-label="环境详情">
      <div className="detail-heading">
        <div><small>{environment.envId}</small><h2>{environment.name}</h2></div>
        <button className="icon-button" type="button" title="关闭详情" aria-label="关闭详情" onClick={onClose}><X size={16} /></button>
      </div>
      <div className="environment-detail-actions">
        <button className="button secondary compact" type="button" disabled={!desktop || busy} onClick={onRefresh}>
          {busy ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}刷新详情
        </button>
        <button className="button secondary compact" type="button" disabled={!desktop || busy || environment.status !== "ready"} onClick={onOpenCheck}>
          <Globe2 size={14} />检查指纹
        </button>
      </div>
      <div className="environment-runtime-line">
        <span className={`status-badge ${environment.status}`}>{runtimeStatus[environment.status] ?? environment.status}</span>
        <span>generation {environment.generation}</span>
      </div>
      <dl className="detail-list compact">
        {rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd title={value}>{value}</dd></div>)}
      </dl>
    </aside>
  );
}
