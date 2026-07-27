use domain::{AiChatResponse, AiConversationMessage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";

#[derive(Debug, Error)]
pub enum AiError {
    #[error("BROSDK_AI_API_KEY is not configured")]
    MissingApiKey,
    #[error("AI request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("AI provider returned HTTP {status}: {message}")]
    Provider { status: u16, message: String },
    #[error("AI provider returned an empty response")]
    EmptyResponse,
    #[error("AI provider returned an invalid tool call: {0}")]
    InvalidToolCall(String),
    #[error("AI provider requested too many tools in one turn")]
    TooManyToolCalls,
    #[error("AI provider requested another tool round after receiving tool results")]
    TooManyToolRounds,
}

const MAX_TOOLS: usize = 64;
const MAX_TOOL_CALLS_PER_TURN: usize = 4;

#[derive(Debug, Clone)]
pub struct AiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<Value>,
    temperature: f32,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct AiToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiModelTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<AiToolCall>,
}

#[derive(Debug, Clone)]
pub struct AiToolResult {
    pub tool_call_id: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ProviderToolCall>,
}

#[derive(Debug, Deserialize)]
struct ProviderToolCall {
    id: String,
    function: ProviderFunctionCall,
}

#[derive(Debug, Deserialize)]
struct ProviderFunctionCall {
    name: String,
    arguments: String,
}

impl AiClient {
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn from_env() -> Result<Self, AiError> {
        let api_key = env_value("BROSDK_AI_API_KEY").ok_or(AiError::MissingApiKey)?;
        let base_url = env_value("BROSDK_AI_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let model = env_value("BROSDK_AI_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into());
        Ok(Self {
            http: reqwest::Client::builder().build()?,
            api_key,
            base_url: base_url.trim_end_matches('/').into(),
            model,
        })
    }

    pub fn from_config(api_key: String, base_url: String, model: String) -> Result<Self, AiError> {
        Ok(Self {
            http: reqwest::Client::builder().build()?,
            api_key,
            base_url: base_url.trim_end_matches('/').into(),
            model,
        })
    }

    pub fn status() -> domain::AiProviderStatus {
        domain::AiProviderStatus {
            provider: "openai-compatible".into(),
            base_url: env_value("BROSDK_AI_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            model: env_value("BROSDK_AI_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
            api_key_present: env_value("BROSDK_AI_API_KEY").is_some(),
            api_key_source: if env_value("BROSDK_AI_API_KEY").is_some() {
                "environment"
            } else {
                "none"
            }
            .into(),
            base_url_source: if env_value("BROSDK_AI_BASE_URL").is_some() {
                "environment"
            } else {
                "default"
            }
            .into(),
            model_source: if env_value("BROSDK_AI_MODEL").is_some() {
                "environment"
            } else {
                "default"
            }
            .into(),
        }
    }

    pub async fn chat(
        &self,
        prompt: &str,
        context: &Value,
        history: &[AiConversationMessage],
    ) -> Result<AiChatResponse, AiError> {
        let turn = self.chat_turn(prompt, context, history, &[]).await?;
        if !turn.tool_calls.is_empty() {
            return Err(AiError::InvalidToolCall(
                "read-only chat unexpectedly requested an unavailable tool".into(),
            ));
        }
        let answer = turn.content.ok_or(AiError::EmptyResponse)?;
        Ok(AiChatResponse {
            answer,
            model: self.model.clone(),
            read_only: true,
        })
    }

    pub async fn chat_turn(
        &self,
        prompt: &str,
        context: &Value,
        history: &[AiConversationMessage],
        tools: &[AiToolDefinition],
    ) -> Result<AiModelTurn, AiError> {
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = format!(
            "You are the read-only BroSDK Dashboard assistant. Use only the supplied redacted snapshot and bound read-only tools. Call tools when current runtime data is needed, then answer from their results. Never claim an accepted browser start is ready. Do not propose hidden write actions. Reply concisely in the user's language.\n\nCurrent Dashboard snapshot:\n{context_text}"
        );
        self.complete_turn(base_messages(&system, prompt, history), tools, 0.2)
            .await
    }

    pub async fn chat_after_tools(
        &self,
        prompt: &str,
        context: &Value,
        history: &[AiConversationMessage],
        tools: &[AiToolDefinition],
        assistant: &AiModelTurn,
        results: &[AiToolResult],
    ) -> Result<AiModelTurn, AiError> {
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = format!(
            "You are the read-only BroSDK Dashboard assistant. Use only the supplied redacted snapshot and bound read-only tools. Answer from the tool results and do not request another tool round. Never claim an accepted browser start is ready. Reply concisely in the user's language.\n\nCurrent Dashboard snapshot:\n{context_text}"
        );
        let mut messages = base_messages(&system, prompt, history);
        messages.push(assistant_message(assistant));
        messages.extend(results.iter().map(|result| {
            json!({
                "role": "tool",
                "tool_call_id": result.tool_call_id,
                "content": result.content,
            })
        }));
        let turn = self.complete_turn(messages, tools, 0.2).await?;
        if !turn.tool_calls.is_empty() {
            return Err(AiError::TooManyToolRounds);
        }
        Ok(turn)
    }

    pub async fn plan_turn(
        &self,
        prompt: &str,
        context: &Value,
        history: &[AiConversationMessage],
        tools: &[AiToolDefinition],
    ) -> Result<AiModelTurn, AiError> {
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = format!(
            "You plan exactly one controlled BroSDK Dashboard action. Use one bound function when an action or MCP operation is needed. Do not invent function names and do not call more than one function. If no action is required, answer normally and no function will be executed. The Manager resolves the final envId, expected state, approval, and idempotency key from current state. Never say accepted means ready.\n\nCurrent Dashboard snapshot:\n{context_text}"
        );
        self.complete_turn(base_messages(&system, prompt, history), tools, 0.0)
            .await
    }

    async fn complete_turn(
        &self,
        messages: Vec<Value>,
        tools: &[AiToolDefinition],
        temperature: f32,
    ) -> Result<AiModelTurn, AiError> {
        let tools = provider_tools(tools)?;
        let tool_choice = (!tools.is_empty()).then_some("auto");
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatCompletionRequest {
                model: &self.model,
                messages,
                temperature,
                stream: false,
                tools,
                tool_choice,
            })
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".into());
            return Err(AiError::Provider {
                status: status.as_u16(),
                message: sanitize_provider_error(&message),
            });
        }
        let response: ChatCompletionResponse = response.json().await?;
        let message = response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or(AiError::EmptyResponse)?;
        parse_turn(message)
    }
}

