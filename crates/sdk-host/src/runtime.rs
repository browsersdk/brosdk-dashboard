use std::{
    collections::{HashMap, HashSet},
    ffi::{c_char, c_void},
    path::Path,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use domain::{
    HostCommand, HostError, HostEvent, HostRequest, HostResponse, HostWireMessage, summarize_json,
};
use runtime_ipc::{IpcListener, read_message, write_message};
use sdk_ffi::{
    BroSdk, SdkFfiError, capabilities_for_path, default_library_path, extract_user_sig,
    get_user_sig_request, init_request, redact_value,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

static CALLBACK_SENDER: OnceLock<Mutex<Option<mpsc::UnboundedSender<RawSdkEvent>>>> =
    OnceLock::new();

#[derive(Debug)]
enum RawEventKind {
    Result,
    Log,
}

#[derive(Debug)]
struct RawSdkEvent {
    kind: RawEventKind,
    code: i32,
    bytes: Vec<u8>,
    received_at: DateTime<Utc>,
}

unsafe extern "C" fn result_callback(
    code: i32,
    _user_data: *mut c_void,
    data: *const c_char,
    len: usize,
) {
    send_raw_callback(RawEventKind::Result, code, data, len);
}

unsafe extern "C" fn log_callback(kind: i32, data: *const c_char, len: usize) {
    send_raw_callback(RawEventKind::Log, kind, data, len);
}

fn send_raw_callback(kind: RawEventKind, code: i32, data: *const c_char, len: usize) {
    let bytes = if data.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: SDK callback data is valid only for the callback duration, so copy it now.
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec()
    };
    let sender = CALLBACK_SENDER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    if let Some(sender) = sender {
        let _ = sender.send(RawSdkEvent {
            kind,
            code,
            bytes,
            received_at: Utc::now(),
        });
    }
}

struct HostRuntime {
    sdk: Option<BroSdk>,
    load_error: Option<String>,
    initialized: bool,
    sequence: u64,
    request_operations: HashMap<i32, String>,
    pending_lifecycle: HashMap<(LifecycleDirection, String), String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LifecycleDirection {
    Open,
    Close,
}

impl HostRuntime {
    fn load(sender: mpsc::UnboundedSender<RawSdkEvent>) -> Self {
        if let Ok(mut guard) = CALLBACK_SENDER.get_or_init(|| Mutex::new(None)).lock() {
            *guard = Some(sender);
        }

        let mut runtime = Self {
            sdk: None,
            load_error: None,
            initialized: false,
            sequence: 0,
            request_operations: HashMap::new(),
            pending_lifecycle: HashMap::new(),
        };
        match BroSdk::load(default_library_path()) {
            Ok(sdk) => {
                if let Err(error) = sdk.register_log_callback(Some(log_callback)) {
                    runtime.load_error = Some(redacted_message(&error.to_string()));
                } else if let Err(error) = sdk.register_result_callback(Some(result_callback)) {
                    runtime.load_error = Some(redacted_message(&error.to_string()));
                }
                runtime.sdk = Some(sdk);
            }
            Err(error) => runtime.load_error = Some(redacted_message(&error.to_string())),
        }
        runtime
    }

    fn handle(&mut self, request: &HostRequest) -> HostResponse {
        let result = match &request.command {
            HostCommand::Health => Ok(json!({
                "state": if self.load_error.is_some() { "degraded" } else { "running" },
                "pid": std::process::id(),
                "dllLoaded": self.sdk.is_some(),
                "initialized": self.initialized,
                "loadError": self.load_error,
            })),
            HostCommand::Capabilities => Ok(serde_json::to_value(capabilities_for_path(
                default_library_path(),
            ))
            .expect("capabilities serialize")),
            HostCommand::Initialize {
                work_dir,
                embedded_port,
                sdk_api_url,
                debug,
            } => self.initialize(
                Path::new(work_dir),
                *embedded_port,
                sdk_api_url.as_deref(),
                *debug,
            ),
            HostCommand::Info => {
                self.with_initialized("sdk_info", |sdk| sdk.info().map(|output| output.value))
            }
            HostCommand::EnvCreate { request } => self.with_initialized("sdk_env_create", |sdk| {
                sdk.env_create(request).map(|output| output.value)
            }),
            HostCommand::EnvUpdate { request } => self.with_initialized("sdk_env_update", |sdk| {
                sdk.env_update(request).map(|output| output.value)
            }),
            HostCommand::EnvDestroy { request } => self
                .with_initialized("sdk_env_destroy", |sdk| {
                    sdk.env_destroy(request).map(|output| output.value)
                }),
            HostCommand::EnvPage { request } => self.with_initialized("sdk_env_page", |sdk| {
                sdk.env_page(request).map(|output| output.value)
            }),
            HostCommand::EnvGetInfo { request } => self
                .with_initialized("sdk_env_getinfo", |sdk| {
                    sdk.env_get_info(request).map(|output| output.value)
                }),
            HostCommand::BrowserInfo => self.with_initialized("sdk_browser_info", |sdk| {
                sdk.browser_info().map(|output| output.value)
            }),
            HostCommand::NetworkDiagnostics { request } => self
                .with_initialized("sdk_network_diagnostics", |sdk| {
                    sdk.network_diagnostics(request).map(|output| output.value)
                }),
            HostCommand::SystemProxyDiagnostics => self
                .with_initialized("sdk_system_proxy_diagnostics", |sdk| {
                    sdk.system_proxy_diagnostics().map(|output| output.value)
                }),
            HostCommand::BrowserInstall { request: body } => {
                self.call_async_operation(request, "sdk_browser_install", |sdk| {
                    sdk.browser_install(body)
                })
            }
            HostCommand::BrowserCleanup { request } => self
                .with_initialized("sdk_browser_cleanup", |sdk| {
                    sdk.browser_cleanup(request).map(|output| output.value)
                }),
            HostCommand::BrowserOpen { request: body } => self.call_lifecycle(
                request,
                body,
                LifecycleDirection::Open,
                "sdk_browser_open",
                |sdk| sdk.browser_open(body),
            ),
            HostCommand::BrowserClose { request: body } => self.call_lifecycle(
                request,
                body,
                LifecycleDirection::Close,
                "sdk_browser_close",
                |sdk| sdk.browser_close(body),
            ),
            HostCommand::TrackMcpLifecycle { env_id, opening } => {
                self.track_mcp_lifecycle(request, env_id, *opening)
            }
            HostCommand::ClearMcpLifecycle { env_id, opening } => {
                self.clear_mcp_lifecycle(request, env_id, *opening)
            }
            HostCommand::BrowserCommand { request } => self
                .with_initialized("sdk_browser_command", |sdk| {
                    sdk.browser_command(request).map(|output| output.value)
                }),
            HostCommand::BrowserSnapshot { request } => self
                .with_initialized("sdk_browser_snapshot", |sdk| {
                    sdk.browser_snapshot(request).map(|output| output.value)
                }),
            HostCommand::BrowserEnvCheck { request } => self
                .with_initialized("sdk_browser_env_check", |sdk| {
                    sdk.browser_env_check(request).map(|output| output.value)
                }),
            HostCommand::Shutdown => self.shutdown(),
        };

        match result {
            Ok(mut value) => {
                redact_value(&mut value);
                HostResponse::success(request, value)
            }
            Err(error) => HostResponse::failure(request, error),
        }
    }

    fn initialize(
        &mut self,
        work_dir: &Path,
        embedded_port: Option<u16>,
        sdk_api_url: Option<&str>,
        debug: bool,
    ) -> HostResult<Value> {
        if self.initialized {
            return Err(host_error(
                "HOST_ALREADY_INITIALIZED",
                "SDK initialization is already complete",
            ));
        }
        let api_key = std::env::var("BROSDK_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                host_error(
                    "HOST_API_KEY_MISSING",
                    "BROSDK_API_KEY is not set in the runtime host environment",
                )
            })?;
        std::fs::create_dir_all(work_dir).map_err(|error| {
            host_error(
                "HOST_WORKDIR_FAILED",
                &format!("failed to create SDK workDir: {error}"),
            )
        })?;
        {
            let sdk = self.sdk()?;
            let user_sig_output = sdk
                .get_user_sig(&get_user_sig_request(&api_key))
                .map_err(sdk_error)?;
            let user_sig = extract_user_sig(&user_sig_output.value)
                .ok_or_else(|| {
                    host_error(
                        "HOST_USERSIG_MISSING",
                        "getUserSig response did not contain data.userSig",
                    )
                })?
                .to_string();
            sdk.init(&init_request(
                &user_sig,
                work_dir,
                embedded_port,
                sdk_api_url,
                debug,
            ))
            .map_err(sdk_error)?;
        }
        self.initialized = true;

        let info = self.sdk()?.info().map_err(sdk_error)?;
        Ok(json!({
            "state": "initialized",
            "embeddedPort": embedded_port,
            "sdkInfo": summarize_json(&info.value),
        }))
    }

    fn with_initialized<F>(&self, _name: &'static str, call: F) -> HostResult<Value>
    where
        F: FnOnce(&BroSdk) -> Result<Value, SdkFfiError>,
    {
        if !self.initialized {
            return Err(host_error(
                "HOST_NOT_INITIALIZED",
                "SDK must be initialized before this call",
            ));
        }
        call(self.sdk()?).map_err(sdk_error)
    }

    fn call_lifecycle<F>(
        &mut self,
        request: &HostRequest,
        body: &Value,
        direction: LifecycleDirection,
        _name: &'static str,
        call: F,
    ) -> HostResult<Value>
    where
        F: FnOnce(&BroSdk) -> Result<i32, SdkFfiError>,
    {
        if !self.initialized {
            return Err(host_error(
                "HOST_NOT_INITIALIZED",
                "SDK must be initialized before this call",
            ));
        }
        let accepted_code = call(self.sdk()?).map_err(sdk_error)?;
        if let Some(operation_id) = request.operation_id.as_ref() {
            for env_id in lifecycle_env_ids(body) {
                self.pending_lifecycle
                    .insert((direction, env_id), operation_id.clone());
            }
        }
        Ok(json!({ "acceptedCode": accepted_code, "state": "accepted" }))
    }

    fn call_async_operation<F>(
        &mut self,
        request: &HostRequest,
        _name: &'static str,
        call: F,
    ) -> HostResult<Value>
    where
        F: FnOnce(&BroSdk) -> Result<i32, SdkFfiError>,
    {
        if !self.initialized {
            return Err(host_error(
                "HOST_NOT_INITIALIZED",
                "SDK must be initialized before this call",
            ));
        }
        let accepted_code = call(self.sdk()?).map_err(sdk_error)?;
        if accepted_code > 0
            && let Some(operation_id) = request.operation_id.as_ref()
        {
            self.request_operations
                .insert(accepted_code, operation_id.clone());
        }
        Ok(json!({ "acceptedCode": accepted_code, "state": "accepted" }))
    }

    fn track_mcp_lifecycle(
        &mut self,
        request: &HostRequest,
        env_id: &str,
        opening: bool,
    ) -> HostResult<Value> {
        if !self.initialized {
            return Err(host_error(
                "HOST_NOT_INITIALIZED",
                "SDK must be initialized before this call",
            ));
        }
        if env_id.is_empty() {
            return Err(host_error("HOST_INVALID_REQUEST", "envId is required"));
        }
        let operation_id = request.operation_id.as_ref().ok_or_else(|| {
            host_error(
                "HOST_INVALID_REQUEST",
                "operationId is required for MCP lifecycle tracking",
            )
        })?;
        let direction = if opening {
            LifecycleDirection::Open
        } else {
            LifecycleDirection::Close
        };
        self.pending_lifecycle
            .insert((direction, env_id.into()), operation_id.clone());
        Ok(json!({ "tracked": true }))
    }

    fn clear_mcp_lifecycle(
        &mut self,
        request: &HostRequest,
        env_id: &str,
        opening: bool,
    ) -> HostResult<Value> {
        let direction = if opening {
            LifecycleDirection::Open
        } else {
            LifecycleDirection::Close
        };
        let key = (direction, env_id.to_string());
        let should_remove = request
            .operation_id
            .as_ref()
            .is_none_or(|operation_id| self.pending_lifecycle.get(&key) == Some(operation_id));
        if should_remove {
            self.pending_lifecycle.remove(&key);
        }
        Ok(json!({ "tracked": false }))
    }

    fn normalize_event(&mut self, raw: RawSdkEvent) -> HostEvent {
        let mut payload = serde_json::from_slice::<Value>(&raw.bytes).unwrap_or_else(|_| {
            json!({
                "message": "non-JSON SDK callback payload omitted",
                "bytes": raw.bytes.len(),
            })
        });
        redact_value(&mut payload);
        let request_id = find_i32(&payload, &["reqId", "requestId", "request_id"]);
        let event_name = find_string(&payload, &["eventName", "event", "name", "type"])
            .unwrap_or_else(|| match raw.kind {
                RawEventKind::Result => "sdk-result".into(),
                RawEventKind::Log => "sdk-log".into(),
            });
        let env_id = find_string(&payload, &["envId", "env_id"]);
        let direction = lifecycle_direction(&event_name);
        let operation_id = request_id
            .and_then(|id| self.request_operations.get(&id).cloned())
            .or_else(|| {
                Some(
                    self.pending_lifecycle
                        .get(&(direction?, env_id.clone()?))?
                        .clone(),
                )
            });
        if let (Some(request_id), Some(operation_id)) = (request_id, operation_id.as_ref()) {
            self.request_operations
                .insert(request_id, operation_id.clone());
        }
        if is_terminal_lifecycle_event(&event_name)
            && let (Some(direction), Some(env_id)) = (direction, env_id.as_ref())
        {
            self.pending_lifecycle.remove(&(direction, env_id.clone()));
        }
        self.sequence += 1;
        HostEvent {
            sequence: self.sequence,
            event_type: match raw.kind {
                RawEventKind::Result => "sdk.result".into(),
                RawEventKind::Log => "sdk.log".into(),
            },
            code: raw.code,
            event_name,
            request_id,
            operation_id,
            env_id,
            payload,
            received_at: raw.received_at,
        }
    }

    fn shutdown(&mut self) -> HostResult<Value> {
        if let Some(sdk) = self.sdk.take() {
            sdk.shutdown().map_err(sdk_error)?;
        }
        self.initialized = false;
        Ok(json!({ "state": "stopped" }))
    }

    fn shutdown_best_effort(&mut self) {
        if let Some(sdk) = self.sdk.take() {
            let _ = sdk.shutdown();
        }
        self.initialized = false;
    }

    fn sdk(&self) -> HostResult<&BroSdk> {
        self.sdk.as_ref().ok_or_else(|| {
            host_error(
                "HOST_DLL_LOAD_FAILED",
                self.load_error
                    .as_deref()
                    .unwrap_or("brosdk.dll is unavailable"),
            )
        })
    }
}

fn lifecycle_env_ids(value: &Value) -> HashSet<String> {
    value
        .get("envs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| match item {
            Value::String(env_id) if !env_id.is_empty() => Some(env_id.clone()),
            Value::Object(_) => find_string(item, &["envId", "env_id", "id"]),
            Value::Number(env_id) => Some(env_id.to_string()),
            _ => None,
        })
        .collect()
}

