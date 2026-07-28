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
    let fingerprint = root
        .get("finger")
        .or_else(|| root.get("fingerprint"))
        .or_else(|| root.get("fingerPrint"))
        .map(sanitize_remote_value)
        .unwrap_or(Value::Null);
    let browser = root.get("browser").and_then(Value::as_object);
    let fingerprint_object = fingerprint.as_object();
    let proxy = ["proxy", "bridgeProxy"]
        .iter()
        .find_map(|key| {
            root.get(*key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(|value| (*key, value))
        })
        .map(|(key, value)| safe_remote_proxy(key, value))
        .unwrap_or(Value::Null);

    json!({
        "fingerprint": fingerprint,
        "proxy": proxy,
        "kernel": {
            "kernel": direct_value(browser, fingerprint_object, &["kernel"]),
            "version": direct_value(browser, fingerprint_object, &["version", "kernelVersion"]),
            "mapId": browser.and_then(|value| value.get("mapId")).cloned(),
            "system": fingerprint_object.and_then(|value| value.get("system")).cloned(),
        },
        "metadata": {
            "envName": root.get("envName").cloned(),
            "serial": root.get("serial").cloned(),
            "enableDevtools": root.get("enableDevtools").cloned(),
            "enableStorage": root.get("enableStorage").cloned(),
        },
    })
}

fn direct_value(
    primary: Option<&Map<String, Value>>,
    fallback: Option<&Map<String, Value>>,
    keys: &[&str],
) -> Option<Value> {
    primary
        .and_then(|value| keys.iter().find_map(|key| value.get(*key)).cloned())
        .or_else(|| fallback.and_then(|value| keys.iter().find_map(|key| value.get(*key)).cloned()))
}

fn safe_remote_proxy(source: &str, value: &str) -> Value {
    match parse_proxy_url(value) {
        Ok(parsed) => json!({
            "source": source,
            "scheme": parsed.summary.scheme,
            "host": parsed.summary.host,
            "port": parsed.summary.port,
            "username": parsed.summary.username,
            "passwordPresent": parsed.summary.password_present,
            "displayUrl": parsed.summary.display_url,
        }),
        Err(_) => json!({ "source": source, "configured": true }),
    }
}

fn sanitize_remote_value(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .filter(|(key, _)| !is_sensitive_remote_key(key))
                .map(|(key, value)| (key.clone(), sanitize_remote_value(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_remote_value).collect()),
        _ => value.clone(),
    }
}

fn is_sensitive_remote_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("cookie")
        || key.contains("storage")
        || key.contains("password")
        || key.contains("secret")
        || key.contains("token")
        || key == "dek"
}

