use std::{error::Error, time::Duration};

use domain::EnvironmentRecord;
use manager::Manager;
use serde_json::{Value, json};
use tokio::{
    net::TcpStream,
    time::{Instant, sleep},
};

const READY_TIMEOUT: Duration = Duration::from_secs(180);
const STOP_TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let manager = Manager::try_new()?;
    let mut runtime_started = false;
    let mut target_started = false;
    let mut target_stopped = false;
    let mut ready_source = None;
    let mut evaluate_verified = false;
    let mut manual_close_verified = false;
    let mut embedded_mcp_reachable = false;
    let mut environment_count = 0;
    let requested_env_id = non_empty_env("BROSDK_E2E_ENV_ID");
    let mut target_env_id = requested_env_id.clone();

    let result = async {
        if non_empty_env("BROSDK_API_KEY").is_none() {
            return Ok::<Value, Box<dyn Error>>(json!({
                "status": "skipped",
                "reason": "BROSDK_API_KEY is not set",
            }));
        }

        manager.start_runtime().await?;
        runtime_started = true;
        let capabilities = manager.snapshot().await?.capabilities;
        let sync = manager.sync_environments().await?;
        ensure_operation_succeeded(&sync)?;
        let reconcile = manager.reconcile_runtimes().await?;
        ensure_operation_succeeded(&reconcile)?;
        let snapshot = manager.snapshot().await?;
        environment_count = snapshot.environments.len();
        if let Some(port) = embedded_port() {
            wait_for_embedded_port(port).await?;
            embedded_mcp_reachable = true;
        }

        if target_env_id.is_none()
            && env_flag("BROSDK_E2E_USE_ONLY_ENV")
            && snapshot.environments.len() == 1
        {
            target_env_id = Some(snapshot.environments[0].env_id.clone());
        }

        let Some(env_id) = target_env_id.as_deref() else {
            return Ok(json!({
                "status": "skipped",
                "reason": "BROSDK_E2E_ENV_ID is not set and the single-environment fallback is disabled or ambiguous; lifecycle mutation was not attempted",
                "environmentCount": environment_count,
                "embeddedMcpAvailable": capabilities.embedded_mcp,
                "embeddedMcpConfigured": non_empty_env("BROSDK_EMBEDDED_PORT").is_some(),
            }));
        };
        if !snapshot
            .environments
            .iter()
            .any(|environment| environment.env_id == env_id)
        {
            return Err("BROSDK_E2E_ENV_ID does not belong to the current account mirror".into());
        }
        let initial = snapshot
            .environments
            .iter()
            .find(|environment| environment.env_id == env_id)
            .ok_or("environment disappeared from the account mirror")?;
        if initial.status == "ready" {
            return Err(
                "target environment was already running; E2E refused to take ownership or stop it"
                    .into(),
            );
        }

        let start = manager.start_environment(env_id).await?;
        ensure_operation_active_or_succeeded(&start)?;
        target_started = true;
        let (ready, source) = wait_for_state(&manager, env_id, "ready", READY_TIMEOUT).await?;
        ready_source = Some(source);

        let targets = manager
            .browser_command(
                env_id,
                "Target.getTargets",
                json!({}),
                None,
            )
            .await?;
        ensure_operation_succeeded(&targets.operation)?;
        let target_id = find_page_target_id(&targets.response)
            .ok_or("Target.getTargets did not return a page target")?;
        let attach = manager
            .browser_command(
                env_id,
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
                None,
            )
            .await?;
        ensure_operation_succeeded(&attach.operation)?;
        let session_id = find_string_key(&attach.response, "sessionId")
            .ok_or("Target.attachToTarget did not return sessionId")?;
        let command = manager
            .browser_command(
                env_id,
                "Runtime.evaluate",
                json!({
                    "expression": "21 * 2",
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
                Some(&session_id),
            )
            .await?;
        ensure_operation_succeeded(&command.operation)?;
        let verified = contains_number(&command.response, 42.0);
        let _ = manager
            .browser_command(
                env_id,
                "Target.detachFromTarget",
                json!({ "sessionId": session_id }),
                None,
            )
            .await;
        if !verified {
            return Err("Runtime.evaluate response did not contain the expected value".into());
        }
        evaluate_verified = true;

        if env_flag("BROSDK_E2E_SIMULATE_MANUAL_CLOSE") {
            let close = manager
                .browser_command(env_id, "Browser.close", json!({}), None)
                .await?;
            ensure_operation_succeeded(&close.operation)?;
            manual_close_verified =
                wait_for_manual_close(&manager, env_id, STOP_TIMEOUT).await?;
            if !manual_close_verified {
                return Err("simulated manual browser close did not reconcile to stopped".into());
            }
            target_stopped = true;
        } else if let Some(timeout) = manual_close_timeout() {
            manual_close_verified = wait_for_manual_close(&manager, env_id, timeout).await?;
            if manual_close_verified {
                target_stopped = true;
            }
        }

        if !target_stopped {
            let close = manager.stop_environment(env_id).await?;
            ensure_operation_active_or_succeeded(&close)?;
            let _ = wait_for_state(&manager, env_id, "stopped", STOP_TIMEOUT).await?;
            target_stopped = true;
        }

        Ok(json!({
            "status": "passed",
            "environmentCount": environment_count,
            "readySource": ready_source,
            "cdpReady": ready.cdp != "-",
            "runtimeEvaluateVerified": evaluate_verified,
            "manualCloseVerified": manual_close_verified,
            "embeddedMcpAvailable": capabilities.embedded_mcp,
            "embeddedMcpConfigured": non_empty_env("BROSDK_EMBEDDED_PORT").is_some(),
            "embeddedMcpReachable": embedded_mcp_reachable,
            "targetSelection": if requested_env_id.is_some() { "explicit" } else { "only-environment" },
        }))
    }
    .await;

    if target_started
        && !target_stopped
        && let Some(env_id) = target_env_id.as_deref()
    {
        let _ = manager.stop_environment(env_id).await;
        let _ = wait_for_state(&manager, env_id, "stopped", STOP_TIMEOUT).await;
    }
    if runtime_started {
        let _ = manager.stop_runtime().await;
    }

    match result {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "failed",
                    "environmentCount": environment_count,
                    "readySource": ready_source,
                    "runtimeEvaluateVerified": evaluate_verified,
                    "manualCloseVerified": manual_close_verified,
                    "embeddedMcpReachable": embedded_mcp_reachable,
                    "error": error.to_string(),
                }))?
            );
            Err(error)
        }
    }
}

