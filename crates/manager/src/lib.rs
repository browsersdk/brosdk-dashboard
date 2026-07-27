use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use domain::{
    AiAgentExecuteRequest, AiAgentExecution, AiAgentPlan, AiAgentPlanRequest, AiChatRequest,
    AiChatResponse, AiConversationMessage, AiProviderConfigInput, AiProviderStatus,
    ApiKeyInitializationResult, ApiKeyStatus, BrowserCommandExecution, DashboardSnapshot,
    EnvironmentBatchAction, EnvironmentBatchInput, EnvironmentBatchResult, EnvironmentCreateInput,
    EnvironmentMetadataUpdateInput, EnvironmentRecord, FingerprintProfile, FingerprintProfileInput,
    HostCommand, KernelInstallInput, KernelRecord, ManagerEvent, ManagerSettings, McpPanel,
    McpToolCallExecution, McpToolCallRequest, McpToolDiscovery, McpToolDiscoveryRequest,
    McpToolScope, McpToolSummary, OperationExecution, OperationRecord, ProxyParseResult,
    ProxyProfile, ProxyProfileInput, RuntimeHostState, RuntimeHostStatus, SdkPanel, SmokeReport,
};
use operation::OperationQueue;
use sdk_client::{RuntimeHost, SdkHostClient};
use serde_json::json;
use sha2::{Digest, Sha256};
use store::{ManagerStore, RuntimeUpdate};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use zeroize::{Zeroize, Zeroizing};

mod mirror;
mod operation;
mod profiles;
mod store;

const ENVIRONMENT_PAGE_SIZE: usize = 200;
const MAX_ENVIRONMENT_PAGES: usize = 500;
const MAX_ENVIRONMENTS: usize = 100_000;
const MAX_ENVIRONMENT_BATCH_SIZE: usize = 20;
const API_KEY_SECRET_ID: &str = "sdk-api-key";
const API_KEY_SECRET_REFERENCE: &str = "sdk-api-key.bin";
const AI_API_KEY_SECRET_ID: &str = "ai-api-key";
const AI_API_KEY_SECRET_REFERENCE: &str = "ai-api-key.bin";
const MAX_AI_HISTORY_MESSAGES: usize = 40;
const MAX_AI_HISTORY_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_AI_HISTORY_BYTES: usize = 128 * 1024;
const MAX_MCP_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_MCP_ARGUMENT_DEPTH: usize = 16;
const MAX_MCP_STRING_CHARS: usize = 16 * 1024;
const GLOBAL_MCP_READ_TOOLS: &[&str] = &[
    "sdk.health",
    "sdk.info",
    "env.list",
    "env.resolve",
    "env.get",
    "browser.status",
    "task.list",
    "task.get",
    "mcp.endpoint",
];
const GLOBAL_ENVIRONMENT_MANAGEMENT_TOOLS: &[&str] = &[
    "env.list",
    "env.resolve",
    "env.get",
    "env.create",
    "env.update",
    "env.destroy",
];
const ENVIRONMENT_MCP_READ_TOOLS: &[&str] = &[
    "browser_state",
    "tabs",
    "snapshot",
    "diff",
    "read",
    "grep",
    "screenshot",
];

#[derive(Default)]
struct EnvironmentPageAccumulator {
    rows: Vec<(String, String, serde_json::Value)>,
    seen: HashSet<String>,
    expected_total: Option<usize>,
}

