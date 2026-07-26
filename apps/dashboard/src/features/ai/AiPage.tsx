import { useEffect, useState } from "react";
import {
  Bot,
  BrainCircuit,
  CheckCircle2,
  Copy,
  LoaderCircle,
  Play,
  Settings,
} from "lucide-react";
import { aiChat, aiExecuteAgent, aiPlanAgent, isDesktopRuntime } from "../../api";
import type {
  AiAgentExecution,
  AiAgentPlan,
  AiChatResponse,
  DashboardSnapshot,
} from "../../types";
import { environmentCdpAddress, environmentCdpLabel, environmentControlChannel } from "../../environmentIdentity";

const statusLabel: Record<string, string> = {
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

export function AiPage({ snapshot, onRefresh, onError, onOpenSettings }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
  onOpenSettings: () => void;
}) {
  const [mode, setMode] = useState<"chat" | "agent">("chat");
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState("");
  const [selectedEnvId, setSelectedEnvId] = useState<string | null>(() => preferredEnvironmentId(snapshot));
  const [chatResponse, setChatResponse] = useState<AiChatResponse | null>(null);
  const [plan, setPlan] = useState<AiAgentPlan | null>(null);
  const [execution, setExecution] = useState<AiAgentExecution | null>(null);

  useEffect(() => {
    const environments = snapshot?.environments ?? [];
    if (selectedEnvId && environments.some((environment) => environment.envId === selectedEnvId)) return;
    setSelectedEnvId(preferredEnvironmentId(snapshot));
  }, [selectedEnvId, snapshot]);

  const environment = snapshot?.environments.find((item) => item.envId === selectedEnvId) ?? null;

  async function submit() {
    if (!prompt.trim()) return;
    setBusy("submit");
    onError("");
    try {
      if (mode === "chat") {
        setChatResponse(await aiChat(prompt, selectedEnvId));
        setPlan(null);
        setExecution(null);
      } else {
        setPlan(await aiPlanAgent(prompt, selectedEnvId));
        setChatResponse(null);
        setExecution(null);
      }
    } catch (requestError) {
      onError(errorMessage(requestError, "AI 请求失败"));
    } finally {
      setBusy("");
    }
  }

  async function executePlan() {
    if (!plan) return;
    setBusy("execute");
    onError("");
    try {
      setExecution(await aiExecuteAgent(plan));
      await onRefresh();
    } catch (requestError) {
      onError(errorMessage(requestError, "Agent 执行失败"));
    } finally {
      setBusy("");
    }
  }

  return (
    <section className="ai-workspace">
      <div className="module-toolbar ai-toolbar">
        <div className="segmented-control" aria-label="AI 模式">
          <button type="button" className={mode === "chat" ? "active" : ""} onClick={() => setMode("chat")}>Chat</button>
          <button type="button" className={mode === "agent" ? "active" : ""} onClick={() => setMode("agent")}>Agent</button>
        </div>
        <div className="ai-toolbar-provider">
          <div className="ai-provider-status">
            <span className={`service-dot ${snapshot?.ai.apiKeyPresent ? "ready" : "error"}`} />
            <strong>{snapshot?.ai.model ?? "-"}</strong>
            <small>{providerKeyLabel(snapshot?.ai.apiKeyPresent, snapshot?.ai.apiKeySource)}</small>
          </div>
          <button className="icon-button" type="button" title="AI Provider 设置" aria-label="AI Provider 设置" onClick={onOpenSettings}>
            <Settings size={15} />
          </button>
        </div>
      </div>

      <section className="ai-environment-context" aria-label="AI 环境详情">
        <div className="ai-context-selector">
          <label htmlFor="ai-context-environment">环境上下文</label>
          <select
            id="ai-context-environment"
            aria-label="AI 环境上下文"
            value={selectedEnvId ?? ""}
            onChange={(event) => setSelectedEnvId(event.target.value || null)}
          >
            {(snapshot?.environments ?? []).length === 0 && <option value="">无环境</option>}
            {(snapshot?.environments ?? []).map((item) => (
              <option key={item.envId} value={item.envId}>{item.name} · {item.envId}</option>
            ))}
          </select>
        </div>
        <dl className="ai-context-details">
          <ContextRow label="状态" value={environment ? statusLabel[environment.status] ?? environment.status : "-"} />
          <ContextRow label="envId" value={environment?.envId ?? "-"} mono />
          <ContextRow label="Generation" value={environment ? String(environment.generation) : "-"} />
          <ContextRow label="Request" value={environment?.requestId === null || environment?.requestId === undefined ? "-" : String(environment.requestId)} />
          <ContextRow label="Operation" value={environment?.currentOperationId ?? "-"} mono />
          <ContextRow label="最近事件" value={environment?.lastEvent || "-"} />
          <ContextRow label="控制通道" value={environment ? environmentControlChannel(environment) : "-"} />
          <div className="ai-context-cdp">
            <dt>CDP 地址</dt>
            <dd title={environment ? environmentCdpLabel(environment) : "-"}>
              <code>{environment ? environmentCdpLabel(environment) : "-"}</code>
              {environment && environmentCdpAddress(environment) && (
                <button className="icon-button" type="button" title="复制 CDP 地址" aria-label="复制 CDP 地址" onClick={() => void navigator.clipboard?.writeText(environmentCdpAddress(environment) ?? "")}>
                  <Copy size={13} />
                </button>
              )}
            </dd>
          </div>
        </dl>
      </section>

      <div className="ai-grid">
        <div className="panel ai-compose">
          <div className="panel-heading"><BrainCircuit size={17} /><h2>{mode === "chat" ? "只读 Chat" : "受控 Agent"}</h2></div>
          <textarea aria-label="AI 请求" value={prompt} onChange={(event) => setPrompt(event.target.value)} placeholder={mode === "chat" ? "询问当前环境、操作、能力或诊断状态" : "描述当前环境的启动、停止、同步或诊断目标"} />
          <button className="button primary" type="button" disabled={!isDesktopRuntime() || !prompt.trim() || Boolean(busy) || !snapshot?.ai.apiKeyPresent} onClick={() => void submit()}>
            {busy === "submit" ? <LoaderCircle className="spin" size={15} /> : <Play size={15} />}
            {mode === "chat" ? "发送" : "生成计划"}
          </button>
        </div>
        <div className="panel ai-result">
          <div className="panel-heading"><Bot size={17} /><h2>{mode === "chat" ? "回答" : "计划与执行"}</h2></div>
          {chatResponse && <div className="ai-answer"><p>{chatResponse.answer}</p><small>{chatResponse.model} · read-only</small></div>}
          {plan && <>
            <dl className="detail-list compact">
              <ContextRow label="Action" value={plan.action} />
              <ContextRow label="Environment" value={plan.envId ?? "-"} />
              <ContextRow label="Expected state" value={plan.expectedState ?? "-"} />
              <ContextRow label="Idempotency" value={plan.idempotencyKey} />
            </dl>
            <p className="agent-summary">{plan.summary}</p>
            <JsonPreview label="参数" value={plan.arguments} />
            <button className="button primary full-width" type="button" disabled={Boolean(busy)} onClick={() => void executePlan()}>
              {busy === "execute" ? <LoaderCircle className="spin" size={15} /> : <CheckCircle2 size={15} />}批准并执行
            </button>
          </>}
          {execution && <div className="agent-execution">
            <strong>{execution.operation ? `Operation ${execution.operation.id}` : execution.action}</strong>
            <p>{execution.statusSemantics}</p>
            {execution.operation && <span className={`status-badge ${execution.operation.status}`}>{statusLabel[execution.operation.status] ?? execution.operation.status}</span>}
          </div>}
          {!chatResponse && !plan && !execution && <div className="empty-state"><BrainCircuit size={22} /><span>等待请求</span></div>}
        </div>
      </div>
    </section>
  );
}

function preferredEnvironmentId(snapshot: DashboardSnapshot | null) {
  const environments = snapshot?.environments ?? [];
  return environments.find((environment) => environment.status === "ready")?.envId
    ?? environments[0]?.envId
    ?? null;
}

function providerKeyLabel(present?: boolean, source?: string) {
  if (!present) return "API Key 未配置";
  if (source === "environment") return "API Key · 环境变量";
  if (source === "secure-storage") return "API Key · 安全存储";
  return "API Key 已配置";
}

function ContextRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div><dt>{label}</dt><dd className={mono ? "mono" : ""} title={value}>{value}</dd></div>;
}

function JsonPreview({ label, value }: { label: string; value: unknown }) {
  return <div className="json-preview"><div><strong>{label}</strong><button className="icon-button" type="button" title="复制 JSON" aria-label={`复制${label}`} onClick={() => void navigator.clipboard?.writeText(JSON.stringify(value, null, 2))}><Copy size={14} /></button></div><pre>{JSON.stringify(value ?? null, null, 2)}</pre></div>;
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? error.message : fallback;
}
