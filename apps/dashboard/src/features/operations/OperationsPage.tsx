import {
  RotateCcw,
  Search,
  SlidersHorizontal,
  Square,
  TerminalSquare,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import {
  cancelOperation,
  isDesktopRuntime,
  retryOperation,
} from "../../api";
import { environmentLabel } from "../../environmentIdentity";
import type { DashboardSnapshot } from "../../types";

const statusLabel: Record<string, string> = {
  queued: "排队中",
  running: "执行中",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

const retryableKinds = new Set([
  "environment.sync",
  "runtime.reconcile",
  "environment.start",
  "environment.stop",
  "kernel.install",
]);

type Operation = DashboardSnapshot["operations"][number];

export function OperationsPage({ snapshot, onRefresh, onError }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [status, setStatus] = useState("all");
  const [kind, setKind] = useState("all");
  const [environmentId, setEnvironmentId] = useState("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busy, setBusy] = useState("");
  const environments = snapshot?.environments ?? [];
  const environmentById = useMemo(() => new Map(environments.map((environment) => [environment.envId, environment])), [environments]);
  const kinds = useMemo(() => Array.from(new Set((snapshot?.operations ?? []).map((operation) => operation.kind))), [snapshot?.operations]);
  const counts = useMemo(() => {
    const operations = snapshot?.operations ?? [];
    return {
      total: operations.length,
      active: operations.filter((operation) => operation.status === "queued" || operation.status === "running").length,
      failed: operations.filter((operation) => operation.status === "failed").length,
    };
  }, [snapshot?.operations]);
  const operations = useMemo(() => (snapshot?.operations ?? []).filter((operation) => {
    const matchesStatus = status === "all" || operation.status === status;
    const matchesKind = kind === "all" || operation.kind === kind;
    const matchesEnvironment = environmentId === "all" || operation.envId === environmentId;
    const environment = operation.envId ? environmentById.get(operation.envId) : null;
    const needle = query.trim().toLocaleLowerCase();
    const searchText = `${operation.label} ${operation.message} ${operation.id} ${operation.envId ?? ""} ${environment?.name ?? ""}`.toLocaleLowerCase();
    return matchesStatus && matchesKind && matchesEnvironment && (!needle || searchText.includes(needle));
  }), [environmentById, environmentId, kind, query, snapshot?.operations, status]);
  const selected = snapshot?.operations.find((operation) => operation.id === selectedId) ?? null;

  async function run(action: string, callback: () => Promise<unknown>) {
    setBusy(action);
    onError("");
    try {
      await callback();
      await onRefresh();
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : "操作处理失败");
    } finally {
      setBusy("");
    }
  }

  return (
    <section className={`module-workspace operation-workspace ${selected ? "with-detail" : ""}`}>
      <div className="module-toolbar operation-toolbar">
        <div className="toolbar-group">
          <label className="search-control"><Search size={14} /><input aria-label="搜索操作" placeholder="搜索操作、环境或 ID" value={query} onChange={(event) => setQuery(event.target.value)} /></label>
          <label className="select-control"><SlidersHorizontal size={14} /><select aria-label="状态筛选" value={status} onChange={(event) => setStatus(event.target.value)}><option value="all">全部状态</option>{Object.entries(statusLabel).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
          <label className="select-control"><TerminalSquare size={14} /><select aria-label="类型筛选" value={kind} onChange={(event) => setKind(event.target.value)}><option value="all">全部类型</option>{kinds.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
          <label className="select-control"><select aria-label="环境筛选" value={environmentId} onChange={(event) => setEnvironmentId(event.target.value)}><option value="all">全部环境</option>{environments.map((environment) => <option key={environment.envId} value={environment.envId}>{environmentLabel(environment)}</option>)}</select></label>
        </div>
        <div className="operation-summary" aria-label="操作摘要">
          <span>显示 <strong>{operations.length}</strong>/{counts.total}</span>
          <span>进行中 <strong>{counts.active}</strong></span>
          <span className={counts.failed ? "has-failures" : ""}>失败 <strong>{counts.failed}</strong></span>
        </div>
      </div>
      <div className="operation-body">
        <div className="table-wrap">
          <table className="module-table">
            <thead><tr><th>操作</th><th>环境</th><th>类型</th><th>状态</th><th>信息</th><th>更新时间</th><th aria-label="操作" /></tr></thead>
            <tbody>
              {operations.map((operation) => {
                const environment = operation.envId ? environmentById.get(operation.envId) : null;
                const canCancel = operation.status === "queued";
                const canRetry = (operation.status === "failed" || operation.status === "cancelled") && retryableKinds.has(operation.kind);
                return (
                  <tr
                    key={operation.id}
                    data-operation-id={operation.id}
                    data-env-id={operation.envId ?? ""}
                    className={selectedId === operation.id ? "selected" : ""}
                    onClick={() => setSelectedId(operation.id)}
                  >
                    <td>{operation.label}</td>
                    <td>{environment ? <span className="operation-environment">{environment.name}<small>{environment.envId}</small></span> : "全局"}</td>
                    <td><code>{operation.kind}</code></td>
                    <td><span className={`status-badge ${operation.status}`}>{statusLabel[operation.status] ?? operation.status}</span></td>
                    <td>{operation.message}</td>
                    <td>{new Date(operation.updatedAt).toLocaleString("zh-CN")}</td>
                    <td className="row-actions inline-actions">
                      {canCancel && <button className="icon-button danger" type="button" title="取消排队中的操作" aria-label={`取消 ${operation.label} ${operation.envId ?? "全局"}`} disabled={!isDesktopRuntime() || Boolean(busy)} onClick={(event) => { event.stopPropagation(); void run(`cancel:${operation.id}`, () => cancelOperation(operation.id)); }}><Square size={14} /></button>}
                      {canRetry && <button className="icon-button" type="button" title="重试失败操作" aria-label={`重试 ${operation.label} ${operation.envId ?? "全局"}`} disabled={!isDesktopRuntime() || Boolean(busy)} onClick={(event) => { event.stopPropagation(); void run(`retry:${operation.id}`, () => retryOperation(operation.id)); }}><RotateCcw size={14} /></button>}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {operations.length === 0 && <div className="environment-empty"><TerminalSquare size={18} /><span>没有匹配操作</span></div>}
        </div>
        {selected && <OperationDetail operation={selected} environment={selected.envId ? environmentById.get(selected.envId) : null} onClose={() => setSelectedId(null)} />}
      </div>
    </section>
  );
}

function OperationDetail({ operation, environment, onClose }: {
  operation: Operation;
  environment: DashboardSnapshot["environments"][number] | null | undefined;
  onClose: () => void;
}) {
  return (
    <aside className="environment-detail operation-detail">
      <div className="detail-heading"><div><small>操作日志</small><h2>{operation.label}</h2></div><button className="icon-button" type="button" title="关闭详情" aria-label="关闭详情" onClick={onClose}><X size={16} /></button></div>
      <dl className="detail-list compact">
        <DetailRow label="Operation ID" value={operation.id} />
        <DetailRow label="Kind" value={operation.kind} />
        <DetailRow label="Environment" value={environment ? environmentLabel(environment) : "全局"} />
        <DetailRow label="ReqId" value={operation.requestId === null ? "-" : String(operation.requestId)} />
        <DetailRow label="Generation" value={String(operation.generation)} />
        <DetailRow label="错误码" value={operation.errorCode ?? "-"} />
        <DetailRow label="创建时间" value={formatTime(operation.createdAt)} />
        <DetailRow label="更新时间" value={formatTime(operation.updatedAt)} />
      </dl>
      <div className="json-preview"><strong>请求快照</strong><pre>{JSON.stringify(operation.request ?? null, null, 2)}</pre></div>
    </aside>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd title={value}>{value}</dd></div>;
}

function formatTime(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}
