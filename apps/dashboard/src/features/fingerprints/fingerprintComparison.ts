import type { DashboardSnapshot, EnvironmentBindingSummary } from "../../types";
import { formatFingerprintValue, formatRemoteValue, readRemoteValue } from "../environments/remoteDetails";

type Environment = DashboardSnapshot["environments"][number];

export type ComparisonState = "same" | "different" | "unknown";

export interface FingerprintComparisonRow {
  key: string;
  label: string;
  values: string[];
  state: ComparisonState;
}

export interface FingerprintComparisonGroup {
  title: string;
  rows: FingerprintComparisonRow[];
}

export interface FingerprintComparison {
  environments: Environment[];
  groups: FingerprintComparisonGroup[];
}

interface FieldDefinition {
  key: string;
  label: string;
  read: (binding: EnvironmentBindingSummary | undefined) => unknown;
}

interface GroupDefinition {
  title: string;
  fields: FieldDefinition[];
}

function fingerprint(keys: string[]): FieldDefinition["read"] {
  return (binding) => readRemoteValue(binding?.remoteFingerprint, keys);
}

const definitions: GroupDefinition[] = [
  {
    title: "环境概要",
    fields: [
      {
        key: "kernel",
        label: "内核",
        read: (binding) => {
          const parts = [
            readRemoteValue(binding?.remoteKernel, ["kernel"]),
            readRemoteValue(binding?.remoteKernel, ["version"]),
          ].filter((value) => value !== null);
          return parts.length > 0 ? parts.join(" ") : null;
        },
      },
      {
        key: "system",
        label: "系统",
        read: (binding) => readRemoteValue(binding?.remoteKernel, ["system"])
          ?? readRemoteValue(binding?.remoteFingerprint, ["system", "os"]),
      },
      {
        key: "proxy",
        label: "代理",
        read: (binding) => readRemoteValue(binding?.remoteProxy, ["displayUrl"]),
      },
      {
        key: "serial",
        label: "序列号",
        read: (binding) => readRemoteValue(binding?.remoteMetadata, ["serial"]),
      },
    ],
  },
  {
    title: "浏览器与系统",
    fields: [
      { key: "platform", label: "平台", read: fingerprint(["platform"]) },
      { key: "ua", label: "User Agent", read: fingerprint(["ua", "userAgent"]) },
      { key: "uaVersion", label: "UA 版本", read: fingerprint(["uaVersion", "browserVersion", "appVersion"]) },
      { key: "language", label: "语言", read: fingerprint(["language", "languages"]) },
      { key: "zone", label: "时区", read: fingerprint(["zone", "timezone", "timeZone"]) },
    ],
  },
  {
    title: "设备",
    fields: [
      { key: "dpi", label: "屏幕", read: fingerprint(["dpi", "screen", "screenResolution"]) },
      { key: "cpu", label: "CPU", read: fingerprint(["cpu", "hardwareConcurrency"]) },
      { key: "mem", label: "内存", read: fingerprint(["mem", "deviceMemory"]) },
      { key: "doNotTrack", label: "Do Not Track", read: fingerprint(["doNotTrack"]) },
    ],
  },
  {
    title: "指纹表面",
    fields: [
      { key: "canvas", label: "Canvas", read: fingerprint(["canvas"]) },
      { key: "webGl", label: "WebGL", read: fingerprint(["webGl", "webGL"]) },
      { key: "webGlVendor", label: "WebGL Vendor", read: fingerprint(["webGlVendor", "webGLVendor"]) },
      { key: "webGlRenderer", label: "WebGL Renderer", read: fingerprint(["webGlRenderer", "webGLRenderer"]) },
      { key: "webRTC", label: "WebRTC", read: fingerprint(["webRTC", "webrtc"]) },
      { key: "audioContext", label: "AudioContext", read: fingerprint(["audioContext"]) },
      { key: "fontFinger", label: "字体指纹", read: fingerprint(["fontFinger"]) },
    ],
  },
];

export function buildFingerprintComparison(
  environments: Environment[],
  bindings: EnvironmentBindingSummary[],
  selectedEnvIds: string[],
): FingerprintComparison {
  const selected = new Set(selectedEnvIds.slice(0, 4));
  const columns = environments.filter((environment) => selected.has(environment.envId));
  const byEnvId = new Map(bindings.map((binding) => [binding.envId, binding]));
  return {
    environments: columns,
    groups: definitions.map((group) => ({
      title: group.title,
      rows: group.fields.map((field) => {
        const rawValues = columns.map((environment) => field.read(byEnvId.get(environment.envId)));
        return {
          key: field.key,
          label: field.label,
          values: rawValues.map((value) => formatFingerprintValue(field.key, value)),
          state: comparisonState(rawValues),
        };
      }),
    })),
  };
}

function comparisonState(values: unknown[]): ComparisonState {
  const normalized = values.map(normalizeComparisonValue);
  if (normalized.length < 2 || normalized.some((value) => value === null)) return "unknown";
  return normalized.every((value) => value === normalized[0]) ? "same" : "different";
}

function normalizeComparisonValue(value: unknown): string | null {
  if (value === undefined || value === null || value === "") return null;
  if (Array.isArray(value) && value.length === 0) return null;
  if (typeof value === "object") return stableJson(value);
  return String(value);
}

function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${stableJson(record[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}
