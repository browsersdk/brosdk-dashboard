use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkCapabilities {
    pub platform: String,
    pub support_status: String,
    pub unsupported_reason: Option<String>,
    pub c_abi: bool,
    pub embedded_web_api: bool,
    pub embedded_mcp: bool,
    pub supports_init_port: bool,
    pub callbacks: Vec<String>,
    pub sync_calls: Vec<String>,
    pub async_calls: Vec<String>,
    pub cdp_calls: Vec<String>,
    pub dll_path: Option<String>,
    pub dll_exists: bool,
    pub library_dir: Option<String>,
    pub library_filename: Option<String>,
    pub secret_backend: Option<String>,
    pub ipc_transport: Option<String>,
}

impl Default for SdkCapabilities {
    fn default() -> Self {
        Self {
            platform: std::env::consts::OS.to_string(),
            support_status: "unavailable".into(),
            unsupported_reason: Some("SDK library has not been resolved".into()),
            c_abi: false,
            embedded_web_api: false,
            embedded_mcp: false,
            supports_init_port: false,
            callbacks: Vec::new(),
            sync_calls: Vec::new(),
            async_calls: Vec::new(),
            cdp_calls: Vec::new(),
            dll_path: None,
            dll_exists: false,
            library_dir: None,
            library_filename: None,
            secret_backend: None,
            ipc_transport: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallbackCounts {
    pub result: usize,
    pub log: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmokeStageStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeStage {
    pub name: String,
    pub status: SmokeStageStatus,
    pub code: Option<i32>,
    pub message: String,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonSummary {
    pub kind: String,
    pub keys: Vec<String>,
    pub item_count: Option<usize>,
    pub total: Option<u64>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeReport {
    pub skipped: bool,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub dll_path: String,
    pub work_dir: Option<String>,
    pub embedded_mcp_port: Option<u16>,
    pub capabilities: SdkCapabilities,
    pub stages: Vec<SmokeStage>,
    pub callbacks: CallbackCounts,
    pub sdk_info: Option<JsonSummary>,
    pub env_page: Option<JsonSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub source: String,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkPanel {
    pub state: String,
    pub runtime: RuntimeHostStatus,
    pub initialized: bool,
    pub api_key: ApiKeyStatus,
    pub host_path: Option<String>,
    pub dll_path: String,
    pub work_dir: String,
    pub last_smoke: Option<SmokeReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyInitializationResult {
    pub environment_count: usize,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeHostState {
    Stopped,
    Starting,
    Running,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHostStatus {
    pub state: RuntimeHostState,
    pub pid: Option<u32>,
    pub generation: u64,
    pub endpoint: Option<String>,
    pub last_error: Option<String>,
}

impl Default for RuntimeHostStatus {
    fn default() -> Self {
        Self {
            state: RuntimeHostState::Stopped,
            pid: None,
            generation: 0,
            endpoint: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HostCommand {
    Health,
    Capabilities,
    Initialize {
        work_dir: String,
        embedded_port: Option<u16>,
        sdk_api_url: Option<String>,
        debug: bool,
    },
    Info,
    EnvCreate {
        request: Value,
    },
    EnvUpdate {
        request: Value,
    },
    EnvDestroy {
        request: Value,
    },
    EnvPage {
        request: Value,
    },
    EnvGetInfo {
        request: Value,
    },
    BrowserInfo,
    NetworkDiagnostics {
        request: Value,
    },
    SystemProxyDiagnostics,
    BrowserInstall {
        request: Value,
    },
    BrowserCleanup {
        request: Value,
    },
    BrowserOpen {
        request: Value,
    },
    BrowserClose {
        request: Value,
    },
    BrowserCommand {
        request: Value,
    },
    BrowserSnapshot {
        request: Value,
    },
    BrowserEnvCheck {
        request: Value,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRequest {
    pub id: String,
    pub operation_id: Option<String>,
    pub command: HostCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostError {
    pub code: String,
    pub message: String,
    pub sdk_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostResponse {
    pub id: String,
    pub operation_id: Option<String>,
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<HostError>,
}

impl HostResponse {
    pub fn success(request: &HostRequest, result: Value) -> Self {
        Self {
            id: request.id.clone(),
            operation_id: request.operation_id.clone(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(request: &HostRequest, error: HostError) -> Self {
        Self {
            id: request.id.clone(),
            operation_id: request.operation_id.clone(),
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEvent {
    pub sequence: u64,
    pub event_type: String,
    pub code: i32,
    pub event_name: String,
    pub request_id: Option<i32>,
    pub operation_id: Option<String>,
    pub env_id: Option<String>,
    pub payload: Value,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum HostWireMessage {
    Request(HostRequest),
    Response(HostResponse),
    Event(HostEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentRecord {
    pub env_id: String,
    pub name: String,
    pub status: String,
    pub cdp: String,
    pub last_event: String,
    pub generation: u64,
    pub request_id: Option<i32>,
    pub current_operation_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCacheStatus {
    pub source: String,
    pub state: String,
    pub count: usize,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentBindingSummary {
    pub env_id: String,
    pub fingerprint_profile_id: Option<String>,
    pub proxy_profile_id: Option<String>,
    pub remote_fingerprint: Value,
    pub remote_proxy: Value,
    pub remote_kernel: Value,
    pub remote_metadata: Value,
    pub refreshed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCreateInput {
    pub proxy_profile_id: Option<String>,
    pub kernel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentMetadataUpdateInput {
    pub env_id: String,
    pub env_name: String,
    pub serial: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EnvironmentBatchAction {
    Start,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentBatchInput {
    pub action: EnvironmentBatchAction,
    pub env_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentBatchResult {
    pub action: EnvironmentBatchAction,
    pub requested: usize,
    pub accepted: usize,
    pub failed: usize,
    pub operations: Vec<OperationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintProfile {
    pub id: String,
    pub name: String,
    pub source: String,
    pub profile: Value,
    pub bound_env_ids: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub profile: Value,
    #[serde(default)]
    pub bound_env_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password_present: bool,
    pub bound_env_ids: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub bound_env_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyParseResult {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password_present: bool,
    pub display_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelRecord {
    pub id: String,
    pub kernel_type: String,
    pub name: String,
    pub major: Option<u32>,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub platform: String,
    pub arch: String,
    pub status: String,
    pub install_path: Option<String>,
    pub download_available: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelInstallInput {
    pub major: u32,
    pub kernel_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationExecution {
    pub operation: OperationRecord,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub id: String,
    pub kind: String,
    pub env_id: Option<String>,
    pub label: String,
    pub status: String,
    pub message: String,
    pub request_id: Option<i32>,
    pub generation: u64,
    pub error_code: Option<String>,
    pub request: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCommandExecution {
    pub operation: OperationRecord,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerSettings {
    pub data_dir: String,
    pub work_dir: String,
    pub extension_dir: String,
    pub log_dir: String,
    pub sdk_api_url: Option<String>,
    pub debug: bool,
    pub startup_policy: String,
    pub embedded_mcp_port: Option<u16>,
    pub ai_base_url: Option<String>,
    pub ai_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerEvent {
    pub sequence: u64,
    pub event_type: String,
    pub env_id: Option<String>,
    pub operation_id: Option<String>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPanel {
    pub mode: String,
    pub embedded_available: bool,
    pub configured: bool,
    pub active: bool,
    pub allowed_tools: Vec<String>,
    pub manager_route: String,
    pub endpoint_hint: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpToolScope {
    Global,
    #[default]
    Environment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallRequest {
    #[serde(default)]
    pub scope: McpToolScope,
    pub env_id: Option<String>,
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallExecution {
    pub operation: OperationRecord,
    pub scope: McpToolScope,
    pub env_id: Option<String>,
    pub tool: String,
    pub protocol_version: String,
    pub advertised_tools: Vec<String>,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDiscoveryRequest {
    pub scope: McpToolScope,
    pub env_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSummary {
    pub name: String,
    pub description: Option<String>,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDiscovery {
    pub operation: OperationRecord,
    pub scope: McpToolScope,
    pub env_id: Option<String>,
    pub protocol_version: String,
    pub advertised_tools: Vec<McpToolSummary>,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderStatus {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_present: bool,
    pub api_key_source: String,
    pub base_url_source: String,
    pub model_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfigInput {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatRequest {
    pub prompt: String,
    pub context_env_id: Option<String>,
    #[serde(default)]
    pub history: Vec<AiConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatResponse {
    pub answer: String,
    pub model: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentPlanRequest {
    pub prompt: String,
    pub context_env_id: Option<String>,
    #[serde(default)]
    pub history: Vec<AiConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentPlan {
    pub summary: String,
    pub action: String,
    pub env_id: Option<String>,
    pub expected_state: Option<String>,
    #[serde(default)]
    pub idempotency_key: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentExecuteRequest {
    pub plan: AiAgentPlan,
    pub approved: bool,
    #[serde(default)]
    pub automatic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentExecution {
    pub action: String,
    pub operation: Option<OperationRecord>,
    pub response: Option<Value>,
    pub status_semantics: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub sdk: SdkPanel,
    pub capabilities: SdkCapabilities,
    pub mcp: McpPanel,
    pub ai: AiProviderStatus,
    pub environments: Vec<EnvironmentRecord>,
    pub environment_cache: EnvironmentCacheStatus,
    pub environment_bindings: Vec<EnvironmentBindingSummary>,
    pub fingerprints: Vec<FingerprintProfile>,
    pub proxies: Vec<ProxyProfile>,
    pub kernels: Vec<KernelRecord>,
    pub operations: Vec<OperationRecord>,
    pub settings: ManagerSettings,
    pub latest_event_sequence: u64,
    pub database_path: String,
}

pub fn summarize_json(value: &Value) -> JsonSummary {
    match value {
        Value::Array(items) => JsonSummary {
            kind: "array".into(),
            keys: Vec::new(),
            item_count: Some(items.len()),
            total: None,
            page: None,
            page_size: None,
        },
        Value::Object(map) => {
            let data = map.get("data");
            let item_count = data.and_then(find_list_len);
            JsonSummary {
                kind: "object".into(),
                keys: map.keys().cloned().collect(),
                item_count,
                total: find_u64(value, &["total", "totalCount", "count"]),
                page: find_u64(value, &["page", "pageIndex", "pageNo", "currentPage"]),
                page_size: find_u64(value, &["pageSize", "size", "limit"]),
            }
        }
        _ => JsonSummary {
            kind: "scalar".into(),
            keys: Vec::new(),
            item_count: None,
            total: None,
            page: None,
            page_size: None,
        },
    }
}

fn find_list_len(value: &Value) -> Option<usize> {
    match value {
        Value::Array(items) => Some(items.len()),
        Value::Object(map) => ["list", "items", "records", "rows"]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_array).map(Vec::len)),
        _ => None,
    }
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(number) = map.get(*key).and_then(Value::as_u64) {
                    return Some(number);
                }
            }
            if let Some(data) = map.get("data").and_then(Value::as_object) {
                for key in keys {
                    if let Some(number) = data.get(*key).and_then(Value::as_u64) {
                        return Some(number);
                    }
                }
            }
            None
        }
        _ => None,
    }
}
