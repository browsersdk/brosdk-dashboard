use chrono::Utc;
use domain::{
    ApiKeyStatus, DashboardSnapshot, EnvironmentRecord, McpPanel, OperationRecord, SdkPanel,
    SmokeReport,
};
use sdk_client::SdkHostClient;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("{0}")]
    SdkHost(#[from] sdk_client::SdkClientError),
}

pub async fn snapshot() -> DashboardSnapshot {
    let dll_path = sdk_ffi::default_library_path();
    let work_dir = platform::default_sdk_work_dir();
    let mut capabilities = sdk_ffi::capabilities_for_path(dll_path.clone());
    let host_path = sdk_client::discover_host_path().ok();

    if let Ok(client) = SdkHostClient::discover()
        && let Ok(host_capabilities) = client.capabilities().await
    {
        capabilities = host_capabilities;
    }

    DashboardSnapshot {
        sdk: SdkPanel {
            state: if capabilities.dll_exists { "host-ready".into() } else { "dll-missing".into() },
            api_key: ApiKeyStatus {
                source: "BROSDK_API_KEY".into(),
                present: std::env::var_os("BROSDK_API_KEY").is_some(),
            },
            host_path: host_path.map(|path| path.display().to_string()),
            dll_path: dll_path.display().to_string(),
            work_dir: work_dir.display().to_string(),
            last_smoke: None,
        },
        capabilities: capabilities.clone(),
        mcp: McpPanel {
            mode: "manager-routed".into(),
            embedded_available: capabilities.embedded_mcp,
            manager_route: "Manager keeps envId routing and can enable DLL embedded MCP through sdk_init port when BROSDK_EMBEDDED_PORT is set.".into(),
            endpoint_hint: std::env::var("BROSDK_EMBEDDED_PORT")
                .ok()
                .map(|port| format!("DLL embedded MCP/WebAPI on 127.0.0.1:{port}"))
                .unwrap_or_else(|| "not enabled; set BROSDK_EMBEDDED_PORT for embedded endpoint smoke".into()),
            notes: vec![
                "Dashboard does not call the embedded MCP endpoint directly.".into(),
                "Manager remains the policy boundary for envId, operation state and future approvals.".into(),
                "DLL embedded MCP is exposed as a host capability, not as the only automation path.".into(),
            ],
        },
        environments: vec![EnvironmentRecord {
            env_id: "-".into(),
            name: "等待 env_page 同步".into(),
            status: "stopped".into(),
            cdp: "-".into(),
            last_event: "尚未运行 smoke 或环境同步".into(),
        }],
        operations: vec![OperationRecord {
            id: Uuid::new_v4().to_string(),
            label: "项目骨架初始化".into(),
            status: "succeeded".into(),
            message: "Tauri/React/Rust workspace ready for SDK smoke".into(),
            updated_at: Utc::now(),
        }],
    }
}

pub async fn run_sdk_smoke() -> Result<SmokeReport, ManagerError> {
    let client = SdkHostClient::discover()?;
    Ok(client.smoke().await?)
}
