use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use domain::{
    HostCommand, HostEvent, HostRequest, HostResponse, HostWireMessage, RuntimeHostState,
    RuntimeHostStatus, SdkCapabilities, SmokeReport,
};
use runtime_ipc::{read_message, write_message};
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    sync::{broadcast, mpsc, oneshot, watch},
};
use uuid::Uuid;

const START_TIMEOUT: Duration = Duration::from_secs(8);
const CALL_TIMEOUT: Duration = Duration::from_secs(15);
static HOST_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum SdkClientError {
    #[error("sdk-host executable not found; run `cargo build -p sdk-host` first")]
    HostNotFound,
    #[error("failed to spawn sdk-host: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("sdk-host exited with {code:?}: {stderr}")]
    Exit { code: Option<i32>, stderr: String },
    #[error("sdk-host returned invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("runtime IPC failed: {0}")]
    Ipc(#[from] runtime_ipc::IpcError),
    #[error("runtime host request timed out after {0:?}")]
    Timeout(Duration),
    #[error("runtime host disconnected: {0}")]
    Disconnected(String),
    #[error("runtime host rejected the request ({code}): {message}")]
    Remote {
        code: String,
        message: String,
        sdk_code: Option<i32>,
    },
    #[error("runtime host command channel is closed")]
    ChannelClosed,
}

#[derive(Debug, Clone)]
pub struct SdkHostClient {
    host_path: PathBuf,
}

impl SdkHostClient {
    pub fn discover() -> Result<Self, SdkClientError> {
        Ok(Self {
            host_path: discover_host_path()?,
        })
    }

    pub fn host_path(&self) -> &PathBuf {
        &self.host_path
    }

    pub async fn capabilities(&self) -> Result<SdkCapabilities, SdkClientError> {
        self.run_json(["capabilities", "--json"]).await
    }

    pub async fn smoke(&self) -> Result<SmokeReport, SdkClientError> {
        self.run_json(["smoke", "--json"]).await
    }

    async fn run_json<T, const N: usize>(&self, args: [&str; N]) -> Result<T, SdkClientError>
    where
        T: serde::de::DeserializeOwned,
    {
        let output = Command::new(&self.host_path).args(args).output().await?;
        if !output.status.success() {
            return Err(SdkClientError::Exit {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(parse_host_json(&output.stdout)?)
    }
}

#[derive(Clone)]
pub struct RuntimeHost {
    commands: mpsc::Sender<ActorCommand>,
    status: watch::Receiver<RuntimeHostStatus>,
    events: broadcast::Sender<HostEvent>,
}

impl RuntimeHost {
    pub async fn start() -> Result<Self, SdkClientError> {
        Self::start_with_path(discover_host_path()?).await
    }

    pub async fn start_with_path(host_path: PathBuf) -> Result<Self, SdkClientError> {
        let generation = HOST_GENERATION.fetch_add(1, Ordering::Relaxed);
        let endpoint = unique_endpoint();
        prepare_endpoint(&endpoint)?;
        let mut command = Command::new(&host_path);
        command
            .arg("serve")
            .arg("--endpoint")
            .arg(&endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        hide_process_window(&mut command);
        let mut child = command.spawn()?;
        let pid = child.id();
        let stream = match runtime_ipc::connect(&endpoint, START_TIMEOUT).await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(error.into());
            }
        };

        let initial_status = RuntimeHostStatus {
            state: RuntimeHostState::Running,
            pid,
            generation,
            endpoint: Some(endpoint),
            last_error: None,
        };
        let (command_tx, command_rx) = mpsc::channel(64);
        let (status_tx, status_rx) = watch::channel(initial_status);
        let (event_tx, _) = broadcast::channel(512);
        tokio::spawn(run_actor(
            child,
            stream,
            command_rx,
            status_tx,
            event_tx.clone(),
        ));

        let host = Self {
            commands: command_tx,
            status: status_rx,
            events: event_tx,
        };
        if let Err(error) = host
            .call_with_timeout(HostCommand::Health, None, Duration::from_secs(3))
            .await
        {
            let _ = host.kill().await;
            return Err(error);
        }
        Ok(host)
    }

    pub fn status(&self) -> RuntimeHostStatus {
        self.status.borrow().clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<HostEvent> {
        self.events.subscribe()
    }

    pub async fn call(
        &self,
        command: HostCommand,
        operation_id: Option<String>,
    ) -> Result<serde_json::Value, SdkClientError> {
        self.call_with_timeout(command, operation_id, CALL_TIMEOUT)
            .await
    }

    pub async fn call_with_timeout(
        &self,
        command: HostCommand,
        operation_id: Option<String>,
        timeout: Duration,
    ) -> Result<serde_json::Value, SdkClientError> {
        let request = HostRequest {
            id: Uuid::new_v4().to_string(),
            operation_id,
            command,
        };
        let (response_tx, response_rx) = oneshot::channel();
        self.commands
            .send(ActorCommand::Call {
                request,
                response: response_tx,
            })
            .await
            .map_err(|_| SdkClientError::ChannelClosed)?;
        let response = tokio::time::timeout(timeout, response_rx)
            .await
            .map_err(|_| SdkClientError::Timeout(timeout))?
            .map_err(|_| SdkClientError::ChannelClosed)??;
        if response.ok {
            Ok(response.result.unwrap_or(serde_json::Value::Null))
        } else {
            let error = response.error.unwrap_or(domain::HostError {
                code: "HOST_UNKNOWN_ERROR".into(),
                message: "runtime host returned an empty error".into(),
                sdk_code: None,
            });
            Err(SdkClientError::Remote {
                code: error.code,
                message: error.message,
                sdk_code: error.sdk_code,
            })
        }
    }

    pub async fn capabilities(&self) -> Result<SdkCapabilities, SdkClientError> {
        let value = self.call(HostCommand::Capabilities, None).await?;
        Ok(serde_json::from_value(value)?)
    }

    pub async fn initialize(
        &self,
        work_dir: String,
        embedded_port: Option<u16>,
    ) -> Result<serde_json::Value, SdkClientError> {
        self.call(
            HostCommand::Initialize {
                work_dir,
                embedded_port,
            },
            None,
        )
        .await
    }

    pub async fn stop(&self) -> Result<RuntimeHostStatus, SdkClientError> {
        let _ = self.call(HostCommand::Shutdown, None).await?;
        self.wait_for_terminal(Duration::from_secs(8)).await
    }

    pub async fn kill(&self) -> Result<RuntimeHostStatus, SdkClientError> {
        self.commands
            .send(ActorCommand::Kill)
            .await
            .map_err(|_| SdkClientError::ChannelClosed)?;
        self.wait_for_terminal(Duration::from_secs(8)).await
    }

    async fn wait_for_terminal(
        &self,
        timeout: Duration,
    ) -> Result<RuntimeHostStatus, SdkClientError> {
        let mut status = self.status.clone();
        let wait = async {
            loop {
                let current = status.borrow().clone();
                if matches!(
                    current.state,
                    RuntimeHostState::Stopped | RuntimeHostState::Degraded
                ) {
                    return Ok(current);
                }
                status
                    .changed()
                    .await
                    .map_err(|_| SdkClientError::ChannelClosed)?;
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .map_err(|_| SdkClientError::Timeout(timeout))?
    }
}

enum ActorCommand {
    Call {
        request: HostRequest,
        response: oneshot::Sender<Result<HostResponse, SdkClientError>>,
    },
    Kill,
}

async fn run_actor(
    mut child: Child,
    stream: runtime_ipc::IpcStream,
    mut commands: mpsc::Receiver<ActorCommand>,
    status: watch::Sender<RuntimeHostStatus>,
    events: broadcast::Sender<HostEvent>,
) {
    trace_ipc("actor started");
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
    let mut pending =
        HashMap::<String, oneshot::Sender<Result<HostResponse, SdkClientError>>>::new();
    let mut graceful_stop = false;
    let mut terminal_error = None;

    loop {
        tokio::select! {
            exit = child.wait() => {
                trace_ipc("child exited");
                terminal_error = match exit {
                    Ok(exit) if graceful_stop && exit.success() => None,
                    Ok(exit) => Some(format!("sdk-host exited with status {exit}")),
                    Err(error) => Some(format!("failed waiting for sdk-host: {error}")),
                };
                break;
            }
            command = commands.recv() => {
                match command {
                    Some(ActorCommand::Call { request, response }) => {
                        trace_ipc("sending request");
                        graceful_stop |= matches!(request.command, HostCommand::Shutdown);
                        let id = request.id.clone();
                        if let Err(error) = write_message(
                            &mut writer,
                            &HostWireMessage::Request(request),
                        ).await {
                            let message = error.to_string();
                            let _ = response.send(Err(SdkClientError::Disconnected(message.clone())));
                            terminal_error = Some(message);
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            break;
                        }
                        pending.insert(id, response);
                        trace_ipc("request sent");
                    }
                    Some(ActorCommand::Kill) => {
                        trace_ipc("forcing child termination");
                        terminal_error = Some("sdk-host was terminated by the supervisor".into());
                        let _ = child.kill().await;
                    }
                    None => {
                        graceful_stop = true;
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        break;
                    }
                }
                pending.retain(|_, sender| !sender.is_closed());
            }
            message = message_rx.recv() => {
                match message {
                    Some(Ok(Some(HostWireMessage::Response(response)))) => {
                        trace_ipc("response received");
                        if let Some(sender) = pending.remove(&response.id) {
                            let _ = sender.send(Ok(response));
                        }
                    }
                    Some(Ok(Some(HostWireMessage::Event(event)))) => {
                        trace_ipc("event received");
                        let _ = events.send(event);
                    }
                    Some(Ok(Some(HostWireMessage::Request(_)))) => {}
                    Some(Ok(None)) | None => {
                        trace_ipc("IPC reached EOF");
                        if !graceful_stop {
                            terminal_error = Some("runtime IPC closed unexpectedly".into());
                        }
                        let _ = child.wait().await;
                        break;
                    }
                    Some(Err(error)) => {
                        trace_ipc("IPC read failed");
                        terminal_error = Some(error.to_string());
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        break;
                    }
                }
                pending.retain(|_, sender| !sender.is_closed());
            }
        }
    }

    let message = terminal_error.unwrap_or_else(|| "runtime host stopped".into());
    for (_, sender) in pending {
        let _ = sender.send(Err(SdkClientError::Disconnected(message.clone())));
    }
    let previous = status.borrow().clone();
    let final_status = RuntimeHostStatus {
        state: if graceful_stop {
            RuntimeHostState::Stopped
        } else {
            RuntimeHostState::Degraded
        },
        pid: None,
        generation: previous.generation,
        endpoint: previous.endpoint,
        last_error: if graceful_stop { None } else { Some(message) },
    };
    let _ = status.send(final_status);
}

fn trace_ipc(message: &str) {
    if std::env::var_os("BROSDK_IPC_TRACE").is_some() {
        eprintln!("sdk-client IPC: {message}");
    }
}

fn parse_host_json<T>(stdout: &[u8]) -> Result<T, serde_json::Error>
where
    T: serde::de::DeserializeOwned,
{
    if let Ok(value) = serde_json::from_slice(stdout) {
        return Ok(value);
    }

    let text = String::from_utf8_lossy(stdout);
    for (idx, _) in text.match_indices('{').rev() {
        let line_start = idx == 0
            || matches!(
                text.as_bytes().get(idx.saturating_sub(1)),
                Some(b'\n') | Some(b'\r')
            );
        if !line_start {
            continue;
        }
        if let Ok(value) = serde_json::from_str(text[idx..].trim()) {
            return Ok(value);
        }
    }

    serde_json::from_slice(stdout)
}

pub fn discover_host_path() -> Result<PathBuf, SdkClientError> {
    let exe_name = format!("sdk-host{}", platform::executable_suffix());

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(dir) = current_exe.parent()
    {
        let sibling = dir.join(&exe_name);
        if sibling.exists() {
            return Ok(sibling);
        }
    }

    let workspace_debug = platform::workspace_root()
        .join("target")
        .join("debug")
        .join(&exe_name);
    if workspace_debug.exists() {
        return Ok(workspace_debug);
    }

    let workspace_release = platform::workspace_root()
        .join("target")
        .join("release")
        .join(&exe_name);
    if workspace_release.exists() {
        return Ok(workspace_release);
    }

    Err(SdkClientError::HostNotFound)
}

#[cfg(windows)]
fn unique_endpoint() -> String {
    format!(r"\\.\pipe\brosdk-dashboard-{}", Uuid::new_v4())
}

#[cfg(unix)]
fn unique_endpoint() -> String {
    std::env::temp_dir()
        .join(format!("brosdk-dashboard-{}.sock", Uuid::new_v4()))
        .display()
        .to_string()
}

fn prepare_endpoint(endpoint: &str) -> Result<(), SdkClientError> {
    #[cfg(unix)]
    if let Some(parent) = std::path::Path::new(endpoint).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = endpoint;
    Ok(())
}

#[cfg(windows)]
fn hide_process_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.as_std_mut().creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_process_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Tiny {
        ok: bool,
    }

    #[test]
    fn parses_json_after_sdk_stdout_logs() {
        let stdout = br#"{"time":"log"}
{"ok":true}
"#;
        let parsed: Tiny = parse_host_json(stdout).expect("parse final object");
        assert_eq!(parsed, Tiny { ok: true });
    }

    #[test]
    fn named_pipe_endpoint_is_unique() {
        assert_ne!(unique_endpoint(), unique_endpoint());
    }
}
