use std::{error::Error, path::Path};

use domain::{EnvironmentCreateInput, KernelRecord, OperationRecord};
use manager::Manager;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if non_empty_env("BROSDK_API_KEY").is_none() {
        print_report(json!({
            "status": "skipped",
            "reason": "BROSDK_API_KEY is not set",
        }))?;
        return Ok(());
    }
    if !env_flag("BROSDK_E2E_ALLOW_MUTATION") {
        print_report(json!({
            "status": "skipped",
            "reason": "BROSDK_E2E_ALLOW_MUTATION=1 is required",
        }))?;
        return Ok(());
    }

    let manager = Manager::try_new()?;
    let mut runtime_started = false;
    let mut created_env_id = None;
    let mut cleanup_attempted = false;
    let mut cleanup_succeeded = false;
    let mut selected_kernel_id = None;

    let result = async {
        manager.start_runtime().await?;
        runtime_started = true;

        let refresh = manager.refresh_kernels().await?;
        ensure_operation_succeeded(&refresh)?;
        let before_sync = manager.sync_environments().await?;
        ensure_operation_succeeded(&before_sync)?;
        let before = manager.snapshot().await?;
        let kernel = newest_usable_kernel(&before.kernels)
            .ok_or("no installed current-platform kernel can create an environment")?;
        selected_kernel_id = Some(kernel.id.clone());

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
        created_env_id = Some(env_id.clone());

        let after_create = manager.snapshot().await?;
        if !after_create
            .environments
            .iter()
            .any(|environment| environment.env_id == env_id)
        {
            return Err("created environment is missing from the local mirror".into());
        }

        let local_cleanup = manager.cleanup_environment_local_data(&env_id).await?;
        ensure_operation_succeeded(&local_cleanup.operation)?;
        ensure_cleanup_summary(&local_cleanup.response)?;

        cleanup_attempted = true;
        let destroy = manager.destroy_environment(&env_id).await?;
        ensure_operation_succeeded(&destroy)?;
        cleanup_succeeded = true;

        let after_destroy_sync = manager.sync_environments().await?;
        ensure_operation_succeeded(&after_destroy_sync)?;
        let after_destroy = manager.snapshot().await?;
        if after_destroy
            .environments
            .iter()
            .any(|environment| environment.env_id == env_id)
        {
            return Err("deleted environment reappeared after env_page reconciliation".into());
        }
        created_env_id = None;

        Ok::<Value, Box<dyn Error>>(json!({
            "status": "passed",
            "kernelId": kernel.id,
            "proxyMode": "local-network",
            "environmentCountBefore": before.environments.len(),
            "environmentCountAfter": after_destroy.environments.len(),
            "createMirrored": true,
            "localDataCleanupSucceeded": true,
            "destroyReconciled": true,
            "cleanupAttempted": cleanup_attempted,
            "cleanupSucceeded": cleanup_succeeded,
        }))
    }
    .await;

    if let Some(env_id) = created_env_id.as_deref() {
        cleanup_attempted = true;
        cleanup_succeeded = manager
            .destroy_environment(env_id)
            .await
            .ok()
            .and_then(|operation| ensure_operation_succeeded(&operation).ok())
            .is_some();
    }
    if runtime_started {
        let _ = manager.stop_runtime().await;
    }

    match result {
        Ok(report) => {
            print_report(report)?;
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "failed",
                    "kernelId": selected_kernel_id,
                    "cleanupAttempted": cleanup_attempted,
                    "cleanupSucceeded": cleanup_succeeded,
                    "error": error.to_string(),
                }))?
            );
            Err(error)
        }
    }
}

fn ensure_cleanup_summary(value: &Value) -> Result<(), Box<dyn Error>> {
    let object = value
        .as_object()
        .ok_or("environment cleanup summary is not an object")?;
    let allowed = ["deleted", "notFound", "failed", "deferred"];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(
            "environment cleanup response exposed fields outside the summary contract".into(),
        );
    }
    let handled = ["deleted", "notFound"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_i64))
        .sum::<i64>();
    if handled != 1 || object.get("failed").and_then(Value::as_i64) != Some(0) {
        return Err("environment cleanup did not handle exactly one temporary environment".into());
    }
    Ok(())
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
    let expected = json!({
        "proxyProfileId": null,
        "kernelId": kernel_id,
    });
    if operation.request.as_ref() != Some(&expected) {
        return Err("create operation persisted fields outside the minimal input contract".into());
    }
    Ok(())
}

fn ensure_operation_succeeded(operation: &OperationRecord) -> Result<(), Box<dyn Error>> {
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

    fn kernel(id: &str, major: u32, overrides: impl FnOnce(&mut KernelRecord)) -> KernelRecord {
        let mut record = KernelRecord {
            id: id.into(),
            kernel_type: "chrome".into(),
            name: format!("Chrome {major}"),
            major: Some(major),
            version: Some(major.to_string()),
            latest_version: None,
            platform: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            status: "installed".into(),
            install_path: Some(std::env::current_dir().expect("cwd").display().to_string()),
            download_available: false,
            updated_at: chrono::Utc::now(),
        };
        overrides(&mut record);
        record
    }

    #[test]
    fn selects_newest_supported_local_kernel() {
        let kernels = [
            kernel("chrome-134", 134, |_| {}),
            kernel("chrome-141", 141, |_| {}),
            kernel("remote-150", 150, |record| record.install_path = None),
            kernel("unsupported-160", 160, |record| {
                record.kernel_type = "yun".into()
            }),
        ];
        let selected = newest_usable_kernel(&kernels).expect("usable kernel");
        assert_eq!(selected.id, "chrome-141");
    }

    #[test]
    fn accepts_only_minimal_create_operation_request() {
        let mut operation: OperationRecord = serde_json::from_value(json!({
            "id": "operation-1",
            "kind": "environment.create",
            "envId": "env-1",
            "label": "create",
            "status": "succeeded",
            "message": "created",
            "requestId": null,
            "generation": 0,
            "errorCode": null,
            "request": { "proxyProfileId": null, "kernelId": "chrome-134" },
            "createdAt": "2026-07-26T00:00:00Z",
            "updatedAt": "2026-07-26T00:00:00Z"
        }))
        .expect("operation");
        ensure_minimal_create_operation(&operation, "chrome-134").expect("minimal request");

        operation.request = Some(json!({
            "proxyProfileId": null,
            "kernelId": "chrome-134",
            "envName": "not allowed"
        }));
        assert!(ensure_minimal_create_operation(&operation, "chrome-134").is_err());
    }
}
