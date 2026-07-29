import type {
  AiAgentExecution,
  AiAgentPlan,
  AiAgentRun,
  AiConversationTurn,
} from "../../types";

export type AiMode = "chat" | "agent";
export type AiExecutionMode = "manual" | "automatic";

export interface AiConversationMessage {
  id: string;
  role: "user" | "assistant";
  mode: AiMode;
  content: string;
  createdAt: string;
  error?: boolean;
  pending?: boolean;
  plan?: AiAgentPlan;
  execution?: AiAgentExecution;
  run?: AiAgentRun;
  executionAttempted?: boolean;
}

export interface AiConversation {
  id: string;
  title: string;
  mode: AiMode;
  executionMode: AiExecutionMode;
  contextEnvId: string | null;
  createdAt: string;
  updatedAt: string;
  messages: AiConversationMessage[];
}

export interface AiConversationState {
  activeConversationId: string;
  conversations: AiConversation[];
}

const STORAGE_KEY = "brosdk-dashboard.ai-conversations.v1";
const PERSISTENCE_KEY = "brosdk-dashboard.ai-conversations.persistence.v1";
const MAX_CONVERSATIONS = 20;
const MAX_STORED_MESSAGES = 80;
const MAX_HISTORY_MESSAGES = 40;
let fallbackId = 0;

export function createConversation(
  contextEnvId: string | null,
  mode: AiMode = "chat",
  executionMode: AiExecutionMode = "manual",
): AiConversation {
  const now = new Date().toISOString();
  return {
    id: createId("conversation"),
    title: "新会话",
    mode,
    executionMode,
    contextEnvId,
    createdAt: now,
    updatedAt: now,
    messages: [],
  };
}

export function createConversationMessage(
  input: Omit<AiConversationMessage, "id" | "createdAt">,
): AiConversationMessage {
  return {
    ...input,
    id: createId("message"),
    createdAt: new Date().toISOString(),
  };
}

export function aiHistoryPersistenceEnabled() {
  try {
    return localStorage.getItem(PERSISTENCE_KEY) === "enabled";
  } catch {
    return false;
  }
}

export function setAiHistoryPersistence(enabled: boolean, state: AiConversationState) {
  try {
    if (enabled) {
      localStorage.setItem(PERSISTENCE_KEY, "enabled");
      writeConversationState(state);
    } else {
      localStorage.removeItem(PERSISTENCE_KEY);
      localStorage.removeItem(STORAGE_KEY);
    }
  } catch {
    // WebView storage can be unavailable; privacy mode remains in-memory.
  }
}

export function loadConversationState(
  defaultContextEnvId: string | null,
  options: { persistent?: boolean } = {},
): AiConversationState {
  if (options.persistent === true) {
    try {
      const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
      if (!isRecord(parsed)) {
        throw new Error("Invalid conversation state");
      }
      const conversations: AiConversation[] = Array.isArray(parsed.conversations)
        ? (parsed.conversations as unknown[]).map(parseConversation).filter(isConversation).slice(0, MAX_CONVERSATIONS)
        : [];
      if (conversations.length > 0) {
        const parsedActiveConversationId = stringValue(parsed.activeConversationId);
        let activeConversationId = conversations[0].id;
        if (
          parsedActiveConversationId
          && conversations.some((conversation) => conversation.id === parsedActiveConversationId)
        ) {
          activeConversationId = parsedActiveConversationId;
        }
        return { activeConversationId, conversations };
      }
    } catch {
      // Invalid or unavailable WebView storage starts a clean local conversation.
    }
  }
  if (options.persistent !== true) removeStoredConversationState();
  const conversation = createConversation(defaultContextEnvId);
  return { activeConversationId: conversation.id, conversations: [conversation] };
}

export function saveConversationState(state: AiConversationState, persistent = aiHistoryPersistenceEnabled()) {
  if (!persistent) {
    removeStoredConversationState();
    return;
  }
  writeConversationState(state);
}

