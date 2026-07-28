import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  BrainCircuit,
  Boxes,
  Bot,
  CheckCircle2,
  CircleAlert,
  CircleDot,
  Copy,
  Database,
  Download,
  Fingerprint,
  FolderOpen,
  Gauge,
  HardDriveDownload,
  KeyRound,
  LoaderCircle,
  Monitor,
  Network,
  Play,
  Plus,
  RefreshCw,
  ServerCog,
  Settings,
  ShieldCheck,
  Square,
  Trash2,
  Search,
  SlidersHorizontal,
  TerminalSquare,
} from "lucide-react";
import {
  batchEnvironmentAction,
  cancelOperation,
  captureEnvironmentDiagnostic,
  cleanupKernelCache,
  cleanupEnvironmentLocalData,
  clearApiKey,
  configureApiKey,
  createEnvironment,
  createDiagnosticBundle,
  deleteProxyProfile,
  destroyEnvironment,
  diagnoseProxy,
  eventsSince,
  getSnapshot,
  installKernel,
  isDesktopRuntime,
  openFingerprintCheck,
  parseProxyUrl,
  pickDirectory,
  reconcileRuntimes,
  refreshEnvironmentDetail,
  refreshKernels,
  retryOperation,
  runSmoke,
  saveFile,
  saveProxyProfile,
  startEnvironment,
  stopEnvironment,
  syncEnvironments,
  systemProxyDiagnostics,
  uninstallKernel,
  updateSettings,
  updateEnvironmentMetadata,
} from "./api";
import { AiPage } from "./features/ai/AiPage";
import { AiProviderSettings } from "./features/ai/AiProviderSettings";
import { EnvironmentCreatePanel } from "./features/environments/EnvironmentCreatePanel";
import { EnvironmentBatchBar } from "./features/environments/EnvironmentBatchBar";
import { EnvironmentDetail } from "./features/environments/EnvironmentDetail";
import { FingerprintPage } from "./features/fingerprints/FingerprintPage";
import { McpPage } from "./features/mcp/McpPage";
import { OperationsPage } from "./features/operations/OperationsPage";
import { ApiKeySetup } from "./features/setup/ApiKeySetup";
import { actionTitle, desktopActionReason } from "./actionTitles";
import { environmentCdpLabel, environmentControlLabel, environmentLabel } from "./environmentIdentity";
import { environmentProgress } from "./environmentProgress";
import type {
  DashboardSnapshot,
  EnvironmentBatchAction,
  KernelRecord,
  ManagerSettings,
  OperationRecord,
  ProxyProfile,
  SmokeReport,
  SmokeStage,
  SmokeStageStatus,
} from "./types";

