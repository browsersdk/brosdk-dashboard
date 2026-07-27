use ai_agent::{AiClient, AiToolDefinition, AiToolResult};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AiClient::from_env()?;
    let response = client
        .chat(
            "Reply with a short health-check sentence. Do not call tools or request secrets.",
            &serde_json::json!({ "test": "ai-smoke", "readOnly": true }),
            &[],
        )
        .await?;
    let tool = AiToolDefinition {
        name: "dashboard_health".into(),
        description: "Read the current Dashboard health status".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    };
    let tool_prompt = "Call dashboard_health to inspect the Dashboard, then summarize the result.";
    let tool_context = serde_json::json!({ "test": "ai-tool-smoke", "readOnly": true });
    let first = client
        .chat_turn(tool_prompt, &tool_context, &[], std::slice::from_ref(&tool))
        .await?;
    let call = first
        .tool_calls
        .first()
        .filter(|call| call.name == tool.name)
        .ok_or("AI provider did not return the bound dashboard_health tool call")?;
    let final_turn = client
        .chat_after_tools(
            tool_prompt,
            &tool_context,
            &[],
            &[tool],
            &first,
            &[AiToolResult {
                tool_call_id: call.id.clone(),
                content: r#"{"status":"ok"}"#.into(),
            }],
        )
        .await?;
    let tool_answer_length = final_turn
        .content
        .as_deref()
        .unwrap_or_default()
        .chars()
        .count();
    println!(
        "{{\"model\":{},\"readOnly\":{},\"answerLength\":{},\"toolCallVerified\":true,\"toolAnswerLength\":{}}}",
        serde_json::to_string(&response.model)?,
        response.read_only,
        response.answer.chars().count(),
        tool_answer_length
    );
    Ok(())
}
