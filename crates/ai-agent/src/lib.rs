use domain::{AiAgentPlan, AiChatResponse};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_MODEL: &str = "deepseek-v4-flash";

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

    pub fn status() -> domain::AiProviderStatus {
        domain::AiProviderStatus {
            provider: "openai-compatible".into(),
            base_url: env_value("BROSDK_AI_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            model: env_value("BROSDK_AI_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
            api_key_present: env_value("BROSDK_AI_API_KEY").is_some(),
        }
    }

    pub async fn chat(&self, prompt: &str, context: &Value) -> Result<AiChatResponse, AiError> {
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = "You are the read-only BroSDK Dashboard assistant. Use only the supplied redacted snapshot. Never claim an accepted browser start is ready. Do not propose hidden write actions. Reply concisely in the user's language.";
        let user = format!("Dashboard snapshot:\n{context_text}\n\nUser question:\n{prompt}");
        let answer = self.complete(system, &user, 0.2).await?;
        Ok(AiChatResponse {
            answer,
            model: self.model.clone(),
            read_only: true,
        })
    }

    pub async fn plan(&self, prompt: &str, context: &Value) -> Result<AiAgentPlan, AiError> {
        let context_text = serde_json::to_string(context).unwrap_or_else(|_| "{}".into());
        let system = "You plan one controlled BroSDK Dashboard action. Return JSON only with keys: summary, action, envId, expectedState, idempotencyKey, arguments. Allowed action values: none, environment.start, environment.stop, environment.sync, runtime.reconcile, proxy.diagnose, environment.diagnose, mcp.read. mcp.read requires envId and arguments with tool=browser_state or tabs; browser_state only allows action=get, tabs only allows action=list or current. envId is required for environment actions and mcp.read. expectedState must match the supplied snapshot. Never say accepted means ready.";
        let user = format!("Dashboard snapshot:\n{context_text}\n\nUser request:\n{prompt}");
        let content = self.complete(system, &user, 0.0).await?;
        parse_plan(&content)
    }

    async fn complete(
        &self,
        system: &str,
        user: &str,
        temperature: f32,
    ) -> Result<String, AiError> {
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatCompletionRequest {
                model: &self.model,
                messages: vec![
                    Message {
                        role: "system",
                        content: system,
                    },
                    Message {
                        role: "user",
                        content: user,
                    },
                ],
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
