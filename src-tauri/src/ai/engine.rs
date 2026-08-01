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

// End-of-turn special tokens from various chat templates (ChatML/Phi/Llama/
// Gemma/...). The llama.cpp server starts without --chat-template (we do no
// capability detection on the model itself — the sidecar does not know which
// model it was handed), so it uses a default template that need not match the one
// the model was trained on. As a result the model sometimes fails to stop at its
// own stop token in time and that token breaks through into the answer as
// ordinary text. We fix it on the way out by cutting the known tokens, rather
// than trusting the server to always recognize and trim them itself.
const KNOWN_STOP_TOKENS: [&str; 6] = [
    "<|end|>", "<|endoftext|>", "<|im_end|>", "<|eot_id|>", "</s>", "<end_of_turn>",
];

fn strip_stop_tokens(s: &str) -> String {
    let mut out = s.to_string();
    for tok in KNOWN_STOP_TOKENS {
        out = out.replace(tok, "");
    }
    out.trim().to_string()
}

pub async fn ask(port: u16, system: &str, user: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "local",
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user}
        ]
    });

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<ChatResponse>()
        .await
        .map_err(|e| e.to_string())?;

    resp.choices
        .into_iter()
        .next()
        .map(|c| strip_stop_tokens(&c.message.content))
        .ok_or_else(|| "empty response from model".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_trailing_end_token() {
        assert_eq!(strip_stop_tokens("Готовый ответ<|end|>"), "Готовый ответ");
    }

    #[test]
    fn strips_various_known_stop_tokens() {
        assert_eq!(strip_stop_tokens("текст<|endoftext|>"), "текст");
        assert_eq!(strip_stop_tokens("текст<|im_end|>"), "текст");
        assert_eq!(strip_stop_tokens("текст<|eot_id|>"), "текст");
        assert_eq!(strip_stop_tokens("текст</s>"), "текст");
        assert_eq!(strip_stop_tokens("текст<end_of_turn>"), "текст");
    }

    #[test]
    fn leaves_clean_text_untouched() {
        assert_eq!(strip_stop_tokens("обычный текст без токенов"), "обычный текст без токенов");
    }

    #[test]
    fn does_not_touch_similar_but_different_tags() {
        // it must not accidentally cut text where the user writes something
        // tag-like that is not one of the model's special tokens
        assert_eq!(strip_stop_tokens("<|not-a-real-token|>"), "<|not-a-real-token|>");
    }
}