pub fn environment_bindings(
    env_ids: &[String],
    remotes: &[(String, Value)],
    details: &[(String, Value, chrono::DateTime<Utc>)],
    fingerprints: &[(String, Vec<String>)],
    proxies: &[(String, Vec<String>)],
) -> Vec<EnvironmentBindingSummary> {
    let remotes = remotes
        .iter()
        .map(|(env_id, value)| (env_id.as_str(), value))
        .collect::<HashMap<_, _>>();
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
            let mut metadata = detail
                .get("metadata")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if let Some(remote) = remotes.get(env_id.as_str()) {
                for key in ["envName", "serial"] {
                    let missing = metadata.get(key).is_none_or(|value| {
                        value.is_null() || value.as_str().is_some_and(str::is_empty)
                    });
                    if missing && let Some(value) = remote.get(key) {
                        metadata.insert(key.into(), value.clone());
                    }
                }
            }
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
                remote_metadata: Value::Object(metadata),
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

pub fn scan_kernels_with_catalogs<'a>(
    work_dir: &Path,
    catalogs: impl IntoIterator<Item = &'a Value>,
) -> Vec<KernelRecord> {
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

    for catalog in catalogs {
        for item in find_kernel_versions(catalog) {
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
        priority_string(value, &["type", "kernelId", "kernel"]).unwrap_or_else(|| "unknown".into());
    let major = priority_string(value, &["majorVersion", "major", "browserMajor"])
        .and_then(|value| value.parse().ok())
        .or_else(|| priority_number(value, &["major"]));
    let version = priority_string(value, &["versionCode", "version", "majorVersion"]);
    let platform =
        priority_string(value, &["platform"]).unwrap_or_else(|| std::env::consts::OS.into());
    let arch = priority_string(value, &["arch"]).unwrap_or_else(|| std::env::consts::ARCH.into());
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
        name: priority_string(value, &["kernelName", "name", "browserName"])
            .unwrap_or_else(|| "Browser core".into()),
        major,
        latest_version: (status == "available").then(|| version.clone()).flatten(),
        version: (status == "installed").then(|| version.clone()).flatten(),
        platform,
        arch,
        status: status.into(),
        install_path,
        download_available: priority_string(value, &["url", "downloadUrl", "downloadURL"])
            .is_some_and(|url| !url.trim().is_empty()),
        updated_at: Utc::now(),
    }
}

fn find_kernel_versions(value: &Value) -> Vec<&Value> {
    let mut versions = Vec::new();
    collect_kernel_versions(value, matches!(value, Value::Array(_)), &mut versions);
    versions
}

fn collect_kernel_versions<'a>(
    value: &'a Value,
    in_kernel_collection: bool,
    output: &mut Vec<&'a Value>,
) {
    match value {
        Value::Object(map) => {
            if in_kernel_collection && is_kernel_version(value) {
                output.push(value);
                return;
            }
            for (key, child) in map {
                collect_kernel_versions(
                    child,
                    in_kernel_collection || is_kernel_collection_key(key),
                    output,
                );
            }
        }
        Value::Array(values) => {
            for item in values {
                collect_kernel_versions(item, in_kernel_collection, output);
            }
        }
        _ => {}
    }
}

fn is_kernel_collection_key(key: &str) -> bool {
    matches!(
        key,
        "kernelVersions"
            | "kernelVersion"
            | "chromeKernelVersion"
            | "chromeKernelversion"
            | "firefoxKernelVersion"
            | "firefoxKernelversion"
            | "platformKernelversion"
            | "platformKernelVersion"
            | "list"
            | "items"
            | "records"
            | "rows"
            | "cores"
    )
}

fn is_kernel_version(value: &Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    let has_kernel_identity = ["type", "kernelId", "kernelName"]
        .iter()
        .any(|key| map.get(*key).is_some());
    let has_version = [
        "majorVersion",
        "major",
        "browserMajor",
        "versionCode",
        "version",
    ]
    .iter()
    .any(|key| map.get(*key).is_some());
    has_kernel_identity && has_version
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

fn priority_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| find_value(value, &HashSet::from([*key])).and_then(json_to_string))
}

fn priority_number(value: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|key| {
        find_value(value, &HashSet::from([*key]))
            .and_then(Value::as_u64)
            .and_then(|value| value.try_into().ok())
    })
}

