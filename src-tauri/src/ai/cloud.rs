use serde::Deserialize;

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

// Anthropic Messages API response
#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    text: String,
}

pub async fn ask_openai(api_key: &str, model: &str, system: &str, user: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user}
        ],
        "max_tokens": 512
    });

    let resp = reqwest::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI error {status}: {text}"));
    }

    resp.json::<ChatResponse>()
        .await
        .map_err(|e| e.to_string())?
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content.trim().to_string())
        .ok_or_else(|| "empty response from OpenAI".into())
}

pub async fn ask_anthropic(api_key: &str, model: &str, system: &str, user: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 512,
        "system": system,
        "messages": [
            {"role": "user", "content": user}
        ]
    });

    let resp = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic error {status}: {text}"));
    }

    resp.json::<AnthropicResponse>()
        .await
        .map_err(|e| e.to_string())?
        .content
        .into_iter()
        .next()
        .map(|b| b.text.trim().to_string())
        .ok_or_else(|| "empty response from Anthropic".into())
}
