use std::error::Error;

use manager::Manager;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let manager = Manager::try_new()?;
    let started = manager.start_runtime().await?;
    let before = manager.snapshot().await?;
    let operation = manager.sync_environments().await?;
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
            "eventCount": events.len(),
            "latestEventSequence": after.latest_event_sequence,
            "runtimeStopped": stopped,
        }))?
    );
    Ok(())
}