fn json_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
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
        let catalog = json!({
                "data": { "kernelVersions": [{ "type": "yun", "majorVersion": "141" }] }
        });
        let records = scan_kernels_with_catalogs(Path::new("missing"), [&catalog]);
        assert_eq!(records.len(), 1);
        assert!(!records[0].download_available);
        assert_eq!(records[0].status, "available");
    }

    #[test]
    fn reads_kernel_versions_from_browser_kernel_list_page() {
        let catalog = json!({
                "code": 200,
                "data": {
                    "total": 1,
                    "list": [{
                        "kernelId": "Chrome",
                        "kernelName": "Chrome",
                        "majorVersion": "142",
                        "versionCode": 142001,
                        "platform": "windows",
                        "arch": "x86_64",
                        "url": "https://download.example.test/chrome-142.zip"
                    }]
                }
        });
        let records = scan_kernels_with_catalogs(Path::new("missing"), [&catalog]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kernel_type, "Chrome");
        assert_eq!(records[0].name, "Chrome");
        assert_eq!(records[0].major, Some(142));
        assert_eq!(records[0].latest_version.as_deref(), Some("142001"));
        assert_eq!(records[0].platform, "windows");
        assert_eq!(records[0].arch, "x86_64");
        assert!(records[0].download_available);
    }

    #[test]
    fn merges_sdk_init_and_sdk_info_kernel_catalogs() {
        let init_catalog = json!({
            "data": {
                "kernelVersions": [{
                    "kernelId": "chrome",
                    "kernelName": "Chrome",
                    "majorVersion": "143",
                    "platform": "windows",
                    "arch": "x86_64",
                    "url": "https://download.example.test/chrome-143.zip"
                }]
            }
        });
        let info_catalog = json!({
            "data": {
                "list": [{
                    "kernelId": "firefox",
                    "kernelName": "Firefox",
                    "majorVersion": "140",
                    "platform": "windows",
                    "arch": "x86_64",
                    "url": "https://download.example.test/firefox-140.zip"
                }]
            }
        });
        let records =
            scan_kernels_with_catalogs(Path::new("missing"), [&init_catalog, &info_catalog]);
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .any(|record| record.id == "chrome-143-windows-x86_64")
        );
        assert!(
            records
                .iter()
                .any(|record| record.id == "firefox-140-windows-x86_64")
        );
    }

    #[test]
    fn extracts_safe_environment_summary() {
        let summary = safe_environment_detail(&json!({
            "code": 200,
            "data": {
                "finger": {
                    "ua": "test",
                    "kernel": "finger-kernel-must-not-replace-browser",
                    "system": "All Windows",
                    "nested": { "token": "must not escape" }
                },
                "proxy": "socks5://alice:secret@example.test:1080",
                "cookie": "must not escape",
                "storage": { "path": "must not escape" },
                "dek": "must not escape",
                "browser": { "kernel": "yun", "version": "141", "mapId": 9 },
                "envName": "Server environment",
                "serial": "A-100"
            }
        }));
        assert_eq!(summary["fingerprint"]["ua"], "test");
        assert_eq!(summary["kernel"]["kernel"], "yun");
        assert_eq!(summary["kernel"]["version"], "141");
        assert_eq!(summary["kernel"]["system"], "All Windows");
        assert_eq!(
            summary["proxy"]["displayUrl"],
            "socks5://alice:***@example.test:1080"
        );
        assert_eq!(summary["metadata"]["envName"], "Server environment");
        let serialized = summary.to_string();
        assert!(!serialized.contains("must not escape"));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn invalid_remote_proxy_is_recorded_without_its_raw_value() {
        let summary = safe_environment_detail(&json!({
            "data": {
                "proxy": "alice:secret-without-a-url",
                "finger": { "language": ["zh-CN"] },
                "browser": { "kernel": "chrome", "version": "141" }
            }
        }));
        assert_eq!(summary["proxy"]["configured"], true);
        assert!(!summary.to_string().contains("secret-without-a-url"));
    }

    #[test]
    fn environment_binding_fills_metadata_from_server_page_cache() {
        let env_ids = vec!["env-1".to_string()];
        let bindings = environment_bindings(
            &env_ids,
            &[(
                "env-1".into(),
                json!({ "envName": "Server name", "serial": "CN-001" }),
            )],
            &[(
                "env-1".into(),
                json!({ "metadata": { "serial": "" }, "fingerprint": {} }),
                Utc::now(),
            )],
            &[],
            &[],
        );
        assert_eq!(bindings[0].remote_metadata["envName"], "Server name");
        assert_eq!(bindings[0].remote_metadata["serial"], "CN-001");
    }
}
