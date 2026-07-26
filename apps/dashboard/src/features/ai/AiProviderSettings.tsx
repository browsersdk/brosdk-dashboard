import { useEffect, useState } from "react";
import { CheckCircle2, KeyRound, LoaderCircle, ShieldCheck, Trash2 } from "lucide-react";
import { clearAiApiKey, configureAiProvider, isDesktopRuntime } from "../../api";
import type { DashboardSnapshot } from "../../types";

export function AiProviderSettings({ snapshot, onRefresh, onError }: {
  snapshot: DashboardSnapshot | null;
  onRefresh: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState("");

  useEffect(() => {
    if (!snapshot) return;
    setBaseUrl(snapshot.settings.aiBaseUrl ?? snapshot.ai.baseUrl);
    setModel(snapshot.settings.aiModel ?? snapshot.ai.model);
  }, [snapshot?.settings.aiBaseUrl, snapshot?.settings.aiModel, snapshot?.ai.baseUrl, snapshot?.ai.model]);

  async function save() {
    setBusy("save");
    onError("");
    try {
      await configureAiProvider({ baseUrl, model, apiKey: apiKey.trim() || null });
      setApiKey("");
      await onRefresh();
    } catch (requestError) {
      onError(errorMessage(requestError, "AI Provider 保存失败"));
    } finally {
      setBusy("");
    }
  }

  async function clearKey() {
    setBusy("clear");
    onError("");
    try {
      await clearAiApiKey();
      setApiKey("");
      await onRefresh();
    } catch (requestError) {
      onError(errorMessage(requestError, "AI API Key 清除失败"));
    } finally {
      setBusy("");
    }
  }

  const envManaged = snapshot?.ai.baseUrlSource === "environment" || snapshot?.ai.modelSource === "environment";
  const keyManaged = snapshot?.ai.apiKeySource === "environment";

  return (
    <div className="settings-section ai-provider-settings">
      <div className="section-heading">
        <div><ShieldCheck size={17} /><h2>AI Provider</h2></div>
        <button className="button primary compact" type="button" disabled={!isDesktopRuntime() || Boolean(busy)} onClick={() => void save()}>
          {busy === "save" ? <LoaderCircle className="spin" size={14} /> : <CheckCircle2 size={14} />}保存
        </button>
      </div>
      <div className="ai-provider-form">
        <label className="field"><span>OpenAI-compatible Base URL</span><input aria-label="OpenAI-compatible Base URL" value={baseUrl} disabled={snapshot?.ai.baseUrlSource === "environment"} onChange={(event) => setBaseUrl(event.target.value)} /></label>
        <label className="field"><span>Model</span><input aria-label="AI Model" value={model} disabled={snapshot?.ai.modelSource === "environment"} onChange={(event) => setModel(event.target.value)} /></label>
        <label className="field"><span>API Key</span><div className="secret-input"><KeyRound size={14} /><input aria-label="AI API Key" type="password" autoComplete="new-password" placeholder={keyManaged ? "由 BROSDK_AI_API_KEY 管理" : "留空则保留已保存密钥"} value={apiKey} disabled={keyManaged} onChange={(event) => setApiKey(event.target.value)} /></div></label>
        <div className="ai-provider-meta">
          <span>Base URL · {sourceLabel(snapshot?.ai.baseUrlSource)}</span>
          <span>Model · {sourceLabel(snapshot?.ai.modelSource)}</span>
          <span>API Key · {snapshot?.ai.apiKeyPresent ? sourceLabel(snapshot.ai.apiKeySource) : "未配置"}</span>
        </div>
      </div>
      <div className="ai-provider-actions">
        <small>API Key 仅保存在平台安全存储，不会写入 SQLite、事件或 AI 上下文。</small>
        <button className="button secondary compact" type="button" disabled={!isDesktopRuntime() || Boolean(busy) || keyManaged || !snapshot?.ai.apiKeyPresent} onClick={() => void clearKey()}>
          {busy === "clear" ? <LoaderCircle className="spin" size={14} /> : <Trash2 size={14} />}清除 API Key
        </button>
      </div>
      {envManaged && <p className="section-note">BROSDK_AI_BASE_URL / BROSDK_AI_MODEL 由部署环境覆盖。</p>}
    </div>
  );
}

function sourceLabel(source?: string) {
  if (source === "environment") return "环境变量";
  if (source === "secure-storage") return "安全存储";
  if (source === "settings") return "本地设置";
  if (source === "default") return "默认值";
  return "-";
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? error.message : fallback;
}