fn base_messages(system: &str, prompt: &str, history: &[AiConversationMessage]) -> Vec<Value> {
    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(json!({ "role": "system", "content": system }));
    messages.extend(
        history
            .iter()
            .map(|message| json!({ "role": message.role, "content": message.content })),
    );
    messages.push(json!({ "role": "user", "content": prompt }));
    messages
}

fn provider_tools(tools: &[AiToolDefinition]) -> Result<Vec<Value>, AiError> {
    if tools.len() > MAX_TOOLS {
        return Err(AiError::InvalidToolCall(format!(
            "at most {MAX_TOOLS} tools may be bound"
        )));
    }
    tools
        .iter()
        .map(|tool| {
            if tool.name.is_empty()
                || tool.name.len() > 64
                || !tool
                    .name
                    .chars()
                    .all(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '-'))
            {
                return Err(AiError::InvalidToolCall(format!(
                    "invalid function name {}",
                    tool.name
                )));
            }
            if !tool.parameters.is_object() {
                return Err(AiError::InvalidToolCall(format!(
                    "function {} parameters must be a JSON object schema",
                    tool.name
                )));
            }
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                }
            }))
        })
        .collect()
}

fn parse_turn(message: ChoiceMessage) -> Result<AiModelTurn, AiError> {
    if message.tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
        return Err(AiError::TooManyToolCalls);
    }
    let tool_calls = message
        .tool_calls
        .into_iter()
        .map(|call| {
            let arguments = serde_json::from_str::<Value>(&call.function.arguments)
                .map_err(|error| AiError::InvalidToolCall(error.to_string()))?;
            if !arguments.is_object() {
                return Err(AiError::InvalidToolCall(format!(
                    "function {} arguments must be a JSON object",
                    call.function.name
                )));
            }
            Ok(AiToolCall {
                id: call.id,
                name: call.function.name,
                arguments,
            })
        })
        .collect::<Result<Vec<_>, AiError>>()?;
    let content = message.content.filter(|content| !content.trim().is_empty());
    if content.is_none() && tool_calls.is_empty() {
        return Err(AiError::EmptyResponse);
    }
    Ok(AiModelTurn {
        content,
        tool_calls,
    })
}

