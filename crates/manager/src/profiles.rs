use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use chrono::Utc;
use domain::{EnvironmentBindingSummary, KernelRecord, ProxyParseResult};
use serde_json::{Map, Value, json};
use url::Url;
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("proxy URL is invalid: {0}")]
    InvalidProxy(String),
    #[error("profile file is invalid: {0}")]
    InvalidProfile(String),
    #[error("profile I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("profile JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct ParsedProxy {
    pub summary: ProxyParseResult,
    pub password: Option<String>,
}

pub fn parse_proxy_url(input: &str) -> Result<ParsedProxy, ProfileError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ProfileError::InvalidProxy("URL must not be empty".into()));
    }
    let url = Url::parse(input).map_err(|error| ProfileError::InvalidProxy(error.to_string()))?;
    let scheme = url.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https" | "socks5" | "socks5h") {
        return Err(ProfileError::InvalidProxy(format!(
            "unsupported scheme {scheme}"
        )));
    }
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| ProfileError::InvalidProxy("host is required".into()))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ProfileError::InvalidProxy("port is required".into()))?;
    let username = (!url.username().is_empty()).then(|| url.username().to_string());
    let password = url.password().map(str::to_string);
    let authority = match username.as_deref() {
        Some(username) => format!("{username}:***@{host}:{port}"),
        None => format!("{host}:{port}"),
    };
    Ok(ParsedProxy {
        summary: ProxyParseResult {
            scheme: scheme.clone(),
            host,
            port,
            username,
            password_present: password.is_some(),
            display_url: format!("{scheme}://{authority}"),
        },
        password,
    })
}

pub fn proxy_url(
    scheme: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
) -> String {
    let credentials = match (username, password) {
        (Some(username), Some(password)) => format!("{username}:{password}@"),
        (Some(username), None) => format!("{username}@"),
        _ => String::new(),
    };
    format!("{scheme}://{credentials}{host}:{port}")
}

pub fn safe_environment_detail(value: &Value) -> Value {
    let root = value.pointer("/data").unwrap_or(value);
    json!({
        "fingerprint": first_object(root, &["finger", "fingerprint", "fingerPrint"]),
        "proxy": first_value(root, &["proxy", "proxyUrl", "bridgeProxy"]),
        "kernel": first_object(root, &["browser", "kernel", "core"]),
    })
}

pub fn environment_bindings(
    env_ids: &[String],
    details: &[(String, Value, chrono::DateTime<Utc>)],
    fingerprints: &[(String, Vec<String>)],
    proxies: &[(String, Vec<String>)],
) -> Vec<EnvironmentBindingSummary> {
    let details = details
        .iter()
        .map(|(env_id, value, refreshed)| (env_id.as_str(), (value, *refreshed)))
        .collect::<HashMap<_, _>>();
    env_ids
        .iter()
        .map(|env_id| {
            let (detail, refreshed_at) = details
                .get(env_id.as_str())
                .map(|(value, refreshed)| ((*value).clone(), Some(*refreshed)))
                .unwrap_or_else(|| (json!({}), None));
            EnvironmentBindingSummary {
                env_id: env_id.clone(),
                fingerprint_profile_id: fingerprints
                    .iter()
                    .find(|(_, ids)| ids.contains(env_id))
                    .map(|(id, _)| id.clone()),
                proxy_profile_id: proxies
                    .iter()
                    .find(|(_, ids)| ids.contains(env_id))
                    .map(|(id, _)| id.clone()),
                remote_fingerprint: detail.get("fingerprint").cloned().unwrap_or(Value::Null),
                remote_proxy: detail.get("proxy").cloned().unwrap_or(Value::Null),
                remote_kernel: detail.get("kernel").cloned().unwrap_or(Value::Null),
                refreshed_at,
            }
        })
        .collect()
}

pub fn parse_profile_document(text: &str) -> Result<(String, Value), ProfileError> {
    let value: Value = serde_json::from_str(text)?;
    let object = value
        .as_object()
        .ok_or_else(|| ProfileError::InvalidProfile("root must be a JSON object".into()))?;
    let profile = object
        .get("profile")
        .cloned()
        .unwrap_or_else(|| value.clone());
    if !profile.is_object() {
        return Err(ProfileError::InvalidProfile(
            "profile must be a JSON object".into(),
        ));
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Imported fingerprint")
        .to_string();
    Ok((name, profile))
}

pub fn scan_kernels(work_dir: &Path, sdk_info: Option<&Value>) -> Vec<KernelRecord> {
    let mut records = HashMap::<String, KernelRecord>::new();
    let cores_dir = find_cores_dir(work_dir);
    if cores_dir.exists() {
        for entry in WalkDir::new(&cores_dir)
            .min_depth(2)
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() == ".core.json")
        {
            if let Ok(text) = std::fs::read_to_string(entry.path())
                && let Ok(value) = serde_json::from_str::<Value>(&text)
            {
                let record = kernel_record_from_value(
                    &value,
                    "installed",
                    entry.path().parent().map(|path| path.display().to_string()),
                );
                records.insert(record.id.clone(), record);
            }
        }
    }

    if let Some(sdk_info) = sdk_info {
        for item in find_kernel_versions(sdk_info) {
            let available = kernel_record_from_value(item, "available", None);
            records
                .entry(available.id.clone())
                .and_modify(|installed| {
                    installed.latest_version = available.version.clone();
                    installed.download_available = available.download_available;
                    if installed.version != installed.latest_version
                        && installed.latest_version.is_some()
                    {
                        installed.status = "update-available".into();
                    }
                })
                .or_insert(available);
        }
    }

    let mut result = records.into_values().collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .major
            .cmp(&left.major)
            .then_with(|| left.kernel_type.cmp(&right.kernel_type))
    });
    result
}

