import { useEffect, useMemo, useRef, useState } from "react";
import { CircleDot, Columns3, Fingerprint, Globe2, List, LoaderCircle, RefreshCw, Search, X } from "lucide-react";
import { isDesktopRuntime, openFingerprintCheck, refreshEnvironmentDetail } from "../../api";
import type { DashboardSnapshot, OperationRecord } from "../../types";
import { environmentControlLabel } from "../../environmentIdentity";
import { fingerprintDetailGroups, formatRemoteValue, readRemoteValue, remoteProxyLabel } from "../environments/remoteDetails";
import { FingerprintComparisonView } from "./FingerprintComparisonView";

interface FingerprintPageProps {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
  desktop?: boolean;
  onRefreshDetail?: (envId: string) => Promise<OperationRecord>;
  onOpenCheck?: (envId: string) => Promise<unknown>;
}

const statusLabel: Record<string, string> = {
  stopped: "已停止",
  starting: "启动中",
  ready: "运行中",
  stopping: "停止中",
  failed: "失败",
  unknown: "未知",
};

export function FingerprintPage({
  snapshot,
  onRefresh,
  onError,
  desktop = isDesktopRuntime(),
  onRefreshDetail = refreshEnvironmentDetail,
  onOpenCheck = openFingerprintCheck,
}: FingerprintPageProps) {
  const environments = snapshot?.environments ?? [];
  const [selectedEnvId, setSelectedEnvId] = useState<string | null>(null);
  const [mode, setMode] = useState<"detail" | "compare">("detail");
  const [comparisonEnvIds, setComparisonEnvIds] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState("");
  const autoRequested = useRef(new Set<string>());
  const selected = environments.find((environment) => environment.envId === selectedEnvId) ?? null;
  const binding = snapshot?.environmentBindings.find((item) => item.envId === selectedEnvId) ?? null;
  const groups = fingerprintDetailGroups(binding?.remoteFingerprint);
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return environments;
    return environments.filter((environment) => [environment.name, environment.envId]
      .some((value) => value.toLocaleLowerCase().includes(normalized)));
  }, [environments, query]);

  useEffect(() => {
    if (environments.length === 0) {
      setSelectedEnvId(null);
      return;
    }
    if (!selectedEnvId || !environments.some((environment) => environment.envId === selectedEnvId)) {
      setSelectedEnvId(environments[0].envId);
    }
  }, [environments, selectedEnvId]);

  useEffect(() => {
    const available = new Set(environments.map((environment) => environment.envId));
    setComparisonEnvIds((current) => current.filter((envId) => available.has(envId)).slice(0, 4));
  }, [environments]);

  async function refresh(envId: string, automatic = false) {
    setBusy(`refresh:${envId}`);
    if (!automatic) onError("");
    try {
      const operation = await onRefreshDetail(envId);
      if (operation.status !== "succeeded") throw new Error(operation.message || "环境详情刷新失败");
      await onRefresh();
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : "环境详情刷新失败");
    } finally {
      setBusy("");
    }
  }

  useEffect(() => {
    if (mode !== "detail" || !desktop || !selected || binding?.refreshedAt || autoRequested.current.has(selected.envId)) return;
    autoRequested.current.add(selected.envId);
    void refresh(selected.envId, true);
  }, [binding?.refreshedAt, desktop, mode, selected?.envId]);

  function changeMode(nextMode: "detail" | "compare") {
    setMode(nextMode);
    if (nextMode === "compare" && comparisonEnvIds.length === 0 && selected) {
      setComparisonEnvIds([selected.envId]);
    }
  }

  function toggleComparison(envId: string, checked: boolean) {
    setComparisonEnvIds((current) => {
      if (!checked) return current.filter((currentId) => currentId !== envId);
      if (current.includes(envId) || current.length >= 4) return current;
      return [...current, envId];
    });
  }

  async function refreshComparison() {
    if (comparisonEnvIds.length === 0) return;
    setBusy("refresh:compare");
    onError("");
    try {
      const operations = await Promise.all(comparisonEnvIds.map((envId) => onRefreshDetail(envId)));
      const failed = operations.find((operation) => operation.status !== "succeeded");
      if (failed) throw new Error(failed.message || "环境详情刷新失败");
      await onRefresh();
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : "环境详情刷新失败");
    } finally {
      setBusy("");
    }
  }

  async function openCheck(envId: string) {
    setBusy(`check:${envId}`);
    onError("");
    try {
      await onOpenCheck(envId);
      await onRefresh();
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : "指纹检查页打开失败");
    } finally {
      setBusy("");
    }
  }

  return (
    <section className="module-workspace fingerprint-workspace">
      <div className="module-toolbar">
        <div className="toolbar-group">
          <span className="toolbar-title">环境指纹</span>
          <div className="segmented-control fingerprint-mode" aria-label="指纹视图">
            <button className={mode === "detail" ? "active" : ""} type="button" onClick={() => changeMode("detail")}><List size={14} />详情</button>
            <button className={mode === "compare" ? "active" : ""} type="button" onClick={() => changeMode("compare")}><Columns3 size={14} />对比</button>
          </div>
          <label className="search-control fingerprint-search">
            <Search size={15} />
            <input aria-label="搜索环境" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索名称或 envId" />
          </label>
        </div>
        <div className="toolbar-group actions">
          {mode === "detail" ? <>
            <button className="button secondary compact" type="button" disabled={!desktop || !selected || Boolean(busy)} onClick={() => selected && void refresh(selected.envId)}>
              {busy === `refresh:${selected?.envId}` ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}刷新
            </button>
            <button className="button secondary compact" type="button" disabled={!desktop || !selected || selected.status !== "ready" || Boolean(busy)} onClick={() => selected && void openCheck(selected.envId)}>
              {busy === `check:${selected?.envId}` ? <LoaderCircle className="spin" size={14} /> : <Globe2 size={14} />}检查页
            </button>
          </> : <>
            <button className="button secondary compact" type="button" disabled={!desktop || comparisonEnvIds.length === 0 || Boolean(busy)} onClick={() => void refreshComparison()}>
              {busy === "refresh:compare" ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}刷新所选
            </button>
            <button className="button secondary compact" type="button" disabled={comparisonEnvIds.length === 0 || Boolean(busy)} onClick={() => setComparisonEnvIds([])}>
              <X size={14} />清除
            </button>
          </>}
        </div>
      </div>
      <div className="fingerprint-body">
        <div className="fingerprint-environment-list" aria-label="环境列表">
          {filtered.map((environment) => {
            const environmentBinding = snapshot?.environmentBindings.find((item) => item.envId === environment.envId);
            const kernel = [readRemoteValue(environmentBinding?.remoteKernel, ["kernel"]), readRemoteValue(environmentBinding?.remoteKernel, ["version"])].filter(Boolean).join(" ");
            return mode === "detail" ? (
              <button key={environment.envId} data-env-id={environment.envId} aria-label={environmentControlLabel("查看", environment)} className={`fingerprint-environment-row ${selectedEnvId === environment.envId ? "selected" : ""}`} type="button" onClick={() => setSelectedEnvId(environment.envId)}>
                <span className="resource-icon"><Fingerprint size={16} /></span>
                <span><strong>{environment.name}</strong><small>{environment.envId}{kernel ? ` · ${kernel}` : ""}</small></span>
                <span className={`status-badge ${environment.status}`}>{statusLabel[environment.status] ?? environment.status}</span>
              </button>
            ) : (
              <label key={environment.envId} data-env-id={environment.envId} className={`fingerprint-environment-row fingerprint-compare-selector ${comparisonEnvIds.includes(environment.envId) ? "selected" : ""}`}>
                <span><input type="checkbox" aria-label={environmentControlLabel("对比", environment)} checked={comparisonEnvIds.includes(environment.envId)} disabled={!comparisonEnvIds.includes(environment.envId) && comparisonEnvIds.length >= 4} onChange={(event) => toggleComparison(environment.envId, event.target.checked)} /></span>
                <span><strong>{environment.name}</strong><small>{environment.envId}{kernel ? ` · ${kernel}` : ""}</small></span>
                <span className={`status-badge ${environment.status}`}>{statusLabel[environment.status] ?? environment.status}</span>
              </label>
            );
          })}
          {filtered.length === 0 && <div className="empty-state compact"><CircleDot size={18} /><span>没有匹配环境</span></div>}
        </div>
        <div className="fingerprint-viewer">
          {mode === "compare" ? (
            <FingerprintComparisonView environments={environments} bindings={snapshot?.environmentBindings ?? []} selectedEnvIds={comparisonEnvIds} />
          ) : selected ? (
            <>
              <div className="fingerprint-heading">
                <div><small>{selected.envId}</small><h2>{selected.name}</h2></div>
                <span>{binding?.refreshedAt ? new Date(binding.refreshedAt).toLocaleString("zh-CN") : "未读取"}</span>
              </div>
              <dl className="fingerprint-summary">
                <div><dt>内核</dt><dd>{formatRemoteValue(readRemoteValue(binding?.remoteKernel, ["kernel"]))} {formatRemoteValue(readRemoteValue(binding?.remoteKernel, ["version"]))}</dd></div>
                <div><dt>系统</dt><dd>{formatRemoteValue(readRemoteValue(binding?.remoteKernel, ["system"]))}</dd></div>
                <div><dt>代理</dt><dd>{binding ? remoteProxyLabel(binding.remoteProxy) : "-"}</dd></div>
                <div><dt>序列号</dt><dd>{formatRemoteValue(readRemoteValue(binding?.remoteMetadata, ["serial"]))}</dd></div>
              </dl>
              {groups.length > 0 ? (
                <div className="fingerprint-groups">
                  {groups.map((group) => (
                    <section key={group.title} className="fingerprint-group">
                      <h3>{group.title}</h3>
                      <dl>{group.rows.map((row) => <div key={row.key}><dt>{row.label}</dt><dd title={formatRemoteValue(row.value)}>{formatRemoteValue(row.value)}</dd></div>)}</dl>
                    </section>
                  ))}
                </div>
              ) : (
                <div className="empty-state"><Fingerprint size={20} /><span>{busy.startsWith("refresh:") ? "正在读取指纹" : "尚未读取指纹"}</span></div>
              )}
            </>
          ) : <div className="empty-state"><CircleDot size={20} /><span>暂无环境</span></div>}
        </div>
      </div>
    </section>
  );
}