fn assistant_message(turn: &AiModelTurn) -> Value {
    let tool_calls = turn
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".into()),
                }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "role": "assistant",
        "content": turn.content,
        "tool_calls": tool_calls,
    })
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn sanitize_provider_error(message: &str) -> String {
    let mut value = serde_json::from_str::<Value>(message).unwrap_or_else(|_| json!(message));
    sdk_redact(&mut value);
    serde_json::to_string(&value)
        .unwrap_or_else(|_| "AI provider request failed".into())
        .chars()
        .take(512)
        .collect()
}

fn sdk_redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "api_key" | "apikey" | "authorization" | "token"
                ) {
                    *child = Value::String("[redacted]".into());
                } else {
                    sdk_redact(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sdk_redact),
        Value::String(text) if text.starts_with("sk-") => *text = "[redacted]".into(),
        _ => {}
    }
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
    fn parses_native_tool_calls() {
        let turn = parse_turn(ChoiceMessage {
            content: None,
            tool_calls: vec![ProviderToolCall {
                id: "call-1".into(),
                function: ProviderFunctionCall {
                    name: "mcp_env_tabs".into(),
                    arguments: r#"{"action":"list"}"#.into(),
                },
            }],
        })
        .expect("turn");
        assert_eq!(
            turn.tool_calls,
            vec![AiToolCall {
                id: "call-1".into(),
                name: "mcp_env_tabs".into(),
                arguments: json!({ "action": "list" }),
            }]
        );
    }

    #[test]
    fn serializes_openai_compatible_function_tools() {
        let tools = provider_tools(&[AiToolDefinition {
            name: "mcp_global_sdk_health".into(),
            description: "SDK health".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }])
        .expect("tools");
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "mcp_global_sdk_health");
    }

    #[test]
    fn rejects_non_object_tool_arguments() {
        let error = parse_turn(ChoiceMessage {
            content: None,
            tool_calls: vec![ProviderToolCall {
                id: "call-1".into(),
                function: ProviderFunctionCall {
                    name: "bad".into(),
                    arguments: "[]".into(),
                },
            }],
        })
        .expect_err("arguments must be an object");
        assert!(matches!(error, AiError::InvalidToolCall(_)));
    }

    #[tokio::test]
    async fn completes_an_openai_compatible_tool_round() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base_url = format!("http://{}", listener.local_addr().expect("address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let server = tokio::spawn(async move {
            for step in 0..2 {
                let (mut stream, _) = listener.accept().await.expect("accept");
                let request = read_request(&mut stream).await;
                observed.lock().expect("requests").push(request.clone());
                let body = if step == 0 {
                    r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"mcp_global_sdk_health","arguments":"{}"}}]}}]}"#
                } else {
                    r#"{"choices":[{"message":{"content":"SDK is healthy.","tool_calls":[]}}]}"#
                };
                stream
                    .write_all(http_response(body).as_bytes())
                    .await
                    .expect("write");
            }
        });

        let client =
            AiClient::from_config("secret".into(), base_url, "test-model".into()).expect("client");
        let tools = vec![AiToolDefinition {
            name: "mcp_global_sdk_health".into(),
            description: "Read SDK health".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }];
        let context = json!({ "readOnly": true });
        let first = client
            .chat_turn("Check SDK health", &context, &[], &tools)
            .await
            .expect("first turn");
        assert_eq!(first.tool_calls[0].name, "mcp_global_sdk_health");
        let final_turn = client
            .chat_after_tools(
                "Check SDK health",
                &context,
                &[],
                &tools,
                &first,
                &[AiToolResult {
                    tool_call_id: "call-1".into(),
                    content: r#"{"status":"ok"}"#.into(),
                }],
            )
            .await
            .expect("final turn");
        assert_eq!(final_turn.content.as_deref(), Some("SDK is healthy."));
        server.await.expect("server");

        let requests = requests.lock().expect("requests");
        assert!(requests[0].contains(r#""tools":[{"#));
        assert!(requests[0].contains("mcp_global_sdk_health"));
        assert!(requests[1].contains(r#""role":"tool""#));
        assert!(requests[1].contains(r#""tool_call_id":"call-1""#));
        assert!(!requests.iter().any(|request| {
            request
                .split("\r\n\r\n")
                .nth(1)
                .is_some_and(|body| body.contains("secret"))
        }));
    }

    #[test]
    fn provider_errors_are_redacted() {
        assert!(!sanitize_provider_error(r#"{"api_key":"sk-secret"}"#).contains("sk-secret"));
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

    fn http_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
}
