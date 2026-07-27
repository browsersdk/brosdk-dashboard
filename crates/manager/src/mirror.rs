use std::collections::{HashMap, HashSet};

use serde_json::Value;
use url::Url;

const CDP_ADDRESS_KEYS: &[&str] = &[
    "cdp",
    "cdpurl",
    "debuggeraddress",
    "websocketdebuggerurl",
    "websocketurl",
    "wsendpoint",
];
const CDP_PORT_KEYS: &[&str] = &[
    "cdpport",
    "debugport",
    "debuggingport",
    "remotedebuggingport",
];

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
            let cdp = cdp_endpoint(item)?;
            Some((env_id, cdp))
        })
        .collect()
}

pub fn cdp_endpoint(value: &Value) -> Option<String> {
    cdp_endpoint_at_depth(value, 0)
}

pub fn is_cdp_endpoint(value: &str) -> bool {
    normalize_cdp_address(value).is_some()
}

pub fn observed_environment_ids(value: &Value) -> HashSet<String> {
    environment_items(value)
        .into_iter()
        .filter_map(|item| string_field(item, &["envId", "env_id", "id"]))
        .collect()
}

pub fn mcp_tool_payload(value: &Value) -> Option<Value> {
    if let Some(payload) = value.get("structuredContent")
        && payload.is_object()
    {
        return Some(payload.clone());
    }
    value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .find_map(|text| serde_json::from_str::<Value>(text).ok())
}

fn environment_items(value: &Value) -> Vec<&Value> {
    if let Value::Array(items) = value {
        return items.iter().collect();
    }
    const POINTERS: &[&str] = &[
        "/data/envList",
        "/data/list",
        "/data/items",
        "/data/records",
        "/data/rows",
        "/data/data/list",
        "/environments",
        "/envList",
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

fn cdp_endpoint_at_depth(value: &Value, depth: usize) -> Option<String> {
    if depth > 12 {
        return None;
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if CDP_ADDRESS_KEYS.contains(&normalized_key(key).as_str())
                    && let Some(endpoint) = value.as_str().and_then(normalize_cdp_address)
                {
                    return Some(endpoint);
                }
            }
            for (key, value) in map {
                if CDP_PORT_KEYS.contains(&normalized_key(key).as_str())
                    && let Some(port) = cdp_port(value)
                {
                    return Some(format!("127.0.0.1:{port}"));
                }
            }
            map.values()
                .find_map(|value| cdp_endpoint_at_depth(value, depth + 1))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| cdp_endpoint_at_depth(value, depth + 1)),
        Value::String(text) => {
            let text = text.trim();
            if !matches!(text.as_bytes().first(), Some(b'{') | Some(b'[')) {
                return None;
            }
            serde_json::from_str::<Value>(text)
                .ok()
                .and_then(|value| cdp_endpoint_at_depth(&value, depth + 1))
        }
        _ => None,
    }
}

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(|character| !matches!(character, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_cdp_address(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || matches!(value.to_ascii_lowercase().as_str(), "-" | "ready") {
        return None;
    }
    if let Ok(url) = Url::parse(value)
        && matches!(url.scheme(), "http" | "https" | "ws" | "wss")
        && url.host().is_some()
    {
        return Some(value.to_string());
    }
    let url = Url::parse(&format!("http://{value}")).ok()?;
    if url.host().is_some()
        && url.port().is_some()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
    {
        Some(value.to_string())
    } else {
        None
    }
}

fn cdp_port(value: &Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .or_else(|| value.as_str()?.trim().parse::<u16>().ok())
        .filter(|port| *port > 0)
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
    fn extracts_global_mcp_status_payload_and_running_ids() {
        let result = json!({
            "content": [{
                "type": "text",
                "text": "{\"ok\":true,\"environments\":[{\"envId\":\"env-1\",\"status\":\"unknown\"}],\"count\":1}"
            }]
        });
        let payload = mcp_tool_payload(&result).expect("MCP JSON payload");
        assert_eq!(payload["count"], 1);
        assert_eq!(
            observed_environment_ids(&payload),
            HashSet::from(["env-1".into()])
        );
        assert!(running_environments(&payload).is_empty());
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

    #[test]
    fn extracts_cdp_from_getinfo_string_port_and_encoded_callback_data() {
        assert_eq!(
            cdp_endpoint(&json!({
                "data": { "browser": { "remote_debugging_port": "9333" } }
            })),
            Some("127.0.0.1:9333".into())
        );
        assert_eq!(
            cdp_endpoint(&json!({
                "data": r#"{"remoteDebuggingPort":9444}"#
            })),
            Some("127.0.0.1:9444".into())
        );
    }

    #[test]
    fn ignores_non_cdp_port_configuration() {
        assert_eq!(
            cdp_endpoint(&json!({
                "proxy": { "port": 1080 },
                "finger": {
                    "blockPortScanning": true,
                    "fpSwitches": { "fpBlockPort": 1 },
                    "portScanningWhitelist": "80,443,9222"
                }
            })),
            None
        );
    }
}