async fn wait_for_state(
    manager: &Manager,
    env_id: &str,
    expected: &str,
    timeout: Duration,
) -> Result<(EnvironmentRecord, &'static str), Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    let mut next_reconcile = Instant::now() + Duration::from_secs(5);
    loop {
        let environment = environment(manager, env_id).await?;
        if environment.status == expected {
            let source = if environment.last_event.contains("reconciliation") {
                "sdk_browser_info"
            } else {
                "sdk_callback"
            };
            return Ok((environment, source));
        }
        if environment.status == "failed" {
            return Err(format!(
                "environment entered failed state: {}",
                environment.last_event
            )
            .into());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for environment state {expected}; last state was {}",
                environment.status
            )
            .into());
        }
        if Instant::now() >= next_reconcile {
            let reconcile = manager.reconcile_runtimes().await?;
            ensure_operation_succeeded(&reconcile)?;
            next_reconcile = Instant::now() + Duration::from_secs(5);
        }
        sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_manual_close(
    manager: &Manager,
    env_id: &str,
    timeout: Duration,
) -> Result<bool, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let reconcile = manager.reconcile_runtimes().await?;
        ensure_operation_succeeded(&reconcile)?;
        if environment(manager, env_id).await?.status == "stopped" {
            return Ok(true);
        }
        sleep(Duration::from_secs(1)).await;
    }
    Ok(false)
}

async fn environment(manager: &Manager, env_id: &str) -> Result<EnvironmentRecord, Box<dyn Error>> {
    manager
        .snapshot()
        .await?
        .environments
        .into_iter()
        .find(|environment| environment.env_id == env_id)
        .ok_or_else(|| "environment disappeared from the local account mirror".into())
}

fn ensure_operation_succeeded(operation: &domain::OperationRecord) -> Result<(), Box<dyn Error>> {
    if operation.status == "succeeded" {
        return Ok(());
    }
    Err(format!(
        "operation {} failed: {} ({})",
        operation.kind,
        operation.message,
        operation.error_code.as_deref().unwrap_or("no error code")
    )
    .into())
}

fn ensure_operation_active_or_succeeded(
    operation: &domain::OperationRecord,
) -> Result<(), Box<dyn Error>> {
    if matches!(operation.status.as_str(), "running" | "succeeded") {
        return Ok(());
    }
    Err(format!(
        "operation {} was not accepted: {} ({})",
        operation.kind,
        operation.message,
        operation.error_code.as_deref().unwrap_or("no error code")
    )
    .into())
}

fn contains_number(value: &Value, expected: f64) -> bool {
    match value {
        Value::Number(number) => number
            .as_f64()
            .is_some_and(|actual| (actual - expected).abs() < f64::EPSILON),
        Value::Array(values) => values.iter().any(|value| contains_number(value, expected)),
        Value::Object(values) => values
            .values()
            .any(|value| contains_number(value, expected)),
        _ => false,
    }
}

fn find_page_target_id(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("page") {
                return map
                    .get("targetId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            map.values().find_map(find_page_target_id)
        }
        Value::Array(values) => values.iter().find_map(find_page_target_id),
        _ => None,
    }
}

fn find_string_key(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| map.values().find_map(|value| find_string_key(value, key))),
        Value::Array(values) => values.iter().find_map(|value| find_string_key(value, key)),
        _ => None,
    }
}

fn manual_close_timeout() -> Option<Duration> {
    non_empty_env("BROSDK_E2E_MANUAL_CLOSE_TIMEOUT_SECS")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
}

async fn wait_for_embedded_port(port: u16) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(
                format!("configured embedded MCP port {port} did not start listening").into(),
            );
        }
        sleep(Duration::from_millis(200)).await;
    }
}

fn embedded_port() -> Option<u16> {
    non_empty_env("BROSDK_EMBEDDED_PORT").and_then(|value| value.parse().ok())
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_flag(name: &str) -> bool {
    non_empty_env(name).is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_runtime_evaluate_result_recursively() {
        assert!(contains_number(
            &json!({ "result": { "result": { "value": 42 } } }),
            42.0
        ));
        assert!(!contains_number(
            &json!({ "result": { "result": { "value": 41 } } }),
            42.0
        ));
    }

    #[test]
    fn finds_page_target_and_session_recursively() {
        let value = json!({
            "result": {
                "targetInfos": [{ "targetId": "page-1", "type": "page" }],
                "sessionId": "session-1"
            }
        });
        assert_eq!(find_page_target_id(&value).as_deref(), Some("page-1"));
        assert_eq!(
            find_string_key(&value, "sessionId").as_deref(),
            Some("session-1")
        );
    }
}
