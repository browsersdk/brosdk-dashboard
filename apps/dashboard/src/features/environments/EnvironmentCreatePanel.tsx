import { useEffect, useMemo, useState } from "react";
import { Boxes, HardDriveDownload, LoaderCircle, Plus, X } from "lucide-react";
import { actionTitle, desktopActionReason } from "../../actionTitles";
import type { EnvironmentCreateInput, KernelRecord, ProxyProfile } from "../../types";

const supportedKernelTypes = new Set(["chrome", "firefox", "chromium", "broium"]);

export function usableEnvironmentKernels(kernels: KernelRecord[], platform: string) {
  const normalizedPlatform = normalizePlatform(platform);
  return kernels
    .filter((kernel) => (
      kernel.major !== null
      && kernel.installPath !== null
      && (kernel.status === "installed" || kernel.status === "update-available")
      && normalizePlatform(kernel.platform) === normalizedPlatform
      && supportedKernelTypes.has(kernel.kernelType.toLocaleLowerCase())
    ))
    .sort((left, right) => (right.major ?? 0) - (left.major ?? 0));
}

export function EnvironmentCreatePanel({
  proxies,
  kernels,
  platform,
  busy,
  desktop,
  onCancel,
  onOpenKernels,
  onCreate,
}: {
  proxies: ProxyProfile[];
  kernels: KernelRecord[];
  platform: string;
  busy: boolean;
  desktop: boolean;
  onCancel: () => void;
  onOpenKernels: () => void;
  onCreate: (input: EnvironmentCreateInput) => void | Promise<void>;
}) {
  const availableKernels = useMemo(
    () => usableEnvironmentKernels(kernels, platform),
    [kernels, platform],
  );
  const [proxyProfileId, setProxyProfileId] = useState("");
  const [kernelId, setKernelId] = useState(availableKernels[0]?.id ?? "");
  const createActionReason = desktopActionReason(desktop, busy, "环境创建正在执行");
  const kernelRequiredReason = kernelId ? "" : "请选择内核版本";

  useEffect(() => {
    if (!availableKernels.some((kernel) => kernel.id === kernelId)) {
      setKernelId(availableKernels[0]?.id ?? "");
    }
  }, [availableKernels, kernelId]);

  useEffect(() => {
    if (proxyProfileId && !proxies.some((proxy) => proxy.id === proxyProfileId)) {
      setProxyProfileId("");
    }
  }, [proxies, proxyProfileId]);

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!desktop || !kernelId || busy) return;
    void onCreate({
      proxyProfileId: proxyProfileId || null,
      kernelId,
    });
  }

  return (
    <form
      className="environment-create-panel"
      aria-label="创建环境"
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === "Escape" && !busy) onCancel();
      }}
    >
      <div className="environment-create-heading">
        <span className="resource-icon"><Boxes size={16} /></span>
        <div>
          <h2>创建环境</h2>
          <small>{availableKernels.length} 个本地内核</small>
        </div>
        <button className="icon-button" type="button" title={actionTitle("关闭", busy ? "环境创建正在执行" : "")} aria-label="关闭创建面板" disabled={busy} onClick={onCancel}>
          <X size={15} />
        </button>
      </div>

      <label className="field environment-create-field">
        <span>代理</span>
        <select autoFocus value={proxyProfileId} disabled={busy} onChange={(event) => setProxyProfileId(event.target.value)}>
          <option value="">本机网络</option>
          {proxies.map((proxy) => (
            <option key={proxy.id} value={proxy.id}>{proxy.name} · {proxy.host}:{proxy.port}</option>
          ))}
        </select>
      </label>

      <label className="field environment-create-field">
        <span>内核版本</span>
        <select value={kernelId} disabled={busy || availableKernels.length === 0} onChange={(event) => setKernelId(event.target.value)}>
          {availableKernels.length === 0 && <option value="">未安装可用内核</option>}
          {availableKernels.map((kernel) => (
            <option key={kernel.id} value={kernel.id}>
              {kernelTypeLabel(kernel.kernelType)} {kernel.major} · {kernel.arch}
            </option>
          ))}
        </select>
      </label>

      <div className="environment-create-actions">
        {availableKernels.length === 0 ? (
          <button className="button secondary compact" type="button" title={actionTitle("前往内核", busy ? "环境创建正在执行" : "")} disabled={busy} onClick={onOpenKernels}>
            <HardDriveDownload size={14} />前往内核
          </button>
        ) : (
          <>
            <button className="button secondary compact" type="button" title={actionTitle("取消创建", busy ? "环境创建正在执行" : "")} disabled={busy} onClick={onCancel}>取消</button>
            <button className="button primary compact" type="submit" title={actionTitle("创建环境", createActionReason || kernelRequiredReason)} disabled={!desktop || !kernelId || busy}>
              {busy ? <LoaderCircle className="spin" size={14} /> : <Plus size={14} />}创建环境
            </button>
          </>
        )}
      </div>
    </form>
  );
}

function normalizePlatform(value: string) {
  const platform = value.trim().toLocaleLowerCase();
  if (["win", "win32", "windows"].includes(platform)) return "windows";
  if (["mac", "macos", "darwin"].includes(platform)) return "macos";
  return platform;
}

function kernelTypeLabel(value: string) {
  const normalized = value.toLocaleLowerCase();
  return ({ chrome: "Chrome", firefox: "Firefox", chromium: "Chromium", broium: "Broium" } as Record<string, string>)[normalized] ?? value;
}
