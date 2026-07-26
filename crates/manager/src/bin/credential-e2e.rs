use std::{error::Error, fs};

use manager::Manager;
use serde_json::json;
use zeroize::Zeroizing;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var("BROSDK_E2E_CREDENTIAL").ok().as_deref() != Some("1") {
        print_report(json!({
            "status": "skipped",
            "reason": "BROSDK_E2E_CREDENTIAL=1 is required",
        }))?;
        return Ok(());
    }

    let api_key = match std::env::var("BROSDK_E2E_API_KEY") {
        Ok(value) if !value.trim().is_empty() => Zeroizing::new(value),
        _ => {
            print_report(json!({
                "status": "skipped",
                "reason": "BROSDK_E2E_API_KEY is not set",
            }))?;
            return Ok(());
        }
    };
    // The runner is single-threaded and removes the handoff variable before any child is spawned.
    unsafe { std::env::remove_var("BROSDK_E2E_API_KEY") };

    let manager = Manager::try_new()?;
    let initialized = manager.configure_api_key(api_key.to_string()).await?;
    let first = manager.snapshot().await?;
    if !first.sdk.initialized || first.sdk.api_key.source != "secure-storage" {
        return Err("first-run initialization did not reach secure-storage ready state".into());
    }

    let environment = first
        .environments
        .first()
        .ok_or("credential E2E requires at least one server environment")?;
    let detail_operation = manager
        .refresh_environment_detail(&environment.env_id)
        .await?;
    if detail_operation.status != "succeeded"
        || detail_operation.env_id.as_deref() != Some(environment.env_id.as_str())
    {
        return Err("focused environment detail refresh did not succeed".into());
    }
    let detailed = manager.snapshot().await?;
    let detail = detailed
        .environment_bindings
        .iter()
        .find(|binding| binding.env_id == environment.env_id)
        .ok_or("focused environment detail was not cached")?;
    let detail_available = detail.remote_fingerprint.is_object()
        && detail.remote_kernel.is_object()
        && detail.refreshed_at.is_some();
    if !detail_available {
        return Err("environment detail cache is missing fingerprint or kernel data".into());
    }

    let data_dir = std::path::Path::new(&first.settings.data_dir);
    let protected = fs::read(platform::secrets_dir(data_dir).join("sdk-api-key.bin"))?;
    let plaintext_present = protected
        .windows(api_key.len())
        .any(|window| window == api_key.as_bytes());
    if plaintext_present {
        return Err("protected credential file contains the plaintext API Key".into());
    }

    manager.stop_runtime().await?;
    drop(manager);

    let restarted_manager = Manager::try_new()?;
    let restarted = restarted_manager.snapshot().await?;
    let restart_loaded = restarted.sdk.initialized
        && restarted.sdk.api_key.source == "secure-storage"
        && restarted.environments.len() == initialized.environment_count;
    if !restart_loaded {
        return Err("stored credential was not restored after Manager restart".into());
    }

    restarted_manager.clear_api_key().await?;
    let cleared = restarted_manager.snapshot().await?;
    let account_state_cleared = !cleared.sdk.api_key.present
        && !cleared.sdk.initialized
        && cleared.environments.is_empty()
        && cleared.environment_bindings.is_empty();
    if !account_state_cleared {
        return Err("credential removal left account-scoped state behind".into());
    }

    print_report(json!({
        "status": "passed",
        "environmentCount": initialized.environment_count,
        "encryptedAtRest": !plaintext_present,
        "restartLoaded": restart_loaded,
        "accountStateCleared": account_state_cleared,
        "focusedDetailLoaded": detail_available,
    }))?;
    Ok(())
}

fn print_report(value: serde_json::Value) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