fn lifecycle_direction(event_name: &str) -> Option<LifecycleDirection> {
    let event_name = event_name.to_ascii_lowercase();
    if event_name.contains("browser-open") {
        Some(LifecycleDirection::Open)
    } else if event_name.contains("browser-close") {
        Some(LifecycleDirection::Close)
    } else {
        None
    }
}

fn is_terminal_lifecycle_event(event_name: &str) -> bool {
    let event_name = event_name.to_ascii_lowercase();
    event_name.contains("success") || event_name.contains("failed") || event_name.contains("error")
}

type HostResult<T> = std::result::Result<T, HostError>;

pub async fn serve(endpoint: &str) -> Result<()> {
    let listener = IpcListener::bind(endpoint)
        .with_context(|| format!("failed to bind runtime IPC at {endpoint}"))?;
    let stream = listener
        .accept()
        .await
        .with_context(|| format!("failed to accept runtime IPC at {endpoint}"))?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (message_tx, mut message_rx) = mpsc::channel(64);
    tokio::spawn(async move {
        loop {
            let message = read_message(&mut reader).await;
            let terminal = matches!(message, Ok(None) | Err(_));
            if message_tx.send(message).await.is_err() || terminal {
                break;
            }
        }
    });
    let (callback_tx, mut callback_rx) = mpsc::unbounded_channel();
    let mut runtime = HostRuntime::load(callback_tx);

    loop {
        tokio::select! {
            message = message_rx.recv() => {
                let Some(message) = message else {
                    break;
                };
                let Some(message) = message.context("failed to read runtime IPC message")? else {
                    break;
                };
                let HostWireMessage::Request(request) = message else {
                    continue;
                };
                let should_stop = matches!(request.command, HostCommand::Shutdown);
                let response = runtime.handle(&request);
                write_message(&mut writer, &HostWireMessage::Response(response))
                    .await
                    .context("failed to write runtime IPC response")?;
                if should_stop {
                    break;
                }
            }
            Some(raw) = callback_rx.recv() => {
                let event = runtime.normalize_event(raw);
                write_message(&mut writer, &HostWireMessage::Event(event))
                    .await
                    .context("failed to write runtime IPC event")?;
            }
        }
    }

    runtime.shutdown_best_effort();
    if let Ok(mut guard) = CALLBACK_SENDER.get_or_init(|| Mutex::new(None)).lock() {
        *guard = None;
    }
    Ok(())
}