impl EnvironmentPageAccumulator {
    fn push(&mut self, value: &serde_json::Value) -> Result<bool, ManagerError> {
        if let Some(total) = mirror::environment_total(value) {
            if total > MAX_ENVIRONMENTS {
                return Err(ManagerError::InvalidHostResponse(format!(
                    "environment page total {total} exceeds safety limit {MAX_ENVIRONMENTS}"
                )));
            }
            if let Some(expected) = self.expected_total
                && expected != total
            {
                return Err(ManagerError::InvalidHostResponse(format!(
                    "environment page total changed from {expected} to {total} during sync"
                )));
            }
            self.expected_total = Some(total);
        }

        let page_rows = mirror::environment_rows(value);
        let page_len = page_rows.len();
        let previous_len = self.rows.len();
        for row in page_rows {
            if self.seen.insert(row.0.clone()) {
                self.rows.push(row);
            }
        }
        if self.rows.len() > MAX_ENVIRONMENTS {
            return Err(ManagerError::InvalidHostResponse(format!(
                "environment row count exceeds safety limit {MAX_ENVIRONMENTS}"
            )));
        }
        if let Some(total) = self.expected_total {
            if self.rows.len() > total {
                return Err(ManagerError::InvalidHostResponse(format!(
                    "environment pagination returned {} rows for reported total {total}",
                    self.rows.len()
                )));
            }
            if self.rows.len() == total {
                return Ok(true);
            }
        }
        if page_len == 0 {
            if let Some(total) = self.expected_total {
                return Err(ManagerError::InvalidHostResponse(format!(
                    "environment pagination ended after {} of {total} rows",
                    self.rows.len()
                )));
            }
            return Ok(true);
        }
        if self.rows.len() == previous_len {
            return Err(ManagerError::InvalidHostResponse(
                "environment pagination returned no new environment ids".into(),
            ));
        }
        Ok(self.expected_total.is_none() && page_len < ENVIRONMENT_PAGE_SIZE)
    }
}

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("{0}")]
    SdkHost(#[from] sdk_client::SdkClientError),
    #[error("{0}")]
    Store(#[from] store::StoreError),
    #[error("runtime host is not running")]
    RuntimeNotRunning,
    #[error("API Key is required before the SDK runtime can start")]
    ApiKeyMissing,
    #[error("API Key must not be empty")]
    ApiKeyInvalid,
    #[error("API Key is managed by BROSDK_API_KEY and cannot be changed in the application")]
    ApiKeyManagedExternally,
    #[error("stored API Key is not valid UTF-8")]
    ApiKeyCorrupt,
    #[error("AI provider configuration is invalid: {0}")]
    InvalidAiProvider(String),
    #[error("AI API Key is managed by BROSDK_AI_API_KEY and cannot be changed in the application")]
    AiApiKeyManagedExternally,
    #[error("stored AI API Key is not valid UTF-8")]
    AiApiKeyCorrupt,
    #[error("invalid runtime host response: {0}")]
    InvalidHostResponse(String),
    #[error("SDK backend rejected the request: {0}")]
    BackendRejected(String),
    #[error("environment was not found in the SDK server cache")]
    EnvironmentNotFound,
    #[error("environment is not ready for browser commands (current state: {0})")]
    EnvironmentNotReady(String),
    #[error("environment batch request is invalid: {0}")]
    InvalidEnvironmentBatch(String),
    #[error("environment metadata is invalid: {0}")]
    InvalidEnvironmentMetadata(String),
    #[error("environment cannot {action} from state {state}")]
    InvalidEnvironmentTransition { action: String, state: String },
    #[error("browser command method must not be empty")]
    InvalidBrowserCommand,
    #[error("{0}")]
    Profile(#[from] profiles::ProfileError),
    #[error("{0}")]
    Platform(#[from] platform::PlatformError),
    #[error("local file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("diagnostic archive failed: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("operation cannot be retried from its current state")]
    OperationNotRetryable,
    #[error("operation can only be cancelled while queued")]
    OperationNotCancellable,
    #[error("kernel is not known to the local manager")]
    KernelNotFound,
    #[error("kernel cannot be used to create an environment: {0}")]
    KernelNotUsable(String),
    #[error("proxy profile was not found")]
    ProxyNotFound,
    #[error("installed kernels cannot be removed while an environment is running")]
    KernelBusy,
    #[error("kernel install path is outside the SDK work directory")]
    UnsafeKernelPath,
    #[error("{0}")]
    Ai(#[from] ai_agent::AiError),
    #[error("AI agent execution requires explicit approval")]
    AgentApprovalRequired,
    #[error("AI agent plan is invalid: {0}")]
    InvalidAgentPlan(String),
    #[error("AI agent expected environment state {expected}, but current state is {actual}")]
    AgentStateMismatch { expected: String, actual: String },
    #[error("AI agent execution for this idempotency key is incomplete or uncertain")]
    AgentExecutionUncertain,
    #[error("embedded MCP request failed")]
    Mcp(#[from] mcp_client::McpClientError),
    #[error("DLL embedded MCP is not configured; set an embedded MCP port and restart the runtime")]
    McpNotConfigured,
    #[error("embedded MCP tool is not allowed by Manager policy: {0}")]
    McpToolNotAllowed(String),
    #[error("embedded MCP tool arguments are invalid: {0}")]
    InvalidMcpArguments(String),
}

#[derive(Clone)]
pub struct Manager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    store: ManagerStore,
    operations: OperationQueue,
    runtime: Mutex<Option<RuntimeHost>>,
    sdk_init_lock: Mutex<()>,
    sdk_initialized: RwLock<bool>,
    active_mcp_port: RwLock<Option<u16>>,
    last_runtime_status: RwLock<RuntimeHostStatus>,
    last_smoke: RwLock<Option<SmokeReport>>,
    agent_execution_lock: Mutex<()>,
    initial_environment_sync_attempted: AtomicBool,
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Manager {
    pub fn new() -> Self {
        Self::try_new().expect("failed to initialize Manager SQLite store")
    }

    pub fn try_new() -> Result<Self, ManagerError> {
        Self::with_store(ManagerStore::open_default()?)
    }

    fn with_store(store: ManagerStore) -> Result<Self, ManagerError> {
        store.recover_interrupted_session()?;
        let operations = OperationQueue::new(store.clone());
        Ok(Self {
            inner: Arc::new(ManagerInner {
                store,
                operations,
                runtime: Mutex::new(None),
                sdk_init_lock: Mutex::new(()),
                sdk_initialized: RwLock::new(false),
                active_mcp_port: RwLock::new(None),
                last_runtime_status: RwLock::new(RuntimeHostStatus::default()),
                last_smoke: RwLock::new(None),
                agent_execution_lock: Mutex::new(()),
                initial_environment_sync_attempted: AtomicBool::new(false),
            }),
        })
    }

    pub async fn start_runtime(&self) -> Result<RuntimeHostStatus, ManagerError> {
        let (api_key, _) = self.resolve_api_key()?.ok_or(ManagerError::ApiKeyMissing)?;
        self.start_runtime_with_api_key(&api_key).await
    }

    async fn start_runtime_with_api_key(
        &self,
        api_key: &str,
    ) -> Result<RuntimeHostStatus, ManagerError> {
        let mut runtime = self.inner.runtime.lock().await;
        if let Some(host) = runtime.as_ref() {
            let status = host.status();
            *self.inner.last_runtime_status.write().await = status.clone();
            return Ok(status);
        }

        *self.inner.last_runtime_status.write().await = RuntimeHostStatus {
            state: RuntimeHostState::Starting,
            ..RuntimeHostStatus::default()
        };
        match RuntimeHost::start_with_api_key(api_key).await {
            Ok(host) => {
                let status = host.status();
                *self.inner.sdk_initialized.write().await = false;
                *self.inner.active_mcp_port.write().await = None;
                *self.inner.last_runtime_status.write().await = status.clone();
                self.inner.store.append_event(
                    "runtime.started",
                    None,
                    None,
                    &json!({
                        "pid": status.pid,
                        "generation": status.generation,
                    }),
                )?;
                self.monitor_host(&host);
                *runtime = Some(host);
                Ok(status)
            }
            Err(error) => {
                let status = RuntimeHostStatus {
                    state: RuntimeHostState::Degraded,
                    last_error: Some(error.to_string()),
                    ..RuntimeHostStatus::default()
                };
                *self.inner.last_runtime_status.write().await = status;
                Err(error.into())
            }
        }
    }

    pub async fn apply_startup_policy(&self) -> Result<(), ManagerError> {
        // Reconciliation only observes SDK state; it never restores or opens a browser.
        // It is therefore required for every startup policy after an unclean client exit.
        let _ = self.reconcile_runtimes().await?;
        Ok(())
    }

    pub async fn stop_runtime(&self) -> Result<RuntimeHostStatus, ManagerError> {
        let host = self.inner.runtime.lock().await.take();
        let status = match host {
            Some(host) => host.stop().await?,
            None => RuntimeHostStatus::default(),
        };
        *self.inner.sdk_initialized.write().await = false;
        *self.inner.active_mcp_port.write().await = None;
        *self.inner.last_runtime_status.write().await = status.clone();
        self.inner
            .store
            .append_event("runtime.stopped", None, None, &json!({}))?;
        Ok(status)
    }

    pub async fn kill_runtime(&self) -> Result<RuntimeHostStatus, ManagerError> {
        let host = self.inner.runtime.lock().await.clone();
        let status = match host {
            Some(host) => host.kill().await?,
            None => RuntimeHostStatus::default(),
        };
        *self.inner.sdk_initialized.write().await = false;
        *self.inner.active_mcp_port.write().await = None;
        *self.inner.last_runtime_status.write().await = status.clone();
        Ok(status)
    }

    pub async fn snapshot(&self) -> Result<DashboardSnapshot, ManagerError> {
        let api_key_status = self.api_key_status()?;
        if api_key_status.present && self.inner.runtime.lock().await.is_none() {
            let _ = self.start_runtime().await;
        }
        if api_key_status.present
            && self
                .inner
                .initial_environment_sync_attempted
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let _ = self.sync_environments().await;
        }

        let dll_path = sdk_ffi::default_library_path();
        let host_path = sdk_client::discover_host_path().ok();
        let host = self.inner.runtime.lock().await.clone();
        let mut runtime_status = self.inner.last_runtime_status.read().await.clone();
        let mut capabilities = sdk_ffi::capabilities_for_path(dll_path.clone());
        if let Some(host) = host {
            runtime_status = host.status();
            if runtime_status.state == RuntimeHostState::Running
                && let Ok(host_capabilities) = host.capabilities().await
            {
                capabilities = host_capabilities;
            }
            *self.inner.last_runtime_status.write().await = runtime_status.clone();
        }

        let state = match runtime_status.state {
            RuntimeHostState::Running => "host-running",
            RuntimeHostState::Starting => "host-starting",
            RuntimeHostState::Degraded => "host-degraded",
            RuntimeHostState::Stopped => "host-stopped",
        };
        let settings = self.inner.store.settings()?;
        let configured_mcp_port = settings.embedded_mcp_port.or_else(embedded_port);
        let active_mcp_port = *self.inner.active_mcp_port.read().await;
        let environments = self.inner.store.list_environments()?;
        let fingerprints = self.inner.store.list_fingerprint_profiles()?;
        let proxies = self.inner.store.list_proxy_profiles()?;
        let bindings = profiles::environment_bindings(
            &environments
                .iter()
                .map(|environment| environment.env_id.clone())
                .collect::<Vec<_>>(),
            &self.inner.store.environment_remote_values()?,
            &self.inner.store.environment_details()?,
            &fingerprints
                .iter()
                .map(|profile| (profile.id.clone(), profile.bound_env_ids.clone()))
                .collect::<Vec<_>>(),
            &proxies
                .iter()
                .map(|profile| (profile.id.clone(), profile.bound_env_ids.clone()))
                .collect::<Vec<_>>(),
        );
        Ok(DashboardSnapshot {
            sdk: SdkPanel {
                state: state.into(),
                runtime: runtime_status,
                initialized: *self.inner.sdk_initialized.read().await,
                api_key: api_key_status,
                host_path: host_path.map(|path| path.display().to_string()),
                dll_path: dll_path.display().to_string(),
                work_dir: settings.work_dir.clone(),
                last_smoke: self.inner.last_smoke.read().await.clone(),
            },
            capabilities: capabilities.clone(),
            mcp: McpPanel {
                mode: "manager-routed".into(),
                embedded_available: capabilities.embedded_mcp,
                configured: configured_mcp_port.is_some(),
                active: active_mcp_port.is_some(),
                allowed_tools: GLOBAL_MCP_READ_TOOLS
                    .iter()
                    .map(|tool| format!("global:{tool}"))
                    .collect(),
                manager_route: "Manager owns the runtime host process, envId routing and operation state; only sdk-host can enable the DLL embedded MCP port.".into(),
                endpoint_hint: active_mcp_port
                    .map(|port| format!("active on 127.0.0.1:{port}"))
                    .or_else(|| configured_mcp_port.map(|port| format!("configured on 127.0.0.1:{port}; runtime initialization pending")))
                    .unwrap_or_else(|| "not configured; internal IPC does not require a TCP port".into()),
                notes: vec![
                    "Dashboard communicates with Manager through Tauri commands.".into(),
                    "Manager communicates with sdk-host through a supervised named pipe/UDS.".into(),
                    "Global MCP is limited to management reads; lifecycle mutations continue through Manager operations.".into(),
                    "Environment MCP tools use the global env.* catalog; Manager injects the selected envId into every call.".into(),
                    "Unexpected host exit becomes degraded state and never exits the desktop UI.".into(),
                ],
            },
            ai: self.ai_provider_status()?,
            environments,
            environment_cache: self.inner.store.environment_cache_status()?,
            environment_bindings: bindings,
            fingerprints,
            proxies,
            kernels: self.inner.store.list_kernel_records()?,
            operations: self.inner.store.list_operations(100)?,
            settings,
            latest_event_sequence: self.inner.store.latest_event_sequence()?,
            database_path: self.inner.store.path().display().to_string(),
        })
    }

    pub fn api_key_status(&self) -> Result<ApiKeyStatus, ManagerError> {
        if environment_api_key_present() {
            return Ok(ApiKeyStatus {
                source: "environment".into(),
                present: true,
            });
        }
        let data_dir = self.credential_data_dir()?;
        let present = platform::secrets_dir(&data_dir)
            .join(API_KEY_SECRET_REFERENCE)
            .is_file();
        Ok(ApiKeyStatus {
            source: if present { "secure-storage" } else { "none" }.into(),
            present,
        })
    }

    pub async fn configure_api_key(
        &self,
        mut api_key: String,
    ) -> Result<ApiKeyInitializationResult, ManagerError> {
        if environment_api_key_present() {
            api_key.zeroize();
            return Err(ManagerError::ApiKeyManagedExternally);
        }
        let trimmed = api_key.trim().to_owned();
        api_key.zeroize();
        if trimmed.is_empty() {
            return Err(ManagerError::ApiKeyInvalid);
        }
        let api_key = Zeroizing::new(trimmed);

        let _ = self.stop_runtime().await;
        let result = async {
            self.start_runtime_with_api_key(&api_key).await?;
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let environments = self.fetch_all_environments(&host, None).await?;

            self.inner.store.reset_account_state()?;
            let data_dir = self.credential_data_dir()?;
            platform::store_secret(&data_dir, API_KEY_SECRET_ID, api_key.as_bytes())?;
            self.inner
                .store
                .replace_remote_environments(&environments)?;
            self.inner
                .initial_environment_sync_attempted
                .store(true, Ordering::Release);
            self.inner.store.append_event(
                "credential.configured",
                None,
                None,
                &json!({ "environmentCount": environments.len() }),
            )?;
            Ok(ApiKeyInitializationResult {
                environment_count: environments.len(),
                source: "secure-storage".into(),
            })
        }
        .await;

        if result.is_err() {
            let _ = self.stop_runtime().await;
        }
        result
    }

    pub async fn clear_api_key(&self) -> Result<(), ManagerError> {
        if environment_api_key_present() {
            return Err(ManagerError::ApiKeyManagedExternally);
        }
        let _ = self.stop_runtime().await;
        let data_dir = self.credential_data_dir()?;
        platform::delete_secret(&data_dir, API_KEY_SECRET_REFERENCE)?;
        self.inner.store.reset_account_state()?;
        self.inner
            .initial_environment_sync_attempted
            .store(false, Ordering::Release);
        Ok(())
    }

    fn resolve_api_key(&self) -> Result<Option<(Zeroizing<String>, &'static str)>, ManagerError> {
        if let Ok(value) = std::env::var("BROSDK_API_KEY")
            && !value.trim().is_empty()
        {
            return Ok(Some((Zeroizing::new(value), "environment")));
        }
        let data_dir = self.credential_data_dir()?;
        let path = platform::secrets_dir(&data_dir).join(API_KEY_SECRET_REFERENCE);
        if !path.is_file() {
            return Ok(None);
        }
        let value = String::from_utf8(platform::read_secret(&data_dir, API_KEY_SECRET_REFERENCE)?)
            .map_err(|_| ManagerError::ApiKeyCorrupt)?;
        if value.trim().is_empty() {
            return Err(ManagerError::ApiKeyCorrupt);
        }
        Ok(Some((Zeroizing::new(value), "secure-storage")))
    }

    fn credential_data_dir(&self) -> Result<PathBuf, ManagerError> {
        Ok(PathBuf::from(self.inner.store.settings()?.data_dir))
    }

    pub fn ai_provider_status(&self) -> Result<AiProviderStatus, ManagerError> {
        let settings = self.inner.store.settings()?;
        let (base_url, base_url_source) = self.effective_ai_base_url(&settings)?;
        let (model, model_source) = self.effective_ai_model(&settings)?;
        let (api_key_present, api_key_source) = match environment_value("BROSDK_AI_API_KEY") {
            Some(_) => (true, "environment"),
            None => {
                let data_dir = self.credential_data_dir()?;
                let path = platform::secrets_dir(&data_dir).join(AI_API_KEY_SECRET_REFERENCE);
                (
                    path.is_file(),
                    if path.is_file() {
                        "secure-storage"
                    } else {
                        "none"
                    },
                )
            }
        };
        Ok(AiProviderStatus {
            provider: "openai-compatible".into(),
            base_url,
            model,
            api_key_present,
            api_key_source: api_key_source.into(),
            base_url_source: base_url_source.into(),
            model_source: model_source.into(),
        })
    }

    pub fn configure_ai_provider(
        &self,
        mut input: AiProviderConfigInput,
    ) -> Result<AiProviderStatus, ManagerError> {
        let base_url = normalize_ai_base_url(&input.base_url)?;
        let model = normalize_ai_model(&input.model)?;
        let mut settings = self.inner.store.settings()?;
        settings.ai_base_url = Some(base_url);
        settings.ai_model = Some(model);

        if let Some(api_key) = input.api_key.as_mut() {
            if environment_value("BROSDK_AI_API_KEY").is_some() {
                api_key.zeroize();
                return Err(ManagerError::AiApiKeyManagedExternally);
            }
            let trimmed = api_key.trim().to_owned();
            api_key.zeroize();
            if trimmed.is_empty() {
                return Err(ManagerError::InvalidAiProvider(
                    "API Key must not be empty".into(),
                ));
            }
            let api_key = Zeroizing::new(trimmed);
            let data_dir = self.credential_data_dir()?;
            platform::store_secret(&data_dir, AI_API_KEY_SECRET_ID, api_key.as_bytes())?;
        }
        input.api_key.zeroize();

        self.inner.store.update_settings(&settings)?;
        self.inner.store.append_event(
            "ai.provider-configured",
            None,
            None,
            &json!({
                "apiKeyConfigured": self.ai_api_key_present()?,
                "baseUrlConfigured": settings.ai_base_url.is_some(),
                "modelConfigured": settings.ai_model.is_some(),
            }),
        )?;
        self.ai_provider_status()
    }

    pub fn clear_ai_api_key(&self) -> Result<AiProviderStatus, ManagerError> {
        if environment_value("BROSDK_AI_API_KEY").is_some() {
            return Err(ManagerError::AiApiKeyManagedExternally);
        }
        let data_dir = self.credential_data_dir()?;
        platform::delete_secret(&data_dir, AI_API_KEY_SECRET_REFERENCE)?;
        self.inner.store.append_event(
            "ai.api-key-cleared",
            None,
            None,
            &json!({ "apiKeyConfigured": false }),
        )?;
        self.ai_provider_status()
    }

    fn ai_api_key_present(&self) -> Result<bool, ManagerError> {
        if environment_value("BROSDK_AI_API_KEY").is_some() {
            return Ok(true);
        }
        let data_dir = self.credential_data_dir()?;
        Ok(platform::secrets_dir(&data_dir)
            .join(AI_API_KEY_SECRET_REFERENCE)
            .is_file())
    }

    fn resolve_ai_api_key(&self) -> Result<Option<Zeroizing<String>>, ManagerError> {
        if let Some(value) = environment_value("BROSDK_AI_API_KEY") {
            return Ok(Some(Zeroizing::new(value)));
        }
        let data_dir = self.credential_data_dir()?;
        let path = platform::secrets_dir(&data_dir).join(AI_API_KEY_SECRET_REFERENCE);
        if !path.is_file() {
            return Ok(None);
        }
        let value = String::from_utf8(platform::read_secret(
            &data_dir,
            AI_API_KEY_SECRET_REFERENCE,
        )?)
        .map_err(|_| ManagerError::AiApiKeyCorrupt)?;
        if value.trim().is_empty() {
            return Err(ManagerError::AiApiKeyCorrupt);
        }
        Ok(Some(Zeroizing::new(value)))
    }

    fn effective_ai_base_url(
        &self,
        settings: &ManagerSettings,
    ) -> Result<(String, &'static str), ManagerError> {
        if let Some(value) = environment_value("BROSDK_AI_BASE_URL") {
            return Ok((normalize_ai_base_url(&value)?, "environment"));
        }
        if let Some(value) = settings.ai_base_url.as_deref() {
            return Ok((normalize_ai_base_url(value)?, "settings"));
        }
        Ok((ai_agent::DEFAULT_BASE_URL.into(), "default"))
    }

    fn effective_ai_model(
        &self,
        settings: &ManagerSettings,
    ) -> Result<(String, &'static str), ManagerError> {
        if let Some(value) = environment_value("BROSDK_AI_MODEL") {
            return Ok((normalize_ai_model(&value)?, "environment"));
        }
        if let Some(value) = settings.ai_model.as_deref() {
            return Ok((normalize_ai_model(value)?, "settings"));
        }
        Ok((ai_agent::DEFAULT_MODEL.into(), "default"))
    }

    fn ai_client(&self) -> Result<ai_agent::AiClient, ManagerError> {
        let api_key = self
            .resolve_ai_api_key()?
            .ok_or(ai_agent::AiError::MissingApiKey)?;
        let settings = self.inner.store.settings()?;
        let (base_url, _) = self.effective_ai_base_url(&settings)?;
        let (model, _) = self.effective_ai_model(&settings)?;
        Ok(ai_agent::AiClient::from_config(
            api_key.to_string(),
            base_url,
            model,
        )?)
    }

    pub async fn ai_chat(&self, request: AiChatRequest) -> Result<AiChatResponse, ManagerError> {
        let prompt = require_prompt(&request.prompt)?;
        validate_ai_history(&request.history)?;
        let context = self.ai_context(request.context_env_id.as_deref()).await?;
        Ok(self
            .ai_client()?
            .chat(prompt, &context, &request.history)
            .await?)
    }

    pub async fn ai_plan_agent(
        &self,
        request: AiAgentPlanRequest,
    ) -> Result<AiAgentPlan, ManagerError> {
        let prompt = require_prompt(&request.prompt)?;
        validate_ai_history(&request.history)?;
        let environments = self.inner.store.list_environments()?;
        let target_env_id =
            resolve_agent_target(prompt, request.context_env_id.as_deref(), &environments)?;
        let context = self.ai_context(target_env_id.as_deref()).await?;
        let mut plan = self
            .ai_client()?
            .plan(prompt, &context, &request.history)
            .await?;
        prepare_agent_plan(&mut plan, target_env_id.as_deref(), &environments)?;
        validate_agent_plan(&plan)?;
        Ok(plan)
    }

    pub async fn ai_execute_agent(
        &self,
        request: AiAgentExecuteRequest,
    ) -> Result<AiAgentExecution, ManagerError> {
        if !request.approved {
            return Err(ManagerError::AgentApprovalRequired);
        }
        let _execution_guard = self.inner.agent_execution_lock.lock().await;
        validate_agent_plan(&request.plan)?;
        let plan_hash = agent_plan_hash(&request.plan)?;
        if let Some(previous) = self
            .inner
            .store
            .agent_execution(&request.plan.idempotency_key)?
        {
            return replay_agent_execution(previous, &plan_hash);
        }

        self.validate_expected_state(&request.plan)?;
        let reservation = AiAgentExecution {
            action: request.plan.action.clone(),
            operation: None,
            response: None,
            status_semantics: "Execution is reserved; final result is pending.".into(),
            replayed: false,
        };
        if !self.inner.store.reserve_agent_execution(
            &request.plan.idempotency_key,
            &plan_hash,
            &reservation,
        )? {
            let previous = self
                .inner
                .store
                .agent_execution(&request.plan.idempotency_key)?
                .ok_or(ManagerError::AgentExecutionUncertain)?;
            return replay_agent_execution(previous, &plan_hash);
        }
        self.inner.store.append_event(
            "ai.agent-reserved",
            request.plan.env_id.as_deref(),
            None,
            &json!({
                "action": request.plan.action,
                "idempotencyKey": request.plan.idempotency_key,
                "approvalMode": if request.automatic { "automatic" } else { "manual" },
            }),
        )?;

        let execution_result = async {
            let execution = match request.plan.action.as_str() {
            "none" => AiAgentExecution {
                action: "none".into(),
                operation: None,
                response: Some(json!({ "summary": request.plan.summary })),
                status_semantics: "No write action was executed.".into(),
                replayed: false,
            },
            "environment.start" => {
                let operation = self.start_environment(required_env_id(&request.plan)?).await?;
                AiAgentExecution {
                    action: request.plan.action.clone(),
                    operation: Some(operation),
                    response: None,
                    status_semantics: "The operation may be accepted or starting; ready requires browser-open-success or a ready browser_info reconciliation.".into(),
                    replayed: false,
                }
            }
            "environment.stop" => AiAgentExecution {
                action: request.plan.action.clone(),
                operation: Some(self.stop_environment(required_env_id(&request.plan)?).await?),
                response: None,
                status_semantics: "The operation may be accepted or stopping; stopped requires browser-close-success or reconciliation.".into(),
                replayed: false,
            },
            "environment.sync" => AiAgentExecution {
                action: request.plan.action.clone(),
                operation: Some(self.sync_environments().await?),
                response: None,
                status_semantics: "The returned operation record is the source of truth.".into(),
                replayed: false,
            },
            "runtime.reconcile" => AiAgentExecution {
                action: request.plan.action.clone(),
                operation: Some(self.reconcile_runtimes().await?),
                response: None,
                status_semantics: "The returned operation record is the source of truth.".into(),
                replayed: false,
            },
            "proxy.diagnose" => {
                let profile_id = request
                    .plan
                    .arguments
                    .get("profileId")
                    .and_then(serde_json::Value::as_str);
                let url = request
                    .plan
                    .arguments
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("https://example.com");
                let result = self.diagnose_proxy(profile_id, url).await?;
                AiAgentExecution {
                    action: request.plan.action.clone(),
                    operation: Some(result.operation),
                    response: Some(result.response),
                    status_semantics: "The diagnostic operation result is final for this request.".into(),
                    replayed: false,
                }
            }
            "environment.diagnose" => {
                let result = self.open_fingerprint_check(required_env_id(&request.plan)?).await?;
                AiAgentExecution {
                    action: request.plan.action.clone(),
                    operation: Some(result.operation),
                    response: Some(result.response),
                    status_semantics: "The diagnostic operation result is final for this request.".into(),
                    replayed: false,
                }
            }
            "mcp.read" => {
                let tool = request
                    .plan
                    .arguments
                    .get("tool")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        ManagerError::InvalidAgentPlan(
                            "mcp.read requires arguments.tool".into(),
                        )
                    })?;
                let arguments = request
                    .plan
                    .arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let arguments = validate_environment_mcp_read_tool_call(tool, arguments)?;
                let result = self
                    .call_embedded_mcp(McpToolCallRequest {
                        scope: McpToolScope::Environment,
                        env_id: Some(required_env_id(&request.plan)?.into()),
                        tool: tool.into(),
                        arguments,
                    })
                    .await?;
                AiAgentExecution {
                    action: request.plan.action.clone(),
                    operation: Some(result.operation),
                    response: Some(result.response),
                    status_semantics: "The read-only MCP operation completed; no browser lifecycle state was inferred from this result.".into(),
                    replayed: false,
                }
            }
            "mcp.call" => {
                let tool = request
                    .plan
                    .arguments
                    .get("tool")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        ManagerError::InvalidAgentPlan(
                            "mcp.call requires arguments.tool".into(),
                        )
                    })?;
                let arguments = request
                    .plan
                    .arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let scope = if request.plan.env_id.is_some() {
                    McpToolScope::Environment
                } else {
                    McpToolScope::Global
                };
                let result = self
                    .call_embedded_mcp(McpToolCallRequest {
                        scope,
                        env_id: request.plan.env_id.clone(),
                        tool: tool.into(),
                        arguments,
                    })
                    .await?;
                AiAgentExecution {
                    action: request.plan.action.clone(),
                    operation: Some(result.operation),
                    response: Some(result.response),
                    status_semantics: "The MCP tool completed for the selected scope; runtime tools/list was used as the availability authority.".into(),
                    replayed: false,
                }
            }
                action => {
                    return Err(ManagerError::InvalidAgentPlan(format!(
                        "unsupported action {action}"
                    )));
                }
            };
            Ok::<AiAgentExecution, ManagerError>(execution)
        }
        .await;
        let execution = match execution_result {
            Ok(execution) => execution,
            Err(error) => {
                self.inner
                    .store
                    .mark_agent_execution_uncertain(&request.plan.idempotency_key)?;
                let _ = self.inner.store.append_event(
                    "ai.agent-uncertain",
                    request.plan.env_id.as_deref(),
                    None,
                    &json!({
                        "action": request.plan.action,
                        "idempotencyKey": request.plan.idempotency_key,
                    }),
                );
                return Err(error);
            }
        };

        self.inner
            .store
            .complete_agent_execution(&request.plan.idempotency_key, &execution)?;

        self.inner.store.append_event(
            "ai.agent-executed",
            request.plan.env_id.as_deref(),
            execution
                .operation
                .as_ref()
                .map(|operation| operation.id.as_str()),
            &json!({
                "action": execution.action,
                "idempotencyKey": request.plan.idempotency_key,
            }),
        )?;
        Ok(execution)
    }

    async fn ai_context(
        &self,
        focused_env_id: Option<&str>,
    ) -> Result<serde_json::Value, ManagerError> {
        let environments = self.inner.store.list_environments()?;
        if let Some(env_id) = focused_env_id
            && !environments
                .iter()
                .any(|environment| environment.env_id == env_id)
        {
            return Err(ManagerError::EnvironmentNotFound);
        }
        let fingerprints = self.inner.store.list_fingerprint_profiles()?;
        let proxies = self.inner.store.list_proxy_profiles()?;
        let kernels = self.inner.store.list_kernel_records()?;
        let operations = self.inner.store.list_operations(30)?;
        let settings = self.inner.store.settings()?;
        let capabilities = sdk_ffi::capabilities_for_path(sdk_ffi::default_library_path());
        let runtime = self.inner.last_runtime_status.read().await.clone();
        Ok(json!({
            "capabilities": {
                "platform": capabilities.platform,
                "supportStatus": capabilities.support_status,
                "unsupportedReason": capabilities.unsupported_reason,
                "embeddedMcp": capabilities.embedded_mcp,
                "embeddedWebApi": capabilities.embedded_web_api,
                "ipcTransport": capabilities.ipc_transport,
                "secretBackend": capabilities.secret_backend,
            },
            "runtime": {
                "state": runtime.state,
                "generation": runtime.generation,
                "lastError": runtime.last_error,
            },
            "focusedEnvId": focused_env_id,
            "environments": environments.into_iter()
                .filter(|environment| focused_env_id.is_none_or(|env_id| environment.env_id == env_id))
                .map(|environment| {
                let cdp_origin = external_cdp_origin(&environment.cdp);
                let control_channel = if cdp_origin.is_some() {
                    "external-cdp"
                } else if environment.status == "ready" {
                    "sdk-browser-command"
                } else {
                    "unavailable"
                };
                json!({
                    "envId": environment.env_id,
                    "name": environment.name,
                    "status": environment.status,
                    "generation": environment.generation,
                    "lastEvent": environment.last_event,
                    "requestId": environment.request_id,
                    "currentOperationId": environment.current_operation_id,
                    "updatedAt": environment.updated_at,
                    "cdpAvailable": cdp_origin.is_some(),
                    "cdpOrigin": cdp_origin,
                    "controlChannel": control_channel,
                })
            }).collect::<Vec<_>>(),
            "fingerprints": fingerprints.into_iter().map(|profile| json!({
                "id": profile.id,
                "name": profile.name,
                "source": profile.source,
                "boundEnvIds": profile.bound_env_ids,
                "profile": profiles::object_without_secrets(&profile.profile),
            })).collect::<Vec<_>>(),
            "proxies": proxies.into_iter().map(|proxy| json!({
                "id": proxy.id,
                "name": proxy.name,
                "scheme": proxy.scheme,
                "host": proxy.host,
                "port": proxy.port,
                "usernamePresent": proxy.username.is_some(),
                "passwordPresent": proxy.password_present,
                "boundEnvIds": proxy.bound_env_ids,
            })).collect::<Vec<_>>(),
            "kernels": kernels.into_iter().map(|kernel| json!({
                "id": kernel.id,
                "kernelType": kernel.kernel_type,
                "name": kernel.name,
                "major": kernel.major,
                "version": kernel.version,
                "latestVersion": kernel.latest_version,
                "platform": kernel.platform,
                "arch": kernel.arch,
                "status": kernel.status,
                "downloadAvailable": kernel.download_available,
            })).collect::<Vec<_>>(),
            "operations": operations.into_iter().map(|operation| json!({
                "id": operation.id,
                "kind": operation.kind,
                "envId": operation.env_id,
                "status": operation.status,
                "message": operation.message,
                "updatedAt": operation.updated_at,
            })).collect::<Vec<_>>(),
            "settings": {
                "debug": settings.debug,
                "startupPolicy": settings.startup_policy,
                "sdkApiOverridePresent": settings.sdk_api_url.is_some(),
                "embeddedMcpEnabled": settings.embedded_mcp_port.is_some(),
            },
            "agentPolicy": {
                "allowedActions": allowed_agent_actions(),
                "readySemantics": "accepted is not ready",
                "writesRequireApproval": true,
                "mcpEnvironmentTools": "use env.* names; Manager injects the selected envId",
            }
        }))
    }

    fn validate_expected_state(&self, plan: &AiAgentPlan) -> Result<(), ManagerError> {
        if let Some(env_id) = plan.env_id.as_deref() {
            let environment = self
                .inner
                .store
                .environment(env_id)?
                .ok_or(ManagerError::EnvironmentNotFound)?;
            let expected = plan.expected_state.as_deref().ok_or_else(|| {
                ManagerError::InvalidAgentPlan(
                    "expectedState is required for environment actions".into(),
                )
            })?;
            if environment.status != expected {
                return Err(ManagerError::AgentStateMismatch {
                    expected: expected.into(),
                    actual: environment.status,
                });
            }
        }
        Ok(())
    }

    pub async fn call_embedded_mcp(
        &self,
        request: McpToolCallRequest,
    ) -> Result<McpToolCallExecution, ManagerError> {
        let port = self
            .inner
            .active_mcp_port
            .read()
            .await
            .ok_or(ManagerError::McpNotConfigured)?;
        let tool = match request.scope {
            McpToolScope::Global => request.tool.clone(),
            McpToolScope::Environment => global_environment_mcp_tool_name(&request.tool)?,
        };
        let (env_id, generation, arguments, kind, label) = match request.scope {
            McpToolScope::Global => {
                if request
                    .env_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                {
                    return Err(ManagerError::InvalidMcpArguments(
                        "global MCP calls must not include envId".into(),
                    ));
                }
                (
                    None,
                    0,
                    validate_global_mcp_tool_call(&tool, request.arguments)?,
                    "mcp.global-tool-call",
                    "调用 DLL 全局 MCP",
                )
            }
            McpToolScope::Environment => {
                let env_id = request
                    .env_id
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        ManagerError::InvalidMcpArguments(
                            "environment MCP calls require envId".into(),
                        )
                    })?;
                let environment = self
                    .inner
                    .store
                    .environment(env_id)?
                    .ok_or(ManagerError::EnvironmentNotFound)?;
                if environment.status != "ready" {
                    return Err(ManagerError::EnvironmentNotReady(environment.status));
                }
                (
                    Some(env_id.to_string()),
                    environment.generation,
                    validate_environment_mcp_tool_call(&tool, request.arguments)?,
                    "mcp.environment-tool-call",
                    "调用 DLL 环境 MCP",
                )
            }
        };
        let request_summary = json!({
            "scope": mcp_scope_name(request.scope),
            "tool": tool,
            "argumentKeys": arguments
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        });
        let operation = self.inner.operations.enqueue(
            kind,
            env_id.as_deref(),
            label,
            generation,
            Some(&request_summary),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "calling embedded MCP")?;
        let result = match request.scope {
            McpToolScope::Global => mcp_client::call_global_tool(port, &tool, arguments).await,
            McpToolScope::Environment => {
                mcp_client::call_env_tool(
                    port,
                    env_id.as_deref().expect("validated environment id"),
                    &tool,
                    arguments,
                )
                .await
            }
        };
        match result {
            Ok(result) => {
                let response = sanitize_mcp_response(result.result);
                let advertised_tools = result
                    .advertised_tools
                    .into_iter()
                    .map(|tool| tool.name)
                    .collect::<Vec<_>>();
                let operation = self
                    .inner
                    .operations
                    .succeed(&operation.id, "embedded MCP tool completed")?;
                self.inner.store.append_event(
                    "mcp.tool-completed",
                    env_id.as_deref(),
                    Some(&operation.id),
                    &json!({
                        "scope": mcp_scope_name(request.scope),
                        "tool": tool,
                        "advertisedToolCount": advertised_tools.len(),
                        "protocolVersion": result.protocol_version,
                    }),
                )?;
                Ok(McpToolCallExecution {
                    operation,
                    scope: request.scope,
                    env_id,
                    tool,
                    protocol_version: result.protocol_version,
                    advertised_tools,
                    response,
                })
            }
            Err(error) => {
                let message = mcp_error_message(&error);
                self.inner
                    .operations
                    .fail(&operation.id, "MCP_TOOL_ERROR", message)?;
                Err(error.into())
            }
        }
    }

    pub async fn discover_embedded_mcp_tools(
        &self,
        request: McpToolDiscoveryRequest,
    ) -> Result<McpToolDiscovery, ManagerError> {
        let port = self
            .inner
            .active_mcp_port
            .read()
            .await
            .ok_or(ManagerError::McpNotConfigured)?;
        let (env_id, generation) = match request.scope {
            McpToolScope::Global => {
                if request
                    .env_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                {
                    return Err(ManagerError::InvalidMcpArguments(
                        "global MCP discovery must not include envId".into(),
                    ));
                }
                (None, 0)
            }
            McpToolScope::Environment => {
                let env_id = request
                    .env_id
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        ManagerError::InvalidMcpArguments(
                            "environment MCP discovery requires envId".into(),
                        )
                    })?;
                let environment = self
                    .inner
                    .store
                    .environment(env_id)?
                    .ok_or(ManagerError::EnvironmentNotFound)?;
                if environment.status != "ready" {
                    return Err(ManagerError::EnvironmentNotReady(environment.status));
                }
                (Some(env_id.to_string()), environment.generation)
            }
        };
        let operation = self.inner.operations.enqueue(
            "mcp.tools-discover",
            env_id.as_deref(),
            "发现 DLL MCP 工具",
            generation,
            Some(&json!({ "scope": mcp_scope_name(request.scope) })),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "discovering embedded MCP tools")?;
        let result = match request.scope {
            McpToolScope::Global => mcp_client::discover_global_tools(port).await,
            McpToolScope::Environment => {
                mcp_client::discover_env_tools(
                    port,
                    env_id.as_deref().expect("validated environment id"),
                )
                .await
            }
        };
        match result {
            Ok(result) => {
                let advertised_tools = result
                    .advertised_tools
                    .into_iter()
                    .map(|tool| McpToolSummary {
                        name: tool.name,
                        description: tool.description,
                        read_only_hint: tool.read_only_hint,
                        destructive_hint: tool.destructive_hint,
                    })
                    .collect::<Vec<_>>();
                let allowed_tools = advertised_tools
                    .iter()
                    .filter(|tool| mcp_tool_allowed(request.scope, &tool.name))
                    .map(|tool| tool.name.clone())
                    .collect::<Vec<_>>();
                let operation = self
                    .inner
                    .operations
                    .succeed(&operation.id, "embedded MCP tools discovered")?;
                self.inner.store.append_event(
                    "mcp.tools-discovered",
                    env_id.as_deref(),
                    Some(&operation.id),
                    &json!({
                        "scope": mcp_scope_name(request.scope),
                        "advertisedToolCount": advertised_tools.len(),
                        "allowedToolCount": allowed_tools.len(),
                        "protocolVersion": result.protocol_version,
                    }),
                )?;
                Ok(McpToolDiscovery {
                    operation,
                    scope: request.scope,
                    env_id,
                    protocol_version: result.protocol_version,
                    advertised_tools,
                    allowed_tools,
                })
            }
            Err(error) => {
                let message = mcp_error_message(&error);
                self.inner
                    .operations
                    .fail(&operation.id, "MCP_DISCOVERY_ERROR", message)?;
                Err(error.into())
            }
        }
    }

    pub async fn sync_environments(&self) -> Result<OperationRecord, ManagerError> {
        let operation =
            self.inner
                .operations
                .enqueue("environment.sync", None, "同步远端环境", 0, None)?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "initializing SDK")?;

        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let rows = self
                .fetch_all_environments(&host, Some(operation.id.clone()))
                .await?;
            self.inner.store.replace_remote_environments(&rows)?;
            self.inner.store.append_event(
                "environment.synced",
                None,
                Some(&operation.id),
                &json!({ "count": rows.len() }),
            )?;
            Ok::<usize, ManagerError>(rows.len())
        }
        .await;

        match result {
            Ok(count) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, &format!("synced {count} environments"))?),
            Err(error) => {
                let message = redacted_response_text(&error.to_string());
                self.inner.store.mark_environment_cache_stale(&message)?;
                Ok(self.inner.operations.fail(
                    &operation.id,
                    manager_error_code(&error),
                    &message,
                )?)
            }
        }
    }

    pub async fn create_environment(
        &self,
        input: EnvironmentCreateInput,
    ) -> Result<OperationRecord, ManagerError> {
        let operation_request = environment_create_operation_request(&input);
        let operation = self.inner.operations.enqueue(
            "environment.create",
            None,
            "创建环境",
            0,
            Some(&operation_request),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "validating proxy and kernel")?;

        let result = async {
            let kernel = self
                .inner
                .store
                .list_kernel_records()?
                .into_iter()
                .find(|kernel| kernel.id == input.kernel_id)
                .ok_or(ManagerError::KernelNotFound)?;
            validate_environment_kernel(&kernel)?;
            let proxy = input
                .proxy_profile_id
                .as_deref()
                .map(|profile_id| self.proxy_url_for_create(profile_id))
                .transpose()?;
            let request = build_environment_create_request(&kernel, proxy.as_deref())?;
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let response = host
                .call(
                    HostCommand::EnvCreate { request },
                    Some(operation.id.clone()),
                )
                .await?;
            ensure_backend_success("environment create", &response)?;
            let env_id = response_env_id(&response).ok_or_else(|| {
                ManagerError::InvalidHostResponse(
                    "environment create response did not contain data.envId".into(),
                )
            })?;
            let name =
                response_environment_name(&response).unwrap_or_else(|| format!("环境 {env_id}"));
            let remote = response
                .get("data")
                .cloned()
                .unwrap_or_else(|| json!({ "envId": env_id, "envName": name }));
            self.inner
                .store
                .upsert_remote_environments(&[(env_id.clone(), name, remote)])?;
            self.inner
                .store
                .attach_operation_environment(&operation.id, &env_id)?;

            let mirror_synced = match self
                .fetch_all_environments(&host, Some(operation.id.clone()))
                .await
            {
                Ok(rows) => {
                    self.inner.store.replace_remote_environments(&rows)?;
                    true
                }
                Err(error) => {
                    self.inner
                        .store
                        .mark_environment_cache_stale(&redacted_response_text(
                            &error.to_string(),
                        ))?;
                    false
                }
            };
            self.inner.store.append_event(
                "environment.created",
                Some(&env_id),
                Some(&operation.id),
                &json!({
                    "kernelId": input.kernel_id,
                    "proxyProfileId": input.proxy_profile_id,
                    "mirrorSynced": mirror_synced,
                }),
            )?;
            Ok::<(String, bool), ManagerError>((env_id, mirror_synced))
        }
        .await;

        match result {
            Ok((_env_id, true)) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, "environment created and mirror synchronized")?),
            Ok((_env_id, false)) => Ok(self.inner.operations.succeed(
                &operation.id,
                "environment created; full mirror refresh deferred",
            )?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn update_environment_metadata(
        &self,
        input: EnvironmentMetadataUpdateInput,
    ) -> Result<OperationRecord, ManagerError> {
        let request = build_environment_metadata_update_request(&input)?;
        let generation = self
            .inner
            .store
            .environment(&input.env_id)?
            .map(|environment| environment.generation)
            .unwrap_or_default();
        let operation = self.inner.operations.enqueue(
            "environment.metadata-update",
            Some(&input.env_id),
            "更新环境信息",
            generation,
            Some(&request),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "validating environment state")?;

        let result = async {
            let environment = self
                .inner
                .store
                .environment(&input.env_id)?
                .ok_or(ManagerError::EnvironmentNotFound)?;
            if environment.status != "stopped" {
                return Err(ManagerError::EnvironmentNotReady(environment.status));
            }

            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let response = host
                .call(
                    HostCommand::EnvUpdate {
                        request: request.clone(),
                    },
                    Some(operation.id.clone()),
                )
                .await?;
            ensure_backend_success("environment metadata update", &response)?;
            let (confirmed_name, confirmed_serial) =
                confirmed_environment_metadata(&response, &request)?;

            let mirror_synced = match self
                .fetch_all_environments(&host, Some(operation.id.clone()))
                .await
            {
                Ok(mut rows) => {
                    merge_confirmed_environment_metadata(
                        &mut rows,
                        &input.env_id,
                        &confirmed_name,
                        &confirmed_serial,
                    );
                    self.inner.store.replace_remote_environments(&rows)?;
                    true
                }
                Err(error) => {
                    self.inner
                        .store
                        .mark_environment_cache_stale(&redacted_response_text(
                            &error.to_string(),
                        ))?;
                    false
                }
            };
            let detail_synced = match host
                .call(
                    HostCommand::EnvGetInfo {
                        request: json!({ "envId": input.env_id }),
                    },
                    Some(operation.id.clone()),
                )
                .await
            {
                Ok(detail) if ensure_backend_success("environment detail", &detail).is_ok() => {
                    self.inner.store.save_environment_detail(
                        &input.env_id,
                        &profiles::safe_environment_detail(&detail),
                    )?;
                    true
                }
                _ => false,
            };
            self.inner.store.append_event(
                "environment.metadata-updated",
                Some(&input.env_id),
                Some(&operation.id),
                &json!({
                    "mirrorSynced": mirror_synced,
                    "detailSynced": detail_synced,
                }),
            )?;
            Ok::<(bool, bool), ManagerError>((mirror_synced, detail_synced))
        }
        .await;

        match result {
            Ok((true, true)) => Ok(self.inner.operations.succeed(
                &operation.id,
                "environment metadata updated and synchronized",
            )?),
            Ok(_) => Ok(self.inner.operations.succeed(
                &operation.id,
                "environment metadata updated; mirror refresh deferred",
            )?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn destroy_environment(&self, env_id: &str) -> Result<OperationRecord, ManagerError> {
        let operation = self.inner.operations.enqueue(
            "environment.destroy",
            Some(env_id),
            "删除环境",
            0,
            Some(&json!({ "envId": env_id })),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "validating environment state")?;
        let result = async {
            let environment = self
                .inner
                .store
                .environment(env_id)?
                .ok_or(ManagerError::EnvironmentNotFound)?;
            if environment.status != "stopped" {
                return Err(ManagerError::EnvironmentNotReady(environment.status));
            }
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let response = host
                .call(
                    HostCommand::EnvDestroy {
                        request: json!({ "envId": env_id }),
                    },
                    Some(operation.id.clone()),
                )
                .await?;
            ensure_backend_success("environment destroy", &response)?;
            self.inner.store.delete_environment(env_id)?;
            self.inner.store.append_event(
                "environment.destroyed",
                Some(env_id),
                Some(&operation.id),
                &json!({}),
            )?;
            Ok::<(), ManagerError>(())
        }
        .await;
        match result {
            Ok(()) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, "environment deleted")?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn reconcile_runtimes(&self) -> Result<OperationRecord, ManagerError> {
        let operation =
            self.inner
                .operations
                .enqueue("runtime.reconcile", None, "对账运行环境", 0, None)?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "reading sdk_browser_info")?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let value = host
                .call(HostCommand::BrowserInfo, Some(operation.id.clone()))
                .await?;
            let running = mirror::running_environments(&value);
            let observed = mirror::observed_environment_ids(&value);
            self.inner
                .store
                .reconcile_running_environments(&running, &observed)?;
            Ok::<usize, ManagerError>(running.len())
        }
        .await;
        match result {
            Ok(count) => Ok(self.inner.operations.succeed(
                &operation.id,
                &format!("reconciled {count} running environments"),
            )?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub fn prepare_environment_operation(
        &self,
        env_id: &str,
        start: bool,
    ) -> Result<OperationRecord, ManagerError> {
        let generation = self.inner.store.next_generation(env_id)?;
        let (kind, label) = if start {
            ("environment.start", "启动环境")
        } else {
            ("environment.stop", "停止环境")
        };
        Ok(self
            .inner
            .operations
            .enqueue(kind, Some(env_id), label, generation, None)?)
    }

    pub fn accept_environment_operation(
        &self,
        operation_id: &str,
        request_id: Option<i32>,
    ) -> Result<OperationRecord, ManagerError> {
        let operation = self
            .inner
            .store
            .operation(operation_id)?
            .ok_or(store::StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))?;
        let env_id = operation
            .env_id
            .as_deref()
            .ok_or(store::StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))?;
        let environment = self
            .inner
            .store
            .list_environments()?
            .into_iter()
            .find(|environment| environment.env_id == env_id)
            .ok_or(store::StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))?;
        let status = if operation.kind == "environment.start" {
            "starting"
        } else {
            "stopping"
        };
        let last_event = request_id
            .map(|request_id| format!("SDK accepted request {request_id}"))
            .unwrap_or_else(|| "SDK accepted request; awaiting callback reqId".into());
        Ok(self.inner.store.accept_environment_operation(
            operation_id,
            request_id,
            status,
            &environment.cdp,
            &last_event,
        )?)
    }

    pub async fn start_environment(&self, env_id: &str) -> Result<OperationRecord, ManagerError> {
        self.execute_environment_operation(env_id, true).await
    }

    pub async fn stop_environment(&self, env_id: &str) -> Result<OperationRecord, ManagerError> {
        self.execute_environment_operation(env_id, false).await
    }

    pub async fn batch_environment_action(
        &self,
        input: EnvironmentBatchInput,
    ) -> Result<EnvironmentBatchResult, ManagerError> {
        validate_environment_batch_shape(&input)?;
        let environments = input
            .env_ids
            .iter()
            .map(|env_id| {
                self.inner
                    .store
                    .environment(env_id)?
                    .ok_or(ManagerError::EnvironmentNotFound)
            })
            .collect::<Result<Vec<_>, ManagerError>>()?;
        validate_environment_batch_states(&input, &environments)?;

        let mut operations = Vec::with_capacity(input.env_ids.len());
        for env_id in &input.env_ids {
            let operation = match input.action {
                EnvironmentBatchAction::Start => self.start_environment(env_id).await?,
                EnvironmentBatchAction::Stop => self.stop_environment(env_id).await?,
            };
            operations.push(operation);
        }
        let failed = operations
            .iter()
            .filter(|operation| operation.status == "failed")
            .count();
        Ok(EnvironmentBatchResult {
            action: input.action,
            requested: input.env_ids.len(),
            accepted: operations.len() - failed,
            failed,
            operations,
        })
    }

    pub async fn browser_command(
        &self,
        env_id: &str,
        method: &str,
        params: serde_json::Value,
        session_id: Option<&str>,
    ) -> Result<BrowserCommandExecution, ManagerError> {
        if method.trim().is_empty() {
            return Err(ManagerError::InvalidBrowserCommand);
        }
        let environment = self
            .inner
            .store
            .environment(env_id)?
            .ok_or(ManagerError::EnvironmentNotFound)?;
        if environment.status != "ready" {
            return Err(ManagerError::EnvironmentNotReady(environment.status));
        }
        let operation = self.inner.operations.enqueue(
            "browser.command",
            Some(env_id),
            "执行浏览器命令",
            environment.generation,
            None,
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "calling sdk_browser_command")?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let mut request = json!({
                "envId": env_id,
                "method": method,
                "params": params,
            });
            if let Some(session_id) = session_id {
                request["sessionId"] = json!(session_id);
            }
            Ok::<serde_json::Value, ManagerError>(
                host.call(
                    HostCommand::BrowserCommand { request },
                    Some(operation.id.clone()),
                )
                .await?,
            )
        }
        .await;
        match result {
            Ok(response) => Ok(BrowserCommandExecution {
                operation: self
                    .inner
                    .operations
                    .succeed(&operation.id, "browser command completed")?,
                response,
            }),
            Err(error) => {
                self.inner.operations.fail(
                    &operation.id,
                    manager_error_code(&error),
                    &error.to_string(),
                )?;
                Err(error)
            }
        }
    }

    pub async fn refresh_environment_details(&self) -> Result<OperationRecord, ManagerError> {
        let operation = self.inner.operations.enqueue(
            "environment.details.refresh",
            None,
            "刷新环境详情",
            0,
            None,
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "reading sdk_env_getinfo")?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let environments = self.inner.store.list_environments()?;
            for environment in &environments {
                let detail = host
                    .call(
                        HostCommand::EnvGetInfo {
                            request: json!({ "envId": environment.env_id }),
                        },
                        Some(operation.id.clone()),
                    )
                    .await?;
                ensure_backend_success("environment detail", &detail)?;
                hydrate_runtime_cdp(
                    &self.inner.store,
                    &environment.env_id,
                    &detail,
                    "sdk_env_getinfo",
                )?;
                self.inner.store.save_environment_detail(
                    &environment.env_id,
                    &profiles::safe_environment_detail(&detail),
                )?;
            }
            Ok::<usize, ManagerError>(environments.len())
        }
        .await;
        match result {
            Ok(count) => Ok(self.inner.operations.succeed(
                &operation.id,
                &format!("refreshed {count} environment details"),
            )?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn refresh_environment_detail(
        &self,
        env_id: &str,
    ) -> Result<OperationRecord, ManagerError> {
        let generation = self
            .inner
            .store
            .environment(env_id)?
            .map(|environment| environment.generation)
            .unwrap_or_default();
        let operation = self.inner.operations.enqueue(
            "environment.detail.refresh",
            Some(env_id),
            "刷新环境详情",
            generation,
            Some(&json!({ "envId": env_id })),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "reading sdk_env_getinfo")?;
        let result = async {
            self.inner
                .store
                .environment(env_id)?
                .ok_or(ManagerError::EnvironmentNotFound)?;
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let detail = host
                .call(
                    HostCommand::EnvGetInfo {
                        request: json!({ "envId": env_id }),
                    },
                    Some(operation.id.clone()),
                )
                .await?;
            ensure_backend_success("environment detail", &detail)?;
            hydrate_runtime_cdp(&self.inner.store, env_id, &detail, "sdk_env_getinfo")?;
            self.inner
                .store
                .save_environment_detail(env_id, &profiles::safe_environment_detail(&detail))?;
            self.inner.store.append_event(
                "environment.detail.refreshed",
                Some(env_id),
                Some(&operation.id),
                &json!({}),
            )?;
            Ok::<(), ManagerError>(())
        }
        .await;
        match result {
            Ok(()) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, "environment detail refreshed")?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub fn parse_proxy_url(&self, url: &str) -> Result<ProxyParseResult, ManagerError> {
        Ok(profiles::parse_proxy_url(url)?.summary)
    }

    pub fn save_proxy_profile(
        &self,
        input: ProxyProfileInput,
    ) -> Result<ProxyProfile, ManagerError> {
        let parsed = profiles::parse_proxy_url(&input.url)?;
        let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let data_dir = PathBuf::from(self.inner.store.settings()?.data_dir);
        let current_secret_ref = self.inner.store.proxy_secret_ref(&id)?;
        let secret_ref = match parsed.password.as_deref() {
            Some(password) => {
                if let Some(reference) = current_secret_ref.as_deref() {
                    let _ = platform::delete_secret(&data_dir, reference);
                }
                Some(platform::store_secret(
                    &data_dir,
                    &format!("proxy-{id}"),
                    password.as_bytes(),
                )?)
            }
            None => current_secret_ref,
        };
        let profile = self.inner.store.upsert_proxy_profile(
            &id,
            input.name.trim(),
            &parsed.summary.scheme,
            &parsed.summary.host,
            parsed.summary.port,
            parsed.summary.username.as_deref(),
            secret_ref.as_deref(),
            &input.bound_env_ids,
        )?;
        self.inner.store.append_event(
            "proxy.updated",
            None,
            None,
            &json!({
                "profileId": profile.id,
                "passwordPresent": profile.password_present,
                "boundEnvIds": profile.bound_env_ids,
            }),
        )?;
        Ok(profile)
    }

    pub fn delete_proxy_profile(&self, id: &str) -> Result<(), ManagerError> {
        let data_dir = PathBuf::from(self.inner.store.settings()?.data_dir);
        if let Some(reference) = self.inner.store.delete_proxy_profile(id)? {
            platform::delete_secret(&data_dir, &reference)?;
        }
        self.inner
            .store
            .append_event("proxy.deleted", None, None, &json!({ "profileId": id }))?;
        Ok(())
    }

    pub async fn diagnose_proxy(
        &self,
        profile_id: Option<&str>,
        url: &str,
    ) -> Result<OperationExecution, ManagerError> {
        let request = json!({ "profileId": profile_id, "url": url });
        let operation = self.inner.operations.enqueue(
            "proxy.diagnose",
            None,
            "代理网络诊断",
            0,
            Some(&request),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "calling sdk_network_diagnostics")?;
        let result = async {
            let proxy = match profile_id {
                Some(profile_id) => Some(self.proxy_url_for_diagnostics(profile_id)?),
                None => None,
            };
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            Ok::<serde_json::Value, ManagerError>(
                host.call(
                    HostCommand::NetworkDiagnostics {
                        request: json!({
                            "proxy": proxy.unwrap_or_default(),
                            "bridgeProxy": "",
                            "url": url,
                        }),
                    },
                    Some(operation.id.clone()),
                )
                .await?,
            )
        }
        .await;
        match result {
            Ok(response) => Ok(OperationExecution {
                operation: self
                    .inner
                    .operations
                    .succeed(&operation.id, "proxy diagnostics completed")?,
                response,
            }),
            Err(error) => {
                self.inner.operations.fail(
                    &operation.id,
                    manager_error_code(&error),
                    &error.to_string(),
                )?;
                Err(error)
            }
        }
    }

    pub async fn system_proxy_diagnostics(&self) -> Result<OperationExecution, ManagerError> {
        let operation = self.inner.operations.enqueue(
            "proxy.system-diagnose",
            None,
            "系统代理诊断",
            0,
            None,
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "calling sdk_system_proxy_diagnostics")?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            Ok::<serde_json::Value, ManagerError>(
                host.call(
                    HostCommand::SystemProxyDiagnostics,
                    Some(operation.id.clone()),
                )
                .await?,
            )
        }
        .await;
        match result {
            Ok(response) => Ok(OperationExecution {
                operation: self
                    .inner
                    .operations
                    .succeed(&operation.id, "system proxy diagnostics completed")?,
                response,
            }),
            Err(error) => {
                self.inner.operations.fail(
                    &operation.id,
                    manager_error_code(&error),
                    &error.to_string(),
                )?;
                Err(error)
            }
        }
    }

    pub fn save_fingerprint_profile(
        &self,
        input: FingerprintProfileInput,
    ) -> Result<FingerprintProfile, ManagerError> {
        let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let profile = self.inner.store.upsert_fingerprint_profile(
            &id,
            input.name.trim(),
            "local",
            &profiles::object_without_secrets(&input.profile),
            &input.bound_env_ids,
        )?;
        self.inner.store.append_event(
            "fingerprint.updated",
            None,
            None,
            &json!({ "profileId": profile.id, "boundEnvIds": profile.bound_env_ids }),
        )?;
        Ok(profile)
    }

    pub fn import_fingerprint_profile(
        &self,
        path: &str,
    ) -> Result<FingerprintProfile, ManagerError> {
        let text = fs::read_to_string(path)?;
        let (name, profile) = profiles::parse_profile_document(&text)?;
        self.save_fingerprint_profile(FingerprintProfileInput {
            id: None,
            name,
            profile,
            bound_env_ids: Vec::new(),
        })
    }

    pub fn export_fingerprint_profile(&self, id: &str, path: &str) -> Result<(), ManagerError> {
        let profile = self
            .inner
            .store
            .list_fingerprint_profiles()?
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or(ManagerError::EnvironmentNotFound)?;
        fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "format": "brosdk-dashboard.fingerprint.v1",
                "name": profile.name,
                "profile": profile.profile,
            }))?,
        )?;
        Ok(())
    }

    pub fn delete_fingerprint_profile(&self, id: &str) -> Result<(), ManagerError> {
        self.inner.store.delete_fingerprint_profile(id)?;
        self.inner.store.append_event(
            "fingerprint.deleted",
            None,
            None,
            &json!({ "profileId": id }),
        )?;
        Ok(())
    }

    pub async fn open_fingerprint_check(
        &self,
        env_id: &str,
    ) -> Result<OperationExecution, ManagerError> {
        let environment = self
            .inner
            .store
            .environment(env_id)?
            .ok_or(ManagerError::EnvironmentNotFound)?;
        if environment.status != "ready" {
            return Err(ManagerError::EnvironmentNotReady(environment.status));
        }
        let execution = self
            .execute_sync_host_operation(
                "fingerprint.check",
                Some(env_id),
                "打开指纹检查页",
                json!({ "envId": env_id }),
                |request| HostCommand::BrowserEnvCheck { request },
            )
            .await?;
        Ok(OperationExecution {
            operation: execution.operation,
            response: summarize_fingerprint_check(&execution.response),
        })
    }

    pub async fn cleanup_environment_local_data(
        &self,
        env_id: &str,
    ) -> Result<OperationExecution, ManagerError> {
        let environment = self
            .inner
            .store
            .environment(env_id)?
            .ok_or(ManagerError::EnvironmentNotFound)?;
        if environment.status != "stopped" {
            return Err(ManagerError::EnvironmentNotReady(environment.status));
        }
        let execution = self
            .execute_sync_host_operation(
                "environment.local-data-cleanup",
                Some(env_id),
                "清理环境本地数据",
                json!({ "envs": [env_id] }),
                |request| HostCommand::BrowserCleanup { request },
            )
            .await?;
        let summary = summarize_environment_cleanup(&execution.response);
        self.inner.store.append_event(
            "environment.local-data-cleaned",
            Some(env_id),
            Some(&execution.operation.id),
            &summary,
        )?;
        Ok(OperationExecution {
            operation: execution.operation,
            response: summary,
        })
    }

    pub async fn capture_environment_diagnostic(
        &self,
        env_id: &str,
    ) -> Result<OperationExecution, ManagerError> {
        let environment = self
            .inner
            .store
            .environment(env_id)?
            .ok_or(ManagerError::EnvironmentNotFound)?;
        if environment.status != "ready" {
            return Err(ManagerError::EnvironmentNotReady(environment.status));
        }
        let execution = self
            .execute_sync_host_operation(
                "environment.page-diagnostic",
                Some(env_id),
                "页面诊断",
                json!({
                    "envId": env_id,
                    "includeHtml": false,
                    "includeScreenshot": false,
                    "emitEvents": false,
                    "maxPages": 32,
                }),
                |request| HostCommand::BrowserSnapshot { request },
            )
            .await?;
        let summary = summarize_browser_snapshot(&execution.response);
        self.inner.store.append_event(
            "environment.page-diagnostic.completed",
            Some(env_id),
            Some(&execution.operation.id),
            &summary,
        )?;
        Ok(OperationExecution {
            operation: execution.operation,
            response: summary,
        })
    }

    pub async fn refresh_kernels(&self) -> Result<OperationRecord, ManagerError> {
        let operation =
            self.inner
                .operations
                .enqueue("kernel.refresh", None, "刷新内核列表", 0, None)?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "reading local cores and SDK catalog")?;
        let result = async {
            let settings = self.inner.store.settings()?;
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let info = host
                .call(HostCommand::Info, Some(operation.id.clone()))
                .await?;
            let records = profiles::scan_kernels(Path::new(&settings.work_dir), Some(&info));
            self.inner.store.replace_kernel_records(&records)?;
            Ok::<usize, ManagerError>(records.len())
        }
        .await;
        match result {
            Ok(count) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, &format!("refreshed {count} kernel records"))?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn install_kernel(
        &self,
        input: KernelInstallInput,
    ) -> Result<OperationRecord, ManagerError> {
        let request = json!({
            "cores": [{ "major": input.major, "type": input.kernel_type }]
        });
        let operation = self.inner.operations.enqueue(
            "kernel.install",
            None,
            "安装或更新内核",
            0,
            Some(&request),
        )?;
        let _execution = self.inner.operations.acquire().await;
        self.inner
            .operations
            .start(&operation.id, "calling sdk_browser_install")?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            host.call(
                HostCommand::BrowserInstall { request },
                Some(operation.id.clone()),
            )
            .await
            .map_err(ManagerError::from)
        }
        .await;
        match result {
            Ok(response) => {
                let request_id = accepted_code(&response);
                Ok(self.inner.store.update_operation_progress(
                    &operation.id,
                    request_id,
                    "SDK accepted install; awaiting progress callback",
                )?)
            }
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn cleanup_kernel_cache(
        &self,
        major: Option<u32>,
    ) -> Result<OperationExecution, ManagerError> {
        let request = json!({
            "cores": major.map(|major| vec![json!({ "major": major })]).unwrap_or_default()
        });
        self.execute_sync_host_operation(
            "kernel.cache-cleanup",
            None,
            "清理内核缓存",
            request,
            |request| HostCommand::BrowserCleanup { request },
        )
        .await
    }

    pub fn uninstall_kernel(&self, id: &str) -> Result<OperationRecord, ManagerError> {
        if self
            .inner
            .store
            .list_environments()?
            .iter()
            .any(|environment| {
                matches!(
                    environment.status.as_str(),
                    "preparing" | "starting" | "ready" | "stopping"
                )
            })
        {
            return Err(ManagerError::KernelBusy);
        }
        let kernel = self
            .inner
            .store
            .list_kernel_records()?
            .into_iter()
            .find(|kernel| kernel.id == id)
            .ok_or(ManagerError::KernelNotFound)?;
        let operation = self.inner.operations.enqueue(
            "kernel.uninstall",
            None,
            "卸载本地内核",
            0,
            Some(&json!({ "kernelId": id })),
        )?;
        self.inner
            .operations
            .start(&operation.id, "removing local core")?;
        let result = (|| {
            let install_path = kernel
                .install_path
                .as_deref()
                .ok_or(ManagerError::KernelNotFound)?;
            let settings = self.inner.store.settings()?;
            let path = fs::canonicalize(install_path)?;
            let work_dir = fs::canonicalize(settings.work_dir)?;
            if !path.starts_with(&work_dir) {
                return Err(ManagerError::UnsafeKernelPath);
            }
            fs::remove_dir_all(path)?;
            self.inner.store.delete_kernel_record(id)?;
            Ok::<(), ManagerError>(())
        })();
        match result {
            Ok(()) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, "local core removed")?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub async fn retry_operation(
        &self,
        operation_id: &str,
    ) -> Result<OperationRecord, ManagerError> {
        let operation = self
            .inner
            .store
            .operation(operation_id)?
            .ok_or(ManagerError::OperationNotRetryable)?;
        if !matches!(operation.status.as_str(), "failed" | "cancelled") {
            return Err(ManagerError::OperationNotRetryable);
        }
        match operation.kind.as_str() {
            "environment.sync" => self.sync_environments().await,
            "runtime.reconcile" => self.reconcile_runtimes().await,
            "environment.start" => {
                self.start_environment(
                    operation
                        .env_id
                        .as_deref()
                        .ok_or(ManagerError::OperationNotRetryable)?,
                )
                .await
            }
            "environment.stop" => {
                self.stop_environment(
                    operation
                        .env_id
                        .as_deref()
                        .ok_or(ManagerError::OperationNotRetryable)?,
                )
                .await
            }
            "kernel.install" => {
                let request = operation
                    .request
                    .ok_or(ManagerError::OperationNotRetryable)?;
                let core = request
                    .pointer("/cores/0")
                    .ok_or(ManagerError::OperationNotRetryable)?;
                self.install_kernel(KernelInstallInput {
                    major: core
                        .get("major")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|value| value.try_into().ok())
                        .ok_or(ManagerError::OperationNotRetryable)?,
                    kernel_type: core
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                })
                .await
            }
            _ => Err(ManagerError::OperationNotRetryable),
        }
    }

    pub fn create_diagnostic_bundle(
        &self,
        output_path: &str,
    ) -> Result<OperationRecord, ManagerError> {
        let operation =
            self.inner
                .operations
                .enqueue("diagnostics.bundle", None, "生成诊断包", 0, None)?;
        self.inner
            .operations
            .start(&operation.id, "collecting diagnostics")?;
        let result = (|| {
            let file = fs::File::create(output_path)?;
            let mut archive = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let settings = self.inner.store.settings()?;
            let summary = json!({
                "generatedAt": chrono::Utc::now(),
                "databasePath": self.inner.store.path().display().to_string(),
                "settings": {
                    "dataDir": settings.data_dir,
                    "workDir": settings.work_dir,
                    "extensionDir": settings.extension_dir,
                    "logDir": settings.log_dir,
                    "sdkApiUrlConfigured": settings.sdk_api_url.is_some(),
                    "debug": settings.debug,
                    "startupPolicy": settings.startup_policy,
                    "embeddedMcpPort": settings.embedded_mcp_port,
                },
                "environments": self.inner.store.list_environments()?,
                "environmentCache": self.inner.store.environment_cache_status()?,
                "operations": self.inner.store.list_operations(200)?,
                "kernels": self.inner.store.list_kernel_records()?,
            });
            archive.start_file("summary.json", options)?;
            archive.write_all(&serde_json::to_vec_pretty(&summary)?)?;
            archive.finish()?;
            Ok::<(), ManagerError>(())
        })();
        match result {
            Ok(()) => Ok(self
                .inner
                .operations
                .succeed(&operation.id, "diagnostic bundle created")?),
            Err(error) => Ok(self.inner.operations.fail(
                &operation.id,
                manager_error_code(&error),
                &error.to_string(),
            )?),
        }
    }

    pub fn cancel_operation(&self, operation_id: &str) -> Result<OperationRecord, ManagerError> {
        let operation = self
            .inner
            .store
            .operation(operation_id)?
            .ok_or(ManagerError::OperationNotCancellable)?;
        if operation.status != "queued" {
            return Err(ManagerError::OperationNotCancellable);
        }
        Ok(self
            .inner
            .operations
            .cancel(operation_id, "cancelled by user")?)
    }

    pub fn events_since(&self, sequence: u64) -> Result<Vec<ManagerEvent>, ManagerError> {
        Ok(self.inner.store.events_since(sequence, 500)?)
    }

    pub fn update_settings(&self, mut settings: ManagerSettings) -> Result<(), ManagerError> {
        for path in [
            &settings.data_dir,
            &settings.work_dir,
            &settings.extension_dir,
            &settings.log_dir,
        ] {
            if path.trim().is_empty() {
                return Err(ManagerError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "configured directories must not be empty",
                )));
            }
            fs::create_dir_all(path)?;
        }
        if !matches!(
            settings.startup_policy.as_str(),
            "restore-none" | "reconcile"
        ) {
            return Err(ManagerError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsupported startup policy",
            )));
        }
        settings.ai_base_url = settings
            .ai_base_url
            .as_deref()
            .map(normalize_ai_base_url)
            .transpose()?;
        settings.ai_model = settings
            .ai_model
            .as_deref()
            .map(normalize_ai_model)
            .transpose()?;
        let current = self.inner.store.settings()?;
        let data_dir_changed = current.data_dir != settings.data_dir;
        if data_dir_changed {
            let destination = Path::new(&settings.data_dir).join("manager.sqlite3");
            self.inner.store.backup_to(&destination)?;
            copy_directory_if_exists(
                &platform::secrets_dir(Path::new(&current.data_dir)),
                &platform::secrets_dir(Path::new(&settings.data_dir)),
            )?;
            platform::set_configured_data_dir(Path::new(&settings.data_dir))?;
        }
        self.inner.store.update_settings(&settings)?;
        self.inner.store.append_event(
            "settings.updated",
            None,
            None,
            &json!({
                "workDir": settings.work_dir,
                "dataDir": settings.data_dir,
                "extensionDir": settings.extension_dir,
                "logDir": settings.log_dir,
                "sdkApiUrlConfigured": settings.sdk_api_url.is_some(),
                "debug": settings.debug,
                "startupPolicy": settings.startup_policy,
                "embeddedMcpPort": settings.embedded_mcp_port,
                "aiBaseUrlConfigured": settings.ai_base_url.is_some(),
                "aiModelConfigured": settings.ai_model.is_some(),
                "restartRequired": data_dir_changed,
            }),
        )?;
        Ok(())
    }

    pub async fn run_sdk_smoke(&self) -> Result<SmokeReport, ManagerError> {
        let should_restart = self.inner.runtime.lock().await.is_some();
        if should_restart {
            self.stop_runtime().await?;
        }
        let result = SdkHostClient::discover()?.smoke().await;
        if should_restart {
            let _ = self.start_runtime().await;
        }
        let report = result?;
        *self.inner.last_smoke.write().await = Some(report.clone());
        Ok(report)
    }

    async fn runtime_handle(&self) -> Result<RuntimeHost, ManagerError> {
        if self.inner.runtime.lock().await.is_none() {
            self.start_runtime().await?;
        }
        let host = self
            .inner
            .runtime
            .lock()
            .await
            .clone()
            .ok_or(ManagerError::RuntimeNotRunning)?;
        if host.status().state != RuntimeHostState::Running {
            return Err(ManagerError::RuntimeNotRunning);
        }
        Ok(host)
    }

    fn proxy_url_for_diagnostics(&self, profile_id: &str) -> Result<String, ManagerError> {
        self.proxy_url_for_profile(profile_id)
    }

    fn proxy_url_for_create(&self, profile_id: &str) -> Result<String, ManagerError> {
        self.proxy_url_for_profile(profile_id)
    }

    fn proxy_url_for_profile(&self, profile_id: &str) -> Result<String, ManagerError> {
        let profile = self
            .inner
            .store
            .list_proxy_profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or(ManagerError::ProxyNotFound)?;
        let settings = self.inner.store.settings()?;
        let password = self
            .inner
            .store
            .proxy_secret_ref(profile_id)?
            .map(|reference| {
                platform::read_secret(Path::new(&settings.data_dir), &reference)
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            })
            .transpose()?;
        Ok(profiles::proxy_url(
            &profile.scheme,
            &profile.host,
            profile.port,
            profile.username.as_deref(),
            password.as_deref(),
        ))
    }

    async fn execute_sync_host_operation<F>(
        &self,
        kind: &str,
        env_id: Option<&str>,
        label: &str,
        request: serde_json::Value,
        command: F,
    ) -> Result<OperationExecution, ManagerError>
    where
        F: FnOnce(serde_json::Value) -> HostCommand,
    {
        let operation = self
            .inner
            .operations
            .enqueue(kind, env_id, label, 0, Some(&request))?;
        let _execution = self.inner.operations.acquire().await;
        self.inner.operations.start(&operation.id, "calling SDK")?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            Ok::<serde_json::Value, ManagerError>(
                host.call(command(request), Some(operation.id.clone()))
                    .await?,
            )
        }
        .await;
        match result {
            Ok(response) => Ok(OperationExecution {
                operation: self
                    .inner
                    .operations
                    .succeed(&operation.id, "SDK call completed")?,
                response,
            }),
            Err(error) => {
                self.inner.operations.fail(
                    &operation.id,
                    manager_error_code(&error),
                    &error.to_string(),
                )?;
                Err(error)
            }
        }
    }

    async fn execute_environment_operation(
        &self,
        env_id: &str,
        start: bool,
    ) -> Result<OperationRecord, ManagerError> {
        let environment = self
            .inner
            .store
            .environment(env_id)?
            .ok_or(ManagerError::EnvironmentNotFound)?;
        let action = if start {
            EnvironmentBatchAction::Start
        } else {
            EnvironmentBatchAction::Stop
        };
        if !environment_action_state_allowed(action, &environment.status) {
            return Err(ManagerError::InvalidEnvironmentTransition {
                action: format!("{action:?}").to_ascii_lowercase(),
                state: environment.status,
            });
        }
        let operation = self.prepare_environment_operation(env_id, start)?;
        let _execution = self.inner.operations.acquire().await;
        self.inner.operations.start(&operation.id, "calling SDK")?;
        let preparing_status = if start { "preparing" } else { "stopping" };
        self.inner.store.set_environment_runtime(RuntimeUpdate {
            env_id,
            generation: operation.generation,
            status: preparing_status,
            request_id: None,
            operation_id: Some(&operation.id),
            cdp: "-",
            last_event: "calling SDK",
        })?;
        let result = async {
            let host = self.runtime_handle().await?;
            self.ensure_sdk_initialized(&host).await?;
            let command = if start {
                HostCommand::BrowserOpen {
                    request: json!({ "envs": [{ "envId": env_id }] }),
                }
            } else {
                HostCommand::BrowserClose {
                    request: json!({ "envs": [env_id] }),
                }
            };
            let response = host.call(command, Some(operation.id.clone())).await?;
            let request_id = response
                .get("requestId")
                .and_then(serde_json::Value::as_i64)
                .and_then(|value| i32::try_from(value).ok());
            self.accept_environment_operation(&operation.id, request_id)
        }
        .await;

        match result {
            Ok(operation) => Ok(operation),
            Err(error) => {
                let failed = self.inner.operations.fail(
                    &operation.id,
                    manager_error_code(&error),
                    &error.to_string(),
                )?;
                let message = error.to_string();
                self.inner.store.set_environment_runtime(RuntimeUpdate {
                    env_id,
                    generation: operation.generation,
                    status: "failed",
                    request_id: None,
                    operation_id: None,
                    cdp: "-",
                    last_event: &message,
                })?;
                Ok(failed)
            }
        }
    }

    async fn fetch_all_environments(
        &self,
        host: &RuntimeHost,
        operation_id: Option<String>,
    ) -> Result<Vec<(String, String, serde_json::Value)>, ManagerError> {
        let mut accumulator = EnvironmentPageAccumulator::default();
        for page in 1..=MAX_ENVIRONMENT_PAGES {
            let value = host
                .call(
                    HostCommand::EnvPage {
                        request: sdk_ffi::env_page_request(
                            page as u64,
                            ENVIRONMENT_PAGE_SIZE as u64,
                        ),
                    },
                    operation_id.clone(),
                )
                .await?;
            ensure_backend_success("environment page", &value)?;
            if accumulator.push(&value)? {
                return Ok(accumulator.rows);
            }
        }
        Err(ManagerError::InvalidHostResponse(format!(
            "environment pagination exceeded {MAX_ENVIRONMENT_PAGES} pages"
        )))
    }

    async fn ensure_sdk_initialized(&self, host: &RuntimeHost) -> Result<(), ManagerError> {
        if *self.inner.sdk_initialized.read().await {
            return Ok(());
        }
        let _guard = self.inner.sdk_init_lock.lock().await;
        if *self.inner.sdk_initialized.read().await {
            return Ok(());
        }
        let settings = self.inner.store.settings()?;
        let port = settings.embedded_mcp_port.or_else(embedded_port);
        host.initialize(
            settings.work_dir,
            port,
            settings.sdk_api_url,
            settings.debug,
        )
        .await?;
        *self.inner.sdk_initialized.write().await = true;
        *self.inner.active_mcp_port.write().await = port;
        self.inner.store.append_event(
            "sdk.initialized",
            None,
            None,
            &json!({ "embeddedPort": port }),
        )?;
        Ok(())
    }

    fn monitor_host(&self, host: &RuntimeHost) {
        let weak = Arc::downgrade(&self.inner);
        let event_host = host.clone();
        let mut events = host.subscribe_events();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        let Some(inner) = weak.upgrade() else {
                            break;
                        };
                        let refresh_cdp = inner.store.apply_host_event(&event).is_ok()
                            && event
                                .event_name
                                .to_ascii_lowercase()
                                .contains("browser-open-success")
                            && event.env_id.as_deref().is_some_and(|env_id| {
                                inner.store.environment(env_id).ok().flatten().is_some_and(
                                    |environment| !mirror::is_cdp_endpoint(&environment.cdp),
                                )
                            });
                        if refresh_cdp && let Some(env_id) = event.env_id.clone() {
                            let refresh_inner = inner.clone();
                            let refresh_host = event_host.clone();
                            tokio::spawn(async move {
                                if let Ok(detail) = refresh_host
                                    .call(
                                        HostCommand::EnvGetInfo {
                                            request: json!({ "envId": env_id }),
                                        },
                                        None,
                                    )
                                    .await
                                    && ensure_backend_success("environment detail", &detail).is_ok()
                                    && hydrate_runtime_cdp(
                                        &refresh_inner.store,
                                        &env_id,
                                        &detail,
                                        "sdk_env_getinfo",
                                    )
                                    .unwrap_or(false)
                                {
                                    return;
                                }
                                for attempt in 0..8 {
                                    if attempt > 0 {
                                        tokio::time::sleep(std::time::Duration::from_millis(500))
                                            .await;
                                    }
                                    let Ok(value) =
                                        refresh_host.call(HostCommand::BrowserInfo, None).await
                                    else {
                                        continue;
                                    };
                                    let running = mirror::running_environments(&value);
                                    if !running.contains_key(&env_id) {
                                        continue;
                                    }
                                    let observed = mirror::observed_environment_ids(&value);
                                    let _ = refresh_inner
                                        .store
                                        .reconcile_running_environments(&running, &observed);
                                    break;
                                }
                            });
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let Some(inner) = weak.upgrade() else {
                            break;
                        };
                        let _ = inner.store.append_event(
                            "runtime.events-lagged",
                            None,
                            None,
                            &json!({ "skipped": skipped }),
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let weak = Arc::downgrade(&self.inner);
        let mut status = host.subscribe_status();
        tokio::spawn(async move {
            while status.changed().await.is_ok() {
                let current = status.borrow().clone();
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                *inner.last_runtime_status.write().await = current.clone();
                if current.state == RuntimeHostState::Degraded {
                    *inner.sdk_initialized.write().await = false;
                    *inner.active_mcp_port.write().await = None;
                    let message = current
                        .last_error
                        .as_deref()
                        .unwrap_or("runtime host degraded");
                    let _ = inner.store.mark_host_degraded(message);
                }
            }
        });
    }
}

fn hydrate_runtime_cdp(
    store: &ManagerStore,
    env_id: &str,
    value: &serde_json::Value,
    source: &str,
) -> Result<bool, store::StoreError> {
    let Some(cdp) = mirror::cdp_endpoint(value) else {
        return Ok(false);
    };
    store.hydrate_environment_cdp(env_id, &cdp, source)
}

fn environment_create_operation_request(input: &EnvironmentCreateInput) -> serde_json::Value {
    json!({
        "proxyProfileId": input.proxy_profile_id,
        "kernelId": input.kernel_id,
    })
}

fn build_environment_metadata_update_request(
    input: &EnvironmentMetadataUpdateInput,
) -> Result<serde_json::Value, ManagerError> {
    let env_id = input.env_id.trim();
    if env_id.is_empty() {
        return Err(ManagerError::InvalidEnvironmentMetadata(
            "envId must not be empty".into(),
        ));
    }
    let env_name = input.env_name.trim();
    if env_name.is_empty() {
        return Err(ManagerError::InvalidEnvironmentMetadata(
            "environment name must not be empty".into(),
        ));
    }
    if env_name.chars().count() > 32 {
        return Err(ManagerError::InvalidEnvironmentMetadata(
            "environment name must not exceed 32 characters".into(),
        ));
    }
    let serial = input.serial.trim();
    if serial.len() > 64 {
        return Err(ManagerError::InvalidEnvironmentMetadata(
            "serial must not exceed 64 UTF-8 bytes".into(),
        ));
    }
    Ok(json!({
        "envId": env_id,
        "envName": env_name,
        "serial": serial,
    }))
}

fn confirmed_environment_metadata(
    response: &serde_json::Value,
    request: &serde_json::Value,
) -> Result<(String, String), ManagerError> {
    let data = response
        .get("data")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            ManagerError::InvalidHostResponse(
                "environment metadata update response did not contain an object data field".into(),
            )
        })?;
    let confirmed_env_id = data
        .get("envId")
        .and_then(|value| match value {
            serde_json::Value::String(value) => Some(value.clone()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .ok_or_else(|| {
            ManagerError::InvalidHostResponse(
                "environment metadata update response did not confirm envId".into(),
            )
        })?;
    let confirmed_name = data
        .get("envName")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ManagerError::InvalidHostResponse(
                "environment metadata update response did not confirm envName".into(),
            )
        })?;
    let confirmed_serial = data
        .get("serial")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ManagerError::InvalidHostResponse(
                "environment metadata update response did not confirm serial".into(),
            )
        })?;
    if request.get("envId").and_then(serde_json::Value::as_str) != Some(confirmed_env_id.as_str())
        || request.get("envName").and_then(serde_json::Value::as_str) != Some(confirmed_name)
        || request.get("serial").and_then(serde_json::Value::as_str) != Some(confirmed_serial)
    {
        return Err(ManagerError::InvalidHostResponse(
            "environment metadata update response did not match the requested values".into(),
        ));
    }
    Ok((confirmed_name.into(), confirmed_serial.into()))
}

