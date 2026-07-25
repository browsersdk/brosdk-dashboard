use std::error::Error;

use domain::{McpToolCallRequest, McpToolDiscoveryRequest, McpToolScope};
use manager::Manager;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let manager = Manager::try_new()?;
    let started = manager.start_runtime().await?;
    let before = manager.snapshot().await?;
    let operation = manager.sync_environments().await?;
    let synced = manager.snapshot().await?;
    let mcp = if synced.mcp.active {
        let discovery = manager
            .discover_embedded_mcp_tools(McpToolDiscoveryRequest {
                scope: McpToolScope::Global,
                env_id: None,
            })
            .await?;
        manager
            .call_embedded_mcp(McpToolCallRequest {
                scope: McpToolScope::Global,
                env_id: None,
                tool: "sdk.health".into(),
                arguments: json!({}),
            })
            .await?;
        manager
            .call_embedded_mcp(McpToolCallRequest {
                scope: McpToolScope::Global,
                env_id: None,
                tool: "env.list".into(),
                arguments: json!({ "page": 1, "pageSize": 10 }),
            })
            .await?;
        let endpoint_resolved = match synced.environments.first() {
            Some(environment) => {
                manager
                    .call_embedded_mcp(McpToolCallRequest {
                        scope: McpToolScope::Global,
                        env_id: None,
                        tool: "mcp.endpoint".into(),
                        arguments: json!({ "envId": environment.env_id }),
                    })
                    .await?;
                true
            }
            None => false,
        };
        json!({
            "active": true,
            "protocolVersion": discovery.protocol_version,
            "advertisedToolCount": discovery.advertised_tools.len(),
            "allowedToolCount": discovery.allowed_tools.len(),
            "healthCalled": true,
            "environmentListCalled": true,
            "endpointResolved": endpoint_resolved,
        })
    } else {
        json!({
            "active": false,
            "healthCalled": false,
            "environmentListCalled": false,
            "endpointResolved": false,
        })
    };
    let after = manager.snapshot().await?;
    let events = manager.events_since(before.latest_event_sequence)?;
    let stopped = manager.stop_runtime().await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "runtimeStarted": started,
            "databasePath": after.database_path,
            "operation": {
                "id": operation.id,
                "kind": operation.kind,
                "status": operation.status,
                "errorCode": operation.error_code,
            },
            "environmentCount": after.environments.len(),
            "environmentCache": {
                "source": after.environment_cache.source,
                "state": after.environment_cache.state,
                "count": after.environment_cache.count,
                "lastSuccessPresent": after.environment_cache.last_success_at.is_some(),
                "lastErrorPresent": after.environment_cache.last_error.is_some(),
            },
            "eventCount": events.len(),
            "latestEventSequence": after.latest_event_sequence,
            "mcp": mcp,
            "runtimeStopped": stopped,
        }))?
    );
    Ok(())
}
