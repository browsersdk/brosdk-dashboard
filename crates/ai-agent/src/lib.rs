use domain::{AiAgentPlan, AiChatResponse, AiConversationMessage};
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
    #[error("AI agent plan is invalid JSON: {0}")]
    InvalidPlan(String),
}

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
    messages: Vec<Message<'a>>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
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
}

impl AiClient {
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
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = format!(
            "You are the read-only BroSDK Dashboard assistant. Use only the supplied redacted snapshot. Never claim an accepted browser start is ready. Do not propose hidden write actions. Reply concisely in the user's language.\n\nCurrent Dashboard snapshot:\n{context_text}"
        );
        let answer = self.complete(&system, prompt, history, 0.2).await?;
        Ok(AiChatResponse {
            answer,
            model: self.model.clone(),
            read_only: true,
        })
    }

    pub async fn plan(
        &self,
        prompt: &str,
        context: &Value,
        history: &[AiConversationMessage],
    ) -> Result<AiAgentPlan, AiError> {
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = format!(
            "You plan one controlled BroSDK Dashboard action. Return JSON only with keys: summary, action, envId, arguments. Allowed action values: none, environment.start, environment.stop, environment.sync, runtime.reconcile, proxy.diagnose, environment.diagnose, mcp.read, mcp.call. mcp.read is the compatibility action for bounded reads. mcp.call invokes one DLL-advertised tool and requires arguments shaped as {{\"tool\":\"tool_name\",\"arguments\":{{...}}}}. Set envId for a single-environment browser tool; omit envId only for a global management read. Current normal single-environment tools include browser_state, tabs, bookmarks, history, tab_groups, navigate, snapshot, diff, act, download, upload, read, grep, screenshot, pdf, wait, windows, and evaluate, but runtime tools/list is authoritative. envId is required for environment actions and single-environment MCP. The Manager resolves the final target, expected state, and idempotency key from current local state. Never say accepted means ready.\n\nCurrent Dashboard snapshot:\n{context_text}"
        );
        let content = self.complete(&system, prompt, history, 0.0).await?;
        parse_plan(&content)
    }

    async fn complete(
        &self,
        system: &str,
        prompt: &str,
        history: &[AiConversationMessage],
        temperature: f32,
    ) -> Result<String, AiError> {
        let mut messages = Vec::with_capacity(history.len() + 2);
        messages.push(Message {
            role: "system",
            content: system,
        });
        messages.extend(history.iter().map(|message| Message {
            role: message.role.as_str(),
            content: message.content.as_str(),
        }));
        messages.push(Message {
            role: "user",
            content: prompt,
        });
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatCompletionRequest {
                model: &self.model,
                messages,
                temperature,
                stream: false,
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
        response
            .choices
            .into_iter()
            .find_map(|choice| choice.message.content)
            .filter(|content| !content.trim().is_empty())
            .ok_or(AiError::EmptyResponse)
    }
}

fn parse_plan(content: &str) -> Result<AiAgentPlan, AiError> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .strip_suffix("```")
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(json_text).map_err(|error| AiError::InvalidPlan(error.to_string()))
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
    use super::*;

    #[test]
    fn parses_json_plan_from_markdown_fence() {
        let plan = parse_plan(
            r#"```json
{"summary":"start","action":"environment.start","envId":"env-1","expectedState":"stopped","idempotencyKey":"key-1","arguments":{}}
```"#,
        )
        .expect("plan");
        assert_eq!(plan.action, "environment.start");
        assert_eq!(plan.env_id.as_deref(), Some("env-1"));
    }

    #[test]
    fn provider_errors_are_redacted() {
        assert!(!sanitize_provider_error(r#"{"api_key":"sk-secret"}"#).contains("sk-secret"));
    }
}
