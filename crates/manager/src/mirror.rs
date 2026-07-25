use std::collections::HashMap;

use serde_json::Value;

pub fn environment_rows(value: &Value) -> Vec<(String, String, Value)> {
    environment_items(value)
        .into_iter()
        .filter_map(|item| {
            let env_id = string_field(item, &["envId", "env_id", "id"])?;
            let name = string_field(item, &["name", "envName", "title"])
                .unwrap_or_else(|| format!("环境 {env_id}"));
            Some((env_id, name, item.clone()))
        })
        .collect()
}

pub fn running_environments(value: &Value) -> HashMap<String, String> {
    environment_items(value)
        .into_iter()
        .filter_map(|item| {
            let env_id = string_field(item, &["envId", "env_id", "id"])?;
            let cdp = string_field(
                item,
                &["cdp", "cdpUrl", "debuggerAddress", "webSocketDebuggerUrl"],
            )
            .unwrap_or_else(|| "ready".into());
            Some((env_id, cdp))
        })
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
    fn extracts_running_cdp_endpoint() {
        let rows = running_environments(&json!([
            { "envId": "env-1", "webSocketDebuggerUrl": "ws://localhost/1" }
        ]));
        assert_eq!(rows["env-1"], "ws://localhost/1");
    }
}
