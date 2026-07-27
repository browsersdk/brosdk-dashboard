use std::{collections::HashSet, error::Error, path::Path, time::Duration};

use domain::{
    AiAgentExecuteRequest, AiAgentPlan, AiAgentPlanRequest, EnvironmentCreateInput,
    EnvironmentMetadataUpdateInput, EnvironmentRecord, KernelRecord, McpToolDiscoveryRequest,
    McpToolScope, OperationRecord,
};
#[cfg(test)]
use domain::{EnvironmentBatchAction, EnvironmentBatchResult};
use manager::Manager;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep};
use uuid::Uuid;

const ENVIRONMENT_COUNT: usize = 2;
const READY_TIMEOUT: Duration = Duration::from_secs(240);
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

    let manager = Manager::try_new()?;
    let mut runtime_started = false;
    let mut owned_env_ids = Vec::with_capacity(ENVIRONMENT_COUNT);
    let mut failed_stage = "runtime-start";
    let mut environment_count_before = 0;
    let mut environment_count_after = 0;
    let mut created_environment_count = 0;
    let mut cleanup_attempted = 0;
    let mut cleanup_succeeded = 0;
    let mut mcp_advertised_count = 0;
    let mut mcp_allowed_count = 0;
    let mut mcp_browser_state_present = false;
    let mut mcp_call_succeeded = false;

    let result = async {
        if !manager.api_key_status()?.present {
            return Err(
                "BroSDK API key is not available from environment or secure storage".into(),
            );
        }
        if !manager.ai_provider_status()?.api_key_present {
            return Err("AI API key is not available from environment or secure storage".into());
        }
        manager.start_runtime().await?;
        runtime_started = true;

        failed_stage = "initial-sync";
        ensure_operation_succeeded(&manager.refresh_kernels().await?)?;
        ensure_operation_succeeded(&manager.sync_environments().await?)?;
        ensure_operation_succeeded(&manager.reconcile_runtimes().await?)?;
        let before = manager.snapshot().await?;
        environment_count_before = before.environments.len();
        let kernel = newest_usable_kernel(&before.kernels)
            .ok_or("no installed current-platform kernel can create environments")?
            .clone();

        failed_stage = "create";
        for _ in 0..ENVIRONMENT_COUNT {
            let create = manager
                .create_environment(EnvironmentCreateInput {
                    proxy_profile_id: None,
                    kernel_id: kernel.id.clone(),
                })
                .await?;
            ensure_operation_succeeded(&create)?;
            ensure_minimal_create_operation(&create, &kernel.id)?;
            let env_id = create
                .env_id
                .clone()
                .ok_or("successful create operation did not retain envId")?;
            if owned_env_ids.contains(&env_id) {
                return Err("server returned a duplicate temporary environment id".into());
            }
            owned_env_ids.push(env_id);
            created_environment_count += 1;
        }
        ensure_unique_environment_ids(&owned_env_ids)?;
        ensure_owned_environments(&manager, &owned_env_ids, "stopped").await?;

        failed_stage = "metadata-update";
        for (index, env_id) in owned_env_ids.iter().enumerate() {
            let suffix = Uuid::new_v4().simple().to_string()[..8].to_string();
            let name = format!("SDK Multi E2E {} {suffix}", index + 1);
            let serial = format!("MULTI-{}-{suffix}", index + 1);
            let update = manager
                .update_environment_metadata(EnvironmentMetadataUpdateInput {
                    env_id: env_id.clone(),
                    env_name: name.clone(),
                    serial: serial.clone(),
                })
                .await?;
            ensure_operation_succeeded(&update)?;
            ensure_minimal_metadata_operation(&update, env_id, &name, &serial)?;
            ensure_metadata_mirrored(&manager, env_id, &name, &serial).await?;
        }

        failed_stage = "agent-start";
        let mut start_operation_ids = HashSet::new();
        for (index, env_id) in owned_env_ids.iter().enumerate() {
            let plan = manager
                .ai_plan_agent(AiAgentPlanRequest {
                    prompt: format!("Start environment {env_id}"),
                    context_env_id: None,
                    history: Vec::new(),
                })
                .await?;
            validate_agent_plan(&plan, "environment.start", env_id, "stopped")?;
            let execution = manager
                .ai_execute_agent(AiAgentExecuteRequest {
                    plan,
                    approved: true,
                    automatic: index % 2 == 1,
                })
                .await?;
            let operation = execution
                .operation
                .ok_or("Agent start did not return an operation")?;
            ensure_agent_operation(&operation, "environment.start", env_id)?;
            if !start_operation_ids.insert(operation.id) {
                return Err("Agent reused a start operation across environments".into());
            }
        }

        failed_stage = "ready";
        for env_id in &owned_env_ids {
            wait_for_state(&manager, env_id, "ready", READY_TIMEOUT).await?;
        }

        let mut discovered_tool_counts = Vec::new();
        for env_id in &owned_env_ids {
            failed_stage = "environment-mcp-discovery";
            let discovery = manager
                .discover_embedded_mcp_tools(McpToolDiscoveryRequest {
                    scope: McpToolScope::Environment,
                    env_id: Some(env_id.clone()),
                })
                .await?;
            mcp_advertised_count = discovery.advertised_tools.len();
            mcp_allowed_count = discovery.allowed_tools.len();
            mcp_browser_state_present = discovery
                .allowed_tools
                .iter()
                .any(|tool| tool == "env.browser_state");
            failed_stage = "environment-mcp-catalog";
            if mcp_advertised_count < 17
                || mcp_allowed_count != mcp_advertised_count
                || !mcp_browser_state_present
                || !discovery
                    .allowed_tools
                    .iter()
                    .any(|tool| tool == "env.tabs")
            {
                return Err(
                    "single-environment MCP did not expose its runtime tool catalog".into(),
                );
            }
            discovered_tool_counts.push(discovery.advertised_tools.len());
            failed_stage = "environment-mcp-call";
            let plan = manager
                .ai_plan_agent(AiAgentPlanRequest {
                    prompt: format!(
                        "Use mcp.call with env.tabs action list for environment {env_id}"
                    ),
                    context_env_id: Some(env_id.clone()),
                    history: Vec::new(),
                })
                .await?;
            validate_agent_plan(&plan, "mcp.call", env_id, "ready")?;
            if plan.arguments.get("tool").and_then(Value::as_str) != Some("env.tabs")
                || plan
                    .arguments
                    .get("arguments")
                    .and_then(|arguments| arguments.get("action"))
                    .and_then(Value::as_str)
                    != Some("list")
            {
                return Err("Agent MCP plan did not preserve the requested tool arguments".into());
            }
            let execution = manager
                .ai_execute_agent(AiAgentExecuteRequest {
                    plan,
                    approved: true,
                    automatic: true,
                })
                .await?;
            let operation = execution
                .operation
                .ok_or("Agent MCP call did not return an operation")?;
            ensure_agent_operation(&operation, "mcp.environment-tool-call", env_id)?;
            mcp_call_succeeded = true;
        }

        failed_stage = "fingerprint-details";
        for env_id in &owned_env_ids {
            ensure_operation_succeeded(&manager.refresh_environment_detail(env_id).await?)?;
        }
        ensure_fingerprint_details(&manager, &owned_env_ids).await?;

        failed_stage = "agent-stop";
        let mut stop_operation_ids = HashSet::new();
        for (index, env_id) in owned_env_ids.iter().enumerate() {
            let plan = manager
                .ai_plan_agent(AiAgentPlanRequest {
                    prompt: format!("Stop environment {env_id}"),
                    context_env_id: None,
                    history: Vec::new(),
                })
                .await?;
            validate_agent_plan(&plan, "environment.stop", env_id, "ready")?;
            let execution = manager
                .ai_execute_agent(AiAgentExecuteRequest {
                    plan,
                    approved: true,
                    automatic: index % 2 == 0,
                })
                .await?;
            let operation = execution
                .operation
                .ok_or("Agent stop did not return an operation")?;
            ensure_agent_operation(&operation, "environment.stop", env_id)?;
            if !stop_operation_ids.insert(operation.id) {
                return Err("Agent reused a stop operation across environments".into());
            }
        }

        failed_stage = "stopped";
        for env_id in &owned_env_ids {
            wait_for_state(&manager, env_id, "stopped", STOP_TIMEOUT).await?;
        }

        failed_stage = "local-cleanup";
        for env_id in &owned_env_ids {
            let cleanup = manager.cleanup_environment_local_data(env_id).await?;
            ensure_operation_succeeded(&cleanup.operation)?;
            ensure_cleanup_summary(&cleanup.response)?;
        }

        failed_stage = "destroy";
        let ids_to_destroy = owned_env_ids.clone();
        for env_id in ids_to_destroy {
            cleanup_attempted += 1;
            let destroy = manager.destroy_environment(&env_id).await?;
            ensure_operation_succeeded(&destroy)?;
            cleanup_succeeded += 1;
            owned_env_ids.retain(|candidate| candidate != &env_id);
        }

        failed_stage = "final-sync";
        ensure_operation_succeeded(&manager.sync_environments().await?)?;
        let after = manager.snapshot().await?;
        environment_count_after = after.environments.len();
        if environment_count_after != environment_count_before {
            return Err("final environment count did not match the baseline".into());
        }

        Ok::<Value, Box<dyn Error>>(json!({
            "status": "passed",
            "environmentCountBefore": environment_count_before,
            "environmentCountAfter": environment_count_after,
            "temporaryEnvironmentCount": ENVIRONMENT_COUNT,
            "uniqueEnvironmentIds": true,
            "metadataUpdated": true,
            "agentManualModeCovered": true,
            "agentAutomaticModeCovered": true,
            "agentExplicitEnvIdOverridesContext": true,
            "independentStartOperations": true,
            "bothReady": true,
            "environmentMcpGlobalRouting": true,
            "agentMcpCallCovered": true,
            "minimumDiscoveredEnvironmentTools": discovered_tool_counts.into_iter().min(),
            "fingerprintDetailsReady": true,
            "independentStopOperations": true,
            "bothStopped": true,
            "localDataCleanupSucceeded": true,
            "destroyReconciled": true,
            "cleanupAttempted": cleanup_attempted,
            "cleanupSucceeded": cleanup_succeeded,
        }))
    }
    .await;

    if result.is_err() && !owned_env_ids.is_empty() {
        let cleanup = compensate(&manager, &mut owned_env_ids).await;
        cleanup_attempted += cleanup.attempted;
        cleanup_succeeded += cleanup.succeeded;
    }
    if runtime_started {
        let _ = manager.stop_runtime().await;
    }

    match result {
        Ok(report) => {
            print_report(report)?;
            Ok(())
        }
        Err(_) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "failed",
                    "failedStage": failed_stage,
                    "temporaryEnvironmentCount": created_environment_count,
                    "mcpAdvertisedToolCount": mcp_advertised_count,
                    "mcpAllowedToolCount": mcp_allowed_count,
                    "mcpBrowserStatePresent": mcp_browser_state_present,
                    "mcpCallSucceeded": mcp_call_succeeded,
                    "cleanupAttempted": cleanup_attempted,
                    "cleanupSucceeded": cleanup_succeeded,
                }))?
            );
            Err(format!("multi-environment E2E failed during {failed_stage}").into())
        }
    }
}