fn merge_confirmed_environment_metadata(
    rows: &mut [(String, String, serde_json::Value)],
    env_id: &str,
    env_name: &str,
    serial: &str,
) {
    let Some((_, name, remote)) = rows.iter_mut().find(|(id, _, _)| id == env_id) else {
        return;
    };
    *name = env_name.into();
    if let Some(remote) = remote.as_object_mut() {
        remote.insert("envName".into(), env_name.into());
        remote.insert("serial".into(), serial.into());
    }
}

fn validate_environment_kernel(kernel: &KernelRecord) -> Result<(), ManagerError> {
    if kernel.major.is_none() {
        return Err(ManagerError::KernelNotUsable(
            "kernel version is missing".into(),
        ));
    }
    if kernel.install_path.is_none()
        || !matches!(kernel.status.as_str(), "installed" | "update-available")
    {
        return Err(ManagerError::KernelNotUsable(
            "kernel is not installed locally".into(),
        ));
    }
    if normalize_platform(&kernel.platform) != normalize_platform(std::env::consts::OS) {
        return Err(ManagerError::KernelNotUsable(format!(
            "kernel platform {} does not match {}",
            kernel.platform,
            std::env::consts::OS
        )));
    }
    if normalize_arch(&kernel.arch) != normalize_arch(std::env::consts::ARCH) {
        return Err(ManagerError::KernelNotUsable(format!(
            "kernel architecture {} does not match {}",
            kernel.arch,
            std::env::consts::ARCH
        )));
    }
    backend_kernel_name(&kernel.kernel_type)?;
    Ok(())
}

