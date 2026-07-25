#[tauri::command]
async fn manager_snapshot(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::DashboardSnapshot, String> {
    Ok(manager.snapshot().await)
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
            sdk_host_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BroSDK Dashboard");
}
