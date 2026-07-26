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

const sensitiveKey = /(cookie|storage|password|secret|token|^dek$)/i;

export function asRemoteRecord(value: unknown): RemoteRecord {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as RemoteRecord
    : {};
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

export function remoteProxyLabel(value: unknown): string {
  return formatRemoteValue(readRemoteValue(value, ["displayUrl"]) ?? value);
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
      ["deviceName", "设备名", ["deviceName"]],
      ["hardware", "硬件", ["hardware"]],
      ["mac", "MAC", ["mac"]],
      ["bluetooth", "蓝牙", ["bluetooth"]],
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
      ["webGlInfo", "WebGL Info", ["webGlInfo", "webGLInfo"]],
      ["webRTC", "WebRTC", ["webRTC", "webrtc"]],
      ["webRTCIP", "WebRTC IP", ["webRTCIP", "webrtcIP"]],
      ["audioContext", "AudioContext", ["audioContext"]],
      ["clientRects", "Client Rects", ["clientRects"]],
      ["font", "字体", ["font", "fonts"]],
      ["fontFinger", "字体指纹", ["fontFinger"]],
      ["speechVoices", "语音", ["speechVoices"]],
      ["mediaDevice", "媒体设备", ["mediaDevice", "mediaDevices"]],
      ["enableScanPort", "端口扫描", ["enableScanPort"]],
    ],
  },
];

export function fingerprintDetailGroups(value: unknown): DetailGroup[] {
  const record = asRemoteRecord(value);
  const usedKeys = new Set<string>();
  const groups = groupDefinitions.map((group) => ({
    title: group.title,
    rows: group.fields.flatMap(([key, label, aliases]) => {
      const matched = aliases.find((alias) => record[alias] !== undefined && record[alias] !== null && record[alias] !== "");
      if (!matched) return [];
      aliases.forEach((alias) => usedKeys.add(alias));
      return [{ key, label, value: record[matched] }];
    }),
  })).filter((group) => group.rows.length > 0);

  const otherRows = Object.entries(record)
    .filter(([key, item]) => !usedKeys.has(key) && !sensitiveKey.test(key) && item !== null && item !== "")
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, item]) => ({ key, label: key, value: item }));
  if (otherRows.length > 0) groups.push({ title: "其它", rows: otherRows });
  return groups;
}
