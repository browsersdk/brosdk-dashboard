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
fn sdk_host_path() -> Result<String, String> {
    sdk_client::discover_host_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

pub fn run() {
    let manager = manager::Manager::new();
    tauri::Builder::default()
        .manage(manager.clone())
        .setup(move |_app| {
            let startup_manager = manager.clone();
            tauri::async_runtime::spawn(async move {
                let _ = startup_manager.start_runtime().await;
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
            sdk_host_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BroSDK Dashboard");
}