fn find_cores_dir(work_dir: &Path) -> std::path::PathBuf {
    let direct = work_dir.join("cores");
    if direct.exists() {
        return direct;
    }
    std::fs::read_dir(work_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("cores"))
        .find(|path| path.exists())
        .unwrap_or(direct)
}

fn kernel_record_from_value(
    value: &Value,
    status: &str,
    install_path: Option<String>,
) -> KernelRecord {
    let kernel_type =
        string(value, &["type", "kernelId", "kernel"]).unwrap_or_else(|| "unknown".into());
    let major = string(value, &["majorVersion", "major", "browserMajor"])
        .and_then(|value| value.parse().ok())
        .or_else(|| number(value, &["major"]));
    let version = string(value, &["versionCode", "version", "majorVersion"]);
    let platform = string(value, &["platform"]).unwrap_or_else(|| std::env::consts::OS.into());
    let arch = string(value, &["arch"]).unwrap_or_else(|| std::env::consts::ARCH.into());
    let id = format!(
        "{}-{}-{}-{}",
        kernel_type,
        major
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".into()),
        platform,
        arch
    );
    KernelRecord {
        id,
        kernel_type,
        name: string(value, &["kernelName", "name"]).unwrap_or_else(|| "Browser core".into()),
        major,
        latest_version: (status == "available").then(|| version.clone()).flatten(),
        version: (status == "installed").then(|| version.clone()).flatten(),
        platform,
        arch,
        status: status.into(),
        install_path,
        download_available: string(value, &["url"]).is_some_and(|url| !url.trim().is_empty()),
        updated_at: Utc::now(),
    }
}

fn find_kernel_versions(value: &Value) -> Vec<&Value> {
    match value {
        Value::Object(map) => {
            if let Some(values) = map.get("kernelVersions").and_then(Value::as_array) {
                return values.iter().collect();
            }
            map.values().flat_map(find_kernel_versions).collect()
        }
        Value::Array(values) => values.iter().flat_map(find_kernel_versions).collect(),
        _ => Vec::new(),
    }
}

fn first_object(value: &Value, keys: &[&str]) -> Value {
    first_value(value, keys)
}

fn first_value(value: &Value, keys: &[&str]) -> Value {
    find_value(value, &keys.iter().copied().collect::<HashSet<_>>())
        .cloned()
        .unwrap_or(Value::Null)
}

fn find_value<'a>(value: &'a Value, keys: &HashSet<&str>) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if keys.contains(key.as_str()) {
                    return Some(child);
                }
            }
            map.values().find_map(|child| find_value(child, keys))
        }
        Value::Array(values) => values.iter().find_map(|child| find_value(child, keys)),
        _ => None,
    }
}

fn string(value: &Value, keys: &[&str]) -> Option<String> {
    let keys = keys.iter().copied().collect::<HashSet<_>>();
    find_value(value, &keys).and_then(|value| match value {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn number(value: &Value, keys: &[&str]) -> Option<u32> {
    let keys = keys.iter().copied().collect::<HashSet<_>>();
    find_value(value, &keys)
        .and_then(Value::as_u64)
        .and_then(|value| value.try_into().ok())
}

pub fn object_without_secrets(value: &Value) -> Value {
    let mut output = Map::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if !key.to_ascii_lowercase().contains("password")
                && !key.to_ascii_lowercase().contains("secret")
                && !key.to_ascii_lowercase().contains("cookie")
                && !key.to_ascii_lowercase().contains("token")
            {
                output.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_authenticated_proxy_without_exposing_password() {
        let parsed = parse_proxy_url("socks5://alice:secret@127.0.0.1:1080").expect("proxy");
        assert_eq!(parsed.summary.scheme, "socks5");
        assert_eq!(parsed.summary.username.as_deref(), Some("alice"));
        assert!(parsed.summary.password_present);
        assert_eq!(parsed.password.as_deref(), Some("secret"));
        assert!(!parsed.summary.display_url.contains("secret"));
    }

    #[test]
    fn rejects_unsupported_proxy_scheme() {
        assert!(parse_proxy_url("ftp://localhost:21").is_err());
    }

    #[test]
    fn remote_kernel_without_url_is_not_downloadable() {
        let records = scan_kernels(
            Path::new("missing"),
            Some(&json!({
                "data": { "kernelVersions": [{ "type": "yun", "majorVersion": "141" }] }
            })),
        );
        assert_eq!(records.len(), 1);
        assert!(!records[0].download_available);
        assert_eq!(records[0].status, "available");
    }

    #[test]
    fn extracts_safe_environment_summary() {
        let summary = safe_environment_detail(&json!({
            "data": {
                "finger": { "ua": "test" },
                "proxy": "http://example.test:8080",
                "cookie": "must not escape",
                "browser": { "version": "141" }
            }
        }));
        assert_eq!(summary["fingerprint"]["ua"], "test");
        assert!(summary.to_string().find("must not escape").is_none());
    }
}
