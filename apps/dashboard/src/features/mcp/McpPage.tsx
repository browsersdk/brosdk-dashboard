import { useMemo, useState } from "react";
import {
  Bot,
  CircleDot,
  Database,
  LoaderCircle,
  LockKeyhole,
  Play,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { callEmbeddedMcp, discoverEmbeddedMcpTools } from "../../api";
import type {
  DashboardSnapshot,
  McpToolCallExecution,
  McpToolDiscovery,
  McpToolScope,
  McpToolSummary,
} from "../../types";

const toolLabels: Record<string, string> = {
  "sdk.health": "SDK 健康",
  "sdk.info": "SDK 信息",
  "env.list": "环境列表",
  "env.resolve": "定位环境",
  "env.get": "环境详情",
  "browser.status": "浏览器状态",
  "task.list": "任务列表",
  "task.get": "任务详情",
  "mcp.endpoint": "环境端点",
  browser_state: "浏览器空间",
  tabs: "标签页",
  snapshot: "页面快照",
  diff: "页面变化",
  read: "页面文本",
  grep: "页面搜索",
  screenshot: "页面截图",
};

type FormState = {
  page: string;
  pageSize: string;
  resolveQuery: string;
  taskLimit: string;
  taskId: string;
  browserStateAction: "get" | "wait";
  sinceSeq: string;
  timeoutMs: string;
  tabsAction: "list" | "current";
  domFallback: boolean;
  pattern: string;
  grepOver: "ax" | "content";
  screenshotFormat: "png" | "jpeg" | "webp";
};

const initialForm: FormState = {
  page: "1",
  pageSize: "50",
  resolveQuery: "",
  taskLimit: "50",
  taskId: "",
  browserStateAction: "get",
  sinceSeq: "0",
  timeoutMs: "10000",
  tabsAction: "list",
  domFallback: false,
  pattern: "",
  grepOver: "ax",
  screenshotFormat: "png",
};

export function McpPage({
  snapshot,
  desktop,
  onRefresh,
  onError,
}: {
  snapshot: DashboardSnapshot | null;
  desktop: boolean;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [scope, setScope] = useState<McpToolScope>("global");
  const [envId, setEnvId] = useState("");
  const [globalEnvId, setGlobalEnvId] = useState("");
  const [tool, setTool] = useState("sdk.health");
  const [form, setForm] = useState<FormState>(initialForm);
  const [discoveryState, setDiscoveryState] = useState<{ key: string; value: McpToolDiscovery } | null>(null);
  const [result, setResult] = useState<McpToolCallExecution | null>(null);
  const [outputView, setOutputView] = useState<"tools" | "response">("tools");
  const [busy, setBusy] = useState<"discover" | "run" | "">("");

  const environments = snapshot?.environments ?? [];
  const readyEnvironments = useMemo(
    () => environments.filter((environment) => environment.status === "ready"),
    [environments],
  );
  const selectedEnvId = readyEnvironments.some((environment) => environment.envId === envId)
    ? envId
    : readyEnvironments[0]?.envId ?? "";
  const selectedGlobalEnvId = tool === "browser.status" && globalEnvId === ""
    ? ""
    : environments.some((environment) => environment.envId === globalEnvId)
      ? globalEnvId
      : environments[0]?.envId ?? "";
  const discoveryKey = `${scope}:${scope === "environment" ? selectedEnvId : "global"}`;
  const discovery = discoveryState?.key === discoveryKey ? discoveryState.value : null;
  const policyTools = useMemo(() => toolsForScope(snapshot, scope), [snapshot, scope]);
  const availableTools = discovery?.allowedTools ?? policyTools;
  const selectedTool = availableTools.includes(tool) ? tool : availableTools[0] ?? "";
  const advertisedTools = discovery?.advertisedTools
    ?? availableTools.map((name) => ({ name, description: null, readOnlyHint: null, destructiveHint: null }));
  const canDiscover = desktop
    && Boolean(snapshot?.mcp.active)
    && !busy
    && (scope === "global" || Boolean(selectedEnvId));
  const arguments_ = buildArguments(selectedTool, form, selectedGlobalEnvId);
  const canRun = canDiscover && Boolean(selectedTool) && validArguments(selectedTool, form, selectedGlobalEnvId);

  function chooseScope(nextScope: McpToolScope) {
    setScope(nextScope);
    setTool(toolsForScope(snapshot, nextScope)[0] ?? "");
    setResult(null);
    setOutputView("tools");
  }

  async function discover() {
    if (!canDiscover) return;
    setBusy("discover");
    onError("");
    try {
      const next = await discoverEmbeddedMcpTools(
        scope,
        scope === "environment" ? selectedEnvId : null,
      );
      setDiscoveryState({ key: discoveryKey, value: next });
      setTool(next.allowedTools[0] ?? "");
      setOutputView("tools");
      await onRefresh();
    } catch (requestError) {
      onError(errorMessage(requestError, "MCP 工具发现失败"));
    } finally {
      setBusy("");
    }
  }

  async function runTool(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canRun) return;
    setBusy("run");
    onError("");
    try {
      const execution = await callEmbeddedMcp(
        scope,
        scope === "environment" ? selectedEnvId : null,
        selectedTool,
        arguments_,
      );
      setResult(execution);
      setOutputView("response");
      await onRefresh();
    } catch (requestError) {
      onError(errorMessage(requestError, "MCP 调用失败"));
    } finally {
      setBusy("");
    }
  }

  return (
    <section className="module-workspace mcp-console">
      <div className="module-toolbar mcp-toolbar">
        <div className="segmented-control" aria-label="MCP 作用域">
          <button type="button" className={scope === "global" ? "active" : ""} onClick={() => chooseScope("global")}>
            <Database size={14} />全局
          </button>
          <button type="button" className={scope === "environment" ? "active" : ""} onClick={() => chooseScope("environment")}>
            <Bot size={14} />单环境
          </button>
        </div>
        <div className="mcp-toolbar-status">
          <span className={`service-dot ${snapshot?.mcp.active ? "ready" : "error"}`} />
          <span>{snapshot?.mcp.active ? "DLL MCP 已连接" : "DLL MCP 未激活"}</span>
          <button className="button secondary compact" type="button" disabled={!canDiscover} onClick={() => void discover()}>
            {busy === "discover" ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}
            发现工具
          </button>
        </div>
      </div>

      <div className="mcp-console-body">
        <form className="mcp-builder" aria-label="MCP 只读调用" onSubmit={(event) => void runTool(event)}>
          <div className="mcp-section-heading">
            <div><ShieldCheck size={16} /><h2>只读调用</h2></div>
            <span>{availableTools.length} 个可用工具</span>
          </div>

          {scope === "environment" && (
            <label className="field">
              <span>运行环境</span>
              <select value={selectedEnvId} disabled={Boolean(busy)} onChange={(event) => setEnvId(event.target.value)}>
                <option value="">没有 ready 环境</option>
                {readyEnvironments.map((environment) => (
                  <option key={environment.envId} value={environment.envId}>{environment.name}</option>
                ))}
              </select>
            </label>
          )}

          <label className="field">
            <span>工具</span>
            <select value={selectedTool} disabled={Boolean(busy) || availableTools.length === 0} onChange={(event) => setTool(event.target.value)}>
              {availableTools.length === 0 && <option value="">未发现可用工具</option>}
              {availableTools.map((name) => <option key={name} value={name}>{toolLabel(name)} · {name}</option>)}
            </select>
          </label>

          <McpArgumentFields
            tool={selectedTool}
            form={form}
            environments={environments}
            selectedGlobalEnvId={selectedGlobalEnvId}
            disabled={Boolean(busy)}
            onFormChange={setForm}
            onGlobalEnvironmentChange={setGlobalEnvId}
          />

          <button className="button primary full-width mcp-run-button" type="submit" disabled={!canRun}>
            {busy === "run" ? <LoaderCircle className="spin" size={15} /> : <Play size={15} />}
            运行只读调用
          </button>
        </form>

        <div className="mcp-output">
          <div className="mcp-output-toolbar">
            <div className="segmented-control" aria-label="MCP 输出">
              <button type="button" className={outputView === "tools" ? "active" : ""} onClick={() => setOutputView("tools")}>工具状态</button>
              <button type="button" className={outputView === "response" ? "active" : ""} disabled={!result} onClick={() => setOutputView("response")}>响应</button>
            </div>
            <small>{discovery?.protocolVersion ?? result?.protocolVersion ?? "等待连接"}</small>
          </div>

          {outputView === "tools" ? (
            <ToolStatusList tools={advertisedTools} allowedTools={availableTools} discovered={Boolean(discovery)} />
          ) : result ? (
            <div className="mcp-result" aria-live="polite">
              <div className="operation-inline">
                <strong>{toolLabel(result.tool)} · {result.tool}</strong>
                <span className={`status-badge ${result.operation.status}`}>{operationStatus(result.operation.status)}</span>
                <small>{result.protocolVersion}</small>
              </div>
              <JsonPreview value={result.response} />
            </div>
          ) : (
            <div className="empty-state"><CircleDot size={22} /><span>尚无调用结果</span></div>
          )}
        </div>
      </div>
    </section>
  );
}

function McpArgumentFields({
  tool,
  form,
  environments,
  selectedGlobalEnvId,
  disabled,
  onFormChange,
  onGlobalEnvironmentChange,
}: {
  tool: string;
  form: FormState;
  environments: DashboardSnapshot["environments"];
  selectedGlobalEnvId: string;
  disabled: boolean;
  onFormChange: React.Dispatch<React.SetStateAction<FormState>>;
  onGlobalEnvironmentChange: (envId: string) => void;
}) {
  const update = <Key extends keyof FormState>(key: Key, value: FormState[Key]) => {
    onFormChange((current) => ({ ...current, [key]: value }));
  };
  const pageField = (
    <label className="field">
      <span>Page</span>
      <input type="number" min="1" inputMode="numeric" value={form.page} disabled={disabled} onChange={(event) => update("page", event.target.value)} />
    </label>
  );

  if (tool === "env.list") {
    return <div className="mcp-parameter-grid">{pageField}<label className="field"><span>每页数量</span><input type="number" min="1" max="200" inputMode="numeric" value={form.pageSize} disabled={disabled} onChange={(event) => update("pageSize", event.target.value)} /></label></div>;
  }
  if (tool === "env.resolve") {
    return <label className="field"><span>名称或 envId</span><input value={form.resolveQuery} disabled={disabled} maxLength={128} onChange={(event) => update("resolveQuery", event.target.value)} /></label>;
  }
  if (["env.get", "mcp.endpoint"].includes(tool)) {
    return <EnvironmentField environments={environments} value={selectedGlobalEnvId} disabled={disabled} onChange={onGlobalEnvironmentChange} />;
  }
  if (tool === "browser.status") {
    return <EnvironmentField environments={environments} value={selectedGlobalEnvId} disabled={disabled} optional onChange={onGlobalEnvironmentChange} />;
  }
  if (tool === "task.list") {
    return <label className="field"><span>任务数量</span><input type="number" min="1" max="100" inputMode="numeric" value={form.taskLimit} disabled={disabled} onChange={(event) => update("taskLimit", event.target.value)} /></label>;
  }
  if (tool === "task.get") {
    return <label className="field"><span>Task ID</span><input value={form.taskId} disabled={disabled} maxLength={128} onChange={(event) => update("taskId", event.target.value)} /></label>;
  }
  if (tool === "browser_state") {
    return <div className="mcp-parameter-grid"><label className="field"><span>Action</span><select value={form.browserStateAction} disabled={disabled} onChange={(event) => update("browserStateAction", event.target.value as FormState["browserStateAction"])}><option value="get">get</option><option value="wait">wait</option></select></label>{form.browserStateAction === "wait" && <><label className="field"><span>Since sequence</span><input type="number" min="0" inputMode="numeric" value={form.sinceSeq} disabled={disabled} onChange={(event) => update("sinceSeq", event.target.value)} /></label><label className="field"><span>Timeout (ms)</span><input type="number" min="1" max="30000" inputMode="numeric" value={form.timeoutMs} disabled={disabled} onChange={(event) => update("timeoutMs", event.target.value)} /></label></>}</div>;
  }
  if (tool === "tabs") {
    return <label className="field"><span>Action</span><select value={form.tabsAction} disabled={disabled} onChange={(event) => update("tabsAction", event.target.value as FormState["tabsAction"])}><option value="list">list</option><option value="current">current</option></select></label>;
  }
  if (["snapshot", "diff"].includes(tool)) {
    return <>{pageField}<label className="toggle-field mcp-toggle"><span><strong>DOM fallback</strong></span><input type="checkbox" checked={form.domFallback} disabled={disabled} onChange={(event) => update("domFallback", event.target.checked)} /></label></>;
  }
  if (tool === "read") return pageField;
  if (tool === "grep") {
    return <><div className="mcp-parameter-grid">{pageField}<label className="field"><span>搜索范围</span><select value={form.grepOver} disabled={disabled} onChange={(event) => update("grepOver", event.target.value as FormState["grepOver"])}><option value="ax">页面快照</option><option value="content">可见内容</option></select></label></div><label className="field"><span>搜索文本</span><input value={form.pattern} disabled={disabled} maxLength={256} onChange={(event) => update("pattern", event.target.value)} /></label></>;
  }
  if (tool === "screenshot") {
    return <div className="mcp-parameter-grid">{pageField}<label className="field"><span>格式</span><select value={form.screenshotFormat} disabled={disabled} onChange={(event) => update("screenshotFormat", event.target.value as FormState["screenshotFormat"])}><option value="png">PNG</option><option value="jpeg">JPEG</option><option value="webp">WebP</option></select></label></div>;
  }
  return null;
}

function EnvironmentField({
  environments,
  value,
  disabled,
  optional = false,
  onChange,
}: {
  environments: DashboardSnapshot["environments"];
  value: string;
  disabled: boolean;
  optional?: boolean;
  onChange: (envId: string) => void;
}) {
  return (
    <label className="field">
      <span>环境</span>
      <select value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)}>
        {optional && <option value="">全部环境</option>}
        {!optional && environments.length === 0 && <option value="">没有环境</option>}
        {environments.map((environment) => <option key={environment.envId} value={environment.envId}>{environment.name}</option>)}
      </select>
    </label>
  );
}

