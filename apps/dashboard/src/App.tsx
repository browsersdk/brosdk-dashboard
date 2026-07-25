import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  Boxes,
  Bot,
  CheckCircle2,
  CircleAlert,
  CircleDot,
  Database,
  Fingerprint,
  LoaderCircle,
  Play,
  RefreshCw,
  ServerCog,
  Settings,
  ShieldCheck,
  Square,
  Search,
  SlidersHorizontal,
  X,
  TerminalSquare,
} from "lucide-react";
import {
  eventsSince,
  getSnapshot,
  isDesktopRuntime,
  reconcileRuntimes,
  runSmoke,
  startEnvironment,
  stopEnvironment,
  syncEnvironments,
} from "./api";
import type {
  DashboardSnapshot,
  SmokeReport,
  SmokeStage,
  SmokeStageStatus,
} from "./types";

const navItems = [
  { key: "overview", label: "总览", icon: Activity },
  { key: "environments", label: "环境", icon: Boxes },
  { key: "mcp", label: "MCP", icon: Bot },
  { key: "operations", label: "操作", icon: TerminalSquare },
  { key: "settings", label: "设置", icon: Settings },
] as const;

type Page = (typeof navItems)[number]["key"];

const statusLabel: Record<string, string> = {
  "host-running": "Host 运行中",
  "host-starting": "Host 启动中",
  "host-degraded": "Host 异常",
  "host-stopped": "Host 已停止",
  "dll-missing": "DLL 缺失",
  "browser-preview": "浏览器预览",
  stopped: "已停止",
  starting: "启动中",
  ready: "运行中",
  stopping: "停止中",
  failed: "失败",
  unknown: "未知",
  queued: "排队中",
  running: "执行中",
  cancelled: "已取消",
  succeeded: "已完成",
};

const stageLabel: Record<SmokeStageStatus, string> = {
  passed: "通过",
  failed: "失败",
  skipped: "跳过",
};

