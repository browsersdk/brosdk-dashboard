use std::time::Duration;

use reqwest::{Client, Response, header::HeaderMap};
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const GLOBAL_ENVIRONMENT_MANAGEMENT_TOOLS: &[&str] = &[
    "env.list",
    "env.resolve",
    "env.get",
    "env.create",
    "env.update",
    "env.destroy",
];

#[derive(Debug, Error)]
pub enum McpClientError {
    #[error("invalid embedded MCP endpoint: {0}")]
    Endpoint(#[from] url::ParseError),
    #[error("embedded MCP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("embedded MCP returned HTTP {status}: {message}")]
    Status { status: u16, message: String },
    #[error("embedded MCP response exceeded {MAX_RESPONSE_BYTES} bytes")]
    ResponseTooLarge,
    #[error("embedded MCP response was invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("embedded MCP initialize response did not include Mcp-Session-Id")]
    MissingSession,
    #[error("embedded MCP tool {0} is not advertised by tools/list")]
    ToolUnavailable(String),
    #[error("embedded MCP JSON-RPC error: {0}")]
    Rpc(String),
    #[error("embedded MCP environment tool name is invalid: {0}")]
    InvalidEnvironmentTool(String),
    #[error("embedded MCP environment arguments must be a JSON object")]
    InvalidEnvironmentArguments,
}

#[derive(Debug, Clone)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct McpToolDiscovery {
    pub protocol_version: String,
    pub advertised_tools: Vec<McpToolDefinition>,
}

#[derive(Debug, Clone)]
pub struct McpToolResult {
    pub protocol_version: String,
    pub advertised_tools: Vec<McpToolDefinition>,
    pub result: Value,
    pub is_error: bool,
}

pub async fn call_global_tool(
    port: u16,
    tool: &str,
    arguments: Value,
) -> Result<McpToolResult, McpClientError> {
    call_tool(global_endpoint(port)?, tool, arguments).await
}

pub async fn call_env_tool(
    port: u16,
    env_id: &str,
    tool: &str,
    arguments: Value,
) -> Result<McpToolResult, McpClientError> {
    let tool = global_environment_tool_name(tool)?;
    let arguments = with_environment_id(arguments, env_id)?;
    let mut result = call_tool(global_endpoint(port)?, &tool, arguments).await?;
    result
        .advertised_tools
        .retain(|tool| is_environment_browser_tool(&tool.name));
    Ok(result)
}

pub async fn discover_global_tools(port: u16) -> Result<McpToolDiscovery, McpClientError> {
    discover_tools(global_endpoint(port)?).await
}

pub async fn discover_env_tools(
    port: u16,
    _env_id: &str,
) -> Result<McpToolDiscovery, McpClientError> {
    let mut discovery = discover_tools(global_endpoint(port)?).await?;
    discovery
        .advertised_tools
        .retain(|tool| is_environment_browser_tool(&tool.name));
    Ok(discovery)
}

pub async fn call_scoped_tool(
    port: u16,
    env_id: Option<&str>,
    tool: &str,
    arguments: Value,
) -> Result<McpToolResult, McpClientError> {
    match env_id {
        Some(env_id) => call_env_tool(port, env_id, tool, arguments).await,
        None => call_global_tool(port, tool, arguments).await,
    }
}

pub async fn discover_scoped_tools(
    port: u16,
    env_id: Option<&str>,
) -> Result<McpToolDiscovery, McpClientError> {
    match env_id {
        Some(env_id) => discover_env_tools(port, env_id).await,
        None => discover_global_tools(port).await,
    }
}

async fn call_tool(
    endpoint: Url,
    tool: &str,
    arguments: Value,
) -> Result<McpToolResult, McpClientError> {
    let client = client()?;
    let (session_id, protocol_version) = initialize(&client, &endpoint).await?;
    let result = async {
        let advertised_tools =
            activate_and_list(&client, &endpoint, &session_id, &protocol_version).await?;
        if !advertised_tools
            .iter()
            .any(|definition| definition.name == tool)
        {
            return Err(McpClientError::ToolUnavailable(tool.into()));
        }
        call_in_session(
            &client,
            &endpoint,
            &session_id,
            &protocol_version,
            tool,
            arguments,
            advertised_tools,
        )
        .await
    }
    .await;
    let _ = close_session(&client, &endpoint, &session_id, &protocol_version).await;
    result
}

