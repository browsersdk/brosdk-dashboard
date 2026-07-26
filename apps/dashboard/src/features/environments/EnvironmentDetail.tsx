import { useState } from "react";
import { Eraser, Globe2, LoaderCircle, Play, RefreshCw, ScanSearch, Square, Trash2, X } from "lucide-react";
import type { DashboardSnapshot, EnvironmentBindingSummary } from "../../types";
import { formatRemoteValue, readRemoteValue, remoteProxyLabel } from "./remoteDetails";

interface EnvironmentDetailProps {
  environment: DashboardSnapshot["environments"][number];
  binding: EnvironmentBindingSummary | null;
  busy: boolean;
  desktop: boolean;
  diagnostic: unknown;
  onClose: () => void;
  onStart: () => void;
  onStop: () => void;
  onRefresh: () => void;
  onOpenCheck: () => void;
  onCaptureDiagnostic: () => void;
  onCleanupLocalData: () => void;
  onDelete: () => void;
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
  diagnostic,
  onClose,
  onStart,
  onStop,
  onRefresh,
  onOpenCheck,
  onCaptureDiagnostic,
  onCleanupLocalData,
  onDelete,
}: EnvironmentDetailProps) {
  const [confirmAction, setConfirmAction] = useState<"cleanup" | "delete" | null>(null);
  const kernel = binding?.remoteKernel;
  const metadata = binding?.remoteMetadata;
  const fingerprint = binding?.remoteFingerprint;
  const diagnosticPages = readRemoteValue(diagnostic, ["pages"]);
  const diagnosticOrigins = Array.isArray(diagnosticPages)
    ? diagnosticPages.map((page) => readRemoteValue(page, ["origin"]))
    : [];
  const canStart = ["stopped", "failed"].includes(environment.status);
  const canStop = ["ready", "starting"].includes(environment.status);
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
        {canStop ? (
          <button className="button secondary compact" type="button" disabled={!desktop || busy} onClick={onStop}>
            <Square size={14} />停止
          </button>
        ) : (
          <button className="button primary compact" type="button" disabled={!desktop || busy || !canStart} onClick={onStart}>
            <Play size={14} />启动
          </button>
        )}
        <button className="button secondary compact" type="button" disabled={!desktop || busy} onClick={onRefresh}>
          {busy ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}刷新详情
        </button>
        <button className="button secondary compact" type="button" disabled={!desktop || busy || environment.status !== "ready"} onClick={onOpenCheck}>
          <Globe2 size={14} />检查指纹
        </button>
        <button className="button secondary compact" type="button" disabled={!desktop || busy || environment.status !== "ready"} onClick={onCaptureDiagnostic}>
          <ScanSearch size={14} />页面诊断
        </button>
        <button className="button secondary compact" type="button" disabled={!desktop || busy || environment.status !== "stopped"} onClick={() => setConfirmAction("cleanup")}>
          <Eraser size={14} />清理本地数据
        </button>
        <button className="button danger compact" type="button" disabled={!desktop || busy || environment.status !== "stopped"} onClick={() => setConfirmAction("delete")}>
          <Trash2 size={14} />删除环境
        </button>
      </div>
      {confirmAction && (
        <div className="environment-confirm" role="alertdialog" aria-label={confirmAction === "delete" ? "确认删除环境" : "确认清理本地数据"}>
          <strong>{confirmAction === "delete" ? "删除服务端环境？" : "清理本地浏览数据？"}</strong>
          <div>
            <button className="button secondary compact" type="button" onClick={() => setConfirmAction(null)}>取消</button>
            <button className={`button compact ${confirmAction === "delete" ? "danger" : "primary"}`} type="button" onClick={() => {
              const action = confirmAction;
              setConfirmAction(null);
              if (action === "delete") onDelete();
              else onCleanupLocalData();
            }}>确认</button>
          </div>
        </div>
      )}
      <div className="environment-runtime-line">
        <span className={`status-badge ${environment.status}`}>{runtimeStatus[environment.status] ?? environment.status}</span>
        <span>generation {environment.generation}</span>
      </div>
      <dl className="detail-list compact">
        {rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd title={value}>{value}</dd></div>)}
      </dl>
      {diagnostic !== null && (
        <div className="environment-diagnostic" role="status">
          <strong>页面诊断</strong>
          <span>{formatRemoteValue(readRemoteValue(diagnostic, ["pageCount"]))} 页 · {formatRemoteValue(readRemoteValue(diagnostic, ["failedPages"]))} 失败</span>
          <small>{formatRemoteValue(diagnosticOrigins)}</small>
        </div>
      )}
    </aside>
  );
}