fn validate_environment_batch_shape(input: &EnvironmentBatchInput) -> Result<(), ManagerError> {
    if input.env_ids.is_empty() {
        return Err(ManagerError::InvalidEnvironmentBatch(
            "at least one environment is required".into(),
        ));
    }
    if input.env_ids.len() > MAX_ENVIRONMENT_BATCH_SIZE {
        return Err(ManagerError::InvalidEnvironmentBatch(format!(
            "at most {MAX_ENVIRONMENT_BATCH_SIZE} environments are allowed"
        )));
    }
    let unique = input.env_ids.iter().collect::<HashSet<_>>();
    if unique.len() != input.env_ids.len() {
        return Err(ManagerError::InvalidEnvironmentBatch(
            "duplicate environment ids are not allowed".into(),
        ));
    }
    Ok(())
}

fn validate_environment_batch_states(
    input: &EnvironmentBatchInput,
    environments: &[EnvironmentRecord],
) -> Result<(), ManagerError> {
    if environments.len() != input.env_ids.len() {
        return Err(ManagerError::InvalidEnvironmentBatch(
            "environment preflight was incomplete".into(),
        ));
    }
    for environment in environments {
        if !environment_action_state_allowed(input.action, &environment.status) {
            return Err(ManagerError::InvalidEnvironmentBatch(format!(
                "environment {} cannot {:?} from state {}",
                environment.env_id, input.action, environment.status
            )));
        }
    }
    Ok(())
}

