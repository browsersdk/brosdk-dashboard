import { useEffect, useState } from "react";
import {
  Bot,
  BrainCircuit,
  CheckCircle2,
  Copy,
  Eraser,
  Globe2,
  LoaderCircle,
  MessageSquarePlus,
  Monitor,
  SendHorizontal,
  Settings,
  Trash2,
  UserRound,
  X,
} from "lucide-react";
import { aiChat, aiExecuteAgent, aiPlanAgent, aiRunAgent, isDesktopRuntime } from "../../api";
import type {
  AiAgentExecution,
  AiAgentPlan,
  AiAgentRun,
  DashboardSnapshot,
} from "../../types";
import {
  environmentCdpAddress,
  environmentCdpLabel,
  environmentControlChannel,
} from "../../environmentIdentity";
import {
  conversationHistory,
  conversationTitle,
  createConversation,
  createConversationMessage,
  loadConversationState,
  saveConversationState,
  type AiConversation,
  type AiConversationMessage,
  type AiMode,
} from "./conversations";

const statusLabel: Record<string, string> = {
  stopped: "已停止",
  preparing: "准备中",
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
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState("");
  const [newConversationOpen, setNewConversationOpen] = useState(false);
  const [newConversationScope, setNewConversationScope] = useState<"global" | "environment">("global");
  const [newConversationEnvId, setNewConversationEnvId] = useState<string | null>(null);
  const [conversationState, setConversationState] = useState(() => (
    loadConversationState(null)
  ));
  const activeConversation = conversationState.conversations.find(
    (conversation) => conversation.id === conversationState.activeConversationId,
  ) ?? conversationState.conversations[0];
  const mode = activeConversation.mode;
  const selectedEnvId = activeConversation.contextEnvId;
  const environment = snapshot?.environments.find((item) => item.envId === selectedEnvId) ?? null;

  useEffect(() => {
    saveConversationState(conversationState);
  }, [conversationState]);

  function updateConversation(
    conversationId: string,
    update: (conversation: AiConversation) => AiConversation,
  ) {
    setConversationState((current) => ({
      ...current,
      conversations: current.conversations.map((conversation) => (
        conversation.id === conversationId ? update(conversation) : conversation
      )),
    }));
  }

  function appendMessage(conversationId: string, message: AiConversationMessage) {
    updateConversation(conversationId, (conversation) => ({
      ...conversation,
      title: conversation.messages.some((item) => item.role === "user")
        ? conversation.title
        : conversationTitle(message.content),
      updatedAt: message.createdAt,
      messages: [...conversation.messages, message].slice(-80),
    }));
  }

  function createNewConversation() {
    setNewConversationScope("global");
    setNewConversationEnvId(preferredEnvironmentId(snapshot));
    setNewConversationOpen(true);
  }

  function confirmNewConversation() {
    const contextEnvId = newConversationScope === "environment" ? newConversationEnvId : null;
    if (newConversationScope === "environment" && !contextEnvId) return;
    const conversation = createConversation(
      contextEnvId,
      mode,
      activeConversation.executionMode,
    );
    setConversationState((current) => ({
      activeConversationId: conversation.id,
      conversations: [conversation, ...current.conversations].slice(0, 20),
    }));
    setNewConversationOpen(false);
    setPrompt("");
    onError("");
  }

  function clearCurrentConversation() {
    updateConversation(activeConversation.id, (conversation) => ({
      ...conversation,
      title: "新会话",
      messages: [],
      updatedAt: new Date().toISOString(),
    }));
    setPrompt("");
    onError("");
  }

  function deleteConversation(conversationId: string) {
    setConversationState((current) => {
      const remaining = current.conversations.filter((conversation) => conversation.id !== conversationId);
      if (remaining.length === 0) {
        const replacement = createConversation(
          null,
          mode,
          activeConversation.executionMode,
        );
        return { activeConversationId: replacement.id, conversations: [replacement] };
      }
      return {
        activeConversationId: current.activeConversationId === conversationId
          ? remaining[0].id
          : current.activeConversationId,
        conversations: remaining,
      };
    });
  }

  function setMode(mode: AiMode) {
    updateConversation(activeConversation.id, (conversation) => ({ ...conversation, mode }));
  }

  function setExecutionMode(executionMode: AiConversation["executionMode"]) {
    updateConversation(activeConversation.id, (conversation) => ({
      ...conversation,
      executionMode,
    }));
  }

  async function submit() {
    const requestPrompt = prompt.trim();
    if (!requestPrompt) return;
    const conversationId = activeConversation.id;
    const history = conversationHistory(activeConversation.messages);
    const contextEnvId = activeConversation.contextEnvId;
    const requestMode = activeConversation.mode;
    const executionMode = activeConversation.executionMode;
    appendMessage(conversationId, createConversationMessage({
      role: "user",
      mode: requestMode,
      content: requestPrompt,
    }));
    setPrompt("");
    setBusy("submit");
    onError("");
    try {
      if (requestMode === "chat") {
        const response = await aiChat(requestPrompt, contextEnvId, history);
        appendMessage(conversationId, createConversationMessage({
          role: "assistant",
          mode: requestMode,
          content: response.answer,
        }));
      } else {
        if (executionMode === "automatic") {
          const run = await aiRunAgent(requestPrompt, contextEnvId, history);
          appendMessage(conversationId, createConversationMessage({
            role: "assistant",
            mode: requestMode,
            content: run.answer,
            run,
          }));
          await onRefresh();
        } else {
          const plan = await aiPlanAgent(requestPrompt, contextEnvId, history);
          appendMessage(conversationId, createConversationMessage({
            role: "assistant",
            mode: requestMode,
            content: plan.summary,
            plan,
          }));
        }
      }
    } catch (requestError) {
      const message = errorMessage(requestError, "AI 请求失败");
      appendMessage(conversationId, createConversationMessage({
        role: "assistant",
        mode: requestMode,
        content: message,
        error: true,
      }));
      onError(message);
    } finally {
      setBusy("");
    }
  }

  async function executePlan(message: AiConversationMessage) {
    if (!message.plan) return;
    await executePlanFor(activeConversation.id, message);
  }

  async function executePlanFor(
    conversationId: string,
    message: AiConversationMessage,
  ) {
    if (!message.plan) return;
    updateConversation(conversationId, (conversation) => ({
      ...conversation,
      updatedAt: new Date().toISOString(),
      messages: conversation.messages.map((item) => (
        item.id === message.id ? { ...item, executionAttempted: true } : item
      )),
    }));
    setBusy(`execute:${message.id}`);
    onError("");
    try {
      const execution = await aiExecuteAgent(message.plan, false);
      updateConversation(conversationId, (conversation) => ({
        ...conversation,
        updatedAt: new Date().toISOString(),
        messages: conversation.messages.map((item) => (
          item.id === message.id ? { ...item, execution } : item
        )),
      }));
      await onRefresh();
    } catch (requestError) {
      const error = errorMessage(requestError, "Agent 执行失败");
      appendMessage(conversationId, createConversationMessage({
        role: "assistant",
        mode: "agent",
        content: error,
        error: true,
      }));
      onError(error);
    } finally {
      setBusy("");
    }
  }

  return (
    <section className="ai-workspace">
      <div className="module-toolbar ai-toolbar">
        <div className="ai-mode-controls">
          <div className="segmented-control" aria-label="AI 模式">
            <button type="button" className={mode === "chat" ? "active" : ""} onClick={() => setMode("chat")}>Chat</button>
            <button type="button" className={mode === "agent" ? "active" : ""} onClick={() => setMode("agent")}>Agent</button>
          </div>
          {mode === "agent" && (
            <div className="segmented-control" aria-label="Agent 执行方式">
              <button type="button" disabled={Boolean(busy)} className={activeConversation.executionMode === "manual" ? "active" : ""} onClick={() => setExecutionMode("manual")}>每次批准</button>
              <button type="button" disabled={Boolean(busy)} className={activeConversation.executionMode === "automatic" ? "active" : ""} onClick={() => setExecutionMode("automatic")}>自动执行</button>
            </div>
          )}
        </div>
        <div className="ai-toolbar-provider">
          <button className="icon-button" type="button" title="清空当前会话" aria-label="清空当前会话" disabled={activeConversation.messages.length === 0 || Boolean(busy)} onClick={clearCurrentConversation}>
            <Eraser size={15} />
          </button>
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

      <div className="ai-conversation-layout">
        <aside className="ai-conversation-sidebar" aria-label="AI 会话历史">
          <div className="ai-conversation-heading">
            <strong>会话</strong>
            <button className="icon-button" type="button" title="新建会话" aria-label="新建会话" onClick={createNewConversation}>
              <MessageSquarePlus size={15} />
            </button>
          </div>
          <div className="ai-conversation-list">
            {conversationState.conversations.map((conversation) => (
              <div className={`ai-conversation-row ${conversation.id === activeConversation.id ? "active" : ""}`} key={conversation.id}>
                <button className="ai-conversation-select" type="button" onClick={() => setConversationState((current) => ({ ...current, activeConversationId: conversation.id }))}>
                  <strong>{conversation.title}</strong>
                  <small>{conversation.mode === "agent" ? "Agent" : "Chat"} · {conversation.contextEnvId ? "单环境" : "全局"} · {formatDate(conversation.updatedAt)}</small>
                </button>
                <button className="icon-button" type="button" title="删除会话" aria-label={`删除会话 ${conversation.title}`} disabled={Boolean(busy)} onClick={() => deleteConversation(conversation.id)}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
        </aside>

        <div className="ai-conversation-main">
          <section className="ai-environment-context" aria-label="AI 关联环境详情">
            <div className="ai-context-selector">
              <span>会话作用域</span>
              <div className="ai-context-scope" aria-label="AI 会话作用域">
                {selectedEnvId ? <Monitor size={15} /> : <Globe2 size={15} />}
                <div>
                  <strong>{selectedEnvId ? "单环境" : "全局"}</strong>
                  <small>{environment ? `${environment.name} · ${environment.envId}` : selectedEnvId ?? "全部环境"}</small>
                </div>
              </div>
            </div>
            {environment ? (
              <dl className="ai-context-details">
                <ContextRow label="状态" value={statusLabel[environment.status] ?? environment.status} />
                <ContextRow label="envId" value={environment.envId} mono />
                <ContextRow label="Operation" value={environment.currentOperationId ?? "-"} mono />
                <ContextRow label="最近事件" value={environment.lastEvent || "-"} />
                <ContextRow label="控制通道" value={environmentControlChannel(environment)} />
                <div className="ai-context-cdp">
                  <dt>CDP 地址</dt>
                  <dd title={environmentCdpLabel(environment)}>
                    <code>{environmentCdpLabel(environment)}</code>
                    {environmentCdpAddress(environment) && (
                      <button className="icon-button" type="button" title="复制 CDP 地址" aria-label="复制 CDP 地址" onClick={() => void navigator.clipboard?.writeText(environmentCdpAddress(environment) ?? "")}>
                        <Copy size={13} />
                      </button>
                    )}
                  </dd>
                </div>
              </dl>
            ) : selectedEnvId ? (
              <div className="ai-context-all"><BrainCircuit size={16} /><span>关联环境不可用</span></div>
            ) : null}
          </section>

          <div className="ai-message-list" aria-label="当前会话消息">
            {activeConversation.messages.map((message) => (
              <article
                className={`ai-message ${message.role} ${message.error ? "error" : ""}`}
                aria-label={message.role === "user" ? "用户消息" : message.error ? "AI 错误" : "AI 回复"}
                key={message.id}
              >
                <header>
                  {message.role === "user" ? <UserRound size={15} /> : <Bot size={15} />}
                  <strong>{message.role === "user" ? "你" : "AI"}</strong>
                  <small>{message.mode === "agent" ? "Agent" : "Chat"} · {formatDate(message.createdAt)}</small>
                </header>
                <p>{message.content}</p>
                {message.plan && (
                  <AgentPlanCard
                    plan={message.plan}
                    execution={message.execution}
                    attempted={message.executionAttempted}
                    busy={busy === `execute:${message.id}`}
                    disabled={Boolean(busy)}
                    onExecute={() => void executePlan(message)}
                  />
                )}
                {message.run && <AgentRunCard run={message.run} />}
              </article>
            ))}
            {activeConversation.messages.length === 0 && (
              <div className="empty-state ai-conversation-empty"><BrainCircuit size={22} /><span>当前会话为空</span></div>
            )}
          </div>

          <div className="ai-composer">
            <div className="ai-composer-shell">
              <textarea
                aria-label="AI 请求"
                value={prompt}
                onChange={(event) => setPrompt(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                    event.preventDefault();
                    void submit();
                  }
                }}
                placeholder={mode === "chat" ? "询问环境、操作、能力或诊断状态" : "描述环境启动、停止、同步或诊断目标"}
              />
              <button
                className="ai-send-button"
                type="button"
                title={mode === "chat" ? "发送" : activeConversation.executionMode === "automatic" ? "运行 Agent" : "生成计划"}
                aria-label={mode === "chat" ? "发送" : activeConversation.executionMode === "automatic" ? "运行 Agent" : "生成计划"}
                disabled={!isDesktopRuntime() || !prompt.trim() || Boolean(busy) || !snapshot?.ai.apiKeyPresent}
                onClick={() => void submit()}
              >
                {busy === "submit" ? <LoaderCircle className="spin" size={17} /> : <SendHorizontal size={17} />}
              </button>
            </div>
          </div>
        </div>
      </div>

      {newConversationOpen && (
        <div className="ai-dialog-backdrop">
          <section className="ai-new-conversation-dialog" role="dialog" aria-modal="true" aria-label="新建 AI 会话">
            <header>
              <div>
                <strong>新建会话</strong>
                <small>作用域创建后不可修改</small>
              </div>
              <button className="icon-button" type="button" title="关闭" aria-label="关闭新建会话" onClick={() => setNewConversationOpen(false)}>
                <X size={15} />
              </button>
            </header>
            <div className="segmented-control ai-scope-control" aria-label="新会话作用域">
              <button type="button" className={newConversationScope === "global" ? "active" : ""} onClick={() => setNewConversationScope("global")}><Globe2 size={14} />全局</button>
              <button type="button" className={newConversationScope === "environment" ? "active" : ""} onClick={() => setNewConversationScope("environment")}><Monitor size={14} />单环境</button>
            </div>
            {newConversationScope === "environment" && (
              <label className="ai-new-conversation-environment">
                <span>关联环境</span>
                <select aria-label="新会话关联环境" value={newConversationEnvId ?? ""} onChange={(event) => setNewConversationEnvId(event.target.value || null)}>
                  <option value="">选择环境</option>
                  {(snapshot?.environments ?? []).map((item) => (
                    <option key={item.envId} value={item.envId}>{item.name} · {item.envId}</option>
                  ))}
                </select>
              </label>
            )}
            <footer>
              <button className="button secondary" type="button" onClick={() => setNewConversationOpen(false)}>取消</button>
              <button className="button primary" type="button" disabled={newConversationScope === "environment" && !newConversationEnvId} onClick={confirmNewConversation}>创建</button>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}

function AgentRunCard({ run }: { run: AiAgentRun }) {
  return (
    <div className="agent-run" aria-label="Agent 自动执行步骤">
      <div className="agent-run-heading">
        <strong>{run.steps.length > 0 ? `${run.steps.length} 个步骤已执行` : "无需执行工具"}</strong>
        <small>{run.model}</small>
      </div>
      {run.steps.map((step, index) => (
        <div className="agent-run-step" key={step.plan.idempotencyKey}>
          <span>{index + 1}</span>
          <div>
            <strong>{step.plan.action}</strong>
            <small>{step.plan.envId ?? "全局"}</small>
          </div>
          <span className={`status-badge ${step.execution.operation?.status ?? "succeeded"}`}>
            {statusLabel[step.execution.operation?.status ?? "succeeded"] ?? step.execution.operation?.status}
          </span>
        </div>
      ))}
    </div>
  );
}

function AgentPlanCard({ plan, execution, attempted, busy, disabled, onExecute }: {
  plan: AiAgentPlan;
  execution?: AiAgentExecution;
  attempted?: boolean;
  busy: boolean;
  disabled: boolean;
  onExecute: () => void;
}) {
  return (
    <div className="agent-plan">
      <dl className="detail-list compact">
        <ContextRow label="动作" value={plan.action} />
        <ContextRow label="环境" value={plan.envId ?? "-"} mono />
        <ContextRow label="前置状态" value={plan.expectedState ?? "-"} />
      </dl>
      <JsonPreview label="参数" value={plan.arguments} />
      {!execution && !attempted && (
        <button className="button primary" type="button" disabled={disabled} onClick={onExecute}>
          {busy ? <LoaderCircle className="spin" size={15} /> : <CheckCircle2 size={15} />}
          批准并执行
        </button>
      )}
      {!execution && attempted && (
        <div className={`agent-execution ${busy ? "" : "error"}`}>
          <strong>{busy ? <><LoaderCircle className="spin" size={15} />正在执行</> : "执行未完成"}</strong>
        </div>
      )}
      {execution && (
        <div className="agent-execution">
          <strong>{execution.operation ? `Operation ${execution.operation.id}` : execution.action}</strong>
          <p>{execution.statusSemantics}</p>
          {execution.operation && <span className={`status-badge ${execution.operation.status}`}>{statusLabel[execution.operation.status] ?? execution.operation.status}</span>}
        </div>
      )}
    </div>
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

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "-";
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function ContextRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div><dt>{label}</dt><dd className={mono ? "mono" : ""} title={value}>{value}</dd></div>;
}

function JsonPreview({ label, value }: { label: string; value: unknown }) {
  return <div className="json-preview"><div><strong>{label}</strong><button className="icon-button" type="button" title="复制 JSON" aria-label={`复制${label}`} onClick={() => void navigator.clipboard?.writeText(JSON.stringify(value, null, 2))}><Copy size={14} /></button></div><pre>{JSON.stringify(value ?? null, null, 2)}</pre></div>;
}

function errorMessage(error: unknown, fallback: string) {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}