async fn discover_tools(endpoint: Url) -> Result<McpToolDiscovery, McpClientError> {
    let client = client()?;
    let (session_id, protocol_version) = initialize(&client, &endpoint).await?;
    let result = activate_and_list(&client, &endpoint, &session_id, &protocol_version)
        .await
        .map(|advertised_tools| McpToolDiscovery {
            protocol_version: protocol_version.clone(),
            advertised_tools,
        });
    let _ = close_session(&client, &endpoint, &session_id, &protocol_version).await;
    result
}

fn client() -> Result<Client, McpClientError> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(45))
        .build()?)
}

fn global_endpoint(port: u16) -> Result<Url, McpClientError> {
    Ok(Url::parse(&format!("http://127.0.0.1:{port}/sdk/v1/mcp"))?)
}

fn global_environment_tool_name(tool: &str) -> Result<String, McpClientError> {
    let tool = tool.trim();
    let base = tool.strip_prefix("env.").unwrap_or(tool);
    if base.is_empty()
        || base.chars().count() > 124
        || !base
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(McpClientError::InvalidEnvironmentTool(tool.into()));
    }
    let tool = format!("env.{base}");
    if GLOBAL_ENVIRONMENT_MANAGEMENT_TOOLS.contains(&tool.as_str()) {
        return Err(McpClientError::InvalidEnvironmentTool(tool));
    }
    Ok(tool)
}

fn is_environment_browser_tool(tool: &str) -> bool {
    tool.starts_with("env.") && !GLOBAL_ENVIRONMENT_MANAGEMENT_TOOLS.contains(&tool)
}

fn with_environment_id(mut arguments: Value, env_id: &str) -> Result<Value, McpClientError> {
    let object = arguments
        .as_object_mut()
        .ok_or(McpClientError::InvalidEnvironmentArguments)?;
    object.insert("envId".into(), Value::String(env_id.into()));
    Ok(arguments)
}

async fn initialize(client: &Client, endpoint: &Url) -> Result<(String, String), McpClientError> {
    let response = post_json(
        client,
        endpoint,
        None,
        None,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "brosdk-dashboard", "version": "0.1.0" }
            }
        }),
    )
    .await?;
    let headers = response.headers().clone();
    let value = response_json(response).await?;
    rpc_result(&value)?;
    let session_id = header(&headers, "mcp-session-id").ok_or(McpClientError::MissingSession)?;
    let protocol_version =
        header(&headers, "mcp-protocol-version").unwrap_or_else(|| MCP_PROTOCOL_VERSION.into());
    Ok((session_id, protocol_version))
}

async fn activate_and_list(
    client: &Client,
    endpoint: &Url,
    session_id: &str,
    protocol_version: &str,
) -> Result<Vec<McpToolDefinition>, McpClientError> {
    let initialized = post_json(
        client,
        endpoint,
        Some(session_id),
        Some(protocol_version),
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await?;
    ensure_success(initialized).await?;

    let list = post_json(
        client,
        endpoint,
        Some(session_id),
        Some(protocol_version),
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }),
    )
    .await?;
    let list = response_json(list).await?;
    Ok(advertised_tools(rpc_result(&list)?))
}

async fn call_in_session(
    client: &Client,
    endpoint: &Url,
    session_id: &str,
    protocol_version: &str,
    tool: &str,
    arguments: Value,
    advertised_tools: Vec<McpToolDefinition>,
) -> Result<McpToolResult, McpClientError> {
    let call = post_json(
        client,
        endpoint,
        Some(session_id),
        Some(protocol_version),
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments }
        }),
    )
    .await?;
    let call = response_json(call).await?;
    let result = rpc_result(&call)?.clone();
    let is_error = result.get("isError").and_then(Value::as_bool) == Some(true);
    Ok(McpToolResult {
        protocol_version: protocol_version.into(),
        advertised_tools,
        result,
        is_error,
    })
}

async fn close_session(
    client: &Client,
    endpoint: &Url,
    session_id: &str,
    protocol_version: &str,
) -> Result<(), McpClientError> {
    let response = client
        .delete(endpoint.clone())
        .header("Mcp-Session-Id", session_id)
        .header("Mcp-Protocol-Version", protocol_version)
        .send()
        .await?;
    ensure_success(response).await
}

async fn post_json(
    client: &Client,
    endpoint: &Url,
    session_id: Option<&str>,
    protocol_version: Option<&str>,
    body: &Value,
) -> Result<Response, McpClientError> {
    let mut request = client
        .post(endpoint.clone())
        .header("Accept", "application/json, text/event-stream")
        .json(body);
    if let Some(session_id) = session_id {
        request = request.header("Mcp-Session-Id", session_id);
    }
    if let Some(protocol_version) = protocol_version {
        request = request.header("Mcp-Protocol-Version", protocol_version);
    }
    let response = request.send().await?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(status_error(response).await)
    }
}