function ToolStatusList({ tools, allowedTools, discovered }: {
  tools: McpToolSummary[];
  allowedTools: string[];
  discovered: boolean;
}) {
  const allowed = new Set(allowedTools);
  return (
    <div className="mcp-tool-list" aria-live="polite">
      {tools.length === 0 && <div className="empty-state"><CircleDot size={22} /><span>没有可用工具</span></div>}
      {tools.map((tool) => {
        const isAllowed = allowed.has(tool.name);
        const label = toolLabel(tool.name);
        return (
          <div className={`mcp-tool-row ${isAllowed ? "allowed" : "blocked"}`} key={tool.name}>
            <span className="mcp-tool-icon">{isAllowed ? <ShieldCheck size={15} /> : <LockKeyhole size={15} />}</span>
            <div><strong>{label}</strong>{label !== tool.name && <code>{tool.name}</code>}</div>
            <span>{isAllowed ? (discovered ? "可调用" : "策略允许") : "策略保护"}</span>
          </div>
        );
      })}
    </div>
  );
}

function JsonPreview({ value }: { value: unknown }) {
  return <div className="json-preview"><div><strong>脱敏响应</strong></div><pre>{JSON.stringify(value ?? null, null, 2)}</pre></div>;
}

function toolsForScope(snapshot: DashboardSnapshot | null, scope: McpToolScope) {
  const prefix = `${scope}:`;
  return (snapshot?.mcp.allowedTools ?? [])
    .filter((name) => name.startsWith(prefix))
    .map((name) => name.slice(prefix.length));
}

