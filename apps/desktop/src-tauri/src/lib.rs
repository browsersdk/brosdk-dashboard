use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    AppHandle, Manager, Runtime, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "brosdk-dashboard-tray";
const TRAY_OPEN_ID: &str = "tray-open";
const TRAY_EXIT_ID: &str = "tray-exit";
static EXITING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayMenuAction {
    Show,
    Exit,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowCloseAction {
    Hide,
    Close,
}

fn tray_menu_action(id: &str) -> TrayMenuAction {
    match id {
        TRAY_OPEN_ID => TrayMenuAction::Show,
        TRAY_EXIT_ID => TrayMenuAction::Exit,
        _ => TrayMenuAction::Ignore,
    }
}

fn window_close_action(label: &str, exiting: bool) -> WindowCloseAction {
    if label == MAIN_WINDOW_LABEL && !exiting {
        WindowCloseAction::Hide
    } else {
        WindowCloseAction::Close
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if EXITING.load(Ordering::Acquire) {
        return;
    }
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn exit_from_tray<R: Runtime>(app: &AppHandle<R>) {
    if EXITING.swap(true, Ordering::AcqRel) {
        return;
    }

    let manager = app.state::<manager::Manager>().inner().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(5), manager.stop_runtime()).await;
        app.exit(0);
    });
}

fn configure_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, TRAY_OPEN_ID, "打开主界面", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, TRAY_EXIT_ID, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &exit])?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("BroSDK Dashboard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match tray_menu_action(event.id().as_ref()) {
            TrayMenuAction::Show => show_main_window(app),
            TrayMenuAction::Exit => exit_from_tray(app),
            TrayMenuAction::Ignore => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
async fn manager_snapshot(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::DashboardSnapshot, String> {
    manager.snapshot().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_configure_api_key(
    manager: tauri::State<'_, manager::Manager>,
    api_key: String,
) -> Result<domain::ApiKeyInitializationResult, String> {
    manager
        .configure_api_key(api_key)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_clear_api_key(manager: tauri::State<'_, manager::Manager>) -> Result<(), String> {
    manager
        .clear_api_key()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_configure_ai_provider(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::AiProviderConfigInput,
) -> Result<domain::AiProviderStatus, String> {
    manager
        .configure_ai_provider(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn manager_clear_ai_api_key(
    manager: tauri::State<'_, manager::Manager>,
) -> Result<domain::AiProviderStatus, String> {
    manager
        .clear_ai_api_key()
        .map_err(|error| error.to_string())
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
async fn manager_create_environment(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::EnvironmentCreateInput,
) -> Result<domain::OperationRecord, String> {
    manager
        .create_environment(input)
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
async fn manager_batch_environment_action(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::EnvironmentBatchInput,
) -> Result<domain::EnvironmentBatchResult, String> {
    manager
        .batch_environment_action(input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_update_environment_metadata(
    manager: tauri::State<'_, manager::Manager>,
    input: domain::EnvironmentMetadataUpdateInput,
) -> Result<domain::OperationRecord, String> {
    manager
        .update_environment_metadata(input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_destroy_environment(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .destroy_environment(&env_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_cleanup_environment_local_data(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationExecution, String> {
    manager
        .cleanup_environment_local_data(&env_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn manager_capture_environment_diagnostic(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationExecution, String> {
    manager
        .capture_environment_diagnostic(&env_id)
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
async fn manager_refresh_environment_detail(
    manager: tauri::State<'_, manager::Manager>,
    env_id: String,
) -> Result<domain::OperationRecord, String> {
    manager
        .refresh_environment_detail(&env_id)
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
async fn manager_discover_embedded_mcp_tools(
    manager: tauri::State<'_, manager::Manager>,
    request: domain::McpToolDiscoveryRequest,
) -> Result<domain::McpToolDiscovery, String> {
    manager
        .discover_embedded_mcp_tools(request)
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
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _cwd| {
                show_main_window(app);
            },
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(manager.clone())
        .setup(move |app| {
            configure_tray(app)?;
            let startup_manager = manager.clone();
            tauri::async_runtime::spawn(async move {
                if startup_manager.start_runtime().await.is_ok() {
                    let _ = startup_manager.apply_startup_policy().await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window_close_action(window.label(), EXITING.load(Ordering::Acquire))
                    == WindowCloseAction::Hide
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            manager_snapshot,
            manager_configure_api_key,
            manager_clear_api_key,
            manager_configure_ai_provider,
            manager_clear_ai_api_key,
            run_sdk_smoke,
            runtime_host_start,
            runtime_host_stop,
            runtime_host_kill,
            manager_sync_environments,
            manager_create_environment,
            manager_reconcile_runtimes,
            manager_start_environment,
            manager_stop_environment,
            manager_batch_environment_action,
            manager_update_environment_metadata,
            manager_destroy_environment,
            manager_cleanup_environment_local_data,
            manager_capture_environment_diagnostic,
            manager_events_since,
            manager_update_settings,
            manager_cancel_operation,
            manager_refresh_environment_details,
            manager_refresh_environment_detail,
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
            manager_discover_embedded_mcp_tools,
            sdk_host_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BroSDK Dashboard");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_ids_map_only_to_supported_actions() {
        assert_eq!(tray_menu_action(TRAY_OPEN_ID), TrayMenuAction::Show);
        assert_eq!(tray_menu_action(TRAY_EXIT_ID), TrayMenuAction::Exit);
        assert_eq!(tray_menu_action("unknown"), TrayMenuAction::Ignore);
    }

    #[test]
    fn main_window_close_hides_until_explicit_exit() {
        assert_eq!(
            window_close_action(MAIN_WINDOW_LABEL, false),
            WindowCloseAction::Hide
        );
        assert_eq!(
            window_close_action(MAIN_WINDOW_LABEL, true),
            WindowCloseAction::Close
        );
        assert_eq!(
            window_close_action("secondary", false),
            WindowCloseAction::Close
        );
    }
}