function writeConversationState(state: AiConversationState) {
  try {
    const conversations = state.conversations.slice(0, MAX_CONVERSATIONS).map((conversation) => ({
      ...conversation,
      messages: conversation.messages.slice(-MAX_STORED_MESSAGES),
    }));
    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      activeConversationId: state.activeConversationId,
      conversations,
    }));
  } catch {
    // Conversation persistence is best effort and must not block AI requests.
  }
}

function removeStoredConversationState() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Storage cleanup is best effort.
  }
}

export function conversationHistory(messages: AiConversationMessage[]): AiConversationTurn[] {
  return messages
    .filter((message) => !message.error && message.content.trim())
    .slice(-MAX_HISTORY_MESSAGES)
    .map((message) => ({ role: message.role, content: message.content.trim() }));
}

export function conversationTitle(prompt: string) {
  const normalized = prompt.replace(/\s+/g, " ").trim();
  return normalized.length > 28 ? `${normalized.slice(0, 28)}...` : normalized || "新会话";
}

function createId(prefix: string) {
  const uuid = globalThis.crypto?.randomUUID?.();
  if (uuid) return `${prefix}-${uuid}`;
  fallbackId += 1;
  return `${prefix}-${Date.now()}-${fallbackId}`;
}

function parseConversation(value: unknown): AiConversation | null {
  if (!isRecord(value)) return null;
  const id = stringValue(value.id);
  const title = stringValue(value.title);
  const createdAt = stringValue(value.createdAt);
  const updatedAt = stringValue(value.updatedAt);
  if (!id || !title || !createdAt || !updatedAt) return null;
  const mode = value.mode === "agent" ? "agent" : "chat";
  const executionMode = value.executionMode === "automatic" ? "automatic" : "manual";
  const contextEnvId = typeof value.contextEnvId === "string" ? value.contextEnvId : null;
  const messages = Array.isArray(value.messages)
    ? value.messages.map(parseMessage).filter(isMessage).slice(-MAX_STORED_MESSAGES)
    : [];
  return { id, title, mode, executionMode, contextEnvId, createdAt, updatedAt, messages };
}

function parseMessage(value: unknown): AiConversationMessage | null {
  if (!isRecord(value)) return null;
  const id = stringValue(value.id);
  const content = stringValue(value.content);
  const createdAt = stringValue(value.createdAt);
  if (!id || !content || !createdAt || !matchesRole(value.role)) return null;
  const message: AiConversationMessage = {
    id,
    role: value.role,
    mode: value.mode === "agent" ? "agent" : "chat",
    content,
    createdAt,
  };
  if (value.error === true) message.error = true;
  if (isAgentPlan(value.plan)) message.plan = value.plan;
  if (isAgentExecution(value.execution)) message.execution = value.execution;
  if (isAgentRun(value.run)) message.run = value.run;
  if (value.executionAttempted === true) message.executionAttempted = true;
  return message;
}

function isAgentPlan(value: unknown): value is AiAgentPlan {
  return isRecord(value)
    && typeof value.summary === "string"
    && typeof value.action === "string"
    && typeof value.idempotencyKey === "string";
}

function isAgentExecution(value: unknown): value is AiAgentExecution {
  return isRecord(value)
    && typeof value.action === "string"
    && typeof value.statusSemantics === "string"
    && typeof value.replayed === "boolean";
}

function isAgentRun(value: unknown): value is AiAgentRun {
  return isRecord(value)
    && typeof value.answer === "string"
    && typeof value.model === "string"
    && Array.isArray(value.steps)
    && value.steps.every((step) => (
      isRecord(step) && isAgentPlan(step.plan) && isAgentExecution(step.execution)
    ));
}

function isConversation(value: AiConversation | null): value is AiConversation {
  return value !== null;
}

function isMessage(value: AiConversationMessage | null): value is AiConversationMessage {
  return value !== null;
}

function matchesRole(value: unknown): value is AiConversationMessage["role"] {
  return value === "user" || value === "assistant";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value : null;
}
