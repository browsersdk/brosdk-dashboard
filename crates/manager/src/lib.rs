use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use domain::{
    ApiKeyStatus, BrowserCommandExecution, DashboardSnapshot, FingerprintProfile,
    FingerprintProfileInput, HostCommand, KernelInstallInput, ManagerEvent, ManagerSettings,
    McpPanel, OperationExecution, OperationRecord, ProxyParseResult, ProxyProfile,
    ProxyProfileInput, RuntimeHostState, RuntimeHostStatus, SdkPanel, SmokeReport,
};
use operation::OperationQueue;
use sdk_client::{RuntimeHost, SdkHostClient};
use serde_json::json;
use store::{ManagerStore, RuntimeUpdate};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

mod mirror;
mod operation;
mod profiles;
mod store;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("{0}")]
    SdkHost(#[from] sdk_client::SdkClientError),
    #[error("{0}")]
    Store(#[from] store::StoreError),
    #[error("runtime host is not running")]
    RuntimeNotRunning,
    #[error("invalid runtime host response: {0}")]
    InvalidHostResponse(String),
    #[error("environment was not found in the local account mirror")]
    EnvironmentNotFound,
    #[error("environment is not ready for browser commands (current state: {0})")]
    EnvironmentNotReady(String),
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
    #[error("kernel is not known to the local manager")]
    KernelNotFound,
    #[error("installed kernels cannot be removed while an environment is running")]
    KernelBusy,
    #[error("kernel install path is outside the SDK work directory")]
    UnsafeKernelPath,
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
    last_runtime_status: RwLock<RuntimeHostStatus>,
    last_smoke: RwLock<Option<SmokeReport>>,
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
        let operations = OperationQueue::new(store.clone());
        Ok(Self {
            inner: Arc::new(ManagerInner {
                store,
                operations,
                runtime: Mutex::new(None),
                sdk_init_lock: Mutex::new(()),
                sdk_initialized: RwLock::new(false),
                last_runtime_status: RwLock::new(RuntimeHostStatus::default()),
                last_smoke: RwLock::new(None),
            }),
        })
    }

    pub async fn start_runtime(&self) -> Result<RuntimeHostStatus, ManagerError> {
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
        match RuntimeHost::start().await {
            Ok(host) => {
                let status = host.status();
                *self.inner.sdk_initialized.write().await = false;
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
        if self.inner.store.settings()?.startup_policy == "reconcile" {
            let _ = self.reconcile_runtimes().await?;
        }
        Ok(())
    }

    pub async fn stop_runtime(&self) -> Result<RuntimeHostStatus, ManagerError> {
        let host = self.inner.runtime.lock().await.take();
        let status = match host {
            Some(host) => host.stop().await?,
            None => RuntimeHostStatus::default(),
        };
        *self.inner.sdk_initialized.write().await = false;
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
        *self.inner.last_runtime_status.write().await = status.clone();
        Ok(status)
    }

    pub async fn snapshot(&self) -> Result<DashboardSnapshot, ManagerError> {
        if self.inner.runtime.lock().await.is_none() {
            let _ = self.start_runtime().await;
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
        let environments = self.inner.store.list_environments()?;
        let fingerprints = self.inner.store.list_fingerprint_profiles()?;
        let proxies = self.inner.store.list_proxy_profiles()?;
        let bindings = profiles::environment_bindings(
            &environments
                .iter()
                .map(|environment| environment.env_id.clone())
                .collect::<Vec<_>>(),
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
                api_key: ApiKeyStatus {
                    source: "BROSDK_API_KEY".into(),
                    present: std::env::var_os("BROSDK_API_KEY").is_some(),
                },
                host_path: host_path.map(|path| path.display().to_string()),
                dll_path: dll_path.display().to_string(),
                work_dir: settings.work_dir.clone(),
                last_smoke: self.inner.last_smoke.read().await.clone(),
            },
            capabilities: capabilities.clone(),
            mcp: McpPanel {
                mode: "manager-routed".into(),
                embedded_available: capabilities.embedded_mcp,
                manager_route: "Manager owns the runtime host process, envId routing and operation state; only sdk-host can enable the DLL embedded MCP port.".into(),
                endpoint_hint: settings
                    .embedded_mcp_port
                    .or_else(embedded_port)
                    .map(|port| format!("configured on 127.0.0.1:{port}; enabled during sdk_init"))
                    .unwrap_or_else(|| "not configured; internal IPC does not require a TCP port".into()),
                notes: vec![
                    "Dashboard communicates with Manager through Tauri commands.".into(),
                    "Manager communicates with sdk-host through a supervised named pipe/UDS.".into(),
                    "Unexpected host exit becomes degraded state and never exits the desktop UI.".into(),
                ],
            },
            environments,
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
            let value = host
                .call(
                    HostCommand::EnvPage {
                        request: sdk_ffi::default_env_page_request(),
                    },
                    Some(operation.id.clone()),
                )
                .await?;
            let rows = mirror::environment_rows(&value);
            self.inner.store.upsert_remote_environments(&rows)?;
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
        self.execute_sync_host_operation(
            "fingerprint.check",
            Some(env_id),
            "打开指纹检查页",
            json!({ "envId": env_id }),
            |request| HostCommand::BrowserEnvCheck { request },
        )
        .await
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
        Ok(self
            .inner
            .operations
            .cancel(operation_id, "cancelled by user")?)
    }

    pub fn events_since(&self, sequence: u64) -> Result<Vec<ManagerEvent>, ManagerError> {
        Ok(self.inner.store.events_since(sequence, 500)?)
    }

    pub fn update_settings(&self, settings: ManagerSettings) -> Result<(), ManagerError> {
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
        let profile = self
            .inner
            .store
            .list_proxy_profiles()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or(ManagerError::EnvironmentNotFound)?;
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
        let mut events = host.subscribe_events();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        let Some(inner) = weak.upgrade() else {
                            break;
                        };
                        let _ = inner.store.apply_host_event(&event);
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
        ManagerError::InvalidHostResponse(_) => "INVALID_HOST_RESPONSE",
        ManagerError::EnvironmentNotFound => "ENVIRONMENT_NOT_FOUND",
        ManagerError::EnvironmentNotReady(_) => "ENVIRONMENT_NOT_READY",
        ManagerError::InvalidBrowserCommand => "INVALID_BROWSER_COMMAND",
        ManagerError::Profile(_) => "PROFILE_ERROR",
        ManagerError::Platform(_) => "PLATFORM_ERROR",
        ManagerError::Io(_) => "IO_ERROR",
        ManagerError::Zip(_) => "ZIP_ERROR",
        ManagerError::Json(_) => "JSON_ERROR",
        ManagerError::OperationNotRetryable => "OPERATION_NOT_RETRYABLE",
        ManagerError::KernelNotFound => "KERNEL_NOT_FOUND",
        ManagerError::KernelBusy => "KERNEL_BUSY",
        ManagerError::UnsafeKernelPath => "UNSAFE_KERNEL_PATH",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            },
        )
        .expect("store");
        let manager = Manager::with_store(store).expect("manager");
        assert_eq!(
            manager.inner.last_runtime_status.blocking_read().state,
            RuntimeHostState::Stopped
        );
    }
}