#[derive(Default)]
struct CleanupResult {
    attempted: usize,
    succeeded: usize,
}

async fn compensate(manager: &Manager, owned_env_ids: &mut Vec<String>) -> CleanupResult {
    let mut result = CleanupResult::default();
    let _ = manager.reconcile_runtimes().await;

    for env_id in owned_env_ids.clone() {
        result.attempted += 1;
        let status = environment(manager, &env_id)
            .await
            .ok()
            .map(|environment| environment.status);
        if status.as_deref() == Some("failed")
            && let Ok(operation) = manager.start_environment(&env_id).await
            && matches!(operation.status.as_str(), "running" | "succeeded")
        {
            let _ = wait_for_state(manager, &env_id, "ready", READY_TIMEOUT).await;
        }
        let status = environment(manager, &env_id)
            .await
            .ok()
            .map(|environment| environment.status);
        if matches!(status.as_deref(), Some("ready" | "starting")) {
            let _ = manager.stop_environment(&env_id).await;
            let _ = wait_for_state(manager, &env_id, "stopped", STOP_TIMEOUT).await;
        }
        let _ = manager.reconcile_runtimes().await;

        let stopped = environment(manager, &env_id)
            .await
            .ok()
            .is_some_and(|environment| environment.status == "stopped");
        if !stopped {
            continue;
        }
        if let Ok(cleanup) = manager.cleanup_environment_local_data(&env_id).await {
            let _ = ensure_operation_succeeded(&cleanup.operation);
        }
        let destroyed = manager
            .destroy_environment(&env_id)
            .await
            .ok()
            .is_some_and(|operation| operation.status == "succeeded");
        if destroyed {
            result.succeeded += 1;
            owned_env_ids.retain(|candidate| candidate != &env_id);
        }
    }
    let _ = manager.sync_environments().await;
    result
}

