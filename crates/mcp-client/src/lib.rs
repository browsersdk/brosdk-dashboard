use std::time::Duration;

use reqwest::{Client, Response, header::HeaderMap};
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

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
    #[error("embedded MCP tool returned isError=true")]
    ToolFailed,
}

#[derive(Debug, Clone)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: Option<String>,
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
    call_tool(env_endpoint(port, env_id)?, tool, arguments).await
}

pub async fn discover_global_tools(port: u16) -> Result<McpToolDiscovery, McpClientError> {
    discover_tools(global_endpoint(port)?).await
}

pub async fn discover_env_tools(
    port: u16,
    env_id: &str,
) -> Result<McpToolDiscovery, McpClientError> {
    discover_tools(env_endpoint(port, env_id)?).await
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

fn env_endpoint(port: u16, env_id: &str) -> Result<Url, McpClientError> {
    let mut endpoint = Url::parse(&format!("http://127.0.0.1:{port}/"))?;
    endpoint
        .path_segments_mut()
        .expect("HTTP URLs support path segments")
        .extend(["sdk", "v1", "mcp", "env", env_id]);
    Ok(endpoint)
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
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(McpClientError::ToolFailed);
    }
    Ok(McpToolResult {
        protocol_version: protocol_version.into(),
        advertised_tools,
        result,
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
    fn endpoint_percent_encodes_environment_id() {
        assert_eq!(
            env_endpoint(9222, "env/one two")
                .expect("endpoint")
                .as_str(),
            "http://127.0.0.1:9222/sdk/v1/mcp/env/env%2Fone%20two"
        );
        assert_eq!(
            global_endpoint(9222).expect("endpoint").as_str(),
            "http://127.0.0.1:9222/sdk/v1/mcp"
        );
    }

    #[test]
    fn extracts_advertised_tool_metadata() {
        let tools = advertised_tools(&json!({
            "tools": [{
                "name": "tabs",
                "description": "Manage tabs",
                "annotations": { "readOnlyHint": false, "destructiveHint": true }
            }, { "name": "snapshot" }]
        }));
        assert_eq!(tools[0].name, "tabs");
        assert_eq!(tools[0].description.as_deref(), Some("Manage tabs"));
        assert_eq!(tools[0].read_only_hint, Some(false));
        assert_eq!(tools[0].destructive_hint, Some(true));
        assert_eq!(tools[1].name, "snapshot");
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
                            r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"tabs"}]}}"#,
                        )
                    }
                    3 => {
                        assert!(request.contains("tools/call"));
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
        assert_eq!(result.advertised_tools[0].name, "tabs");
        server.await.expect("server");
        let methods = methods.lock().expect("methods");
        assert_eq!(methods.len(), 5);
        assert!(methods[0].starts_with("POST "));
        assert!(methods[4].starts_with("DELETE "));
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