fn sdk_error(error: SdkFfiError) -> HostError {
    let sdk_code = match &error {
        SdkFfiError::Call { code, .. } => Some(*code),
        _ => None,
    };
    HostError {
        code: "HOST_SDK_CALL_FAILED".into(),
        message: redacted_message(&error.to_string()),
        sdk_code,
    }
}

fn host_error(code: &str, message: &str) -> HostError {
    HostError {
        code: code.into(),
        message: redacted_message(message),
        sdk_code: None,
    }
}

fn redacted_message(message: &str) -> String {
    let mut value = Value::String(message.to_string());
    redact_value(&mut value);
    value
        .as_str()
        .unwrap_or("runtime host error")
        .chars()
        .take(512)
        .collect()
}

fn find_i32(value: &Value, keys: &[&str]) -> Option<i32> {
    find_value(value, keys).and_then(|value| {
        value
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    })
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    find_value(value, keys).and_then(|value| value.as_str().map(str::to_string))
}

fn find_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key) {
                    return Some(value);
                }
            }
            map.values().find_map(|value| find_value(value, keys))
        }
        Value::Array(values) => values.iter().find_map(|value| find_value(value, keys)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_normalization_maps_operation_and_redacts() {
        let mut runtime = HostRuntime {
            sdk: None,
            load_error: None,
            initialized: false,
            sequence: 0,
            request_operations: HashMap::from([(42, "operation-1".into())]),
            pending_lifecycle: HashMap::new(),
        };
        let event = runtime.normalize_event(RawSdkEvent {
            kind: RawEventKind::Result,
            code: 0,
            bytes: br#"{"type":"browser-open-success","reqId":42,"envId":"env-1","data":{"remoteDebuggingPort":9222},"authorization":"secret"}"#.to_vec(),
            received_at: Utc::now(),
        });

        assert_eq!(event.operation_id.as_deref(), Some("operation-1"));
        assert_eq!(event.env_id.as_deref(), Some("env-1"));
        assert_eq!(event.event_name, "browser-open-success");
        assert_eq!(event.payload["data"]["remoteDebuggingPort"], 9222);
        assert_eq!(event.payload["authorization"], "[redacted]");
    }

    #[test]
    fn callback_req_id_is_bound_from_pending_environment_operation() {
        let mut runtime = HostRuntime {
            sdk: None,
            load_error: None,
            initialized: false,
            sequence: 0,
            request_operations: HashMap::new(),
            pending_lifecycle: HashMap::from([(
                (LifecycleDirection::Open, "env-1".into()),
                "operation-1".into(),
            )]),
        };
        let event = runtime.normalize_event(RawSdkEvent {
            kind: RawEventKind::Result,
            code: 0,
            bytes: br#"{"type":"browser-open-success","reqId":42,"envId":"env-1"}"#.to_vec(),
            received_at: Utc::now(),
        });

        assert_eq!(event.operation_id.as_deref(), Some("operation-1"));
        assert_eq!(
            runtime.request_operations.get(&42).map(String::as_str),
            Some("operation-1")
        );
        assert!(runtime.pending_lifecycle.is_empty());
    }

    #[test]
    fn mcp_lifecycle_tracking_binds_callbacks_before_the_http_tool_call() {
        let mut runtime = HostRuntime {
            sdk: None,
            load_error: None,
            initialized: true,
            sequence: 0,
            request_operations: HashMap::new(),
            pending_lifecycle: HashMap::new(),
        };
        let request = HostRequest {
            id: "request-1".into(),
            operation_id: Some("operation-1".into()),
            command: HostCommand::TrackMcpLifecycle {
                env_id: "env-1".into(),
                opening: true,
            },
        };
        runtime
            .track_mcp_lifecycle(&request, "env-1", true)
            .expect("track lifecycle");
        let event = runtime.normalize_event(RawSdkEvent {
            kind: RawEventKind::Result,
            code: 0,
            bytes: br#"{"type":"browser-open-success","reqId":42,"envId":"env-1"}"#.to_vec(),
            received_at: Utc::now(),
        });

        assert_eq!(event.operation_id.as_deref(), Some("operation-1"));
        assert_eq!(
            runtime.request_operations.get(&42).map(String::as_str),
            Some("operation-1")
        );
        assert!(runtime.pending_lifecycle.is_empty());
    }

    #[test]
    fn non_json_callback_payload_is_not_forwarded() {
        let mut runtime = HostRuntime {
            sdk: None,
            load_error: None,
            initialized: false,
            sequence: 0,
            request_operations: HashMap::new(),
            pending_lifecycle: HashMap::new(),
        };
        let event = runtime.normalize_event(RawSdkEvent {
            kind: RawEventKind::Log,
            code: 1,
            bytes: b"possible credential text".to_vec(),
            received_at: Utc::now(),
        });
        assert_eq!(event.payload["bytes"], 24);
        assert!(!event.payload.to_string().contains("credential"));
    }

    #[test]
    fn initialized_runtime_rejects_second_init_before_reading_credentials() {
        let mut runtime = HostRuntime {
            sdk: None,
            load_error: None,
            initialized: true,
            sequence: 0,
            request_operations: HashMap::new(),
            pending_lifecycle: HashMap::new(),
        };
        let error = runtime
            .initialize(Path::new("unused"), None, None, false)
            .expect_err("second init must fail");
        assert_eq!(error.code, "HOST_ALREADY_INITIALIZED");
    }
}