async fn ensure_owned_environments(
    manager: &Manager,
    env_ids: &[String],
    expected_status: &str,
) -> Result<(), Box<dyn Error>> {
    if env_ids.len() != ENVIRONMENT_COUNT {
        return Err("temporary environment count is incomplete".into());
    }
    for env_id in env_ids {
        if environment(manager, env_id).await?.status != expected_status {
            return Err("temporary environment has an unexpected initial state".into());
        }
    }
    Ok(())
}

fn ensure_unique_environment_ids(env_ids: &[String]) -> Result<(), Box<dyn Error>> {
    if env_ids.iter().any(|env_id| env_id.trim().is_empty())
        || env_ids.iter().collect::<HashSet<_>>().len() != env_ids.len()
    {
        return Err("temporary environment ids are empty or duplicated".into());
    }
    Ok(())
}

async fn ensure_metadata_mirrored(
    manager: &Manager,
    env_id: &str,
    expected_name: &str,
    expected_serial: &str,
) -> Result<(), Box<dyn Error>> {
    let snapshot = manager.snapshot().await?;
    if snapshot
        .environments
        .iter()
        .find(|environment| environment.env_id == env_id)
        .map(|environment| environment.name.as_str())
        != Some(expected_name)
    {
        return Err("updated environment name is missing from the server mirror".into());
    }
    if snapshot
        .environment_bindings
        .iter()
        .find(|binding| binding.env_id == env_id)
        .and_then(|binding| binding.remote_metadata.get("serial"))
        .and_then(Value::as_str)
        != Some(expected_serial)
    {
        return Err("updated serial is missing from the environment detail mirror".into());
    }
    Ok(())
}