fn environment_action_state_allowed(action: EnvironmentBatchAction, status: &str) -> bool {
    match action {
        EnvironmentBatchAction::Start => matches!(status, "stopped" | "failed"),
        EnvironmentBatchAction::Stop => matches!(status, "ready" | "starting"),
    }
}

fn build_environment_create_request(
    kernel: &KernelRecord,
    proxy: Option<&str>,
) -> Result<serde_json::Value, ManagerError> {
    validate_environment_kernel(kernel)?;
    let mut request = json!({
        "kernel": backend_kernel_name(&kernel.kernel_type)?,
        "kernelVersion": kernel.major.expect("validated kernel major").to_string(),
    });
    if let Some(proxy) = proxy.filter(|value| !value.trim().is_empty()) {
        request["proxy"] = json!(proxy);
    }
    Ok(request)
}

fn backend_kernel_name(kernel_type: &str) -> Result<&'static str, ManagerError> {
    match kernel_type.trim().to_ascii_lowercase().as_str() {
        "chrome" => Ok("Chrome"),
        "firefox" => Ok("Firefox"),
        "chromium" => Ok("Chromium"),
        "broium" => Ok("Broium"),
        other => Err(ManagerError::KernelNotUsable(format!(
            "unsupported kernel type {other}"
        ))),
    }
}

fn normalize_platform(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "win" | "win32" | "windows" => "windows",
        "mac" | "macos" | "darwin" => "macos",
        "linux" => "linux",
        _ => "unknown",
    }
}

fn normalize_arch(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x64" | "x86_64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        "x86" | "i386" | "i686" => "x86",
        _ => "unknown",
    }
}

fn ensure_backend_success(action: &str, response: &serde_json::Value) -> Result<(), ManagerError> {
    let code = response.get("code").and_then(value_as_i64).ok_or_else(|| {
        ManagerError::InvalidHostResponse(format!(
            "{action} response did not contain a numeric code"
        ))
    })?;
    if code == 200 {
        return Ok(());
    }
    let message = response
        .get("msg")
        .or_else(|| response.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(redacted_response_text)
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "request failed".into());
    Err(ManagerError::BackendRejected(format!(
        "{action} returned code {code}: {message}"
    )))
}

fn value_as_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn redacted_response_text(text: &str) -> String {
    let mut value = serde_json::Value::String(text.to_string());
    sdk_ffi::redact_value(&mut value);
    value
        .as_str()
        .unwrap_or("[redacted]")
        .chars()
        .take(256)
        .collect()
}

fn summarize_environment_cleanup(response: &serde_json::Value) -> serde_json::Value {
    let data = response.get("data").unwrap_or(response);
    let deferred = data
        .get("results")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("deferredDelete")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or_default();
    json!({
        "deleted": data.get("deleted").and_then(value_as_i64).unwrap_or_default(),
        "notFound": data.get("notFound").and_then(value_as_i64).unwrap_or_default(),
        "failed": data.get("failed").and_then(value_as_i64).unwrap_or_default(),
        "deferred": deferred,
    })
}

fn summarize_fingerprint_check(response: &serde_json::Value) -> serde_json::Value {
    json!({
        "opened": response.get("newTab").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "newTab": response.get("newTab").and_then(serde_json::Value::as_bool).unwrap_or(false),
        "source": match response.get("source").and_then(serde_json::Value::as_str) {
            Some("embedded-memory") => "embedded",
            _ => "unknown",
        },
    })
}

