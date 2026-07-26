import { useState, type FormEvent } from "react";
import { Eye, EyeOff, KeyRound, LoaderCircle, LogOut, ShieldCheck } from "lucide-react";

interface ApiKeySetupProps {
  mode: "first-run" | "settings";
  desktop: boolean;
  source: string;
  busy: boolean;
  error?: string;
  onSubmit: (apiKey: string) => Promise<void>;
  onClear?: () => Promise<void>;
}

export function ApiKeySetup({
  mode,
  desktop,
  source,
  busy,
  error,
  onSubmit,
  onClear,
}: ApiKeySetupProps) {
  const [apiKey, setApiKey] = useState("");
  const [visible, setVisible] = useState(false);
  const managed = source === "environment";

  async function submit(event: FormEvent) {
    event.preventDefault();
    const value = apiKey.trim();
    if (!value || busy || managed || !desktop) return;
    await onSubmit(value);
    setApiKey("");
  }

  const form = (
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
      {!desktop && <p className="credential-state">桌面预览模式</p>}
      {managed && <p className="credential-state">由系统环境管理</p>}
      <div className="credential-actions">
        {mode === "settings" && onClear && (
          <button className="button secondary" type="button" disabled={busy || managed || !desktop} onClick={() => void onClear()}>
            <LogOut size={15} />移除
          </button>
        )}
        <button className="button primary" type="submit" disabled={!apiKey.trim() || busy || managed || !desktop}>
          {busy ? <LoaderCircle className="spin" size={16} /> : <ShieldCheck size={16} />}
          {mode === "first-run" ? "初始化" : "更换并初始化"}
        </button>
      </div>
    </form>
  );

  if (mode === "settings") return form;
  return (
    <main className="setup-screen">
      <section className="setup-panel" aria-labelledby="setup-title">
        <div className="setup-brand"><span><img src="/fingerprint.svg" alt="" /></span><strong>BroSDK</strong></div>
        <div className="setup-heading">
          <small>首次启动</small>
          <h1 id="setup-title">连接 SDK 服务</h1>
          <p>凭据由系统安全存储保护</p>
        </div>
        {form}
      </section>
    </main>
  );
}