async fn ensure_fingerprint_details(
    manager: &Manager,
    env_ids: &[String],
) -> Result<(), Box<dyn Error>> {
    let snapshot = manager.snapshot().await?;
    for env_id in env_ids {
        let binding = snapshot
            .environment_bindings
            .iter()
            .find(|binding| binding.env_id == *env_id)
            .ok_or("refreshed fingerprint binding is missing")?;
        if binding.refreshed_at.is_none()
            || binding
                .remote_fingerprint
                .as_object()
                .is_none_or(|fingerprint| fingerprint.is_empty())
        {
            return Err("refreshed fingerprint detail is empty".into());
        }
    }
    Ok(())
}

fn validate_agent_plan(
    plan: &AiAgentPlan,
    action: &str,
    env_id: &str,
    expected_state: &str,
) -> Result<(), Box<dyn Error>> {
    if plan.action != action
        || plan.env_id.as_deref() != Some(env_id)
        || plan.expected_state.as_deref() != Some(expected_state)
        || Uuid::parse_str(&plan.idempotency_key).is_err()
    {
        return Err("Agent plan did not retain the explicit envId and current state".into());
    }
    Ok(())
}

fn ensure_agent_operation(
    operation: &OperationRecord,
    kind: &str,
    env_id: &str,
) -> Result<(), Box<dyn Error>> {
    if operation.kind != kind
        || operation.env_id.as_deref() != Some(env_id)
        || !matches!(operation.status.as_str(), "running" | "succeeded")
    {
        return Err("Agent execution returned an operation for the wrong environment".into());
    }
    Ok(())
}