fn summarize_browser_snapshot(response: &serde_json::Value) -> serde_json::Value {
    let pages = response
        .get("pages")
        .and_then(serde_json::Value::as_array)
        .map(|pages| {
            pages
                .iter()
                .map(|page| {
                    json!({
                        "status": page.get("status").and_then(serde_json::Value::as_str).unwrap_or("unknown"),
                        "origin": page.get("url")
                            .or_else(|| page.get("targetUrl"))
                            .and_then(serde_json::Value::as_str)
                            .map(safe_url_origin)
                            .unwrap_or_else(|| "unknown".into()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let reported_count = response
        .get("pageCount")
        .and_then(value_as_i64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(pages.len());
    let failed_pages = pages
        .iter()
        .filter(|page| page.get("status").and_then(serde_json::Value::as_str) != Some("ok"))
        .count();
    json!({
        "status": response.get("status").and_then(serde_json::Value::as_str).unwrap_or("unknown"),
        "pageCount": reported_count,
        "failedPages": failed_pages,
        "pages": pages,
        "htmlIncluded": false,
        "screenshotIncluded": false,
    })
}

fn safe_url_origin(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|host| {
                    let port = url
                        .port()
                        .map(|port| format!(":{port}"))
                        .unwrap_or_default();
                    format!("{}://{host}{port}", url.scheme())
                })
                .or_else(|| (url.scheme() == "about").then(|| format!("about:{}", url.path())))
        })
        .unwrap_or_else(|| "internal".into())
}

fn external_cdp_origin(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "-" || value == "ready" {
        return None;
    }
    let candidate = if value.contains("://") {
        value.to_owned()
    } else {
        if !value.contains(':') {
            return None;
        }
        format!("http://{value}")
    };
    let Ok(url) = url::Url::parse(&candidate) else {
        return None;
    };
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss") {
        return None;
    }
    let host = url.host_str()?;
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.into()
    };
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    Some(format!("{}://{host}{port}", url.scheme()))
}

fn normalize_ai_base_url(value: &str) -> Result<String, ManagerError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ManagerError::InvalidAiProvider(
            "OpenAI-compatible Base URL must not be empty".into(),
        ));
    }
    let url = url::Url::parse(value).map_err(|error| {
        ManagerError::InvalidAiProvider(format!("Base URL is not valid: {error}"))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ManagerError::InvalidAiProvider(
            "Base URL must use http or https".into(),
        ));
    }
    if url.host_str().is_none() {
        return Err(ManagerError::InvalidAiProvider(
            "Base URL must include a host".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ManagerError::InvalidAiProvider(
            "Base URL must not include credentials, query, or fragment".into(),
        ));
    }
    Ok(value.trim_end_matches('/').into())
}

fn normalize_ai_model(value: &str) -> Result<String, ManagerError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ManagerError::InvalidAiProvider(
            "model must not be empty".into(),
        ));
    }
    if value.len() > 128 {
        return Err(ManagerError::InvalidAiProvider(
            "model must be 128 characters or fewer".into(),
        ));
    }
    Ok(value.into())
}

fn response_env_id(response: &serde_json::Value) -> Option<String> {
    response
        .pointer("/data/envId")
        .or_else(|| response.get("envId"))
        .and_then(|value| match value {
            serde_json::Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
}

fn response_environment_name(response: &serde_json::Value) -> Option<String> {
    response
        .pointer("/data/envName")
        .or_else(|| response.get("envName"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn embedded_port() -> Option<u16> {
    std::env::var("BROSDK_EMBEDDED_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
}

fn accepted_code(value: &serde_json::Value) -> Option<i32> {
    value
        .get("acceptedCode")
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn require_prompt(prompt: &str) -> Result<&str, ManagerError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(ManagerError::InvalidAgentPlan(
            "prompt must not be empty".into(),
        ));
    }
    Ok(prompt)
}

fn validate_ai_history(history: &[AiConversationMessage]) -> Result<(), ManagerError> {
    if history.len() > MAX_AI_HISTORY_MESSAGES {
        return Err(ManagerError::InvalidAgentPlan(format!(
            "conversation history exceeds {MAX_AI_HISTORY_MESSAGES} messages"
        )));
    }
    let mut total_bytes = 0usize;
    for message in history {
        if !matches!(message.role.as_str(), "user" | "assistant") {
            return Err(ManagerError::InvalidAgentPlan(
                "conversation history role must be user or assistant".into(),
            ));
        }
        let content = message.content.trim();
        if content.is_empty() {
            return Err(ManagerError::InvalidAgentPlan(
                "conversation history content must not be empty".into(),
            ));
        }
        let bytes = content.len();
        if bytes > MAX_AI_HISTORY_MESSAGE_BYTES {
            return Err(ManagerError::InvalidAgentPlan(format!(
                "conversation message exceeds {MAX_AI_HISTORY_MESSAGE_BYTES} bytes"
            )));
        }
        total_bytes = total_bytes.saturating_add(bytes);
    }
    if total_bytes > MAX_AI_HISTORY_BYTES {
        return Err(ManagerError::InvalidAgentPlan(format!(
            "conversation history exceeds {MAX_AI_HISTORY_BYTES} bytes"
        )));
    }
    Ok(())
}

fn resolve_agent_target(
    prompt: &str,
    selected_env_id: Option<&str>,
    environments: &[EnvironmentRecord],
) -> Result<Option<String>, ManagerError> {
    let mentioned = environments
        .iter()
        .filter(|environment| prompt_mentions_env_id(prompt, &environment.env_id))
        .map(|environment| environment.env_id.clone())
        .collect::<Vec<_>>();
    if mentioned.len() > 1 {
        return Err(ManagerError::InvalidAgentPlan(
            "one Agent plan can target only one environment".into(),
        ));
    }
    if let Some(env_id) = mentioned.into_iter().next() {
        return Ok(Some(env_id));
    }
    let Some(env_id) = selected_env_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if !environments
        .iter()
        .any(|environment| environment.env_id == env_id)
    {
        return Err(ManagerError::EnvironmentNotFound);
    }
    Ok(Some(env_id.into()))
}

fn prompt_mentions_env_id(prompt: &str, env_id: &str) -> bool {
    prompt.match_indices(env_id).any(|(start, _)| {
        let before = prompt[..start].chars().next_back();
        let after = prompt[start + env_id.len()..].chars().next();
        before.is_none_or(|value| !is_env_id_character(value))
            && after.is_none_or(|value| !is_env_id_character(value))
    })
}

fn is_env_id_character(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '-' | '_')
}

fn prepare_agent_plan(
    plan: &mut AiAgentPlan,
    resolved_env_id: Option<&str>,
    environments: &[EnvironmentRecord],
) -> Result<(), ManagerError> {
    let requested_env_id = resolved_env_id.or_else(|| {
        plan.env_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    });
    let environment_bound = matches!(
        plan.action.as_str(),
        "environment.start" | "environment.stop" | "environment.diagnose" | "mcp.read"
    ) || (plan.action == "mcp.call" && requested_env_id.is_some());
    if environment_bound {
        let env_id = requested_env_id
            .ok_or_else(|| ManagerError::InvalidAgentPlan("envId is required".into()))?;
        let environment = environments
            .iter()
            .find(|environment| environment.env_id == env_id)
            .ok_or(ManagerError::EnvironmentNotFound)?;
        if plan.action == "environment.start"
            && !environment_action_state_allowed(EnvironmentBatchAction::Start, &environment.status)
        {
            return Err(ManagerError::InvalidEnvironmentTransition {
                action: "start".into(),
                state: environment.status.clone(),
            });
        }
        if plan.action == "environment.stop"
            && !environment_action_state_allowed(EnvironmentBatchAction::Stop, &environment.status)
        {
            return Err(ManagerError::InvalidEnvironmentTransition {
                action: "stop".into(),
                state: environment.status.clone(),
            });
        }
        if matches!(
            plan.action.as_str(),
            "environment.diagnose" | "mcp.read" | "mcp.call"
        ) && environment.status != "ready"
        {
            return Err(ManagerError::AgentStateMismatch {
                expected: "ready".into(),
                actual: environment.status.clone(),
            });
        }
        plan.env_id = Some(environment.env_id.clone());
        plan.expected_state = Some(environment.status.clone());
    } else {
        plan.env_id = None;
        plan.expected_state = None;
    }
    plan.idempotency_key = uuid::Uuid::new_v4().to_string();
    Ok(())
}

fn allowed_agent_actions() -> &'static [&'static str] {
    &[
        "none",
        "environment.start",
        "environment.stop",
        "environment.sync",
        "runtime.reconcile",
        "proxy.diagnose",
        "environment.diagnose",
        "mcp.read",
        "mcp.call",
    ]
}

fn validate_agent_plan(plan: &AiAgentPlan) -> Result<(), ManagerError> {
    if !allowed_agent_actions().contains(&plan.action.as_str()) {
        return Err(ManagerError::InvalidAgentPlan(format!(
            "unsupported action {}",
            plan.action
        )));
    }
    if plan.idempotency_key.trim().is_empty() {
        return Err(ManagerError::InvalidAgentPlan(
            "idempotencyKey must not be empty".into(),
        ));
    }
    if plan.action.starts_with("environment.")
        && plan.action != "environment.sync"
        && plan.env_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(ManagerError::InvalidAgentPlan(
            "envId is required for environment actions".into(),
        ));
    }
    if plan.action == "mcp.read" && plan.env_id.as_deref().is_none_or(str::is_empty) {
        return Err(ManagerError::InvalidAgentPlan(
            "envId is required for mcp.read".into(),
        ));
    }
    if matches!(plan.action.as_str(), "mcp.read" | "mcp.call")
        && plan
            .arguments
            .get("tool")
            .and_then(serde_json::Value::as_str)
            .is_none_or(str::is_empty)
    {
        return Err(ManagerError::InvalidAgentPlan(format!(
            "{} requires arguments.tool",
            plan.action
        )));
    }
    Ok(())
}

fn required_env_id(plan: &AiAgentPlan) -> Result<&str, ManagerError> {
    plan.env_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ManagerError::InvalidAgentPlan("envId is required".into()))
}

fn agent_plan_hash(plan: &AiAgentPlan) -> Result<String, ManagerError> {
    let serialized = serde_json::to_vec(plan)?;
    Ok(format!("{:x}", Sha256::digest(serialized)))
}

fn replay_agent_execution(
    previous: store::StoredAgentExecution,
    plan_hash: &str,
) -> Result<AiAgentExecution, ManagerError> {
    if previous.plan_hash != plan_hash {
        return Err(ManagerError::InvalidAgentPlan(
            "idempotencyKey was already used for a different plan".into(),
        ));
    }
    if previous.state != "completed" {
        return Err(ManagerError::AgentExecutionUncertain);
    }
    Ok(AiAgentExecution {
        replayed: true,
        ..previous.execution
    })
}

fn validate_global_mcp_tool_call(
    tool: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, ManagerError> {
    let object = mcp_argument_object(&arguments)?;
    match tool {
        "sdk.health" | "sdk.info" => {
            ensure_mcp_keys(object, &[])?;
            Ok(json!({}))
        }
        "env.list" => {
            ensure_mcp_keys(object, &["page", "pageSize"])?;
            Ok(json!({
                "page": mcp_bounded_u64(object, "page", 1, 1_000_000, 1)?,
                "pageSize": mcp_bounded_u64(object, "pageSize", 1, 200, 50)?,
            }))
        }
        "env.resolve" => {
            ensure_mcp_keys(object, &["envId", "query"])?;
            match (object.get("envId"), object.get("query")) {
                (Some(env_id), None) => Ok(json!({
                    "envId": validate_mcp_env_id(mcp_required_string(env_id, "envId", 32)?)?
                })),
                (None, Some(query)) => Ok(json!({
                    "query": mcp_required_string(query, "query", 128)?
                })),
                _ => Err(ManagerError::InvalidMcpArguments(
                    "env.resolve requires exactly one of envId or query".into(),
                )),
            }
        }
        "env.get" | "mcp.endpoint" => {
            ensure_mcp_keys(object, &["envId"])?;
            let env_id = object
                .get("envId")
                .ok_or_else(|| ManagerError::InvalidMcpArguments("envId is required".into()))?;
            Ok(json!({
                "envId": validate_mcp_env_id(mcp_required_string(env_id, "envId", 32)?)?
            }))
        }
        "browser.status" => {
            ensure_mcp_keys(object, &["envId"])?;
            if let Some(env_id) = object.get("envId") {
                Ok(json!({
                    "envId": validate_mcp_env_id(mcp_required_string(env_id, "envId", 32)?)?
                }))
            } else {
                Ok(json!({}))
            }
        }
        "task.list" => {
            ensure_mcp_keys(object, &["limit"])?;
            Ok(json!({
                "limit": mcp_bounded_u64(object, "limit", 1, 100, 50)?
            }))
        }
        "task.get" => {
            ensure_mcp_keys(object, &["taskId"])?;
            let task_id = object
                .get("taskId")
                .ok_or_else(|| ManagerError::InvalidMcpArguments("taskId is required".into()))?;
            Ok(json!({ "taskId": mcp_required_string(task_id, "taskId", 128)? }))
        }
        _ => Err(ManagerError::McpToolNotAllowed(format!("global:{tool}"))),
    }
}

fn validate_environment_mcp_read_tool_call(
    tool: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, ManagerError> {
    let tool = environment_mcp_base_tool(tool)?;
    if !ENVIRONMENT_MCP_READ_TOOLS.contains(&tool) {
        return Err(ManagerError::McpToolNotAllowed(format!(
            "environment:{tool}"
        )));
    }
    let object = mcp_argument_object(&arguments)?;
    match tool {
        "browser_state" => {
            ensure_mcp_keys(object, &["action", "sinceSeq", "timeoutMs"])?;
            let action = object
                .get("action")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("get");
            if action == "get" {
                if object.contains_key("sinceSeq") || object.contains_key("timeoutMs") {
                    return Err(ManagerError::InvalidMcpArguments(
                        "browser_state get does not accept wait parameters".into(),
                    ));
                }
                return Ok(json!({ "action": "get" }));
            }
            if action != "wait" {
                return Err(ManagerError::InvalidMcpArguments(
                    "browser_state only allows action=get or action=wait".into(),
                ));
            }
            let since_seq = object.get("sinceSeq").ok_or_else(|| {
                ManagerError::InvalidMcpArguments("browser_state wait requires sinceSeq".into())
            })?;
            let since_seq = since_seq.as_u64().ok_or_else(|| {
                ManagerError::InvalidMcpArguments("sinceSeq must be an integer".into())
            })?;
            Ok(json!({
                "action": "wait",
                "sinceSeq": since_seq,
                "timeoutMs": mcp_bounded_u64(object, "timeoutMs", 1, 30_000, 10_000)?,
            }))
        }
        "tabs" => {
            ensure_mcp_keys(object, &["action"])?;
            let action = object
                .get("action")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("list");
            if !matches!(action, "list" | "current") {
                return Err(ManagerError::InvalidMcpArguments(
                    "tabs only allows action=list or action=current".into(),
                ));
            }
            Ok(json!({ "action": action }))
        }
        "snapshot" | "diff" => {
            ensure_mcp_keys(object, &["page", "domFallback", "maxNodes", "maxTextBytes"])?;
            let mut normalized = serde_json::Map::new();
            normalized.insert("page".into(), json!(mcp_required_page(object)?));
            if let Some(value) = object.get("domFallback") {
                normalized.insert("domFallback".into(), json!(mcp_bool(value, "domFallback")?));
            }
            if object.contains_key("maxNodes") {
                normalized.insert(
                    "maxNodes".into(),
                    json!(mcp_bounded_u64(object, "maxNodes", 1, 2_000, 2_000)?),
                );
            }
            if object.contains_key("maxTextBytes") {
                normalized.insert(
                    "maxTextBytes".into(),
                    json!(mcp_bounded_u64(
                        object,
                        "maxTextBytes",
                        1,
                        1_048_576,
                        1_048_576,
                    )?),
                );
            }
            Ok(serde_json::Value::Object(normalized))
        }
        "read" => {
            ensure_mcp_keys(object, &["page"])?;
            Ok(json!({ "page": mcp_required_page(object)? }))
        }
        "grep" => {
            ensure_mcp_keys(object, &["page", "pattern", "over", "limit"])?;
            let pattern = object.get("pattern").ok_or_else(|| {
                ManagerError::InvalidMcpArguments("grep pattern is required".into())
            })?;
            let mut normalized = serde_json::Map::from_iter([
                ("page".into(), json!(mcp_required_page(object)?)),
                (
                    "pattern".into(),
                    json!(mcp_required_string(pattern, "pattern", 256)?),
                ),
            ]);
            if let Some(over) = object.get("over") {
                let over = mcp_required_string(over, "over", 16)?;
                if !matches!(over.as_str(), "ax" | "content") {
                    return Err(ManagerError::InvalidMcpArguments(
                        "grep over must be ax or content".into(),
                    ));
                }
                normalized.insert("over".into(), json!(over));
            }
            if object.contains_key("limit") {
                normalized.insert(
                    "limit".into(),
                    json!(mcp_bounded_u64(object, "limit", 1, 200, 100)?),
                );
            }
            Ok(serde_json::Value::Object(normalized))
        }
        "screenshot" => {
            ensure_mcp_keys(object, &["page", "format", "quality", "size", "fullPage"])?;
            if object
                .get("fullPage")
                .map(|value| mcp_bool(value, "fullPage"))
                .transpose()?
                == Some(true)
            {
                return Err(ManagerError::InvalidMcpArguments(
                    "full-page screenshots are not allowed by Manager policy".into(),
                ));
            }
            let mut normalized =
                serde_json::Map::from_iter([("page".into(), json!(mcp_required_page(object)?))]);
            if let Some(format) = object.get("format") {
                let format = mcp_required_string(format, "format", 8)?;
                if !matches!(format.as_str(), "jpeg" | "png" | "webp") {
                    return Err(ManagerError::InvalidMcpArguments(
                        "screenshot format must be jpeg, png or webp".into(),
                    ));
                }
                normalized.insert("format".into(), json!(format));
            }
            if object.contains_key("quality") {
                normalized.insert(
                    "quality".into(),
                    json!(mcp_bounded_u64(object, "quality", 0, 100, 80)?),
                );
            }
            if let Some(size) = object.get("size") {
                let size = size.as_object().ok_or_else(|| {
                    ManagerError::InvalidMcpArguments("screenshot size must be an object".into())
                })?;
                ensure_mcp_keys(size, &["width", "height"])?;
                let width = mcp_bounded_u64(size, "width", 1, 2_048, 1_280)?;
                let height = mcp_bounded_u64(size, "height", 1, 2_048, 720)?;
                normalized.insert("size".into(), json!({ "width": width, "height": height }));
            }
            normalized.insert("fullPage".into(), json!(false));
            Ok(serde_json::Value::Object(normalized))
        }
        _ => Err(ManagerError::McpToolNotAllowed(format!(
            "environment:{tool}"
        ))),
    }
}

fn validate_environment_mcp_tool_call(
    tool: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, ManagerError> {
    if tool.trim().is_empty() || tool.chars().count() > 128 {
        return Err(ManagerError::InvalidMcpArguments(
            "tool must be a non-empty name of at most 128 characters".into(),
        ));
    }
    mcp_argument_object(&arguments)?;
    if serde_json::to_vec(&arguments)?.len() > MAX_MCP_ARGUMENT_BYTES {
        return Err(ManagerError::InvalidMcpArguments(format!(
            "arguments exceed {MAX_MCP_ARGUMENT_BYTES} bytes"
        )));
    }
    validate_mcp_argument_value(&arguments, 0)?;
    Ok(arguments)
}

fn environment_mcp_base_tool(tool: &str) -> Result<&str, ManagerError> {
    let tool = tool.trim();
    let base = tool.strip_prefix("env.").unwrap_or(tool);
    if base.is_empty()
        || base.chars().count() > 124
        || !base
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(ManagerError::InvalidMcpArguments(
            "environment tool must be an env.* catalog name".into(),
        ));
    }
    Ok(base)
}

fn global_environment_mcp_tool_name(tool: &str) -> Result<String, ManagerError> {
    let tool = format!("env.{}", environment_mcp_base_tool(tool)?);
    if GLOBAL_ENVIRONMENT_MANAGEMENT_TOOLS.contains(&tool.as_str()) {
        return Err(ManagerError::McpToolNotAllowed(format!(
            "environment:{tool}"
        )));
    }
    Ok(tool)
}

fn validate_mcp_argument_value(
    value: &serde_json::Value,
    depth: usize,
) -> Result<(), ManagerError> {
    if depth > MAX_MCP_ARGUMENT_DEPTH {
        return Err(ManagerError::InvalidMcpArguments(format!(
            "arguments exceed {MAX_MCP_ARGUMENT_DEPTH} nesting levels"
        )));
    }
    match value {
        serde_json::Value::Object(object) => {
            for child in object.values() {
                validate_mcp_argument_value(child, depth + 1)?;
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                validate_mcp_argument_value(child, depth + 1)?;
            }
        }
        serde_json::Value::String(value) if value.chars().count() > MAX_MCP_STRING_CHARS => {
            return Err(ManagerError::InvalidMcpArguments(format!(
                "argument strings must not exceed {MAX_MCP_STRING_CHARS} characters"
            )));
        }
        _ => {}
    }
    Ok(())
}

fn mcp_argument_object(
    arguments: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, ManagerError> {
    arguments
        .as_object()
        .ok_or_else(|| ManagerError::InvalidMcpArguments("arguments must be a JSON object".into()))
}

fn ensure_mcp_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<(), ManagerError> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ManagerError::InvalidMcpArguments(
            "arguments contain unsupported fields".into(),
        ));
    }
    Ok(())
}

fn mcp_bounded_u64(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    minimum: u64,
    maximum: u64,
    default: u64,
) -> Result<u64, ManagerError> {
    let value = match object.get(key) {
        Some(value) => value.as_u64().ok_or_else(|| {
            ManagerError::InvalidMcpArguments(format!("{key} must be an integer"))
        })?,
        None => default,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(ManagerError::InvalidMcpArguments(format!(
            "{key} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn mcp_required_page(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<u64, ManagerError> {
    if !object.contains_key("page") {
        return Err(ManagerError::InvalidMcpArguments("page is required".into()));
    }
    mcp_bounded_u64(object, "page", 1, u32::MAX as u64, 1)
}

fn mcp_required_string(
    value: &serde_json::Value,
    key: &str,
    maximum_chars: usize,
) -> Result<String, ManagerError> {
    let value = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ManagerError::InvalidMcpArguments(format!("{key} must be a non-empty string"))
        })?;
    if value.chars().count() > maximum_chars {
        return Err(ManagerError::InvalidMcpArguments(format!(
            "{key} exceeds {maximum_chars} characters"
        )));
    }
    Ok(value.into())
}

fn mcp_bool(value: &serde_json::Value, key: &str) -> Result<bool, ManagerError> {
    value
        .as_bool()
        .ok_or_else(|| ManagerError::InvalidMcpArguments(format!("{key} must be a boolean")))
}

fn validate_mcp_env_id(env_id: String) -> Result<String, ManagerError> {
    if env_id.chars().all(|character| character.is_ascii_digit()) {
        Ok(env_id)
    } else {
        Err(ManagerError::InvalidMcpArguments(
            "envId must be a decimal string".into(),
        ))
    }
}

fn mcp_scope_name(scope: McpToolScope) -> &'static str {
    match scope {
        McpToolScope::Global => "global",
        McpToolScope::Environment => "environment",
    }
}

fn mcp_tool_allowed(scope: McpToolScope, tool: &str) -> bool {
    match scope {
        McpToolScope::Global => GLOBAL_MCP_READ_TOOLS.contains(&tool),
        McpToolScope::Environment => tool.strip_prefix("env.").is_some_and(|base| {
            environment_mcp_base_tool(base).is_ok()
                && !GLOBAL_ENVIRONMENT_MANAGEMENT_TOOLS.contains(&tool)
        }),
    }
}

fn sanitize_mcp_response(mut value: serde_json::Value) -> serde_json::Value {
    sdk_ffi::redact_value(&mut value);
    sanitize_mcp_urls(&mut value);
    value
}

fn sanitize_mcp_urls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                if key.to_ascii_lowercase().contains("url")
                    && let Some(url) = child.as_str()
                    && let Ok(url) = url::Url::parse(url)
                {
                    *child = serde_json::Value::String(url.origin().ascii_serialization());
                } else {
                    sanitize_mcp_urls(child);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(sanitize_mcp_urls),
        serde_json::Value::String(text) => {
            if let Ok(mut nested) = serde_json::from_str::<serde_json::Value>(text) {
                sanitize_mcp_urls(&mut nested);
                if let Ok(serialized) = serde_json::to_string(&nested) {
                    *text = serialized;
                }
            }
        }
        _ => {}
    }
}

fn mcp_error_message(error: &mcp_client::McpClientError) -> &'static str {
    match error {
        mcp_client::McpClientError::Endpoint(_) => "embedded MCP endpoint is invalid",
        mcp_client::McpClientError::Http(_) => "embedded MCP request failed",
        mcp_client::McpClientError::Status { .. } => "embedded MCP returned an HTTP error",
        mcp_client::McpClientError::ResponseTooLarge => "embedded MCP response was too large",
        mcp_client::McpClientError::Json(_) => "embedded MCP returned invalid JSON",
        mcp_client::McpClientError::MissingSession => "embedded MCP session was not created",
        mcp_client::McpClientError::ToolUnavailable(_) => "embedded MCP tool is unavailable",
        mcp_client::McpClientError::Rpc(_) => "embedded MCP returned a JSON-RPC error",
        mcp_client::McpClientError::ToolFailed => "embedded MCP tool failed",
        mcp_client::McpClientError::InvalidEnvironmentTool(_) => {
            "embedded MCP environment tool name is invalid"
        }
        mcp_client::McpClientError::InvalidEnvironmentArguments => {
            "embedded MCP environment arguments are invalid"
        }
    }
}