const navItems = [
  { key: "overview", label: "总览", icon: Activity },
  { key: "environments", label: "环境", icon: Boxes },
  { key: "fingerprints", label: "指纹", icon: Fingerprint },
  { key: "proxies", label: "代理", icon: Network },
  { key: "kernels", label: "内核", icon: HardDriveDownload },
  { key: "mcp", label: "MCP", icon: Bot },
  { key: "ai", label: "AI 助手", icon: BrainCircuit },
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

function mcpMetricValue(snapshot: DashboardSnapshot | null) {
  if (!snapshot) return "-";
  if (snapshot.mcp.active) return "已连接";
  if (!snapshot.capabilities.embeddedMcp) return "不可用";
  return snapshot.mcp.configured ? "待连接" : "未启用";
}

function mcpMetricDetail(snapshot: DashboardSnapshot | null) {
  const hint = snapshot?.mcp.endpointHint;
  if (!hint || hint === "not enabled") return snapshot?.mcp.active ? "已连接" : "端口未启用";
  return hint;
}

export default function App() {
  const [page, setPage] = useState<Page>(() => {
    const previewPage = new URLSearchParams(window.location.search).get("page");
    return navItems.some((item) => item.key === previewPage) ? previewPage as Page : "overview";
  });
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [smoke, setSmoke] = useState<SmokeReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [smokeBusy, setSmokeBusy] = useState(false);
  const [credentialBusy, setCredentialBusy] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setSnapshot(await getSnapshot());
      setError("");
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : "读取状态失败");
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
  const readyEnvironmentCount = snapshot?.environments.filter((environment) => environment.status === "ready").length ?? 0;

  const navigateToPage = useCallback((nextPage: Page, options: { replace?: boolean } = {}) => {
    setPage(nextPage);
    const url = new URL(window.location.href);
    if (url.searchParams.get("page") === nextPage) return;
    url.searchParams.set("page", nextPage);
    window.history[options.replace ? "replaceState" : "pushState"](null, "", url);
  }, []);

  useEffect(() => {
    const onPopState = () => {
      const nextPage = new URLSearchParams(window.location.search).get("page");
      setPage(navItems.some((item) => item.key === nextPage) ? nextPage as Page : "overview");
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  async function executeSmoke() {
    setSmokeBusy(true);
    setError("");
    try {
      const report = await runSmoke();
      setSmoke(report);
      await load();
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : "SDK 自检执行失败");
    } finally {
      setSmokeBusy(false);
    }
  }

  async function initialize(apiKey: string) {
    setCredentialBusy(true);
    setError("");
    try {
      await configureApiKey(apiKey);
      await load();
      navigateToPage("environments", { replace: true });
    } catch (requestError) {
      setError(errorMessage(requestError, "初始化失败"));
    } finally {
      setCredentialBusy(false);
    }
  }

  if (loading && !snapshot) {
    return <main className="setup-screen"><LoaderCircle className="spin" size={24} aria-label="读取客户端状态" /></main>;
  }

  if (snapshot && !snapshot.sdk.apiKey.present) {
    return (
      <ApiKeySetup
        mode="first-run"
        desktop={isDesktopRuntime()}
        source={snapshot.sdk.apiKey.source}
        busy={credentialBusy}
        error={error}
        onSubmit={initialize}
        onPreview={() => {
          const url = new URL(window.location.href);
          url.searchParams.set("preview", "workspace");
          window.location.assign(url);
        }}
      />
    );
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
              onClick={() => navigateToPage(item.key)}
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

      <main className={`main-content page-${page}`}>
        <header className="page-header">
          <div>
            <div className="breadcrumb"><span>本地客户端</span><strong>{navItems.find((item) => item.key === page)?.label}</strong></div>
            <h1>{page === "overview" ? "总览" : navItems.find((item) => item.key === page)?.label}</h1>
          </div>
          <div className="header-actions">
            {page !== "kernels" && <button className="button secondary" type="button" onClick={() => void load()} disabled={loading}>
              <RefreshCw className={loading ? "spin" : ""} size={16} />
              刷新
            </button>}
          </div>
        </header>

        {error && <div className="error-banner" role="alert"><CircleAlert size={17} /><span>{error}</span></div>}

        {page === "overview" && (
          <>
            <section className="summary-band" aria-label="运行概览">
              <Metric icon={ServerCog} tone="blue" label="SDK" value={statusLabel[snapshot?.sdk.state ?? ""] ?? "-"} detail={snapshot?.capabilities.dllExists ? "DLL present" : "DLL missing"} />
              <Metric icon={ShieldCheck} tone="green" label="API Key" value={snapshot?.sdk.apiKey.present ? "已设置" : "未设置"} detail={snapshot?.sdk.apiKey.source ?? "BROSDK_API_KEY"} />
              <Metric icon={Bot} tone="amber" label="内嵌 MCP" value={mcpMetricValue(snapshot)} detail={mcpMetricDetail(snapshot)} />
              <Metric icon={Boxes} tone="gray" label="环境" value={String(snapshot?.environments.length ?? 0)} detail={`${readyEnvironmentCount} 个运行中`} />
            </section>
            <section className="workspace overview-grid">
              <SdkPanel snapshot={snapshot} />
              <EnvironmentActivityPanel snapshot={snapshot} />
            </section>
          </>
        )}

        {page === "environments" && (
          <EnvironmentPage
            snapshot={snapshot}
            onRefresh={load}
            onError={(message) => setError(message)}
            onOpenKernels={() => navigateToPage("kernels")}
          />
        )}
        {page === "fingerprints" && <FingerprintPage snapshot={snapshot} onRefresh={load} onError={setError} />}
        {page === "proxies" && <ProxyPage snapshot={snapshot} onRefresh={load} onError={setError} />}
        {page === "kernels" && <KernelPage snapshot={snapshot} onRefresh={load} onError={setError} />}
        {page === "mcp" && <McpPage snapshot={snapshot} desktop={isDesktopRuntime()} onRefresh={load} onError={setError} />}
        {page === "ai" && <AiPage snapshot={snapshot} onRefresh={load} onError={setError} onOpenSettings={() => navigateToPage("settings")} />}
        {page === "operations" && <OperationsPage snapshot={snapshot} onRefresh={load} onError={setError} />}
        {page === "settings" && (
          <SettingsPage
            snapshot={snapshot}
            onRefresh={load}
            onError={setError}
            onCredentialChange={initialize}
            credentialBusy={credentialBusy}
            selfCheckReport={latestSmoke}
            selfCheckBusy={smokeBusy}
            onRunSelfCheck={executeSmoke}
          />
        )}
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

function EnvironmentRuntimeStatus({ status, lastEvent }: { status: string; lastEvent: string }) {
  const progress = status === "starting" ? environmentProgress(lastEvent) : null;
  return (
    <div className="environment-runtime-status">
      <span className={`status-badge ${status}`}>{statusLabel[status] ?? status}</span>
      {progress !== null && (
        <div className="environment-progress" role="progressbar" aria-label="环境启动进度" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress}>
          <span><i style={{ width: `${progress}%` }} /></span>
          <small>{progress}%</small>
        </div>
      )}
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
    ["支持状态", capabilities?.supportStatus ?? "-"],
    ["C ABI", capabilities?.cAbi ? "ready" : "unavailable"],
    ["动态库目录", capabilities?.libraryDir ?? "-"],
    ["IPC", capabilities?.ipcTransport ?? "-"],
    ["密钥后端", capabilities?.secretBackend ?? "-"],
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
      {capabilities?.unsupportedReason && <div className="note-list"><span>{capabilities.unsupportedReason}</span></div>}
    </section>
  );
}

function EnvironmentActivityPanel({ snapshot }: { snapshot: DashboardSnapshot | null }) {
  const environments = snapshot?.environments ?? [];
  const recentOperations = (snapshot?.operations ?? []).slice(0, 5);
  const statusCounts = [
    { label: "运行中", count: environments.filter((environment) => environment.status === "ready").length },
    { label: "变更中", count: environments.filter((environment) => ["starting", "stopping"].includes(environment.status)).length },
    { label: "已停止", count: environments.filter((environment) => environment.status === "stopped").length },
    { label: "需关注", count: environments.filter((environment) => !["ready", "starting", "stopping", "stopped"].includes(environment.status)).length },
  ];
  return (
    <section className="panel">
      <div className="panel-heading"><Activity size={17} /><h2>运行活动</h2></div>
      <div className="overview-status-grid">
        {statusCounts.map((item) => (
          <div key={item.label}><strong>{item.count}</strong><span>{item.label}</span></div>
        ))}
      </div>
      <div className="overview-operation-list">
        {recentOperations.length ? recentOperations.map((operation) => (
          <article key={operation.id}>
            <span className={`status-dot ${operation.status}`} />
            <div><strong>{operation.label}</strong><small>{operation.envId ?? "全局"}</small></div>
            <em>{statusLabel[operation.status] ?? operation.status}</em>
          </article>
        )) : (
          <div className="empty-state compact"><CircleDot size={20} /><span>暂无操作记录</span></div>
        )}
      </div>
    </section>
  );
}

function SelfCheckResult({ report }: { report: SmokeReport | null }) {
  if (!report) return null;
  const failed = report.stages.filter((stage) => stage.status === "failed").length;
  const passed = report.stages.filter((stage) => stage.status === "passed").length;
  return (
    <div className="self-check-result">
      <header><strong>最近自检</strong><span className={failed ? "failed" : "passed"}>{failed ? `${failed} 项失败` : `${passed}/${report.stages.length} 通过`}</span></header>
      <div className="stage-list">
        {report.stages.map((stage) => <StageRow key={stage.name} stage={stage} />)}
      </div>
      <footer className="panel-footer">
        <span>result cb {report.callbacks.result}</span>
        <span>log cb {report.callbacks.log}</span>
        <span>{report.embeddedMcpPort ? `MCP :${report.embeddedMcpPort}` : "MCP off"}</span>
      </footer>
    </div>
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

function EnvironmentPage({ snapshot, onRefresh, onError, onOpenKernels }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
  onOpenKernels: () => void;
}) {
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [selectedEnvId, setSelectedEnvId] = useState<string | null>(null);
  const [selectedEnvIds, setSelectedEnvIds] = useState<string[]>([]);
  const [busyAction, setBusyAction] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [diagnostics, setDiagnostics] = useState<Record<string, unknown>>({});
  const rows = useMemo(() => snapshot?.environments ?? [], [snapshot?.environments]);
  const filteredRows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return rows.filter((environment) => {
      const matchesStatus = status === "all" || environment.status === status;
      const matchesQuery = !normalized || [environment.name, environment.envId]
        .some((value) => value.toLocaleLowerCase().includes(normalized));
      return matchesStatus && matchesQuery;
    });
  }, [query, rows, status]);
  const selected = rows.find((environment) => environment.envId === selectedEnvId) ?? null;
  const selectedBinding = snapshot?.environmentBindings.find((binding) => binding.envId === selectedEnvId) ?? null;
  const cache = snapshot?.environmentCache;
  const cacheLabel = cache?.state === "fresh"
    ? `服务端 · ${cache.count}`
    : cache?.state === "stale"
      ? `缓存 · ${cache.count}`
      : "待同步";
  const cacheTitle = cache?.state === "fresh"
    ? `最近同步：${cache.lastSuccessAt ? new Date(cache.lastSuccessAt).toLocaleString("zh-CN") : "刚刚"}`
      : cache?.lastError ?? "尚未从 SDK 服务器同步环境";

  useEffect(() => {
    const available = new Set(rows.map((environment) => environment.envId));
    setSelectedEnvIds((current) => current.filter((envId) => available.has(envId)));
  }, [rows]);

  async function runAction<T>(action: string, callback: () => Promise<T>): Promise<T | null> {
    setBusyAction(action);
    try {
      const result = await callback();
      const value = result && typeof result === "object" ? result as Record<string, unknown> : null;
      const operationValue = value?.operation && typeof value.operation === "object"
        ? value.operation as Record<string, unknown>
        : value;
      if (operationValue?.status === "failed") {
        throw new Error(typeof operationValue.message === "string" ? operationValue.message : "环境操作失败");
      }
      await onRefresh();
      return result;
    } catch (requestError) {
      onError(requestError instanceof Error ? requestError.message : "环境操作失败");
      return null;
    } finally {
      setBusyAction("");
    }
  }

  async function create(input: Parameters<typeof createEnvironment>[0]) {
    setBusyAction("create");
    onError("");
    try {
      const operation = await createEnvironment(input);
      if (operation.status !== "succeeded") {
        onError(operation.message || "环境创建失败");
        return;
      }
      await onRefresh();
      setSelectedEnvId(operation.envId);
      setCreateOpen(false);
    } catch (requestError) {
      onError(errorMessage(requestError, "环境创建失败"));
    } finally {
      setBusyAction("");
    }
  }

  function toggleEnvironment(envId: string, checked: boolean) {
    setSelectedEnvIds((current) => {
      if (!checked) return current.filter((currentId) => currentId !== envId);
      if (current.includes(envId)) return current;
      if (current.length >= 20) {
        onError("每批最多选择 20 个环境");
        return current;
      }
      return [...current, envId];
    });
  }

  async function runBatch(action: EnvironmentBatchAction, envIds: string[]) {
    const result = await runAction(`batch:${action}`, () => batchEnvironmentAction(action, envIds));
    if (!result) return;
    if (result.failed > 0) {
      onError(`${result.failed} 个环境操作未被 SDK 接受`);
    }
    setSelectedEnvIds([]);
  }

  const selectableVisibleIds = filteredRows.slice(0, 20).map((environment) => environment.envId);
  const allVisibleSelected = selectableVisibleIds.length > 0
    && selectableVisibleIds.every((envId) => selectedEnvIds.includes(envId));
  const desktop = isDesktopRuntime();
  const environmentActionReason = desktopActionReason(desktop, Boolean(busyAction), "环境操作正在执行");

  return (
    <section className={`module-workspace environment-workspace ${selected ? "with-detail" : ""}`}>
      <div className="module-toolbar">
        <div className="toolbar-group">
          <span className="toolbar-title">环境列表</span>
          <span className={`cache-state ${cache?.state ?? "empty"}`} title={cacheTitle} aria-live="polite">
            <Database size={13} />{cacheLabel}
          </span>
          {!desktop && <span className="cache-state empty" title="浏览器预览只读，真实环境操作请使用桌面客户端"><Monitor size={13} />浏览器预览 · 只读</span>}
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
          <button className="button secondary compact" type="button" title={actionTitle("同步环境", environmentActionReason)} disabled={!desktop || Boolean(busyAction)} onClick={() => void runAction("sync", syncEnvironments)}>
            <RefreshCw className={busyAction === "sync" ? "spin" : ""} size={14} />同步
          </button>
          <button className="button secondary compact" type="button" title={actionTitle("对账运行态", environmentActionReason)} disabled={!desktop || Boolean(busyAction)} onClick={() => void runAction("reconcile", reconcileRuntimes)}>
            <Activity className={busyAction === "reconcile" ? "spin" : ""} size={14} />对账
          </button>
          <button className="button primary compact" type="button" title={actionTitle("新建环境", busyAction ? "环境操作正在执行" : "")} aria-expanded={createOpen} disabled={Boolean(busyAction)} onClick={() => { onError(""); setCreateOpen((open) => !open); }}>
            <Plus size={14} />新建环境
          </button>
        </div>
      </div>
      {createOpen && (
        <EnvironmentCreatePanel
          proxies={snapshot?.proxies ?? []}
          kernels={snapshot?.kernels ?? []}
          platform={snapshot?.capabilities.platform ?? ""}
          busy={busyAction === "create"}
          desktop={desktop}
          onCancel={() => setCreateOpen(false)}
          onOpenKernels={() => { setCreateOpen(false); onOpenKernels(); }}
          onCreate={create}
        />
      )}
      <EnvironmentBatchBar
        environments={rows}
        selectedIds={selectedEnvIds}
        desktop={desktop}
        busy={Boolean(busyAction)}
        onAction={(action, envIds) => void runBatch(action, envIds)}
        onClear={() => setSelectedEnvIds([])}
      />
      <div className="environment-body">
      <div className="table-wrap environment-table-wrap">
        <table className="module-table">
          <thead><tr><th className="selection-cell"><input type="checkbox" aria-label="选择当前结果（最多 20 个）" checked={allVisibleSelected} disabled={selectableVisibleIds.length === 0} onChange={(event) => setSelectedEnvIds(event.target.checked ? selectableVisibleIds : [])} /></th><th>环境</th><th>状态</th><th>CDP</th><th>最后事件</th><th aria-label="操作" /></tr></thead>
          <tbody>
            {filteredRows.map((environment) => {
              const startReason = environmentActionReason
                || (!["stopped", "failed"].includes(environment.status) ? "当前状态不能启动" : "");
              const stopReason = environmentActionReason;
              return (
              <tr key={environment.envId} data-env-id={environment.envId} className={selectedEnvId === environment.envId ? "selected" : ""} onClick={() => setSelectedEnvId(environment.envId)}>
                <td className="selection-cell"><input type="checkbox" aria-label={environmentControlLabel("选择", environment)} checked={selectedEnvIds.includes(environment.envId)} disabled={!selectedEnvIds.includes(environment.envId) && selectedEnvIds.length >= 20} onClick={(event) => event.stopPropagation()} onChange={(event) => toggleEnvironment(environment.envId, event.target.checked)} /></td>
                <td><div className="resource-name"><span className="resource-icon"><Boxes size={16} /></span><div><strong>{environment.name}</strong><small>{environment.envId}</small></div></div></td>
                <td><EnvironmentRuntimeStatus status={environment.status} lastEvent={environment.lastEvent} /></td>
                <td><code title={environment.cdp}>{environmentCdpLabel(environment)}</code></td>
                <td>{environment.lastEvent}</td>
                <td className="row-actions">
                  {environment.status === "ready" || environment.status === "starting" ? (
                    <button className="icon-button danger" type="button" title={actionTitle("停止环境", stopReason)} aria-label={environmentControlLabel("停止", environment)} disabled={!desktop || Boolean(busyAction)} onClick={(event) => { event.stopPropagation(); void runAction(`stop:${environment.envId}`, () => stopEnvironment(environment.envId)); }}>
                      {busyAction === `stop:${environment.envId}` ? <LoaderCircle className="spin" size={15} /> : <Square size={15} />}
                    </button>
                  ) : (
                    <button className="icon-button" type="button" title={actionTitle("启动环境", startReason)} aria-label={environmentControlLabel("启动", environment)} disabled={!desktop || Boolean(busyAction) || !["stopped", "failed"].includes(environment.status)} onClick={(event) => { event.stopPropagation(); void runAction(`start:${environment.envId}`, () => startEnvironment(environment.envId)); }}>
                      {busyAction === `start:${environment.envId}` ? <LoaderCircle className="spin" size={15} /> : <Play size={15} />}
                    </button>
                  )}
                </td>
              </tr>
              );
            })}
          </tbody>
        </table>
        {filteredRows.length === 0 && (
          <div className="environment-empty" role="status">
            <CircleDot size={18} />
            <span>没有匹配环境</span>
          </div>
        )}
      </div>
      {selected && (
        <EnvironmentDetail
          key={selected.envId}
          environment={selected}
          binding={selectedBinding}
          busy={Boolean(busyAction)}
          desktop={desktop}
          diagnostic={diagnostics[selected.envId] ?? null}
          onClose={() => setSelectedEnvId(null)}
          onStart={() => void runAction(`start:${selected.envId}`, () => startEnvironment(selected.envId))}
          onStop={() => void runAction(`stop:${selected.envId}`, () => stopEnvironment(selected.envId))}
          onRefresh={() => void runAction(`detail:${selected.envId}`, () => refreshEnvironmentDetail(selected.envId))}
          onUpdateMetadata={async (input) => Boolean(await runAction(
            `metadata:${selected.envId}`,
            () => updateEnvironmentMetadata({ envId: selected.envId, ...input }),
          ))}
          onOpenCheck={() => void runAction(`check:${selected.envId}`, () => openFingerprintCheck(selected.envId))}
          onCaptureDiagnostic={() => void (async () => {
            const result = await runAction(`diagnostic:${selected.envId}`, () => captureEnvironmentDiagnostic(selected.envId));
            if (result) setDiagnostics((current) => ({ ...current, [selected.envId]: result.response }));
          })()}
          onCleanupLocalData={() => void runAction(`cleanup:${selected.envId}`, () => cleanupEnvironmentLocalData(selected.envId))}
          onDelete={() => void (async () => {
            const result = await runAction(`delete:${selected.envId}`, () => destroyEnvironment(selected.envId));
            if (result) setSelectedEnvId(null);
          })()}
        />
      )}
      </div>
    </section>
  );
}

function ProxyPage({ snapshot, onRefresh, onError }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [boundEnvId, setBoundEnvId] = useState("");
  const [targetUrl, setTargetUrl] = useState("https://www.baidu.com");
  const [parseSummary, setParseSummary] = useState("");
  const [diagnostic, setDiagnostic] = useState<unknown>(null);
  const [busy, setBusy] = useState("");
  const selected = snapshot?.proxies.find((profile) => profile.id === selectedId) ?? null;
  const desktop = isDesktopRuntime();
  const proxyActionReason = desktopActionReason(desktop, Boolean(busy), "代理操作正在执行");
  const proxyUrlMissingReason = url.trim() ? "" : "请输入代理 URL";
  const diagnosticUrlMissingReason = targetUrl.trim() ? "" : "请输入诊断 URL";

  useEffect(() => {
    if (!selected) return;
    setName(selected.name);
    setUrl(proxyDisplayUrl(selected));
    setBoundEnvId(selected.boundEnvIds[0] ?? "");
    setParseSummary("");
  }, [selected]);

  async function run(action: string, callback: () => Promise<unknown>, capture = false) {
    setBusy(action);
    onError("");
    try {
      const result = await callback();
      if (capture) setDiagnostic(result);
      await onRefresh();
      return result;
    } catch (requestError) {
      onError(errorMessage(requestError, "代理操作失败"));
      return null;
    } finally {
      setBusy("");
    }
  }

  async function preview() {
    const parsed = await run("parse", () => parseProxyUrl(url));
    if (parsed) setParseSummary((parsed as Awaited<ReturnType<typeof parseProxyUrl>>).displayUrl);
  }

  async function save() {
    await run("save", () => saveProxyProfile({
      id: selected?.id,
      name: name.trim() || "未命名代理",
      url,
      boundEnvIds: boundEnvId ? [boundEnvId] : [],
    }));
    setSelectedId(null);
    setName("");
    setUrl("");
    setBoundEnvId("");
    setParseSummary("");
  }

  return (
    <section className="module-workspace resource-workspace">
      <div className="module-toolbar">
        <div className="toolbar-group"><span className="toolbar-title">代理档案</span></div>
        <div className="toolbar-group actions">
          <button className="button secondary compact" type="button" title={actionTitle("系统代理诊断", proxyActionReason)} disabled={!desktop || Boolean(busy)} onClick={() => void run("system", systemProxyDiagnostics, true)}><Gauge size={14} />系统代理</button>
          <button className="button primary compact" type="button" title={actionTitle("新建代理", busy ? "代理操作正在执行" : "")} disabled={Boolean(busy)} onClick={() => { setSelectedId(null); setName(""); setUrl(""); setBoundEnvId(""); }}><Network size={14} />新建</button>
        </div>
      </div>
      <div className="resource-body">
        <div className="table-wrap">
          <table className="module-table">
            <thead><tr><th>代理</th><th>协议</th><th>地址</th><th>凭据</th><th>绑定</th><th aria-label="操作" /></tr></thead>
            <tbody>{(snapshot?.proxies ?? []).map((profile) => (
              <tr key={profile.id} data-resource-id={profile.id} className={selectedId === profile.id ? "selected" : ""} onClick={() => setSelectedId(profile.id)}>
                <td><div className="resource-name"><span className="resource-icon"><Network size={16} /></span><div><strong>{profile.name}</strong><small>{profile.id}</small></div></div></td>
                <td>{profile.scheme.toUpperCase()}</td><td><code>{profile.host}:{profile.port}</code></td>
                <td>{profile.passwordPresent ? <span className="credential-state"><KeyRound size={13} />已保护</span> : "无"}</td>
                <td>{profile.boundEnvIds.length ? profile.boundEnvIds.join(", ") : "-"}</td>
                <td className="row-actions"><button className="icon-button danger" type="button" title={actionTitle("删除", proxyActionReason)} aria-label={`删除 ${profile.name}`} disabled={!desktop || Boolean(busy)} onClick={(event) => { event.stopPropagation(); void run(`delete:${profile.id}`, () => deleteProxyProfile(profile.id)); }}><Trash2 size={15} /></button></td>
              </tr>
            ))}</tbody>
          </table>
          {(snapshot?.proxies.length ?? 0) === 0 && <div className="environment-empty"><Network size={18} /><span>暂无代理档案</span></div>}
        </div>
        <aside className="resource-editor">
          <div className="panel-heading"><Network size={17} /><h2>{selected ? "编辑代理" : "新建代理"}</h2></div>
          <label className="field"><span>名称</span><input value={name} onChange={(event) => setName(event.target.value)} /></label>
          <label className="field"><span>代理 URL</span><input placeholder="socks5://user:pass@host:1080" value={url} onChange={(event) => { setUrl(event.target.value); setParseSummary(""); }} /></label>
          <div className="inline-form-actions"><button className="button secondary compact" type="button" title={actionTitle("解析代理 URL", proxyActionReason || proxyUrlMissingReason)} disabled={!desktop || !url.trim() || Boolean(busy)} onClick={() => void preview()}><Search size={14} />解析</button>{parseSummary && <code>{parseSummary}</code>}</div>
          <label className="field"><span>绑定环境</span><select value={boundEnvId} onChange={(event) => setBoundEnvId(event.target.value)}><option value="">不绑定</option>{snapshot?.environments.map((environment) => <option key={environment.envId} value={environment.envId}>{environmentLabel(environment)}</option>)}</select></label>
          <div className="form-actions"><button className="button primary" type="button" title={actionTitle("保存代理", proxyActionReason || proxyUrlMissingReason)} disabled={!desktop || !url.trim() || Boolean(busy)} onClick={() => void save()}><CheckCircle2 size={15} />保存</button></div>
          <div className="divider" />
          <label className="field"><span>诊断 URL</span><input value={targetUrl} onChange={(event) => setTargetUrl(event.target.value)} /></label>
          <button className="button secondary full-width" type="button" title={actionTitle("运行诊断", proxyActionReason || diagnosticUrlMissingReason)} disabled={!desktop || !targetUrl.trim() || Boolean(busy)} onClick={() => void run("diagnose", () => diagnoseProxy(selectedId, targetUrl), true)}>{busy === "diagnose" ? <LoaderCircle className="spin" size={15} /> : <Activity size={15} />}运行诊断</button>
          {diagnostic !== null && <JsonPreview label="诊断结果" value={diagnostic} />}
        </aside>
      </div>
    </section>
  );
}

function KernelPage({ snapshot, onRefresh, onError }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [busy, setBusy] = useState("");
  const [focusedOperationId, setFocusedOperationId] = useState<string | null>(null);
  const desktop = isDesktopRuntime();
  const kernelActionReason = desktopActionReason(desktop, Boolean(busy), "内核操作正在执行");
  const installOperations = useMemo(
    () => snapshot?.operations.filter((operation) => operation.kind === "kernel.install") ?? [],
    [snapshot?.operations],
  );
  const activeInstallOperation = installOperations.find(
    (operation) => operation.status === "queued" || operation.status === "running",
  ) ?? null;
  const focusedOperation = focusedOperationId
    ? installOperations.find((operation) => operation.id === focusedOperationId) ?? null
    : null;
  const visibleInstallOperation = focusedOperation ?? activeInstallOperation;
  const pendingInstallKernel = useMemo(
    () => snapshot?.kernels.find((kernel) => busy === `install:${kernel.id}`) ?? null,
    [busy, snapshot?.kernels],
  );
  const visibleInstallPanel = visibleInstallOperation
    ? {
      label: visibleInstallOperation.label,
      message: visibleInstallOperation.message || "等待 SDK 回调更新安装状态",
      status: visibleInstallOperation.status,
    }
    : pendingInstallKernel
      ? {
        label: "安装或更新内核",
        message: `${pendingInstallKernel.name} ${pendingInstallKernel.major ?? "未知版本"} · 已发送安装请求，等待 SDK 受理`,
        status: "queued",
      }
      : null;

  async function run(action: string, callback: () => Promise<unknown>) {
    setBusy(action); onError("");
    try {
      const result = await callback();
      if (isOperationRecord(result)) {
        setFocusedOperationId(result.id);
      }
      await onRefresh();
    }
    catch (requestError) { onError(errorMessage(requestError, "内核操作失败")); }
    finally { setBusy(""); }
  }
  return (
    <section className="module-workspace">
      <div className="module-toolbar">
        <div className="toolbar-group"><span className="toolbar-title">内核与缓存</span></div>
        <div className="toolbar-group actions">
          <button className="button secondary compact" type="button" title={actionTitle("清理缓存", kernelActionReason)} disabled={!desktop || Boolean(busy)} onClick={() => void run("cleanup", () => cleanupKernelCache(null))}><Trash2 size={14} />清理缓存</button>
          <button className="button primary compact" type="button" title={actionTitle("刷新内核", kernelActionReason)} disabled={!desktop || Boolean(busy)} onClick={() => void run("refresh", refreshKernels)}><RefreshCw className={busy === "refresh" ? "spin" : ""} size={14} />刷新</button>
        </div>
      </div>
      {visibleInstallPanel && (
        <div className="kernel-operation-panel" aria-live="polite" aria-label="内核安装进度">
          {visibleInstallPanel.status === "running" || visibleInstallPanel.status === "queued"
            ? <LoaderCircle className="spin" size={16} />
            : visibleInstallPanel.status === "failed"
              ? <CircleAlert size={16} />
              : <CheckCircle2 size={16} />}
          <div>
            <strong>{visibleInstallPanel.label}</strong>
            <span>{visibleInstallPanel.message}</span>
          </div>
          <span className={`status-badge ${visibleInstallPanel.status}`}>
            {statusLabel[visibleInstallPanel.status] ?? visibleInstallPanel.status}
          </span>
        </div>
      )}
      <div className="table-wrap">
        <table className="module-table kernel-table">
          <thead><tr><th>内核</th><th>主版本</th><th>本地版本</th><th>最新版本</th><th>平台</th><th>状态</th><th>下载源</th><th aria-label="操作" /></tr></thead>
          <tbody>{(snapshot?.kernels ?? []).map((kernel) => {
            const installOperation = installOperations.find((operation) => operationTargetsKernel(operation, kernel)
              && (operation.status === "queued" || operation.status === "running" || operation.id === focusedOperationId)) ?? null;
            const locallyInstalling = busy === `install:${kernel.id}`;
            const installing = locallyInstalling || installOperation?.status === "queued" || installOperation?.status === "running";
            const installProgressStatus = installOperation?.status ?? (locallyInstalling ? "queued" : "");
            const installProgressMessage = installOperation?.message ?? (locallyInstalling ? "已发送安装请求，等待 SDK 受理" : "");
            const installReason = !desktop
              ? kernelActionReason
              : installing
                ? `正在安装: ${installProgressMessage}`
                : kernelActionReason || (!kernel.downloadAvailable ? "下载源未知" : "");
            return (
              <tr key={kernel.id}>
                <td><div className="resource-name"><span className="resource-icon"><HardDriveDownload size={16} /></span><div><strong>{kernel.name}</strong><small>{kernel.kernelType}</small></div></div></td>
                <td>{kernel.major ?? "未知"}</td><td>{kernel.version ?? "未知"}</td><td>{kernel.latestVersion ?? "未知"}</td><td>{kernel.platform} / {kernel.arch}</td>
                <td className="kernel-state-cell">
                  <span className={`status-badge ${kernel.status}`}>{kernelStatus(kernel.status)}</span>
                  {(installOperation || locallyInstalling) && <small>{statusLabel[installProgressStatus] ?? installProgressStatus} · {installProgressMessage}</small>}
                </td>
                <td>{kernel.downloadAvailable ? "可用" : "未知"}</td>
                <td className="row-actions inline-actions">
                  {kernel.major !== null && <button className="icon-button" type="button" title={actionTitle("安装或更新", installReason)} aria-label={`安装 ${kernel.name}`} disabled={!desktop || !kernel.downloadAvailable || Boolean(busy) || installing} onClick={() => void run(`install:${kernel.id}`, () => installKernel(kernel.major!, kernel.kernelType))}>{installing ? <LoaderCircle className="spin" size={15} /> : <HardDriveDownload size={15} />}</button>}
                  {kernel.installPath && <button className="icon-button danger" type="button" title={actionTitle("卸载", kernelActionReason)} aria-label={`卸载 ${kernel.name}`} disabled={!desktop || Boolean(busy)} onClick={() => void run(`uninstall:${kernel.id}`, () => uninstallKernel(kernel.id))}><Trash2 size={15} /></button>}
                </td>
              </tr>
            );
          })}</tbody>
        </table>
        {(snapshot?.kernels.length ?? 0) === 0 && <div className="environment-empty"><HardDriveDownload size={18} /><span>尚未扫描内核</span></div>}
      </div>
    </section>
  );
}

function SettingsPage({ snapshot, onRefresh, onError, onCredentialChange, credentialBusy, selfCheckReport, selfCheckBusy, onRunSelfCheck }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
  onCredentialChange: (apiKey: string) => Promise<void>;
  credentialBusy: boolean;
  selfCheckReport: SmokeReport | null;
  selfCheckBusy: boolean;
  onRunSelfCheck: () => Promise<void>;
}) {
  const [settings, setSettings] = useState<ManagerSettings | null>(snapshot?.settings ?? null);
  const [busy, setBusy] = useState("");
  const selfCheckBlocked = (snapshot?.environments ?? []).some((environment) => environment.status !== "stopped");
  const desktop = isDesktopRuntime();
  const settingsActionReason = desktopActionReason(desktop, Boolean(busy), "设置操作正在执行");
  const diagnosticsReason = settingsActionReason || (selfCheckBusy ? "SDK 自检正在执行" : "");
  const selfCheckReason = diagnosticsReason
    || (selfCheckBlocked ? "需先停止全部环境并完成状态对账" : "");
  useEffect(() => { if (snapshot?.settings) setSettings(snapshot.settings); }, [snapshot?.settings]);
  if (!settings) return <section className="module-workspace"><div className="empty-state"><LoaderCircle className="spin" size={18} />读取设置</div></section>;

  async function run(action: string, callback: () => Promise<unknown>) {
    setBusy(action); onError("");
    try { await callback(); await onRefresh(); }
    catch (requestError) { onError(errorMessage(requestError, "设置操作失败")); }
    finally { setBusy(""); }
  }

  async function choose(key: "dataDir" | "workDir" | "extensionDir" | "logDir") {
    const currentSettings = settings;
    if (!currentSettings) return;
    const selected = await pickDirectory(currentSettings[key]);
    if (selected) setSettings((current) => current ? { ...current, [key]: selected } : current);
  }

  async function exportDiagnostics() {
    const path = await saveFile("brosdk-diagnostics.zip", "zip");
    if (path) await run("diagnostics", () => createDiagnosticBundle(path));
  }

  async function removeCredential() {
    await run("credential", clearApiKey);
  }

  return (
    <section className="settings-layout">
      <div className="settings-section">
        <div className="section-heading"><div><Settings size={17} /><h2>目录与运行</h2></div><button className="button primary compact" type="button" title={actionTitle("保存设置", settingsActionReason)} disabled={!desktop || Boolean(busy)} onClick={() => void run("save", () => updateSettings(settings))}><CheckCircle2 size={14} />保存设置</button></div>
        <div className="settings-form">
          <DirectoryField label="数据目录" value={settings.dataDir} onChange={(value) => setSettings({ ...settings, dataDir: value })} onPick={() => void choose("dataDir")} />
          <DirectoryField label="SDK WorkDir" value={settings.workDir} onChange={(value) => setSettings({ ...settings, workDir: value })} onPick={() => void choose("workDir")} />
          <DirectoryField label="扩展目录" value={settings.extensionDir} onChange={(value) => setSettings({ ...settings, extensionDir: value })} onPick={() => void choose("extensionDir")} />
          <DirectoryField label="日志目录" value={settings.logDir} onChange={(value) => setSettings({ ...settings, logDir: value })} onPick={() => void choose("logDir")} />
          <label className="field"><span>SDK API URL</span><input placeholder="默认 https://api.brosdk.com" value={settings.sdkApiUrl ?? ""} onChange={(event) => setSettings({ ...settings, sdkApiUrl: event.target.value || null })} /></label>
          <label className="field"><span>启动策略</span><select value={settings.startupPolicy} onChange={(event) => setSettings({ ...settings, startupPolicy: event.target.value })}><option value="restore-none">不恢复环境</option><option value="reconcile">启动后对账</option></select></label>
          <label className="field"><span>DLL MCP 端口</span><input inputMode="numeric" placeholder="留空则自动选择" value={settings.embeddedMcpPort ?? ""} onChange={(event) => setSettings({ ...settings, embeddedMcpPort: event.target.value ? Number(event.target.value) : null })} /></label>
          <label className="toggle-field"><span><strong>Debug 日志</strong><small>增加 SDK 与 Manager 诊断信息</small></span><input type="checkbox" checked={settings.debug} onChange={(event) => setSettings({ ...settings, debug: event.target.checked })} /></label>
        </div>
        <p className="section-note">数据目录变更会迁移 SQLite 与受保护凭据，并在下次启动生效。</p>
      </div>
      <div className="settings-section">
        <div className="section-heading"><div><ShieldCheck size={17} /><h2>安全与诊断</h2></div></div>
        <dl className="detail-list">
          <div><dt>API Key 来源</dt><dd>{credentialSourceLabel(snapshot?.sdk.apiKey.source)}</dd></div><div><dt>SDK 初始化</dt><dd>{snapshot?.sdk.initialized ? "已完成" : "待重试"}</dd></div><div><dt>SQLite</dt><dd>{snapshot?.databasePath ?? "-"}</dd></div><div><dt>DLL</dt><dd>{snapshot?.sdk.dllPath ?? "-"}</dd></div><div><dt>Host</dt><dd>{snapshot?.sdk.hostPath ?? "-"}</dd></div>
        </dl>
        <ApiKeySetup
          mode="settings"
          desktop={desktop}
          source={snapshot?.sdk.apiKey.source ?? "none"}
          busy={busy === "credential" || credentialBusy}
          onSubmit={onCredentialChange}
          onClear={removeCredential}
        />
        <div className="diagnostic-actions">
          <button className="button secondary" type="button" title={actionTitle("导出诊断包", diagnosticsReason)} disabled={!desktop || Boolean(busy) || selfCheckBusy} onClick={() => void exportDiagnostics()}>{busy === "diagnostics" ? <LoaderCircle className="spin" size={15} /> : <Download size={15} />}导出诊断包</button>
          <button className="button primary" type="button" title={actionTitle("运行 SDK 自检", selfCheckReason)} disabled={!desktop || Boolean(busy) || selfCheckBusy || selfCheckBlocked} onClick={() => void onRunSelfCheck()}>{selfCheckBusy ? <LoaderCircle className="spin" size={15} /> : <Play size={15} />}运行 SDK 自检</button>
        </div>
        {selfCheckBlocked && <p className="section-note self-check-blocked"><CircleAlert size={13} />需先停止全部环境并完成状态对账</p>}
        <SelfCheckResult report={selfCheckReport} />
      </div>
      <AiProviderSettings snapshot={snapshot} onRefresh={onRefresh} onError={onError} />
    </section>
  );
}

function DirectoryField({ label, value, onChange, onPick }: { label: string; value: string; onChange: (value: string) => void; onPick: () => void }) {
  const desktop = isDesktopRuntime();
  return <label className="field directory-field"><span>{label}</span><div><input value={value} onChange={(event) => onChange(event.target.value)} /><button className="icon-button" type="button" title={actionTitle(`选择${label}`, desktopActionReason(desktop, false))} aria-label={`选择${label}`} disabled={!desktop} onClick={onPick}><FolderOpen size={15} /></button></div></label>;
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd title={value}>{value}</dd></div>;
}

function JsonPreview({ label, value }: { label: string; value: unknown }) {
  return <div className="json-preview"><div><strong>{label}</strong><button className="icon-button" type="button" title="复制 JSON" aria-label={`复制${label}`} onClick={() => void navigator.clipboard?.writeText(JSON.stringify(value, null, 2))}><Copy size={14} /></button></div><pre>{JSON.stringify(value ?? null, null, 2)}</pre></div>;
}

function errorMessage(error: unknown, fallback: string) { return error instanceof Error ? error.message : fallback; }
function credentialSourceLabel(source?: string) { return ({ environment: "系统环境", "secure-storage": "系统安全存储", none: "未设置" } as Record<string, string>)[source ?? "none"] ?? "未知"; }
function formatTime(value: string) { return new Date(value).toLocaleString("zh-CN"); }
function proxyDisplayUrl(profile: ProxyProfile) { return `${profile.scheme}://${profile.username ? `${profile.username}@` : ""}${profile.host}:${profile.port}`; }
function kernelStatus(status: string) { return ({ installed: "已安装", available: "可安装", "update-available": "可更新", unknown: "未知" } as Record<string, string>)[status] ?? status; }

function isOperationRecord(value: unknown): value is OperationRecord {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const candidate = value as Partial<OperationRecord>;
  return typeof candidate.id === "string"
    && typeof candidate.kind === "string"
    && typeof candidate.status === "string"
    && typeof candidate.message === "string";
}

function operationTargetsKernel(operation: OperationRecord, kernel: KernelRecord): boolean {
  const core = kernelInstallCore(operation);
  if (!core || kernel.major === null || core.major !== kernel.major) return false;
  return core.kernelType === null || core.kernelType === kernel.kernelType;
}

function kernelInstallCore(operation: OperationRecord): { major: number; kernelType: string | null } | null {
  const request = operation.request;
  if (!request || typeof request !== "object" || Array.isArray(request)) return null;
  const cores = (request as { cores?: unknown }).cores;
  if (!Array.isArray(cores) || cores.length === 0) return null;
  const core = cores[0];
  if (!core || typeof core !== "object" || Array.isArray(core)) return null;
  const major = Number((core as { major?: unknown }).major);
  if (!Number.isFinite(major)) return null;
  const type = (core as { type?: unknown }).type;
  return {
    major,
    kernelType: typeof type === "string" && type.trim() ? type : null,
  };
}