#[cfg(test)]
fn validate_batch_result(
    result: &EnvironmentBatchResult,
    expected_action: EnvironmentBatchAction,
    env_ids: &[String],
) -> Result<(), Box<dyn Error>> {
    if result.action != expected_action
        || result.requested != env_ids.len()
        || result.accepted != env_ids.len()
        || result.failed != 0
        || result.operations.len() != env_ids.len()
    {
        return Err("batch result did not accept every requested environment".into());
    }
    let expected_ids = env_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let operation_ids = result
        .operations
        .iter()
        .map(|operation| operation.id.as_str())
        .collect::<HashSet<_>>();
    let operation_env_ids = result
        .operations
        .iter()
        .filter_map(|operation| operation.env_id.as_deref())
        .collect::<HashSet<_>>();
    if operation_ids.len() != env_ids.len() || operation_env_ids != expected_ids {
        return Err("batch result did not contain independent per-environment operations".into());
    }
    let expected_kind = match expected_action {
        EnvironmentBatchAction::Start => "environment.start",
        EnvironmentBatchAction::Stop => "environment.stop",
    };
    if result.operations.iter().any(|operation| {
        operation.kind != expected_kind
            || !matches!(operation.status.as_str(), "running" | "succeeded")
    }) {
        return Err("batch operation was not accepted by the SDK".into());
    }
    Ok(())
}

async fn wait_for_state(
    manager: &Manager,
    env_id: &str,
    expected: &str,
    timeout: Duration,
) -> Result<EnvironmentRecord, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    let mut next_reconcile = Instant::now() + Duration::from_secs(5);
    loop {
        let current = environment(manager, env_id).await?;
        if current.status == expected {
            return Ok(current);
        }
        if current.status == "failed" {
            return Err("environment entered failed state".into());
        }
        if Instant::now() >= deadline {
            return Err("environment state transition timed out".into());
        }
        if Instant::now() >= next_reconcile {
            ensure_operation_succeeded(&manager.reconcile_runtimes().await?)?;
            next_reconcile = Instant::now() + Duration::from_secs(5);
        }
        sleep(Duration::from_millis(500)).await;
    }
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

fn newest_usable_kernel(kernels: &[KernelRecord]) -> Option<&KernelRecord> {
    kernels
        .iter()
        .filter(|kernel| {
            kernel.major.is_some()
                && kernel
                    .install_path
                    .as_deref()
                    .is_some_and(|path| !path.trim().is_empty() && Path::new(path).exists())
                && matches!(kernel.status.as_str(), "installed" | "update-available")
                && normalize_platform(&kernel.platform) == normalize_platform(std::env::consts::OS)
                && normalize_arch(&kernel.arch) == normalize_arch(std::env::consts::ARCH)
                && matches!(
                    kernel.kernel_type.trim().to_ascii_lowercase().as_str(),
                    "chrome" | "firefox" | "chromium" | "broium"
                )
        })
        .max_by_key(|kernel| kernel.major)
}

fn ensure_minimal_create_operation(
    operation: &OperationRecord,
    kernel_id: &str,
) -> Result<(), Box<dyn Error>> {
    if operation.request.as_ref()
        != Some(&json!({
            "proxyProfileId": null,
            "kernelId": kernel_id,
        }))
    {
        return Err("create operation persisted fields outside the minimal input contract".into());
    }
    Ok(())
}

