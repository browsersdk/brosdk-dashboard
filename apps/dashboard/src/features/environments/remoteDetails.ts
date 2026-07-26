export type RemoteRecord = Record<string, unknown>;

export interface DetailRow {
  key: string;
  label: string;
  value: unknown;
}

export interface DetailGroup {
  title: string;
  rows: DetailRow[];
}

export function asRemoteRecord(value: unknown): RemoteRecord {
  if (value !== null && typeof value === "object" && !Array.isArray(value)) return value as RemoteRecord;
  if (typeof value !== "string" || value.length > 200_000) return {};
  const trimmed = value.trim();
  if (!trimmed.startsWith("{")) return {};
  try {
    const parsed: unknown = JSON.parse(trimmed);
    return parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)
      ? parsed as RemoteRecord
      : {};
  } catch {
    return {};
  }
}

export function readRemoteValue(value: unknown, keys: string[]): unknown {
  const record = asRemoteRecord(value);
  for (const key of keys) {
    if (record[key] !== undefined && record[key] !== null && record[key] !== "") return record[key];
  }
  return null;
}

export function formatRemoteValue(value: unknown): string {
  if (value === undefined || value === null || value === "") return "-";
  if (typeof value === "boolean") return value ? "是" : "否";
  if (Array.isArray(value)) return value.map(formatRemoteValue).join(", ");
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

const fingerprintModes: Record<string, Record<string, string>> = {
  canvas: { "0": "一致性", "1": "真实", "2": "随机", "3": "关闭", "4": "一致性" },
  webGl: { "0": "真实", "1": "隐身", "2": "真实" },
  webGlInfo: { "0": "关闭", "1": "真实", "2": "自定义", "3": "自动生成" },
  webRTC: { "0": "禁用", "1": "真实 IP", "2": "代理 IP", "3": "隐身", "4": "转发", "5": "禁用" },
  audioContext: { "0": "真实", "1": "隐身", "2": "真实" },
  fontFinger: { "0": "真实", "1": "隐身", "2": "真实" },
  clientRects: { "0": "真实", "1": "隐身", "2": "真实" },
  speechVoices: { "1": "真实", "2": "噪声" },
  mediaDevice: { "1": "真实", "2": "噪声" },
  doNotTrack: { "1": "启用", "2": "默认", "3": "不启用" },
};

function compactFingerprintObject(value: unknown): string {
  if (value === null || typeof value !== "object") return "-";
  return Object.keys(value as Record<string, unknown>).length > 0 ? "已配置" : "-";
}

export function formatFingerprintValue(key: string, value: unknown): string {
  if (value === undefined || value === null || value === "") return "-";
  const mode = fingerprintModes[key]?.[String(value)];
  if (mode) return mode;
  if (Array.isArray(value)) {
    return value.every((item) => item === null || ["string", "number", "boolean"].includes(typeof item))
      ? value.map((item) => formatFingerprintValue(key, item)).join(", ")
      : `${value.length} 项`;
  }
  if (typeof value === "object") return compactFingerprintObject(value);
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
      try {
        const parsed: unknown = JSON.parse(trimmed);
        if (parsed !== null && typeof parsed === "object") return compactFingerprintObject(parsed);
      } catch {
        // Keep malformed server text as-is instead of exposing a parser error.
      }
    }
  }
  return String(value);
}

export function remoteProxyLabel(value: unknown): string {
  const displayUrl = readRemoteValue(value, ["displayUrl"]);
  if (displayUrl !== null) return formatRemoteValue(displayUrl);
  return readRemoteValue(value, ["configured", "passwordPresent"]) ? "已配置" : "-";
}

const groupDefinitions: Array<{
  title: string;
  fields: Array<[string, string, string[]]>;
}> = [
  {
    title: "浏览器与系统",
    fields: [
      ["system", "系统", ["system", "os"]],
      ["platform", "平台", ["platform"]],
      ["ua", "User Agent", ["ua", "userAgent"]],
      ["uaVersion", "UA 版本", ["uaVersion", "browserVersion", "appVersion"]],
      ["language", "语言", ["language", "languages"]],
      ["zone", "时区", ["zone", "timezone", "timeZone"]],
    ],
  },
  {
    title: "设备",
    fields: [
      ["dpi", "屏幕", ["dpi", "screen", "screenResolution"]],
      ["cpu", "CPU", ["cpu", "hardwareConcurrency"]],
      ["mem", "内存", ["mem", "deviceMemory"]],
      ["doNotTrack", "Do Not Track", ["doNotTrack"]],
    ],
  },
  {
    title: "指纹表面",
    fields: [
      ["canvas", "Canvas", ["canvas"]],
      ["webGl", "WebGL", ["webGl", "webGL"]],
      ["webGlVendor", "WebGL Vendor", ["webGlVendor", "webGLVendor"]],
      ["webGlRenderer", "WebGL Renderer", ["webGlRenderer", "webGLRenderer"]],
      ["webRTC", "WebRTC", ["webRTC", "webrtc"]],
      ["audioContext", "AudioContext", ["audioContext"]],
      ["fontFinger", "字体指纹", ["fontFinger"]],
      ["clientRects", "Client Rects", ["clientRects"]],
      ["speechVoices", "语音指纹", ["speechVoices"]],
      ["mediaDevice", "媒体设备", ["mediaDevice", "mediaDevices"]],
    ],
  },
];

export function fingerprintDetailGroups(value: unknown): DetailGroup[] {
  const record = asRemoteRecord(value);
  return groupDefinitions.map((group) => ({
    title: group.title,
    rows: group.fields.flatMap(([key, label, aliases]) => {
      const matched = aliases.find((alias) => record[alias] !== undefined && record[alias] !== null && record[alias] !== "");
      if (!matched) return [];
      return [{ key, label, value: formatFingerprintValue(key, record[matched]) }];
    }),
  })).filter((group) => group.rows.length > 0);
}
