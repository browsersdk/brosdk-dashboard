use std::collections::{HashMap, HashSet};

use serde_json::Value;

pub fn environment_rows(value: &Value) -> Vec<(String, String, Value)> {
    environment_items(value)
        .into_iter()
        .filter_map(|item| {
            let env_id = string_field(item, &["envId", "env_id", "id"])?;
            let name = string_field(item, &["name", "envName", "title"])
                .unwrap_or_else(|| format!("环境 {env_id}"));
            let mut cached = item.clone();
            sdk_ffi::redact_value(&mut cached);
            Some((env_id, name, cached))
        })
        .collect()
}

pub fn environment_total(value: &Value) -> Option<usize> {
    ["/data/total", "/data/count", "/total", "/count"]
        .iter()
        .find_map(|pointer| value.pointer(pointer))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .and_then(|value| usize::try_from(value).ok())
}

pub fn running_environments(value: &Value) -> HashMap<String, String> {
    environment_items(value)
        .into_iter()
        .filter_map(|item| {
            let env_id = string_field(item, &["envId", "env_id", "id"])?;
            if item.get("cdpReady").and_then(Value::as_bool) == Some(false)
                || item.get("isRunning").and_then(Value::as_bool) == Some(false)
            {
                return None;
            }
            if let Some(status) = string_field(item, &["statusName", "state"])
                && !matches!(
                    status.to_ascii_lowercase().as_str(),
                    "started" | "running" | "ready"
                )
            {
                return None;
            }
            let cdp = string_field(
                item,
                &["cdp", "cdpUrl", "debuggerAddress", "webSocketDebuggerUrl"],
            )
            .or_else(|| {
                item.get("remoteDebuggingPort")
                    .and_then(Value::as_u64)
                    .filter(|port| *port > 0)
                    .map(|port| format!("127.0.0.1:{port}"))
            })?;
            Some((env_id, cdp))
        })
        .collect()
}

pub fn observed_environment_ids(value: &Value) -> HashSet<String> {
    environment_items(value)
        .into_iter()
        .filter_map(|item| string_field(item, &["envId", "env_id", "id"]))
        .collect()
}

fn environment_items(value: &Value) -> Vec<&Value> {
    if let Value::Array(items) = value {
        return items.iter().collect();
    }
    const POINTERS: &[&str] = &[
        "/data/list",
        "/data/items",
        "/data/records",
        "/data/rows",
        "/data/data/list",
        "/list",
        "/items",
        "/records",
        "/rows",
    ];
    POINTERS
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_array))
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| match value {
            Value::String(value) if !value.is_empty() => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_nested_environment_page() {
        let rows = environment_rows(&json!({
            "data": {
                "list": [
                    { "envId": "env-1", "name": "Primary" },
                    { "id": 2 }
                ]
            }
        }));
        assert_eq!(rows[0].0, "env-1");
        assert_eq!(rows[0].1, "Primary");
        assert_eq!(rows[1].0, "2");
    }

    #[test]
    fn extracts_total_and_redacts_cached_secrets() {
        let value = json!({
            "data": {
                "total": "1",
                "list": [{
                    "envId": "env-1",
                    "proxy": "socks5://alice:secret@127.0.0.1:1080",
                    "cookie": "private"
                }]
            }
        });
        let rows = environment_rows(&value);
        assert_eq!(environment_total(&value), Some(1));
        assert_eq!(rows[0].2["cookie"], "[redacted]");
        assert_eq!(rows[0].2["proxy"], "socks5://alice:***@127.0.0.1:1080");
    }

    #[test]
    fn extracts_running_cdp_endpoint() {
        let rows = running_environments(&json!([
            { "envId": "env-1", "webSocketDebuggerUrl": "ws://localhost/1" }
        ]));
        assert_eq!(rows["env-1"], "ws://localhost/1");
    }

    #[test]
    fn ignores_browser_info_until_a_cdp_endpoint_is_ready() {
        let rows = running_environments(&json!([
            { "envId": "env-1", "remoteDebuggingPort": 0 },
            { "envId": "env-2", "statusName": "Starting", "remoteDebuggingPort": 9222 },
            { "envId": "env-3", "statusName": "Started", "cdpReady": false, "remoteDebuggingPort": 9223 }
        ]));
        assert!(rows.is_empty());
    }

    #[test]
    fn extracts_ready_remote_debugging_port() {
        let rows = running_environments(&json!([
            { "envId": "env-1", "remoteDebuggingPort": 9222 }
        ]));
        assert_eq!(rows["env-1"], "127.0.0.1:9222");
    }
}