fn ensure_minimal_metadata_operation(
    operation: &OperationRecord,
    env_id: &str,
    env_name: &str,
    serial: &str,
) -> Result<(), Box<dyn Error>> {
    if operation.request.as_ref()
        != Some(&json!({
            "envId": env_id,
            "envName": env_name,
            "serial": serial,
        }))
    {
        return Err("metadata operation persisted fields outside the update contract".into());
    }
    Ok(())
}

fn ensure_cleanup_summary(value: &Value) -> Result<(), Box<dyn Error>> {
    let object = value
        .as_object()
        .ok_or("environment cleanup summary is not an object")?;
    let allowed = ["deleted", "notFound", "failed", "deferred"];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("environment cleanup response exceeded the summary contract".into());
    }
    let handled = ["deleted", "notFound"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_i64))
        .sum::<i64>();
    if handled != 1 || object.get("failed").and_then(Value::as_i64) != Some(0) {
        return Err("environment cleanup did not handle exactly one environment".into());
    }
    Ok(())
}

fn ensure_operation_succeeded(operation: &OperationRecord) -> Result<(), Box<dyn Error>> {
    if operation.status == "succeeded" {
        return Ok(());
    }
    Err(format!("{} operation did not succeed", operation.kind).into())
}

fn normalize_platform(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "win" | "win32" | "windows" => "windows",
        "mac" | "macos" | "darwin" => "macos",
        "linux" => "linux",
        _ => "unknown",
    }
}

fn normalize_arch(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x64" | "x86_64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        "x86" | "i386" | "i686" => "x86",
        _ => "unknown",
    }
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

fn print_report(report: Value) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(id: &str, env_id: &str, kind: &str) -> OperationRecord {
        serde_json::from_value(json!({
            "id": id,
            "kind": kind,
            "envId": env_id,
            "label": "batch operation",
            "status": "running",
            "message": "accepted",
            "requestId": null,
            "generation": 1,
            "errorCode": null,
            "request": null,
            "createdAt": "2026-07-26T00:00:00Z",
            "updatedAt": "2026-07-26T00:00:00Z"
        }))
        .expect("operation")
    }

    fn batch(operations: Vec<OperationRecord>) -> EnvironmentBatchResult {
        EnvironmentBatchResult {
            action: EnvironmentBatchAction::Start,
            requested: 2,
            accepted: 2,
            failed: 0,
            operations,
        }
    }

    #[test]
    fn accepts_independent_operations_for_each_environment() {
        validate_batch_result(
            &batch(vec![
                operation("operation-1", "env-1", "environment.start"),
                operation("operation-2", "env-2", "environment.start"),
            ]),
            EnvironmentBatchAction::Start,
            &["env-1".into(), "env-2".into()],
        )
        .expect("independent operations");
    }

    #[test]
    fn rejects_a_reused_operation_id() {
        assert!(
            validate_batch_result(
                &batch(vec![
                    operation("operation-1", "env-1", "environment.start"),
                    operation("operation-1", "env-2", "environment.start"),
                ]),
                EnvironmentBatchAction::Start,
                &["env-1".into(), "env-2".into()],
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_an_incomplete_environment_set() {
        let mut result = batch(vec![
            operation("operation-1", "env-1", "environment.start"),
            operation("operation-2", "env-3", "environment.start"),
        ]);
        result.accepted = 1;
        result.failed = 1;
        assert!(
            validate_batch_result(
                &result,
                EnvironmentBatchAction::Start,
                &["env-1".into(), "env-2".into()],
            )
            .is_err()
        );
    }

    #[test]
    fn requires_non_empty_unique_environment_ids() {
        ensure_unique_environment_ids(&["env-1".into(), "env-2".into()]).expect("unique ids");
        assert!(ensure_unique_environment_ids(&["env-1".into(), "env-1".into()]).is_err());
        assert!(ensure_unique_environment_ids(&["env-1".into(), " ".into()]).is_err());
    }
}
