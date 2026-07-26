use ai_agent::AiClient;

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
    println!(
        "{{\"model\":{},\"readOnly\":{},\"answerLength\":{}}}",
        serde_json::to_string(&response.model)?,
        response.read_only,
        response.answer.chars().count()
    );
    Ok(())
}
