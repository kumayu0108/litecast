use serde_json::{json, Value};

use crate::config::AiConfig;
use crate::secrets;

/// Ask the configured AI backend a question, optionally about an image (a PNG
/// file path). Runs on a background thread; uses blocking HTTP via ureq.
pub fn ask(config: &AiConfig, prompt: &str, image_path: Option<&str>) -> Result<String, String> {
    let key = secrets::get_api_key(&config.provider)
        .ok_or_else(|| format!("No API key set for {}", config.provider))?;

    let image_b64 = match image_path {
        Some(path) => Some(
            std::fs::read(path)
                .map(|bytes| base64_encode(&bytes))
                .map_err(|e| format!("Could not read screenshot: {e}"))?,
        ),
        None => None,
    };

    match config.provider.as_str() {
        "anthropic" => ask_anthropic(config, &key, prompt, image_b64.as_deref()),
        "openai" | "cursor" => ask_openai(config, &key, prompt, image_b64.as_deref()),
        other => Err(format!("Unknown AI provider: {other}")),
    }
}

fn ask_anthropic(
    config: &AiConfig,
    key: &str,
    prompt: &str,
    image_b64: Option<&str>,
) -> Result<String, String> {
    let mut content = vec![json!({"type": "text", "text": prompt})];
    if let Some(b64) = image_b64 {
        content.push(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": "image/png", "data": b64}
        }));
    }
    let body = json!({
        "model": config.model,
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": content}],
    });

    let mut resp = ureq::post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|e| e.to_string())?;

    let value: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    value
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|first| first.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Unexpected response from Anthropic".to_string())
}

fn ask_openai(
    config: &AiConfig,
    key: &str,
    prompt: &str,
    image_b64: Option<&str>,
) -> Result<String, String> {
    let base = if config.endpoint.is_empty() {
        "https://api.openai.com".to_string()
    } else {
        config.endpoint.trim_end_matches('/').to_string()
    };
    let url = format!("{base}/v1/chat/completions");

    let mut content = vec![json!({"type": "text", "text": prompt})];
    if let Some(b64) = image_b64 {
        content.push(json!({
            "type": "image_url",
            "image_url": {"url": format!("data:image/png;base64,{b64}")}
        }));
    }
    let body = json!({
        "model": config.model,
        "messages": [{"role": "user", "content": content}],
    });

    let mut resp = ureq::post(&url)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|e| e.to_string())?;

    let value: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Unexpected response from OpenAI-compatible endpoint".to_string())
}

/// Minimal standard base64 encoder (no padding shortcuts), hand-rolled to avoid
/// pulling in a crate for such a small need.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