function buildArguments(tool: string, form: FormState, envId: string): Record<string, unknown> {
  switch (tool) {
    case "env.list": return { page: Number(form.page), pageSize: Number(form.pageSize) };
    case "env.resolve": return { query: form.resolveQuery.trim() };
    case "env.get":
    case "mcp.endpoint": return { envId };
    case "browser.status": return envId ? { envId } : {};
    case "task.list": return { limit: Number(form.taskLimit) };
    case "task.get": return { taskId: form.taskId.trim() };
    case "browser_state": return form.browserStateAction === "wait"
      ? { action: "wait", sinceSeq: Number(form.sinceSeq), timeoutMs: Number(form.timeoutMs) }
      : { action: "get" };
    case "tabs": return { action: form.tabsAction };
    case "snapshot":
    case "diff": return { page: Number(form.page), domFallback: form.domFallback };
    case "read": return { page: Number(form.page) };
    case "grep": return { page: Number(form.page), pattern: form.pattern.trim(), over: form.grepOver };
    case "screenshot": return { page: Number(form.page), format: form.screenshotFormat };
    default: return {};
  }
}

function validArguments(tool: string, form: FormState, envId: string) {
  const positivePage = positiveInteger(form.page);
  switch (tool) {
    case "env.list": return positiveInteger(form.page) && boundedInteger(form.pageSize, 1, 200);
    case "env.resolve": return Boolean(form.resolveQuery.trim());
    case "env.get":
    case "mcp.endpoint": return Boolean(envId);
    case "task.list": return boundedInteger(form.taskLimit, 1, 100);
    case "task.get": return Boolean(form.taskId.trim());
    case "browser_state": return form.browserStateAction === "get"
      || (nonNegativeInteger(form.sinceSeq) && boundedInteger(form.timeoutMs, 1, 30_000));
    case "snapshot":
    case "diff":
    case "read":
    case "screenshot": return positivePage;
    case "grep": return positivePage && Boolean(form.pattern.trim());
    default: return true;
  }
}

function positiveInteger(value: string) {
  return boundedInteger(value, 1, Number.MAX_SAFE_INTEGER);
}

function nonNegativeInteger(value: string) {
  const number = Number(value);
  return Number.isSafeInteger(number) && number >= 0;
}

function boundedInteger(value: string, minimum: number, maximum: number) {
  const number = Number(value);
  return Number.isSafeInteger(number) && number >= minimum && number <= maximum;
}

function toolLabel(tool: string) {
  return toolLabels[tool] ?? tool;
}

function operationStatus(status: string) {
  return ({ succeeded: "已完成", failed: "失败", running: "执行中", queued: "排队中" } as Record<string, string>)[status] ?? status;
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}
