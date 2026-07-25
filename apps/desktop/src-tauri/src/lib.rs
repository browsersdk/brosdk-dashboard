#[tauri::command]
async fn manager_snapshot(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::DashboardSnapshot, String> {
    manager.snapshot().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_sdk_smoke(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::SmokeReport, String> {
    manager
        .run_sdk_smoke()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_host_start(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::RuntimeHostStatus, String> {
    manager
        .start_runtime()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_host_stop(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::RuntimeHostStatus, String> {
    manager
        .stop_runtime()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn runtime_host_kill(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::RuntimeHostStatus, String> {
    manager
        .kill_runtime()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_sync_environments(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::OperationRecord, String> {
    manager
        .sync_environments()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_reconcile_runtimes(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::OperationRecord, String> {
    manager
        .reconcile_runtimes()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_start_environment(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .start_environment(&env_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_stop_environment(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .stop_environment(&env_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_events_since(
    manager: tauri::State<'_, manager::Manager>,
    sequence: u64,
) -> Result<Vec<domain::ManagerEvent>, String> {
    manager
        .events_since(sequence)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_update_settings(
    manager: tauri::State<'_, manager::Manager>,
    settings: domain::ManagerSettings,
) -> Result<(), String> {
    manager
        .update_settings(settings)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_cancel_operation(
    manager: tauri::State<'_, manager::Manager>,
    operation_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .cancel_operation(&operation_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_refresh_environment_details(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::OperationRecord, String> {
    manager
        .refresh_environment_details()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_parse_proxy_url(
    manager: tauri::State<'_, manager::Manager>,
    url: String,
) -> Result<domain::ProxyParseResult, String> {
    manager
        .parse_proxy_url(&url)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_save_proxy_profile(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::ProxyProfileInput,
) -> Result<domain::ProxyProfile, String> {
    manager
        .save_proxy_profile(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_delete_proxy_profile(
    manager: tauri::State<'_, manager::Manager>,
    profile_id: String,
) -> Result<(), String> {
    manager
        .delete_proxy_profile(&profile_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_diagnose_proxy(
    manager: tauri::State<'_, manager::Manager>,
    profile_id: Option<String>,
    url: String,
) -> Result<domain::OperationExecution, String> {
    manager
        .diagnose_proxy(profile_id.as_deref(), &url)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_system_proxy_diagnostics(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::OperationExecution, String> {
    manager
        .system_proxy_diagnostics()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_save_fingerprint_profile(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::FingerprintProfileInput,
) -> Result<domain::FingerprintProfile, String> {
    manager
        .save_fingerprint_profile(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_import_fingerprint_profile(
    manager: tauri::State<'_, manager::Manager>,
    path: String,
) -> Result<domain::FingerprintProfile, String> {
    manager
        .import_fingerprint_profile(&path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_export_fingerprint_profile(
    manager: tauri::State<'_, manager::Manager>,
    profile_id: String,
    path: String,
) -> Result<(), String> {
    manager
        .export_fingerprint_profile(&profile_id, &path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_delete_fingerprint_profile(
    manager: tauri::State<'_, manager::Manager>,
    profile_id: String,
) -> Result<(), String> {
    manager
        .delete_fingerprint_profile(&profile_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_open_fingerprint_check(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationExecution, String> {
    manager
        .open_fingerprint_check(&env_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_refresh_kernels(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::OperationRecord, String> {
    manager
        .refresh_kernels()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_install_kernel(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::KernelInstallInput,
) -> Result<domain::OperationRecord, String> {
    manager
        .install_kernel(input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_cleanup_kernel_cache(
    manager: tauri::State<'_, manager::Manager>,
    major: Option<u32>,
) -> Result<domain::OperationExecution, String> {
    manager
        .cleanup_kernel_cache(major)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_uninstall_kernel(
    manager: tauri::State<'_, manager::Manager>,
    kernel_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .uninstall_kernel(&kernel_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_retry_operation(
    manager: tauri::State<'_, manager::Manager>,
    operation_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .retry_operation(&operation_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_create_diagnostic_bundle(
    manager: tauri::State<'_, manager::Manager>,
    output_path: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .create_diagnostic_bundle(&output_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_ai_chat(
    manager: tauri::State<'_, manager::Manager>,
    request: domain::AiChatRequest,
) -> Result<domain::AiChatResponse, String> {
    manager
        .ai_chat(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_ai_plan_agent(
    manager: tauri::State<'_, manager::Manager>,
    request: domain::AiAgentPlanRequest,
) -> Result<domain::AiAgentPlan, String> {
    manager
        .ai_plan_agent(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_ai_execute_agent(
    manager: tauri::State<'_, manager::Manager>,
    request: domain::AiAgentExecuteRequest,
) -> Result<domain::AiAgentExecution, String> {
    manager
        .ai_execute_agent(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_call_embedded_mcp(
    manager: tauri::State<'_, manager::Manager>,
    request: domain::McpToolCallRequest,
) -> Result<domain::McpToolCallExecution, String> {
    manager
        .call_embedded_mcp(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn sdk_host_path() -> Result<String, String> {
    sdk_client::discover_host_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

pub fn run() {
    let manager = manager::Manager::new();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(manager.clone())
        .setup(move |_app| {
            let startup_manager = manager.clone();
            tauri::async_runtime::spawn(async move {
                if startup_manager.start_runtime().await.is_ok() {
                    let _ = startup_manager.apply_startup_policy().await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            manager_snapshot,
            run_sdk_smoke,
            runtime_host_start,
            runtime_host_stop,
            runtime_host_kill,
            manager_sync_environments,
            manager_reconcile_runtimes,
            manager_start_environment,
            manager_stop_environment,
            manager_events_since,
            manager_update_settings,
            manager_cancel_operation,
            manager_refresh_environment_details,
            manager_parse_proxy_url,
            manager_save_proxy_profile,
            manager_delete_proxy_profile,
            manager_diagnose_proxy,
            manager_system_proxy_diagnostics,
            manager_save_fingerprint_profile,
            manager_import_fingerprint_profile,
            manager_export_fingerprint_profile,
            manager_delete_fingerprint_profile,
            manager_open_fingerprint_check,
            manager_refresh_kernels,
            manager_install_kernel,
            manager_cleanup_kernel_cache,
            manager_uninstall_kernel,
            manager_retry_operation,
            manager_create_diagnostic_bundle,
            manager_ai_chat,
            manager_ai_plan_agent,
            manager_ai_execute_agent,
            manager_call_embedded_mcp,
            sdk_host_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BroSDK Dashboard");
}
