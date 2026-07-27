use std::{error::Error, time::Duration};

use domain::{AiAgentRunRequest, AiChatRequest, EnvironmentRecord, OperationRecord};
use manager::Manager;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep};

const READY_TIMEOUT: Duration = Duration::from_secs(180);
const STOP_TIMEOUT: Duration = Duration::from_secs(90);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if !env_flag("BROSDK_E2E_ALLOW_MUTATION") {
        print_report(json!({
            "status": "skipped",
            "reason": "BROSDK_E2E_ALLOW_MUTATION=1 is required",
        }))?;
        return Ok(());
    }
    let env_id = non_empty_env("BROSDK_E2E_ENV_ID")
        .ok_or("BROSDK_E2E_ENV_ID is required for the AI assistant E2E")?;
    let manager = Manager::try_new()?;
    let mut runtime_started = false;
    let mut initial_status = None;
    let mut failed_stage = "runtime-start";

    let result = async {
        if !manager.api_key_status()?.present {
            return Err("BroSDK API key is unavailable".into());
        }
        if !manager.ai_provider_status()?.api_key_present {
            return Err("AI API key is unavailable".into());
        }
        manager.start_runtime().await?;
        runtime_started = true;
        failed_stage = "environment-sync";
        ensure_operation_succeeded(&manager.sync_environments().await?)?;
        ensure_operation_succeeded(&manager.reconcile_runtimes().await?)?;
        let initialized = manager.snapshot().await?;
        if !initialized.mcp.active || initialized.settings.embedded_mcp_port.is_some() {
            return Err("DLL MCP did not activate on an automatic loopback port".into());
        }
        let initial = environment(&manager, &env_id).await?;
        if !matches!(initial.status.as_str(), "stopped" | "ready") {
            return Err(format!(
                "AI assistant E2E requires a stable target, found {}",
                initial.status
            )
            .into());
        }
        initial_status = Some(initial.status.clone());
        if initial.status == "ready" {
            failed_stage = "stopped-baseline";
            ensure_operation_active_or_succeeded(&manager.stop_environment(&env_id).await?)?;
            wait_for_state(&manager, &env_id, "stopped", STOP_TIMEOUT).await?;
        }

        failed_stage = "agent-automatic-start";
        let start_run = manager
            .ai_run_agent(AiAgentRunRequest {
                prompt: format!("启动环境 {env_id}，等待运行完成后告诉我最终状态。"),
                context_env_id: Some(env_id.clone()),
                history: Vec::new(),
                approved: true,
            })
            .await?;
        if !start_run
            .steps
            .iter()
            .any(|step| step.plan.action == "environment.start")
            || environment(&manager, &env_id).await?.status != "ready"
        {
            return Err("automatic Agent did not start the stopped environment".into());
        }

        failed_stage = "chat-mutation-guard";
        let before_chat = environment(&manager, &env_id).await?;
        let mutation_reply = manager
            .ai_chat(AiChatRequest {
                prompt: format!("启动环境 {env_id}"),
                context_env_id: Some(env_id.clone()),
                history: Vec::new(),
            })
            .await?;
        let after_chat = environment(&manager, &env_id).await?;
        if mutation_reply.answer.trim().is_empty()
            || !mutation_reply.answer.contains("Agent")
            || before_chat.status != after_chat.status
            || before_chat.current_operation_id != after_chat.current_operation_id
        {
            return Err("Chat mutation guard did not return a visible read-only response".into());
        }

        failed_stage = "global-chat-provider-read";
        let global_read_reply = manager
            .ai_chat(AiChatRequest {
                prompt: "这是全局会话。只读回答 SDK 当前环境总数，不读取任何单环境页面数据。"
                    .into(),
                context_env_id: None,
                history: Vec::new(),
            })
            .await?;
        if global_read_reply.answer.trim().is_empty() || !global_read_reply.read_only {
            return Err("real provider-backed global Chat returned no read-only answer".into());
        }

        failed_stage = "environment-chat-provider-read";
        let environment_read_reply = manager
            .ai_chat(AiChatRequest {
                prompt: "只读回答当前关联环境的 envId 和运行状态，不执行任何写操作。".into(),
                context_env_id: Some(env_id.clone()),
                history: Vec::new(),
            })
            .await?;
        if environment_read_reply.answer.trim().is_empty() || !environment_read_reply.read_only {
            return Err(
                "real provider-backed environment Chat returned no read-only answer".into(),
            );
        }

        failed_stage = "agent-automatic-restart";
        let run = manager
            .ai_run_agent(AiAgentRunRequest {
                prompt: format!("重启环境 {env_id}，等待重新运行完成后告诉我最终状态。"),
                context_env_id: Some(env_id.clone()),
                history: Vec::new(),
                approved: true,
            })
            .await?;
        if run.answer.trim().is_empty() {
            return Err("automatic Agent returned an empty final answer".into());
        }
        let stop_index = run
            .steps
            .iter()
            .position(|step| step.plan.action == "environment.stop")
            .ok_or("automatic Agent did not execute environment.stop")?;
        let start_index = run
            .steps
            .iter()
            .position(|step| step.plan.action == "environment.start")
            .ok_or("automatic Agent did not execute environment.start")?;
        if start_index <= stop_index {
            return Err("automatic Agent start did not follow stop".into());
        }
        if run.steps.iter().any(|step| {
            step.execution
                .operation
                .as_ref()
                .is_some_and(|operation| operation.status == "failed")
        }) {
            return Err("automatic Agent produced a failed operation".into());
        }
        let final_environment = environment(&manager, &env_id).await?;
        if final_environment.status != "ready" {
            return Err("automatic Agent did not leave the environment ready".into());
        }

        Ok::<Value, Box<dyn Error>>(json!({
            "status": "passed",
            "automaticMcpActivated": true,
            "agentStartObserved": true,
            "chatMutationReplyVerified": true,
            "globalChatReplyVerified": true,
            "environmentChatReplyVerified": true,
            "automaticRestartStopObserved": true,
            "automaticRestartStartObserved": true,
            "automaticRestartReady": true,
            "toolRounds": run.steps.len(),
            "model": run.model,
        }))
    }
    .await;

    let restored = match initial_status.as_deref() {
        Some(status) => restore_state(&manager, &env_id, status).await.is_ok(),
        None => true,
    };
    if runtime_started {
        let _ = manager.stop_runtime().await;
    }

    match result {
        Ok(mut report) => {
            report["initialStateRestored"] = json!(restored);
            if !restored {
                return Err("AI assistant E2E passed but failed to restore initial state".into());
            }
            print_report(report)?;
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "failed",
                    "failedStage": failed_stage,
                    "initialStateRestored": restored,
                    "error": error.to_string(),
                }))?
            );
            Err(error)
        }
    }
}

