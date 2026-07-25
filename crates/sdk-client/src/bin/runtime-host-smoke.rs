use std::time::Duration;

use anyhow::{Context, Result};
use domain::{HostCommand, RuntimeHostState};
use sdk_client::RuntimeHost;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let graceful = RuntimeHost::start()
        .await
        .context("start graceful runtime host")?;
    let health = graceful.call(HostCommand::Health, None).await?;
    let capabilities = graceful.capabilities().await?;
    let stopped = graceful.stop().await?;
    anyhow::ensure!(stopped.state == RuntimeHostState::Stopped);

    let killed = RuntimeHost::start()
        .await
        .context("start kill-test runtime host")?;
    let degraded = killed.kill().await?;
    anyhow::ensure!(degraded.state == RuntimeHostState::Degraded);

    tokio::time::sleep(Duration::from_millis(100)).await;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "health": health,
            "embeddedMcp": capabilities.embedded_mcp,
            "gracefulStop": stopped,
            "forcedKill": degraded,
        }))?
    );
    Ok(())
}