async fn ensure_success(response: Response) -> Result<(), McpClientError> {
    if response.status().is_success() {
        Ok(())
    } else {
        Err(status_error(response).await)
    }
}

async fn status_error(response: Response) -> McpClientError {
    let status = response.status().as_u16();
    let message = bounded_text(response)
        .await
        .unwrap_or_else(|_| "request failed".into());
    McpClientError::Status {
        status,
        message: summarize_message(&message),
    }
}

async fn response_json(response: Response) -> Result<Value, McpClientError> {
    let bytes = bounded_bytes(response).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn bounded_text(response: Response) -> Result<String, McpClientError> {
    let bytes = bounded_bytes(response).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn bounded_bytes(response: Response) -> Result<Vec<u8>, McpClientError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(McpClientError::ResponseTooLarge);
    }
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(McpClientError::ResponseTooLarge);
    }
    Ok(bytes.to_vec())
}

fn rpc_result(value: &Value) -> Result<&Value, McpClientError> {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown JSON-RPC error");
        return Err(McpClientError::Rpc(summarize_message(message)));
    }
    value
        .get("result")
        .ok_or_else(|| McpClientError::Rpc("response did not contain result".into()))
}

fn advertised_tools(result: &Value) -> Vec<McpToolDefinition> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(Value::as_str)?.to_string();
            let annotations = tool.get("annotations");
            Some(McpToolDefinition {
                name,
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                input_schema: tool
                    .get("inputSchema")
                    .filter(|value| value.is_object())
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                read_only_hint: annotations
                    .and_then(|value| value.get("readOnlyHint"))
                    .and_then(Value::as_bool),
                destructive_hint: annotations
                    .and_then(|value| value.get("destructiveHint"))
                    .and_then(Value::as_bool),
            })
        })
        .collect()
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn summarize_message(message: &str) -> String {
    message
        .chars()
        .filter(|character| !character.is_control() || *character == ' ')
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn environment_tools_use_global_names_and_explicit_arguments() {
        assert_eq!(
            global_environment_tool_name("tabs").expect("legacy tool"),
            "env.tabs"
        );
        assert_eq!(
            global_environment_tool_name("env.snapshot").expect("global tool"),
            "env.snapshot"
        );
        assert!(global_environment_tool_name("env.create").is_err());
        assert_eq!(
            with_environment_id(json!({ "envId": "spoofed", "action": "list" }), "env-1")
                .expect("arguments"),
            json!({ "envId": "env-1", "action": "list" })
        );
    }

    #[test]
    fn extracts_advertised_tool_metadata() {
        let tools = advertised_tools(&json!({
            "tools": [{
                "name": "tabs",
                "description": "Manage tabs",
                "inputSchema": {
                    "type": "object",
                    "properties": { "action": { "type": "string" } },
                    "required": ["action"]
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": true }
            }, { "name": "snapshot" }]
        }));
        assert_eq!(tools[0].name, "tabs");
        assert_eq!(tools[0].description.as_deref(), Some("Manage tabs"));
        assert_eq!(tools[0].input_schema["required"], json!(["action"]));
        assert_eq!(tools[0].read_only_hint, Some(false));
        assert_eq!(tools[0].destructive_hint, Some(true));
        assert_eq!(tools[1].name, "snapshot");
        assert_eq!(
            tools[1].input_schema,
            json!({ "type": "object", "properties": {} })
        );
    }

    #[tokio::test]
    async fn completes_strict_streamable_http_lifecycle() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let methods = Arc::new(Mutex::new(Vec::new()));
        let observed = methods.clone();
        let server = tokio::spawn(async move {
            for step in 0..5 {
                let (mut stream, _) = listener.accept().await.expect("accept");
                let request = read_request(&mut stream).await;
                let request_lower = request.to_ascii_lowercase();
                let first_line = request.lines().next().unwrap_or_default().to_string();
                observed.lock().expect("methods").push(first_line);
                let response = match step {
                    0 => http_response(
                        "200 OK",
                        &[
                            ("Mcp-Session-Id", "session-1"),
                            ("Mcp-Protocol-Version", MCP_PROTOCOL_VERSION),
                        ],
                        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}}"#,
                    ),
                    1 => {
                        assert!(request_lower.contains("mcp-session-id: session-1"));
                        assert!(request.contains("notifications/initialized"));
                        http_response("202 Accepted", &[], "")
                    }
                    2 => {
                        assert!(request_lower.contains("mcp-protocol-version: 2025-11-25"));
                        assert!(request.contains("tools/list"));
                        http_response(
                            "200 OK",
                            &[],
                            r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"sdk.health"},{"name":"env.get"},{"name":"env.create"},{"name":"env.tabs"}]}}"#,
                        )
                    }
                    3 => {
                        assert!(request.contains("tools/call"));
                        assert!(request.contains(r#""name":"env.tabs""#));
                        assert!(request.contains(r#""envId":"env-1""#));
                        http_response(
                            "200 OK",
                            &[],
                            r#"{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{\"pages\":[]}"}]}}"#,
                        )
                    }
                    _ => http_response("200 OK", &[], "{}"),
                };
                stream.write_all(response.as_bytes()).await.expect("write");
            }
        });

        let result = call_env_tool(port, "env-1", "tabs", json!({ "action": "list" }))
            .await
            .expect("tool call");
        assert_eq!(result.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(result.advertised_tools.len(), 1);
        assert_eq!(result.advertised_tools[0].name, "env.tabs");
        server.await.expect("server");
        let methods = methods.lock().expect("methods");
        assert_eq!(methods.len(), 5);
        assert!(methods[0].starts_with("POST /sdk/v1/mcp "));
        assert!(methods[4].starts_with("DELETE "));
    }

    #[tokio::test]
    async fn returns_structured_tool_errors_without_turning_them_into_transport_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = tokio::spawn(async move {
            for step in 0..5 {
                let (mut stream, _) = listener.accept().await.expect("accept");
                let request = read_request(&mut stream).await;
                let response = match step {
                    0 => http_response(
                        "200 OK",
                        &[("Mcp-Session-Id", "error-session")],
                        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}}"#,
                    ),
                    1 => http_response("202 Accepted", &[], ""),
                    2 => http_response(
                        "200 OK",
                        &[],
                        r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"env.get"}]}}"#,
                    ),
                    3 => {
                        assert!(request.contains(r#""name":"env.get""#));
                        http_response(
                            "200 OK",
                            &[],
                            r#"{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"browser environment is not active"}],"structuredContent":{"code":"ENV_NOT_FOUND","message":"browser environment is not active"},"isError":true}}"#,
                        )
                    }
                    _ => http_response("200 OK", &[], "{}"),
                };
                stream.write_all(response.as_bytes()).await.expect("write");
            }
        });

        let result = call_global_tool(port, "env.get", json!({ "envId": "123" }))
            .await
            .expect("tool-level errors remain valid MCP results");
        assert!(result.is_error);
        assert_eq!(result.result["structuredContent"]["code"], "ENV_NOT_FOUND");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn discovers_global_tools_and_closes_session() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let methods = Arc::new(Mutex::new(Vec::new()));
        let observed = methods.clone();
        let server = tokio::spawn(async move {
            for step in 0..4 {
                let (mut stream, _) = listener.accept().await.expect("accept");
                let request = read_request(&mut stream).await;
                observed
                    .lock()
                    .expect("methods")
                    .push(request.lines().next().unwrap_or_default().to_string());
                let response = match step {
                    0 => http_response(
                        "200 OK",
                        &[("Mcp-Session-Id", "global-session")],
                        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}}"#,
                    ),
                    1 => http_response("202 Accepted", &[], ""),
                    2 => http_response(
                        "200 OK",
                        &[],
                        r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"sdk.health","annotations":{"readOnlyHint":true}}]}}"#,
                    ),
                    _ => http_response("200 OK", &[], "{}"),
                };
                stream.write_all(response.as_bytes()).await.expect("write");
            }
        });

        let discovery = discover_global_tools(port).await.expect("discovery");
        assert_eq!(discovery.advertised_tools[0].name, "sdk.health");
        assert_eq!(discovery.advertised_tools[0].read_only_hint, Some(true));
        server.await.expect("server");
        let methods = methods.lock().expect("methods");
        assert_eq!(methods.len(), 4);
        assert!(methods[0].contains("/sdk/v1/mcp "));
        assert!(methods[3].starts_with("DELETE "));
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).await.expect("read");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers =
                    String::from_utf8_lossy(&bytes[..header_end + 4]).to_ascii_lowercase();
                let content_length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or_default();
                if bytes.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        String::from_utf8(bytes).expect("UTF-8 request")
    }

    fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
        let extra_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
            body.len()
        )
    }
}