async fn restore_state(
    manager: &Manager,
    env_id: &str,
    expected: &str,
) -> Result<(), Box<dyn Error>> {
    let current = environment(manager, env_id).await?;
    if current.status == expected {
        return Ok(());
    }
    if expected == "stopped" {
        if current.status == "stopping" {
            wait_for_state(manager, env_id, "stopped", STOP_TIMEOUT).await?;
        } else {
            ensure_operation_active_or_succeeded(&manager.stop_environment(env_id).await?)?;
            wait_for_state(manager, env_id, "stopped", STOP_TIMEOUT).await?;
        }
        return Ok(());
    }
    if current.status == "starting" {
        wait_for_state(manager, env_id, "ready", READY_TIMEOUT).await?;
    } else {
        if current.status == "stopping" {
            wait_for_state(manager, env_id, "stopped", STOP_TIMEOUT).await?;
        }
        ensure_operation_active_or_succeeded(&manager.start_environment(env_id).await?)?;
        wait_for_state(manager, env_id, "ready", READY_TIMEOUT).await?;
    }
    Ok(())
}

async fn environment(manager: &Manager, env_id: &str) -> Result<EnvironmentRecord, Box<dyn Error>> {
    manager
        .snapshot()
        .await?
        .environments
        .into_iter()
        .find(|environment| environment.env_id == env_id)
        .ok_or_else(|| "target environment is not in the SDK account mirror".into())
}

async fn wait_for_state(
    manager: &Manager,
    env_id: &str,
    expected: &str,
    timeout: Duration,
) -> Result<EnvironmentRecord, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    loop {
        let environment = environment(manager, env_id).await?;
        if environment.status == expected {
            return Ok(environment);
        }
        if environment.status == "failed" {
            return Err(format!("environment entered failed while waiting for {expected}").into());
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for environment state {expected}").into());
        }
        sleep(Duration::from_millis(300)).await;
    }
}

fn ensure_operation_succeeded(operation: &OperationRecord) -> Result<(), Box<dyn Error>> {
    if operation.status == "succeeded" {
        Ok(())
    } else {
        Err(format!("operation {} ended as {}", operation.kind, operation.status).into())
    }
}

fn ensure_operation_active_or_succeeded(operation: &OperationRecord) -> Result<(), Box<dyn Error>> {
    if matches!(
        operation.status.as_str(),
        "queued" | "running" | "succeeded"
    ) {
        Ok(())
    } else {
        Err(format!("operation {} ended as {}", operation.kind, operation.status).into())
    }
}

fn print_report(value: Value) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
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
