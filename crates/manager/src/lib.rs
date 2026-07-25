use std::sync::Arc;

use domain::{
    ApiKeyStatus, BrowserCommandExecution, DashboardSnapshot, HostCommand, ManagerEvent,
    ManagerSettings, McpPanel, OperationRecord, RuntimeHostState, RuntimeHostStatus, SdkPanel,
    SmokeReport,
};
use operation::OperationQueue;
use sdk_client::{RuntimeHost, SdkHostClient};
use serde_json::json;
use store::{ManagerStore, RuntimeUpdate};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

mod mirror;
mod operation;
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
                endpoint_hint: std::env::var("BROSDK_EMBEDDED_PORT")
                    .ok()
                    .map(|port| format!("configured on 127.0.0.1:{port}; enabled during sdk_init"))
                    .unwrap_or_else(|| "not configured; internal IPC does not require a TCP port".into()),
                notes: vec![
                    "Dashboard communicates with Manager through Tauri commands.".into(),
                    "Manager communicates with sdk-host through a supervised named pipe/UDS.".into(),
                    "Unexpected host exit becomes degraded state and never exits the desktop UI.".into(),
                ],
            },
            environments: self.inner.store.list_environments()?,
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
                .enqueue("environment.sync", None, "同步远端环境", 0)?;
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
                .enqueue("runtime.reconcile", None, "对账运行环境", 0)?;
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
            .enqueue(kind, Some(env_id), label, generation)?)
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
        self.inner.store.update_settings(&settings)?;
        self.inner.store.append_event(
            "settings.updated",
            None,
            None,
            &json!({
                "workDir": settings.work_dir,
                "extensionDir": settings.extension_dir,
                "logDir": settings.log_dir,
                "sdkApiUrlConfigured": settings.sdk_api_url.is_some(),
                "debug": settings.debug,
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
        host.initialize(settings.work_dir, embedded_port()).await?;
        *self.inner.sdk_initialized.write().await = true;
        self.inner.store.append_event(
            "sdk.initialized",
            None,
            None,
            &json!({ "embeddedPort": embedded_port() }),
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

fn manager_error_code(error: &ManagerError) -> &'static str {
    match error {
        ManagerError::SdkHost(_) => "SDK_HOST_ERROR",
        ManagerError::Store(_) => "STORE_ERROR",
        ManagerError::RuntimeNotRunning => "RUNTIME_NOT_RUNNING",
        ManagerError::InvalidHostResponse(_) => "INVALID_HOST_RESPONSE",
        ManagerError::EnvironmentNotFound => "ENVIRONMENT_NOT_FOUND",
        ManagerError::EnvironmentNotReady(_) => "ENVIRONMENT_NOT_READY",
        ManagerError::InvalidBrowserCommand => "INVALID_BROWSER_COMMAND",
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
                work_dir: "work".into(),
                extension_dir: "extensions".into(),
                log_dir: "logs".into(),
                sdk_api_url: None,
                debug: false,
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