fn copy_directory_if_exists(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory_if_exists(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn manager_error_code(error: &ManagerError) -> &'static str {
    match error {
        ManagerError::SdkHost(_) => "SDK_HOST_ERROR",
        ManagerError::Store(_) => "STORE_ERROR",
        ManagerError::RuntimeNotRunning => "RUNTIME_NOT_RUNNING",
        ManagerError::ApiKeyMissing => "API_KEY_MISSING",
        ManagerError::ApiKeyInvalid => "API_KEY_INVALID",
        ManagerError::ApiKeyManagedExternally => "API_KEY_MANAGED_EXTERNALLY",
        ManagerError::ApiKeyCorrupt => "API_KEY_CORRUPT",
        ManagerError::InvalidAiProvider(_) => "AI_PROVIDER_INVALID",
        ManagerError::AiApiKeyManagedExternally => "AI_API_KEY_MANAGED_EXTERNALLY",
        ManagerError::AiApiKeyCorrupt => "AI_API_KEY_CORRUPT",
        ManagerError::InvalidHostResponse(_) => "INVALID_HOST_RESPONSE",
        ManagerError::BackendRejected(_) => "BACKEND_REJECTED",
        ManagerError::EnvironmentNotFound => "ENVIRONMENT_NOT_FOUND",
        ManagerError::EnvironmentNotReady(_) => "ENVIRONMENT_NOT_READY",
        ManagerError::InvalidEnvironmentBatch(_) => "INVALID_ENVIRONMENT_BATCH",
        ManagerError::InvalidEnvironmentMetadata(_) => "INVALID_ENVIRONMENT_METADATA",
        ManagerError::InvalidEnvironmentTransition { .. } => "INVALID_ENVIRONMENT_TRANSITION",
        ManagerError::InvalidBrowserCommand => "INVALID_BROWSER_COMMAND",
        ManagerError::Profile(_) => "PROFILE_ERROR",
        ManagerError::Platform(_) => "PLATFORM_ERROR",
        ManagerError::Io(_) => "IO_ERROR",
        ManagerError::Zip(_) => "ZIP_ERROR",
        ManagerError::Json(_) => "JSON_ERROR",
        ManagerError::OperationNotRetryable => "OPERATION_NOT_RETRYABLE",
        ManagerError::OperationNotCancellable => "OPERATION_NOT_CANCELLABLE",
        ManagerError::KernelNotFound => "KERNEL_NOT_FOUND",
        ManagerError::KernelNotUsable(_) => "KERNEL_NOT_USABLE",
        ManagerError::ProxyNotFound => "PROXY_NOT_FOUND",
        ManagerError::KernelBusy => "KERNEL_BUSY",
        ManagerError::UnsafeKernelPath => "UNSAFE_KERNEL_PATH",
        ManagerError::Ai(_) => "AI_PROVIDER_ERROR",
        ManagerError::AgentApprovalRequired => "AGENT_APPROVAL_REQUIRED",
        ManagerError::InvalidAgentPlan(_) => "INVALID_AGENT_PLAN",
        ManagerError::AgentStateMismatch { .. } => "AGENT_STATE_MISMATCH",
        ManagerError::AgentExecutionUncertain => "AGENT_EXECUTION_UNCERTAIN",
        ManagerError::Mcp(_) => "MCP_TOOL_ERROR",
        ManagerError::McpNotConfigured => "MCP_NOT_CONFIGURED",
        ManagerError::McpToolNotAllowed(_) => "MCP_TOOL_NOT_ALLOWED",
        ManagerError::InvalidMcpArguments(_) => "INVALID_MCP_ARGUMENTS",
    }
}

fn environment_api_key_present() -> bool {
    std::env::var_os("BROSDK_API_KEY").is_some_and(|value| !value.is_empty())
}

fn environment_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreError;

    fn usable_kernel() -> KernelRecord {
        KernelRecord {
            id: "chrome-134-current".into(),
            kernel_type: "chrome".into(),
            name: "Chrome 134".into(),
            major: Some(134),
            version: Some("3".into()),
            latest_version: Some("3".into()),
            platform: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            status: "installed".into(),
            install_path: Some("cores/chrome-134".into()),
            download_available: true,
            updated_at: chrono::Utc::now(),
        }
    }

    fn environment_record(env_id: &str, status: &str) -> EnvironmentRecord {
        EnvironmentRecord {
            env_id: env_id.into(),
            name: env_id.into(),
            status: status.into(),
            cdp: "-".into(),
            last_event: "synced".into(),
            generation: 0,
            request_id: None,
            current_operation_id: None,
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn environment_metadata_update_matches_minimal_server_contract() {
        let request = build_environment_metadata_update_request(&EnvironmentMetadataUpdateInput {
            env_id: "  env-1  ".into(),
            env_name: "  上海办公  ".into(),
            serial: "  CN-001  ".into(),
        })
        .expect("valid metadata");
        assert_eq!(
            request,
            json!({
                "envId": "env-1",
                "envName": "上海办公",
                "serial": "CN-001",
            })
        );
        assert_eq!(request.as_object().expect("object").len(), 3);
    }

    #[test]
    fn environment_metadata_update_enforces_rune_and_utf8_byte_limits() {
        let valid = EnvironmentMetadataUpdateInput {
            env_id: "env-1".into(),
            env_name: "界".repeat(32),
            serial: "界".repeat(21),
        };
        build_environment_metadata_update_request(&valid).expect("within server limits");

        for invalid in [
            EnvironmentMetadataUpdateInput {
                env_name: "界".repeat(33),
                ..valid.clone()
            },
            EnvironmentMetadataUpdateInput {
                serial: "界".repeat(22),
                ..valid.clone()
            },
            EnvironmentMetadataUpdateInput {
                env_name: "   ".into(),
                ..valid
            },
        ] {
            assert!(matches!(
                build_environment_metadata_update_request(&invalid),
                Err(ManagerError::InvalidEnvironmentMetadata(_))
            ));
        }
    }

    #[test]
    fn environment_metadata_update_requires_matching_server_confirmation() {
        let request = json!({
            "envId": "env-1",
            "envName": "Server name",
            "serial": "CN-001",
        });
        let confirmed = confirmed_environment_metadata(
            &json!({ "code": 200, "data": request.clone() }),
            &request,
        )
        .expect("matching confirmation");
        assert_eq!(confirmed, ("Server name".into(), "CN-001".into()));
        assert!(
            confirmed_environment_metadata(
                &json!({ "code": 200, "data": { "envName": "Other", "serial": "CN-001" } }),
                &request,
            )
            .is_err()
        );
    }

    #[test]
    fn confirmed_metadata_completes_environment_page_cache() {
        let mut rows = vec![(
            "env-1".into(),
            "Old name".into(),
            json!({ "envId": "env-1" }),
        )];
        merge_confirmed_environment_metadata(&mut rows, "env-1", "New name", "CN-002");
        assert_eq!(rows[0].1, "New name");
        assert_eq!(rows[0].2["envName"], "New name");
        assert_eq!(rows[0].2["serial"], "CN-002");
    }

    #[test]
    fn environment_batch_rejects_empty_duplicate_and_oversized_requests() {
        let empty = EnvironmentBatchInput {
            action: EnvironmentBatchAction::Start,
            env_ids: Vec::new(),
        };
        assert!(matches!(
            validate_environment_batch_shape(&empty),
            Err(ManagerError::InvalidEnvironmentBatch(_))
        ));

        let duplicate = EnvironmentBatchInput {
            action: EnvironmentBatchAction::Start,
            env_ids: vec!["env-1".into(), "env-1".into()],
        };
        assert!(matches!(
            validate_environment_batch_shape(&duplicate),
            Err(ManagerError::InvalidEnvironmentBatch(_))
        ));

        let oversized = EnvironmentBatchInput {
            action: EnvironmentBatchAction::Stop,
            env_ids: (0..=MAX_ENVIRONMENT_BATCH_SIZE)
                .map(|index| format!("env-{index}"))
                .collect(),
        };
        assert!(matches!(
            validate_environment_batch_shape(&oversized),
            Err(ManagerError::InvalidEnvironmentBatch(_))
        ));
    }

    #[test]
    fn environment_batch_preflights_every_lifecycle_state() {
        let start = EnvironmentBatchInput {
            action: EnvironmentBatchAction::Start,
            env_ids: vec!["env-1".into(), "env-2".into()],
        };
        validate_environment_batch_shape(&start).expect("valid shape");
        validate_environment_batch_states(
            &start,
            &[
                environment_record("env-1", "stopped"),
                environment_record("env-2", "failed"),
            ],
        )
        .expect("startable environments");
        assert!(matches!(
            validate_environment_batch_states(
                &start,
                &[
                    environment_record("env-1", "stopped"),
                    environment_record("env-2", "ready"),
                ],
            ),
            Err(ManagerError::InvalidEnvironmentBatch(_))
        ));

        let stop = EnvironmentBatchInput {
            action: EnvironmentBatchAction::Stop,
            env_ids: vec!["env-1".into(), "env-2".into()],
        };
        validate_environment_batch_states(
            &stop,
            &[
                environment_record("env-1", "ready"),
                environment_record("env-2", "starting"),
            ],
        )
        .expect("stoppable environments");
    }

    #[test]
    fn manager_reads_persistent_snapshot() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ManagerStore::open(
            directory.path().join("manager.sqlite3"),
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "work".into(),
                extension_dir: "extensions".into(),
                log_dir: "logs".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
                ai_base_url: None,
                ai_model: None,
            },
        )
        .expect("store");
        let manager = Manager::with_store(store).expect("manager");
        assert_eq!(
            manager.inner.last_runtime_status.blocking_read().state,
            RuntimeHostState::Stopped
        );
    }

    #[test]
    fn manager_cancels_only_queued_operations() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ManagerStore::open(
            directory.path().join("manager.sqlite3"),
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "work".into(),
                extension_dir: "extensions".into(),
                log_dir: "logs".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
                ai_base_url: None,
                ai_model: None,
            },
        )
        .expect("store");
        let manager = Manager::with_store(store.clone()).expect("manager");
        let queued = store
            .create_operation("environment.stop", Some("env-1"), "stop", 1, None)
            .expect("queued operation");
        let cancelled = manager
            .cancel_operation(&queued.id)
            .expect("queued operation can be cancelled");
        assert_eq!(cancelled.status, "cancelled");

        let running = store
            .create_operation("environment.start", Some("env-1"), "start", 2, None)
            .expect("running operation");
        store
            .transition_operation(&running.id, "running", "started", None)
            .expect("running transition");
        assert!(matches!(
            manager.cancel_operation(&running.id),
            Err(ManagerError::OperationNotCancellable)
        ));
        assert!(matches!(
            store.transition_operation(&running.id, "cancelled", "cancelled", None),
            Err(StoreError::InvalidTransition { .. })
        ));
        assert_eq!(
            manager_error_code(&ManagerError::OperationNotCancellable),
            "OPERATION_NOT_CANCELLABLE"
        );
    }

    #[test]
    fn agent_plan_rejects_unlisted_actions() {
        let plan = AiAgentPlan {
            summary: "bad".into(),
            action: "environment.destroy".into(),
            env_id: Some("env-1".into()),
            expected_state: Some("stopped".into()),
            idempotency_key: "key-1".into(),
            arguments: json!({}),
        };
        assert!(matches!(
            validate_agent_plan(&plan),
            Err(ManagerError::InvalidAgentPlan(_))
        ));
    }

    #[test]
    fn environment_create_operation_request_only_contains_profile_and_kernel_ids() {
        let request = environment_create_operation_request(&EnvironmentCreateInput {
            proxy_profile_id: Some("proxy-1".into()),
            kernel_id: "chrome-134-current".into(),
        });
        assert_eq!(
            request,
            json!({
                "proxyProfileId": "proxy-1",
                "kernelId": "chrome-134-current",
            })
        );
        assert!(!request.to_string().contains("secret"));
        assert!(!request.to_string().contains("socks5://"));
    }

    #[test]
    fn environment_create_request_matches_server_minimal_contract() {
        let request = build_environment_create_request(
            &usable_kernel(),
            Some("socks5://alice:secret@127.0.0.1:1080"),
        )
        .expect("create request");
        assert_eq!(request["kernel"], "Chrome");
        assert_eq!(request["kernelVersion"], "134");
        assert_eq!(request["proxy"], "socks5://alice:secret@127.0.0.1:1080");
        assert_eq!(request.as_object().expect("object").len(), 3);
        assert!(request.get("customerId").is_none());
        assert!(request.get("envName").is_none());
        assert!(request.get("finger").is_none());
    }

    #[test]
    fn environment_create_request_omits_unselected_proxy() {
        let request =
            build_environment_create_request(&usable_kernel(), None).expect("create request");
        assert_eq!(request.as_object().expect("object").len(), 2);
        assert!(request.get("proxy").is_none());
    }

    #[test]
    fn environment_create_rejects_catalog_only_or_wrong_platform_kernel() {
        let mut kernel = usable_kernel();
        kernel.status = "available".into();
        kernel.install_path = None;
        assert!(matches!(
            validate_environment_kernel(&kernel),
            Err(ManagerError::KernelNotUsable(_))
        ));

        let mut kernel = usable_kernel();
        kernel.platform = if cfg!(windows) { "linux" } else { "windows" }.into();
        assert!(matches!(
            validate_environment_kernel(&kernel),
            Err(ManagerError::KernelNotUsable(_))
        ));
    }

    #[test]
    fn backend_response_requires_business_success_code() {
        ensure_backend_success("environment create", &json!({ "code": 200, "data": {} }))
            .expect("success");
        assert!(matches!(
            ensure_backend_success(
                "environment create",
                &json!({
                    "code": 400,
                    "msg": "proxy socks5://alice:secret@127.0.0.1:1080 failed"
                })
            ),
            Err(ManagerError::BackendRejected(message))
                if !message.contains("secret") && message.contains("***")
        ));
        assert!(matches!(
            ensure_backend_success("environment create", &json!({ "data": {} })),
            Err(ManagerError::InvalidHostResponse(_))
        ));
    }

    #[test]
    fn environment_cleanup_summary_omits_local_paths_and_environment_ids() {
        let summary = summarize_environment_cleanup(&json!({
            "code": 0,
            "data": {
                "deleted": 1,
                "notFound": 0,
                "failed": 0,
                "results": [{
                    "envId": "123",
                    "status": "deleted",
                    "userDataDir": "C:/sensitive/profile/123",
                    "cleanupPath": "C:/sensitive/tombstone",
                    "deferredDelete": true
                }]
            }
        }));
        assert_eq!(summary["deleted"], 1);
        assert_eq!(summary["deferred"], 1);
        assert!(!summary.to_string().contains("sensitive"));
        assert!(!summary.to_string().contains("123"));
    }

    #[test]
    fn fingerprint_check_summary_omits_cdp_identifiers() {
        let summary = summarize_fingerprint_check(&json!({
            "url": "about:blank",
            "source": "embedded-memory",
            "targetId": "target-secret",
            "sessionId": "session-secret",
            "newTab": true,
            "cdp": { "inject": "private-page-content" }
        }));
        assert_eq!(
            summary,
            json!({ "opened": true, "newTab": true, "source": "embedded" })
        );
        let serialized = summary.to_string();
        for forbidden in ["target", "session", "private", "cdp"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn browser_snapshot_summary_keeps_only_page_status_and_origin() {
        let summary = summarize_browser_snapshot(&json!({
            "type": "browser.snapshot.result",
            "snapshotId": "secret-snapshot-id",
            "envId": "123",
            "status": "ok",
            "pageCount": 2,
            "pages": [{
                "status": "ok",
                "url": "https://example.com/private/path?token=secret",
                "title": "Private title",
                "targetId": "target-secret",
                "sessionId": "session-secret"
            }, {
                "status": "attach-failed",
                "targetUrl": "chrome://settings/content"
            }],
            "chunks": [{ "data": "page body" }]
        }));
        assert_eq!(summary["pageCount"], 2);
        assert_eq!(summary["failedPages"], 1);
        assert_eq!(summary["pages"][0]["origin"], "https://example.com");
        assert_eq!(summary["pages"][1]["origin"], "chrome://settings");
        let serialized = summary.to_string();
        for forbidden in ["private", "secret", "targetId", "sessionId", "chunks"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn environment_pages_merge_duplicates_until_reported_total() {
        let mut pages = EnvironmentPageAccumulator::default();
        assert!(
            !pages
                .push(&json!({
                    "code": 200,
                    "data": {
                        "total": 3,
                        "list": [
                            { "envId": "env-1", "envName": "One" },
                            { "envId": "env-2", "envName": "Two" }
                        ]
                    }
                }))
                .expect("first page")
        );
        assert!(
            pages
                .push(&json!({
                    "code": 200,
                    "data": {
                        "total": 3,
                        "list": [
                            { "envId": "env-2", "envName": "Two" },
                            { "envId": "env-3", "envName": "Three" }
                        ]
                    }
                }))
                .expect("second page")
        );
        assert_eq!(pages.rows.len(), 3);
        assert_eq!(pages.rows[2].0, "env-3");
    }

    #[test]
    fn environment_pages_reject_changed_totals_and_no_progress() {
        let mut changed_total = EnvironmentPageAccumulator::default();
        changed_total
            .push(&json!({
                "data": { "total": 2, "list": [{ "envId": "env-1" }] }
            }))
            .expect("first page");
        assert!(matches!(
            changed_total.push(&json!({
                "data": { "total": 3, "list": [{ "envId": "env-2" }] }
            })),
            Err(ManagerError::InvalidHostResponse(message))
                if message.contains("total changed")
        ));

        let mut repeated = EnvironmentPageAccumulator::default();
        let page = json!({
            "data": { "total": 2, "list": [{ "envId": "env-1" }] }
        });
        repeated.push(&page).expect("first page");
        assert!(matches!(
            repeated.push(&page),
            Err(ManagerError::InvalidHostResponse(message))
                if message.contains("no new environment ids")
        ));
    }

    #[test]
    fn environment_pages_handle_empty_missing_total_and_safety_limit() {
        let mut empty = EnvironmentPageAccumulator::default();
        assert!(
            empty
                .push(&json!({ "data": { "total": 0, "list": [] } }))
                .expect("empty page")
        );
        assert!(empty.rows.is_empty());

        let mut no_total = EnvironmentPageAccumulator::default();
        assert!(
            no_total
                .push(&json!({
                    "data": { "list": [{ "envId": "env-1" }] }
                }))
                .expect("short page without total")
        );
        assert_eq!(no_total.rows.len(), 1);

        let mut over_limit = EnvironmentPageAccumulator::default();
        assert!(matches!(
            over_limit.push(&json!({
                "data": { "total": MAX_ENVIRONMENTS + 1, "list": [] }
            })),
            Err(ManagerError::InvalidHostResponse(message))
                if message.contains("exceeds safety limit")
        ));
    }

    #[test]
    fn environment_create_response_accepts_string_or_numeric_id() {
        assert_eq!(
            response_env_id(&json!({ "data": { "envId": "2034183257439866880" } })),
            Some("2034183257439866880".into())
        );
        assert_eq!(
            response_env_id(&json!({ "data": { "envId": 42 } })),
            Some("42".into())
        );
    }

    #[test]
    fn agent_plan_requires_environment_id() {
        let plan = AiAgentPlan {
            summary: "start".into(),
            action: "environment.start".into(),
            env_id: None,
            expected_state: Some("stopped".into()),
            idempotency_key: "key-1".into(),
            arguments: json!({}),
        };
        assert!(matches!(
            validate_agent_plan(&plan),
            Err(ManagerError::InvalidAgentPlan(_))
        ));
    }

    #[test]
    fn agent_plan_prefers_explicit_known_env_id_and_uses_current_state() {
        let environments = vec![
            environment_record("env-selected", "ready"),
            environment_record("2044366881367789568", "stopped"),
        ];
        let target = resolve_agent_target(
            "启动环境 2044366881367789568，完成后告诉我状态",
            Some("env-selected"),
            &environments,
        )
        .expect("target");
        assert_eq!(target.as_deref(), Some("2044366881367789568"));

        let mut plan = AiAgentPlan {
            summary: "start".into(),
            action: "environment.start".into(),
            env_id: Some("env-selected".into()),
            expected_state: Some("ready".into()),
            idempotency_key: "model-generated".into(),
            arguments: json!({}),
        };
        prepare_agent_plan(&mut plan, target.as_deref(), &environments).expect("prepared plan");

        assert_eq!(plan.env_id.as_deref(), Some("2044366881367789568"));
        assert_eq!(plan.expected_state.as_deref(), Some("stopped"));
        assert_ne!(plan.idempotency_key, "model-generated");
        assert!(uuid::Uuid::parse_str(&plan.idempotency_key).is_ok());
    }

    #[test]
    fn agent_mcp_call_selects_environment_scope() {
        let environments = vec![
            environment_record("env-selected", "stopped"),
            environment_record("env-target", "ready"),
        ];
        let mut environment_plan = AiAgentPlan {
            summary: "list tabs".into(),
            action: "mcp.call".into(),
            env_id: Some("env-selected".into()),
            expected_state: None,
            idempotency_key: "model-generated".into(),
            arguments: json!({ "tool": "tabs", "arguments": { "action": "list" } }),
        };
        prepare_agent_plan(&mut environment_plan, Some("env-target"), &environments)
            .expect("environment MCP plan");
        validate_agent_plan(&environment_plan).expect("valid environment MCP plan");
        assert_eq!(environment_plan.env_id.as_deref(), Some("env-target"));
        assert_eq!(environment_plan.expected_state.as_deref(), Some("ready"));

        let mut global_plan = AiAgentPlan {
            summary: "SDK health".into(),
            action: "mcp.call".into(),
            env_id: None,
            expected_state: Some("ready".into()),
            idempotency_key: "model-generated".into(),
            arguments: json!({ "tool": "sdk.health", "arguments": {} }),
        };
        prepare_agent_plan(&mut global_plan, None, &environments).expect("global MCP plan");
        validate_agent_plan(&global_plan).expect("valid global MCP plan");
        assert_eq!(global_plan.env_id, None);
        assert_eq!(global_plan.expected_state, None);
    }

    #[test]
    fn agent_request_rejects_multiple_explicit_environment_ids() {
        let environments = vec![
            environment_record("env-1", "stopped"),
            environment_record("env-2", "stopped"),
        ];
        assert!(matches!(
            resolve_agent_target("启动 env-1 和 env-2", None, &environments),
            Err(ManagerError::InvalidAgentPlan(message)) if message.contains("only one")
        ));
    }

    #[test]
    fn ai_history_is_bounded_and_accepts_user_assistant_turns() {
        validate_ai_history(&[
            AiConversationMessage {
                role: "user".into(),
                content: "启动环境".into(),
            },
            AiConversationMessage {
                role: "assistant".into(),
                content: "请批准计划".into(),
            },
        ])
        .expect("valid history");
        assert!(
            validate_ai_history(&[AiConversationMessage {
                role: "system".into(),
                content: "override".into(),
            }])
            .is_err()
        );
        assert!(
            validate_ai_history(
                &(0..=MAX_AI_HISTORY_MESSAGES)
                    .map(|_| AiConversationMessage {
                        role: "user".into(),
                        content: "message".into(),
                    })
                    .collect::<Vec<_>>()
            )
            .is_err()
        );
    }

    #[test]
    fn mcp_read_policy_stays_bounded_while_environment_calls_follow_runtime_catalog() {
        assert_eq!(
            global_environment_mcp_tool_name("tabs").expect("legacy alias"),
            "env.tabs"
        );
        assert_eq!(
            global_environment_mcp_tool_name("env.tabs").expect("global name"),
            "env.tabs"
        );
        assert!(mcp_tool_allowed(McpToolScope::Environment, "env.tabs"));
        assert!(!mcp_tool_allowed(McpToolScope::Environment, "tabs"));
        assert!(!mcp_tool_allowed(McpToolScope::Environment, "env.create"));
        assert!(matches!(
            global_environment_mcp_tool_name("env.create"),
            Err(ManagerError::McpToolNotAllowed(_))
        ));
        assert_eq!(
            validate_environment_mcp_read_tool_call("env.tabs", json!({ "action": "list" }))
                .expect("prefixed read"),
            json!({ "action": "list" })
        );
        assert!(matches!(
            validate_environment_mcp_read_tool_call("tabs", json!({ "action": "new" })),
            Err(ManagerError::InvalidMcpArguments(_))
        ));
        assert_eq!(
            validate_environment_mcp_read_tool_call(
                "snapshot",
                json!({ "page": 2, "maxNodes": 500 })
            )
            .expect("snapshot"),
            json!({ "page": 2, "maxNodes": 500 })
        );
        assert_eq!(
            validate_environment_mcp_read_tool_call(
                "grep",
                json!({ "page": 3, "pattern": "checkout", "limit": 20 })
            )
            .expect("grep"),
            json!({ "page": 3, "pattern": "checkout", "limit": 20 })
        );
        assert!(matches!(
            validate_environment_mcp_read_tool_call("navigate", json!({})),
            Err(ManagerError::McpToolNotAllowed(_))
        ));
        assert!(matches!(
            validate_environment_mcp_read_tool_call(
                "screenshot",
                json!({ "page": 1, "fullPage": true })
            ),
            Err(ManagerError::InvalidMcpArguments(_))
        ));
        assert_eq!(
            validate_environment_mcp_tool_call("navigate", json!({ "url": "https://example.com" }))
                .expect("runtime-advertised call"),
            json!({ "url": "https://example.com" })
        );
        assert!(matches!(
            validate_environment_mcp_tool_call("navigate", json!("https://example.com")),
            Err(ManagerError::InvalidMcpArguments(_))
        ));
    }

    #[test]
    fn global_mcp_policy_normalizes_reads_and_rejects_writes() {
        assert_eq!(
            validate_global_mcp_tool_call("env.list", json!({ "pageSize": 200 }))
                .expect("env list"),
            json!({ "page": 1, "pageSize": 200 })
        );
        assert_eq!(
            validate_global_mcp_tool_call("env.resolve", json!({ "query": "Primary" }))
                .expect("env resolve"),
            json!({ "query": "Primary" })
        );
        assert!(matches!(
            validate_global_mcp_tool_call("env.create", json!({ "request": {} })),
            Err(ManagerError::McpToolNotAllowed(_))
        ));
        assert!(matches!(
            validate_global_mcp_tool_call("mcp.endpoint", json!({ "envId": "not-numeric" })),
            Err(ManagerError::InvalidMcpArguments(_))
        ));
    }

    #[test]
    fn mcp_response_reduces_urls_to_origins() {
        let value = sanitize_mcp_response(json!({
            "content": [{
                "type": "text",
                "text": "{\"pages\":[{\"url\":\"https://example.com/path?token=secret\"}]}"
            }]
        }));
        let text = value
            .pointer("/content/0/text")
            .and_then(serde_json::Value::as_str)
            .expect("text");
        assert!(text.contains("https://example.com"));
        assert!(!text.contains("token"));
        assert!(!text.contains("secret"));
    }

    #[test]
    fn cdp_origin_removes_credentials_paths_and_queries() {
        assert_eq!(
            external_cdp_origin(
                "ws://user:pass@127.0.0.1:9333/devtools/browser/private?token=secret"
            ),
            Some("ws://127.0.0.1:9333".into())
        );
        assert_eq!(
            external_cdp_origin("127.0.0.1:9223"),
            Some("http://127.0.0.1:9223".into())
        );
        assert_eq!(external_cdp_origin("ready"), None);
        assert_eq!(external_cdp_origin("-"), None);
    }

    #[test]
    fn ai_provider_key_uses_secure_storage_and_never_sqlite() {
        if environment_value("BROSDK_AI_API_KEY").is_some() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let database = directory.path().join("manager.sqlite3");
        let store = ManagerStore::open(
            &database,
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: directory.path().join("work").display().to_string(),
                extension_dir: directory.path().join("extensions").display().to_string(),
                log_dir: directory.path().join("logs").display().to_string(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
                ai_base_url: None,
                ai_model: None,
            },
        )
        .expect("store");
        let manager = Manager::with_store(store).expect("manager");
        let secret = "stage16-secret-not-in-sqlite";
        let status = manager
            .configure_ai_provider(AiProviderConfigInput {
                base_url: "https://api.deepseek.com/".into(),
                model: "deepseek-v4-flash".into(),
                api_key: Some(secret.into()),
            })
            .expect("provider configured");
        assert!(status.api_key_present);
        assert_eq!(status.api_key_source, "secure-storage");
        assert_eq!(status.base_url, "https://api.deepseek.com");
        let database_bytes = fs::read(&database).expect("database bytes");
        assert!(
            !database_bytes
                .windows(secret.len())
                .any(|bytes| bytes == secret.as_bytes())
        );
        let protected =
            fs::read(platform::secrets_dir(directory.path()).join(AI_API_KEY_SECRET_REFERENCE))
                .expect("protected key");
        assert!(
            !protected
                .windows(secret.len())
                .any(|bytes| bytes == secret.as_bytes())
        );
        let events = manager.events_since(0).expect("events");
        assert!(
            !serde_json::to_string(&events)
                .expect("events json")
                .contains(secret)
        );
    }

    #[tokio::test]
    async fn ai_context_focuses_env_id_and_exposes_only_cdp_origin() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ManagerStore::open(
            directory.path().join("manager.sqlite3"),
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: directory.path().join("work").display().to_string(),
                extension_dir: directory.path().join("extensions").display().to_string(),
                log_dir: directory.path().join("logs").display().to_string(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: None,
                ai_base_url: None,
                ai_model: None,
            },
        )
        .expect("store");
        store
            .upsert_remote_environments(&[(
                "env-focused".into(),
                "Shared".into(),
                json!({ "envId": "env-focused" }),
            )])
            .expect("environment");
        let manager = Manager::with_store(store).expect("manager");
        manager
            .inner
            .store
            .set_environment_runtime(RuntimeUpdate {
                env_id: "env-focused",
                generation: 0,
                status: "ready",
                request_id: Some(44),
                operation_id: Some("op-focused"),
                cdp: "ws://user:pass@127.0.0.1:9333/devtools/browser/private?token=secret",
                last_event: "browser-open-success",
            })
            .expect("runtime");
        let context = manager
            .ai_context(Some("env-focused"))
            .await
            .expect("context");
        assert_eq!(context["focusedEnvId"], "env-focused");
        assert_eq!(context["environments"][0]["cdpAvailable"], true);
        assert_eq!(
            context["environments"][0]["cdpOrigin"],
            "ws://127.0.0.1:9333"
        );
        assert_eq!(context["environments"][0]["controlChannel"], "external-cdp");
        let serialized = context.to_string();
        for forbidden in ["user:pass", "/private", "token=secret", "devtools/browser"] {
            assert!(
                !serialized.contains(forbidden),
                "found {forbidden} in {serialized}"
            );
        }

        manager
            .inner
            .store
            .set_environment_runtime(RuntimeUpdate {
                env_id: "env-focused",
                generation: 0,
                status: "ready",
                request_id: Some(45),
                operation_id: None,
                cdp: "-",
                last_event: "browser-open-success",
            })
            .expect("pipe-only runtime");
        let pipe_context = manager
            .ai_context(Some("env-focused"))
            .await
            .expect("pipe context");
        assert_eq!(pipe_context["environments"][0]["cdpAvailable"], false);
        assert_eq!(
            pipe_context["environments"][0]["cdpOrigin"],
            serde_json::Value::Null
        );
        assert_eq!(
            pipe_context["environments"][0]["controlChannel"],
            "sdk-browser-command"
        );
    }

    #[tokio::test]
    async fn mcp_call_requires_an_active_initialized_port() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ManagerStore::open(
            directory.path().join("manager.sqlite3"),
            &ManagerSettings {
                data_dir: directory.path().display().to_string(),
                work_dir: "work".into(),
                extension_dir: "extensions".into(),
                log_dir: "logs".into(),
                sdk_api_url: None,
                debug: false,
                startup_policy: "restore-none".into(),
                embedded_mcp_port: Some(9222),
                ai_base_url: None,
                ai_model: None,
            },
        )
        .expect("store");
        store
            .upsert_remote_environments(&[(
                "env-1".into(),
                "Environment".into(),
                json!({ "envId": "env-1" }),
            )])
            .expect("environment");
        store
            .set_environment_runtime(RuntimeUpdate {
                env_id: "env-1",
                generation: 0,
                status: "ready",
                request_id: None,
                operation_id: None,
                cdp: "127.0.0.1:9223",
                last_event: "ready",
            })
            .expect("runtime");
        let manager = Manager::with_store(store).expect("manager");
        let error = manager
            .call_embedded_mcp(McpToolCallRequest {
                scope: McpToolScope::Environment,
                env_id: Some("env-1".into()),
                tool: "tabs".into(),
                arguments: json!({ "action": "list" }),
            })
            .await
            .expect_err("inactive port must fail");
        assert!(matches!(error, ManagerError::McpNotConfigured));
    }
}
