export type SmokeStageStatus = "passed" | "failed" | "skipped";

export interface SdkCapabilities {
  platform: string;
  cAbi: boolean;
  embeddedWebApi: boolean;
  embeddedMcp: boolean;
  supportsInitPort: boolean;
  callbacks: string[];
  syncCalls: string[];
  asyncCalls: string[];
  cdpCalls: string[];
  dllPath: string | null;
  dllExists: boolean;
}

export interface JsonSummary {
  kind: string;
  keys: string[];
  itemCount: number | null;
  total: number | null;
  page: number | null;
  pageSize: number | null;
}

export interface SmokeStage {
  name: string;
  status: SmokeStageStatus;
  code: number | null;
  message: string;
  durationMs: number;
}

export interface SmokeReport {
  skipped: boolean;
  startedAt: string;
  finishedAt: string;
  dllPath: string;
  workDir: string | null;
  embeddedMcpPort: number | null;
  capabilities: SdkCapabilities;
  stages: SmokeStage[];
  callbacks: {
    result: number;
    log: number;
  };
  sdkInfo: JsonSummary | null;
  envPage: JsonSummary | null;
}

export interface DashboardSnapshot {
  sdk: {
    state: string;
    runtime: {
      state: "stopped" | "starting" | "running" | "degraded";
      pid: number | null;
      generation: number;
      endpoint: string | null;
      lastError: string | null;
    };
    apiKey: {
      source: string;
      present: boolean;
    };
    hostPath: string | null;
    dllPath: string;
    workDir: string;
    lastSmoke: SmokeReport | null;
  };
  capabilities: SdkCapabilities;
  mcp: {
    mode: string;
    embeddedAvailable: boolean;
    managerRoute: string;
    endpointHint: string;
    notes: string[];
  };
  environments: Array<{
    envId: string;
    name: string;
    status: string;
    cdp: string;
    lastEvent: string;
  }>;
  operations: Array<{
    id: string;
    label: string;
    status: string;
    message: string;
    updatedAt: string;
  }>;
}