export default function App() {
  const [page, setPage] = useState<Page>("overview");
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [smoke, setSmoke] = useState<SmokeReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [smokeBusy, setSmokeBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setSnapshot(await getSnapshot());
      setError("");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : "读取本地状态失败");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    let disposed = false;
    let sequence = snapshot?.latestEventSequence ?? 0;
    const timer = window.setInterval(() => {
      void eventsSince(sequence)
        .then((events) => {
          if (disposed || events.length === 0) return;
          sequence = events[events.length - 1].sequence;
          return load();
        })
        .catch((requestError) => {
          if (!disposed) {
            setError(requestError instanceof Error ? requestError.message : "读取事件失败");
          }
        });
    }, 1500);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [load, snapshot?.latestEventSequence]);

  const latestSmoke = smoke ?? snapshot?.sdk.lastSmoke ?? null;
  const failedStages = latestSmoke?.stages.filter((stage) => stage.status === "failed").length ?? 0;
  const passedStages = latestSmoke?.stages.filter((stage) => stage.status === "passed").length ?? 0;

  async function executeSmoke() {
    setSmokeBusy(true);
    setError("");
    try {
      const report = await runSmoke();
      setSmoke(report);
      await load();
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : "SDK smoke 执行失败");
    } finally {
      setSmokeBusy(false);
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand" aria-label="BroSDK Dashboard">
          <span className="brand-mark"><img src="/fingerprint.svg" alt="" /></span>
          <span className="brand-name">BroSDK</span>
        </div>
        <nav className="primary-nav" aria-label="主导航">
          {navItems.map((item) => (
            <button
              key={item.key}
              aria-label={item.label}
              className={page === item.key ? "active" : ""}
              title={item.label}
              type="button"
              onClick={() => setPage(item.key)}
            >
              <item.icon size={18} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className={`service-dot ${snapshot?.capabilities.dllExists ? "ready" : "error"}`} />
          <div>
            <strong>Runtime Host</strong>
            <small>{snapshot?.sdk.hostPath ? "已发现" : "待构建"}</small>
          </div>
        </div>
      </aside>

      <main className="main-content">
        <header className="page-header">
          <div>
            <div className="breadcrumb"><span>本地客户端</span><strong>{navItems.find((item) => item.key === page)?.label}</strong></div>
            <h1>{page === "overview" ? "总览" : navItems.find((item) => item.key === page)?.label}</h1>
          </div>
          <div className="header-actions">
            <button className="button secondary" type="button" onClick={() => void load()} disabled={loading}>
              <RefreshCw className={loading ? "spin" : ""} size={16} />
              刷新
            </button>
            <button className="button primary smoke-button" type="button" onClick={() => void executeSmoke()} disabled={smokeBusy}>
              {smokeBusy ? <LoaderCircle className="spin" size={16} /> : <Play size={16} />}
              SDK Smoke
            </button>
          </div>
        </header>

        {error && <div className="error-banner" role="alert"><CircleAlert size={17} /><span>{error}</span></div>}

        {page === "overview" && (
          <>
            <section className="summary-band" aria-label="运行概览">
              <Metric icon={ServerCog} tone="blue" label="SDK" value={statusLabel[snapshot?.sdk.state ?? ""] ?? "-"} detail={snapshot?.capabilities.dllExists ? "DLL present" : "DLL missing"} />
              <Metric icon={ShieldCheck} tone="green" label="API Key" value={snapshot?.sdk.apiKey.present ? "已设置" : "未设置"} detail={snapshot?.sdk.apiKey.source ?? "BROSDK_API_KEY"} />
              <Metric icon={Bot} tone="amber" label="内嵌 MCP" value={snapshot?.capabilities.embeddedMcp ? "可用" : "未知"} detail={snapshot?.mcp.endpointHint ?? "-"} />
              <Metric icon={Database} tone="gray" label="Smoke" value={latestSmoke ? `${passedStages}/${latestSmoke.stages.length}` : "未运行"} detail={failedStages ? `${failedStages} failed` : latestSmoke?.skipped ? "live skipped" : "ready"} />
            </section>
            <section className="workspace overview-grid">
              <SdkPanel snapshot={snapshot} />
              <SmokePanel report={latestSmoke} busy={smokeBusy} />
            </section>
          </>
        )}

        {page === "environments" && (
          <EnvironmentPage
            snapshot={snapshot}
            onRefresh={load}
            onError={(message) => setError(message)}
          />
        )}
        {page === "mcp" && <McpPage snapshot={snapshot} />}
        {page === "operations" && <OperationsPage snapshot={snapshot} />}
        {page === "settings" && <SettingsPage snapshot={snapshot} />}
      </main>
    </div>
  );
}

function Metric({ icon: Icon, tone, label, value, detail }: {
  icon: typeof Activity;
  tone: "blue" | "green" | "amber" | "gray";
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="metric">
      <span className={`metric-icon ${tone}`}><Icon size={18} /></span>
      <div>
        <small>{label}</small>
        <strong>{value}</strong>
        <span>{detail}</span>
      </div>
    </div>
  );
}

function SdkPanel({ snapshot }: { snapshot: DashboardSnapshot | null }) {
  const capabilities = snapshot?.capabilities;
  const rows = useMemo(() => [
    ["DLL", snapshot?.sdk.dllPath ?? "-"],
    ["Host", snapshot?.sdk.hostPath ?? "-"],
    ["Runtime", snapshot?.sdk.runtime.state ?? "-"],
    ["PID / Generation", snapshot?.sdk.runtime.pid ? `${snapshot.sdk.runtime.pid} / ${snapshot.sdk.runtime.generation}` : "-"],
    ["WorkDir", snapshot?.sdk.workDir ?? "-"],
    ["Database", snapshot?.databasePath ?? "-"],
    ["Event sequence", snapshot ? String(snapshot.latestEventSequence) : "-"],
    ["平台", capabilities?.platform ?? "-"],
    ["C ABI", capabilities?.cAbi ? "ready" : "unknown"],
    ["CDP", capabilities?.cdpCalls.join(", ") || "-"],
  ], [capabilities, snapshot]);

  return (
    <section className="panel">
      <div className="panel-heading"><ServerCog size={17} /><h2>SDK Runtime</h2></div>
      <dl className="detail-list">
        {rows.map(([label, value]) => (
          <div key={label}><dt>{label}</dt><dd title={value}>{value}</dd></div>
        ))}
      </dl>
    </section>
  );
}

function SmokePanel({ report, busy }: { report: SmokeReport | null; busy: boolean }) {
  return (
    <section className="panel">
      <div className="panel-heading"><Activity size={17} /><h2>Smoke 阶段</h2>{busy && <LoaderCircle className="spin" size={16} />}</div>
      <div className="stage-list">
        {report?.stages.length ? report.stages.map((stage) => <StageRow key={stage.name} stage={stage} />) : (
          <div className="empty-state"><CircleDot size={22} /><span>等待执行</span></div>
        )}
      </div>
      {report && (
        <footer className="panel-footer">
          <span>result cb {report.callbacks.result}</span>
          <span>log cb {report.callbacks.log}</span>
          <span>{report.embeddedMcpPort ? `MCP :${report.embeddedMcpPort}` : "MCP off"}</span>
        </footer>
      )}
    </section>
  );
}

function StageRow({ stage }: { stage: SmokeStage }) {
  return (
    <article className={`stage-row ${stage.status}`}>
      <span>{stage.status === "passed" ? <CheckCircle2 size={16} /> : stage.status === "failed" ? <CircleAlert size={16} /> : <CircleDot size={16} />}</span>
      <div>
        <strong>{stage.name}</strong>
        <small>{stageLabel[stage.status]} · {stage.durationMs}ms{stage.code !== null ? ` · code ${stage.code}` : ""}</small>
      </div>
      <em title={stage.message}>{stage.message}</em>
    </article>
  );
}

function EnvironmentPage({ snapshot, onRefresh, onError }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [selectedEnvId, setSelectedEnvId] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState("");
  const rows = snapshot?.environments ?? [];
  const filteredRows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return rows.filter((environment) => {
      const matchesStatus = status === "all" || environment.status === status;
      const matchesQuery = !normalized || [environment.name, environment.localLabel, environment.envId, ...environment.tags]
        .some((value) => value.toLocaleLowerCase().includes(normalized));
      return matchesStatus && matchesQuery;
    });
  }, [query, rows, status]);
  const selected = rows.find((environment) => environment.envId === selectedEnvId) ?? null;

  async function runAction(action: string, callback: () => Promise<unknown>) {
    setBusyAction(action);
    try {
      await callback();
      await onRefresh();
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : "环境操作失败");
    } finally {
      setBusyAction("");
    }
  }

  return (
    <section className={`module-workspace environment-workspace ${selected ? "with-detail" : ""}`}>
      <div className="module-toolbar">
        <div className="toolbar-group">
          <span className="toolbar-title">环境列表</span>
          <label className="search-control">
            <Search size={15} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索名称或 envId" />
          </label>
          <label className="select-control">
            <SlidersHorizontal size={15} />
            <select value={status} onChange={(event) => setStatus(event.target.value)} aria-label="状态筛选">
              <option value="all">全部状态</option>
              <option value="stopped">已停止</option>
              <option value="starting">启动中</option>
              <option value="ready">运行中</option>
              <option value="stopping">停止中</option>
              <option value="failed">失败</option>
              <option value="unknown">未知</option>
            </select>
          </label>
        </div>
        <div className="toolbar-group actions">
          <button className="button secondary compact" type="button" disabled={!isDesktopRuntime() || Boolean(busyAction)} onClick={() => void runAction("sync", syncEnvironments)}>
            <RefreshCw className={busyAction === "sync" ? "spin" : ""} size={14} />同步
          </button>
          <button className="button secondary compact" type="button" disabled={!isDesktopRuntime() || Boolean(busyAction)} onClick={() => void runAction("reconcile", reconcileRuntimes)}>
            <Activity className={busyAction === "reconcile" ? "spin" : ""} size={14} />对账
          </button>
        </div>
      </div>
      <div className="environment-body">
      <div className="table-wrap environment-table-wrap">
        <table className="module-table">
          <thead><tr><th>环境</th><th>状态</th><th>CDP</th><th>最后事件</th><th aria-label="操作" /></tr></thead>
          <tbody>
            {filteredRows.map((environment) => (
              <tr key={environment.envId} className={selectedEnvId === environment.envId ? "selected" : ""} onClick={() => setSelectedEnvId(environment.envId)}>
                <td><div className="resource-name"><span className="resource-icon"><Boxes size={16} /></span><div><strong>{environment.localLabel || environment.name}</strong><small>{environment.envId}</small></div></div></td>
                <td><span className={`status-badge ${environment.status}`}>{statusLabel[environment.status] ?? environment.status}</span></td>
                <td><code>{environment.cdp}</code></td>
                <td>{environment.lastEvent}</td>
                <td className="row-actions">
                  {environment.status === "ready" || environment.status === "starting" ? (
                    <button className="icon-button danger" type="button" title="停止环境" aria-label={`停止 ${environment.name}`} disabled={!isDesktopRuntime() || Boolean(busyAction)} onClick={(event) => { event.stopPropagation(); void runAction(`stop:${environment.envId}`, () => stopEnvironment(environment.envId)); }}>
                      {busyAction === `stop:${environment.envId}` ? <LoaderCircle className="spin" size={15} /> : <Square size={15} />}
                    </button>
                  ) : (
                    <button className="icon-button" type="button" title="启动环境" aria-label={`启动 ${environment.name}`} disabled={!isDesktopRuntime() || Boolean(busyAction)} onClick={(event) => { event.stopPropagation(); void runAction(`start:${environment.envId}`, () => startEnvironment(environment.envId)); }}>
                      {busyAction === `start:${environment.envId}` ? <LoaderCircle className="spin" size={15} /> : <Play size={15} />}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {filteredRows.length === 0 && (
          <div className="environment-empty" role="status">
            <CircleDot size={18} />
            <span>没有匹配环境</span>
          </div>
        )}
      </div>
      {selected && <EnvironmentDetail environment={selected} onClose={() => setSelectedEnvId(null)} />}
      </div>
    </section>
  );
}

function EnvironmentDetail({ environment, onClose }: {
  environment: DashboardSnapshot["environments"][number];
  onClose: () => void;
}) {
  const rows = [
    ["envId", environment.envId],
    ["状态", statusLabel[environment.status] ?? environment.status],
    ["Generation", String(environment.generation)],
    ["ReqId", environment.requestId === null ? "-" : String(environment.requestId)],
    ["Operation", environment.currentOperationId ?? "-"],
    ["CDP", environment.cdp],
    ["最后事件", environment.lastEvent],
    ["更新时间", new Date(environment.updatedAt).toLocaleString("zh-CN")],
  ];
  return (
    <aside className="environment-detail" aria-label="环境详情">
      <div className="detail-heading">
        <div><small>运行详情</small><h2>{environment.localLabel || environment.name}</h2></div>
        <button className="icon-button" type="button" title="关闭详情" aria-label="关闭详情" onClick={onClose}><X size={16} /></button>
      </div>
      <dl className="detail-list compact">
        {rows.map(([label, value]) => <div key={label}><dt>{label}</dt><dd title={value}>{value}</dd></div>)}
      </dl>
    </aside>
  );
}

function McpPage({ snapshot }: { snapshot: DashboardSnapshot | null }) {
  return (
    <section className="workspace single-panel">
      <div className="panel">
        <div className="panel-heading"><Bot size={17} /><h2>DLL 内嵌 MCP</h2></div>
        <dl className="detail-list">
          <div><dt>能力</dt><dd>{snapshot?.mcp.embeddedAvailable ? "available" : "unknown"}</dd></div>
          <div><dt>模式</dt><dd>{snapshot?.mcp.mode ?? "-"}</dd></div>
          <div><dt>端点</dt><dd>{snapshot?.mcp.endpointHint ?? "-"}</dd></div>
          <div><dt>路由</dt><dd>{snapshot?.mcp.managerRoute ?? "-"}</dd></div>
        </dl>
        <div className="note-list">
          {snapshot?.mcp.notes.map((note) => <span key={note}>{note}</span>)}
        </div>
      </div>
      <div className="panel">
        <div className="panel-heading"><Fingerprint size={17} /><h2>自动化边界</h2></div>
        <dl className="detail-list">
          <div><dt>工具参数</dt><dd>envId</dd></div>
          <div><dt>CDP</dt><dd>{snapshot?.capabilities.cdpCalls.join(", ") || "-"}</dd></div>
          <div><dt>HTTP/WS</dt><dd>{snapshot?.capabilities.embeddedWebApi ? "available" : "unknown"}</dd></div>
        </dl>
      </div>
    </section>
  );
}

function OperationsPage({ snapshot }: { snapshot: DashboardSnapshot | null }) {
  return (
    <section className="module-workspace">
      <div className="table-wrap">
        <table className="module-table">
          <thead><tr><th>操作</th><th>状态</th><th>信息</th><th>更新时间</th></tr></thead>
          <tbody>
            {(snapshot?.operations ?? []).map((operation) => (
              <tr key={operation.id}>
                <td>{operation.label}</td>
                <td><span className={`status-badge ${operation.status}`}>{statusLabel[operation.status] ?? operation.status}</span></td>
                <td>{operation.message}</td>
                <td>{new Date(operation.updatedAt).toLocaleString("zh-CN")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function SettingsPage({ snapshot }: { snapshot: DashboardSnapshot | null }) {
  return (
    <section className="workspace single-panel">
      <div className="panel">
        <div className="panel-heading"><Settings size={17} /><h2>设置</h2></div>
        <dl className="detail-list">
          <div><dt>API Key 来源</dt><dd>{snapshot?.sdk.apiKey.source ?? "BROSDK_API_KEY"}</dd></div>
          <div><dt>API Key 状态</dt><dd>{snapshot?.sdk.apiKey.present ? "present" : "missing"}</dd></div>
          <div><dt>SDK WorkDir</dt><dd>{snapshot?.sdk.workDir ?? "-"}</dd></div>
          <div><dt>扩展目录</dt><dd>{snapshot?.settings.extensionDir ?? "-"}</dd></div>
          <div><dt>日志目录</dt><dd>{snapshot?.settings.logDir ?? "-"}</dd></div>
          <div><dt>SDK API URL</dt><dd>{snapshot?.settings.sdkApiUrl ?? "默认"}</dd></div>
          <div><dt>Debug</dt><dd>{snapshot?.settings.debug ? "enabled" : "disabled"}</dd></div>
          <div><dt>SQLite</dt><dd>{snapshot?.databasePath ?? "-"}</dd></div>
          <div><dt>DLL</dt><dd>{snapshot?.sdk.dllPath ?? "-"}</dd></div>
        </dl>
      </div>
    </section>
  );
}
