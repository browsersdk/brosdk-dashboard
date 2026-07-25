export type SmokeStageStatus = "passed" | "failed" | "skipped";

export interface SdkCapabilities {
  platform: string;
  supportStatus: string;
  unsupportedReason: string | null;
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
  libraryDir: string | null;
  libraryFilename: string | null;
  secretBackend: string | null;
  ipcTransport: string | null;
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
    configured: boolean;
    active: boolean;
    allowedTools: string[];
    managerRoute: string;
    endpointHint: string;
    notes: string[];
  };
  ai: AiProviderStatus;
  environments: Array<{
    envId: string;
    name: string;
    status: string;
    cdp: string;
    lastEvent: string;
    generation: number;
    requestId: number | null;
    currentOperationId: string | null;
    updatedAt: string;
  }>;
  environmentCache: {
    source: string;
    state: "fresh" | "stale" | "empty";
    count: number;
    lastSuccessAt: string | null;
    lastAttemptAt: string | null;
    lastError: string | null;
  };
  environmentBindings: EnvironmentBindingSummary[];
  fingerprints: FingerprintProfile[];
  proxies: ProxyProfile[];
  kernels: KernelRecord[];
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
    request: unknown | null;
    createdAt: string;
    updatedAt: string;
  }>;
  settings: ManagerSettings;
  latestEventSequence: number;
  databasePath: string;
}

export interface AiProviderStatus {
  provider: string;
  baseUrl: string;
  model: string;
  apiKeyPresent: boolean;
}

export interface AiChatResponse {
  answer: string;
  model: string;
  readOnly: boolean;
}

export interface AiAgentPlan {
  summary: string;
  action: string;
  envId: string | null;
  expectedState: string | null;
  idempotencyKey: string;
  arguments: unknown;
}

export interface AiAgentExecution {
  action: string;
  operation: DashboardSnapshot["operations"][number] | null;
  response: unknown | null;
  statusSemantics: string;
  replayed: boolean;
}

export interface McpToolCallExecution {
  operation: DashboardSnapshot["operations"][number];
  scope: "global" | "environment";
  envId: string | null;
  tool: string;
  protocolVersion: string;
  advertisedTools: string[];
  response: unknown;
}

export interface ManagerSettings {
  dataDir: string;
  workDir: string;
  extensionDir: string;
  logDir: string;
  sdkApiUrl: string | null;
  debug: boolean;
  startupPolicy: string;
  embeddedMcpPort: number | null;
}

export interface EnvironmentBindingSummary {
  envId: string;
  fingerprintProfileId: string | null;
  proxyProfileId: string | null;
  remoteFingerprint: unknown;
  remoteProxy: unknown;
  remoteKernel: unknown;
  refreshedAt: string | null;
}

export interface FingerprintProfile {
  id: string;
  name: string;
  source: string;
  profile: Record<string, unknown>;
  boundEnvIds: string[];
  updatedAt: string;
}

export interface FingerprintProfileInput {
  id?: string | null;
  name: string;
  profile: Record<string, unknown>;
  boundEnvIds: string[];
}

export interface ProxyProfile {
  id: string;
  name: string;
  scheme: string;
  host: string;
  port: number;
  username: string | null;
  passwordPresent: boolean;
  boundEnvIds: string[];
  updatedAt: string;
}

export interface ProxyProfileInput {
  id?: string | null;
  name: string;
  url: string;
  boundEnvIds: string[];
}

export interface ProxyParseResult {
  scheme: string;
  host: string;
  port: number;
  username: string | null;
  passwordPresent: boolean;
  displayUrl: string;
}

export interface KernelRecord {
  id: string;
  kernelType: string;
  name: string;
  major: number | null;
  version: string | null;
  latestVersion: string | null;
  platform: string;
  arch: string;
  status: string;
  installPath: string | null;
  downloadAvailable: boolean;
  updatedAt: string;
}

export interface EnvironmentCreateInput {
  proxyProfileId: string | null;
  kernelId: string;
}

export type OperationRecord = DashboardSnapshot["operations"][number];

export interface OperationExecution {
  operation: DashboardSnapshot["operations"][number];
  response: unknown;
}

export interface ManagerEvent {
  sequence: number;
  eventType: string;
  envId: string | null;
  operationId: string | null;
  payload: unknown;
  createdAt: string;
}
