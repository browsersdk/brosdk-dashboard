use std::path::PathBuf;

use domain::{SdkCapabilities, SmokeReport};
use thiserror::Error;
use tokio::process::Command;

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
}
