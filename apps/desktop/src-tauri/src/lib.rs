#[tauri::command]
async fn manager_snapshot() -> Result<domain::DashboardSnapshot, String> {
    Ok(manager::snapshot().await)
}

#[tauri::command]
async fn run_sdk_smoke() -> Result<domain::SmokeReport, String> {
    manager::run_sdk_smoke()
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
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            manager_snapshot,
            run_sdk_smoke,
            sdk_host_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BroSDK Dashboard");
}
