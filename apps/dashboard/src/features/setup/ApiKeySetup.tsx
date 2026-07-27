import { useState, type FormEvent } from "react";
import { Eye, EyeOff, KeyRound, LayoutDashboard, LoaderCircle, LogOut, ShieldCheck } from "lucide-react";
import { actionTitle, desktopActionReason } from "../../actionTitles";

interface ApiKeySetupProps {
  mode: "first-run" | "settings";
  desktop: boolean;
  source: string;
  busy: boolean;
  error?: string;
  onSubmit: (apiKey: string) => Promise<void>;
  onClear?: () => Promise<void>;
  onPreview?: () => void;
}

export function ApiKeySetup({
  mode,
  desktop,
  source,
  busy,
  error,
  onSubmit,
  onClear,
  onPreview,
}: ApiKeySetupProps) {
  const [apiKey, setApiKey] = useState("");
  const [visible, setVisible] = useState(false);
  const managed = source === "environment";
  const credentialActionReason = desktopActionReason(desktop, busy, "凭据操作正在执行")
    || (managed ? "API Key 由环境变量管理" : "");
  const submitReason = credentialActionReason || (!apiKey.trim() ? "请输入 API Key" : "");

  async function submit(event: FormEvent) {
    event.preventDefault();
    const value = apiKey.trim();
    if (!value || busy || managed || !desktop) return;
    await onSubmit(value);
    setApiKey("");
  }

  const content = !desktop ? (
    <div className="credential-form">
      <p className="credential-state">浏览器预览模式</p>
      {mode === "first-run" && (
        <div className="credential-actions">
          <button className="button primary" type="button" title={actionTitle("进入工作台预览", onPreview ? "" : "预览不可用")} onClick={onPreview} disabled={!onPreview}>
            <LayoutDashboard size={16} />进入工作台预览
          </button>
        </div>
      )}
    </div>
  ) : (
    <form className="credential-form" onSubmit={(event) => void submit(event)}>
      <label className="field credential-field">
        <span>API Key</span>
        <div>
          <KeyRound size={16} />
          <input
            aria-label="API Key"
            autoComplete="off"
            autoFocus={mode === "first-run"}
            disabled={busy || managed}
            type={visible ? "text" : "password"}
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
          />
          <button
            className="credential-reveal"
            type="button"
            title={visible ? "隐藏 API Key" : "显示 API Key"}
            aria-label={visible ? "隐藏 API Key" : "显示 API Key"}
            disabled={busy || managed}
            onClick={() => setVisible((current) => !current)}
          >
            {visible ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        </div>
      </label>
      {error && <p className="credential-error" role="alert">{error}</p>}
      {managed && <p className="credential-state">由系统环境管理</p>}
      <div className="credential-actions">
        {mode === "settings" && onClear && (
          <button className="button secondary" type="button" title={actionTitle("移除 API Key", credentialActionReason)} disabled={busy || managed || !desktop} onClick={() => void onClear()}>
            <LogOut size={15} />移除
          </button>
        )}
        <button className="button primary" type="submit" title={actionTitle(mode === "first-run" ? "初始化" : "更换并初始化", submitReason)} disabled={!apiKey.trim() || busy || managed || !desktop}>
          {busy ? <LoaderCircle className="spin" size={16} /> : <ShieldCheck size={16} />}
          {mode === "first-run" ? "初始化" : "更换并初始化"}
        </button>
      </div>
    </form>
  );

  if (mode === "settings") return content;
  return (
    <main className="setup-screen">
      <section className="setup-panel" aria-labelledby="setup-title">
        <div className="setup-brand"><span><img src="/fingerprint.svg" alt="" /></span><strong>BroSDK</strong></div>
        <div className="setup-heading">
          <small>首次启动</small>
          <h1 id="setup-title">连接 SDK 服务</h1>
          <p>凭据由系统安全存储保护</p>
        </div>
        {content}
      </section>
    </main>
  );
}
