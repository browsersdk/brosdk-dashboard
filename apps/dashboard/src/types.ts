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
    localLabel: string;
    tags: string[];
    status: string;
    cdp: string;
    lastEvent: string;
    generation: number;
    requestId: number | null;
    currentOperationId: string | null;
    updatedAt: string;
  }>;
  operations: Array<{
    id: string;
    kind: string;
    envId: string | null;
    label: string;
    status: string;
    message: string;
    requestId: number | null;
    generation: number;
    errorCode: string | null;
    createdAt: string;
    updatedAt: string;
  }>;
  settings: ManagerSettings;
  latestEventSequence: number;
  databasePath: string;
}

export interface ManagerSettings {
  workDir: string;
  extensionDir: string;
  logDir: string;
  sdkApiUrl: string | null;
  debug: boolean;
}

export interface ManagerEvent {
  sequence: number;
  eventType: string;
  envId: string | null;
  operationId: string | null;
  payload: unknown;
  createdAt: string;
}
