use std::sync::Arc;

use chrono::Utc;
use domain::{
    ApiKeyStatus, DashboardSnapshot, EnvironmentRecord, McpPanel, OperationRecord,
    RuntimeHostState, RuntimeHostStatus, SdkPanel, SmokeReport,
};
use sdk_client::{RuntimeHost, SdkHostClient};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("{0}")]
    SdkHost(#[from] sdk_client::SdkClientError),
}

#[derive(Clone)]
pub struct Manager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    runtime: Mutex<Option<RuntimeHost>>,
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
        Self {
            inner: Arc::new(ManagerInner {
                runtime: Mutex::new(None),
                last_runtime_status: RwLock::new(RuntimeHostStatus::default()),
                last_smoke: RwLock::new(None),
            }),
        }
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
                *self.inner.last_runtime_status.write().await = status.clone();
                *runtime = Some(host);
                Ok(status)
            }
            Err(error) => {
                *self.inner.last_runtime_status.write().await = RuntimeHostStatus {
                    state: RuntimeHostState::Degraded,
                    last_error: Some(error.to_string()),
                    ..RuntimeHostStatus::default()
                };
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
        *self.inner.last_runtime_status.write().await = status.clone();
        Ok(status)
    }

    pub async fn kill_runtime(&self) -> Result<RuntimeHostStatus, ManagerError> {
        let host = self.inner.runtime.lock().await.clone();
        let status = match host {
            Some(host) => host.kill().await?,
            None => RuntimeHostStatus::default(),
        };
        *self.inner.last_runtime_status.write().await = status.clone();
        Ok(status)
    }

    pub async fn snapshot(&self) -> DashboardSnapshot {
        if self.inner.runtime.lock().await.is_none() {
            let _ = self.start_runtime().await;
        }

        let dll_path = sdk_ffi::default_library_path();
        let work_dir = platform::default_sdk_work_dir();
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
        DashboardSnapshot {
            sdk: SdkPanel {
                state: state.into(),
                runtime: runtime_status,
                api_key: ApiKeyStatus {
                    source: "BROSDK_API_KEY".into(),
                    present: std::env::var_os("BROSDK_API_KEY").is_some(),
                },
                host_path: host_path.map(|path| path.display().to_string()),
                dll_path: dll_path.display().to_string(),
                work_dir: work_dir.display().to_string(),
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
            environments: vec![EnvironmentRecord {
                env_id: "-".into(),
                name: "等待 env_page 同步".into(),
                status: "stopped".into(),
                cdp: "-".into(),
                last_event: "Runtime Host 已隔离，等待 Manager Domain 同步".into(),
            }],
            operations: vec![OperationRecord {
                id: Uuid::new_v4().to_string(),
                label: "Runtime Host 监督".into(),
                status: if state == "host-running" {
                    "succeeded".into()
                } else {
                    "failed".into()
                },
                message: format!("runtime state: {state}"),
                updated_at: Utc::now(),
            }],
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manager_starts_stopped() {
        let manager = Manager::new();
        assert_eq!(
            manager.inner.last_runtime_status.blocking_read().state,
            RuntimeHostState::Stopped
        );
    }
}
