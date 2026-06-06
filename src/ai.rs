use std::time::Duration;

use serde_json::{json, Value};

use crate::config::AiConfig;
use crate::secrets;
use crate::security::url::validate_ai_endpoint;

/// One turn in a conversation.
#[derive(Clone, Debug)]
pub enum Role {
    User,
    Assistant,
}

/// A single conversation message. `content` is plain text; images are attached
/// separately to the final user turn (see `ask_chat`).
#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub role: Role,
    pub content: String,
}

impl ChatMsg {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// Ask the configured AI backend, threading the full conversation `history`
/// (oldest first; the last entry is the newest user turn). An optional image is
/// attached to the final user turn. Runs on a background thread; blocking ureq.
pub fn ask_chat(
    config: &AiConfig,
    history: &[ChatMsg],
    image_path: Option<&str>,
) -> Result<String, String> {
    let image_b64 = match image_path {
        Some(path) => Some(
            std::fs::read(path)
                .map(|bytes| base64_encode(&bytes))
                .map_err(|e| format!("Could not read screenshot: {e}"))?,
        ),
        None => None,
    };

    if history.is_empty() {
        return Err("No prompt to send".to_string());
    }

    validate_ai_endpoint(
        &config.provider,
        &config.endpoint,
        config.allow_private_endpoint,
    )?;

    match config.provider.as_str() {
        "anthropic" => {
            let key = secrets::api_key_for_chat(&config.provider, &config.endpoint)
                .ok_or_else(|| format!("No API key set for {}", config.provider))?;
            ask_anthropic(config, &key, history, image_b64.as_deref())
        }
        "openai" | "openai-compatible" | "cursor" => {
            let key = secrets::api_key_for_chat(&config.provider, &config.endpoint)
                .ok_or_else(|| format!("No API key set for {}", config.provider))?;
            ask_openai(config, Some(key.as_str()), history, image_b64.as_deref())
        }
        "ollama" => ask_ollama(config, history, image_b64.as_deref()),
        "gemini" | "google" => {
            let key = secrets::api_key_for_chat(&config.provider, &config.endpoint)
                .ok_or_else(|| format!("No API key set for {}", config.provider))?;
            ask_gemini(config, &key, history, image_b64.as_deref())
        }
        other => Err(format!("Unknown AI provider: {other}")),
    }
}

/// List installed Ollama models from `GET /api/tags`.
pub fn list_ollama_models(endpoint: &str) -> Result<Vec<String>, String> {
    validate_ai_endpoint("ollama", endpoint, false)?;
    let base = ollama_base(endpoint);
    let url = format!("{base}/api/tags");
    let mut resp = ureq::get(&url)
        .config()
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|e| format!("Could not reach Ollama at {base}: {e}"))?;
    let value: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    let models: Vec<String> = value
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if models.is_empty() {
        Err("No models installed. Run `ollama pull llama3.2` (or `ollama pull hf.co/...` for Hugging Face GGUF models)".to_string())
    } else {
        Ok(models)
    }
}

fn ollama_base(endpoint: &str) -> String {
    if endpoint.is_empty() {
        "http://127.0.0.1:11434".to_string()
    } else {
        endpoint.trim_end_matches('/').to_string()
    }
}

/// Quick reachability probe against the Ollama endpoint. Hits `GET /api/tags`
/// with a short timeout; returns true only on a real HTTP response (any
/// status), false on connect/timeout errors.
fn ollama_reachable(base: &str) -> bool {
    let url = format!("{base}/api/tags");
    let reachable = ureq::get(&url)
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_millis(800)))
        .build()
        .call()
        .is_ok();
    crate::debug_log::log(
        "ai::ollama",
        "reachability_check",
        &format!(r#"{{"base":"{base}","reachable":{reachable}}}"#),
    );
    reachable
}

/// Resolve the `ollama` binary. A GUI `.app` launched from Finder inherits a
/// minimal PATH (often `/usr/bin:/bin`), so the Homebrew install locations are
/// checked explicitly before falling back to a bare `ollama` (PATH lookup).
fn resolve_ollama_binary() -> Option<String> {
    const CANDIDATES: &[&str] = &["/opt/homebrew/bin/ollama", "/usr/local/bin/ollama"];
    for path in CANDIDATES {
        if std::path::Path::new(path).exists() {
            return Some((*path).to_string());
        }
    }
    // Fall back to PATH lookup via `which`-style probing of the inherited PATH.
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = std::path::Path::new(dir).join("ollama");
            if candidate.exists() {
                return candidate.to_str().map(str::to_string);
            }
        }
    }
    None
}

/// Ensure the Ollama server is reachable before sending a request. If it is not
/// running, attempt to start `ollama serve` as a detached background process
/// and poll for readiness. Returns a human-readable error if Ollama is not
/// installed or could not be started within the bounded window.
///
/// Bounded everywhere: the reachability probe and readiness polls use short
/// timeouts, so this never hangs the caller (the AI worker thread).
fn ensure_ollama_running(base: &str) -> Result<(), String> {
    if ollama_reachable(base) {
        return Ok(());
    }

    let binary = match resolve_ollama_binary() {
        Some(b) => b,
        None => {
            crate::debug_log::log(
                "ai::ollama",
                "spawn_skipped",
                r#"{"reason":"binary_not_found"}"#,
            );
            return Err("Ollama is not installed - see https://ollama.com".to_string());
        }
    };

    crate::debug_log::log(
        "ai::ollama",
        "spawn_attempt",
        &format!(r#"{{"binary":"{binary}"}}"#),
    );

    use std::process::{Command, Stdio};
    let spawn_result = Command::new(&binary)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match spawn_result {
        Ok(_child) => {
            // Detach: drop the handle without waiting. `ollama serve` keeps
            // running independently; macOS reaps it via launchd-style orphan
            // adoption, so no zombie is left behind for litecast.
        }
        Err(e) => {
            crate::debug_log::log(
                "ai::ollama",
                "spawn_failed",
                &format!(r#"{{"binary":"{binary}","error":"{e}"}}"#),
            );
            return Err(format!("Couldn't start Ollama: {e}"));
        }
    }

    // Poll for readiness with bounded backoff (~6s total worst case).
    for attempt in 0..12 {
        std::thread::sleep(Duration::from_millis(500));
        if ollama_reachable(base) {
            crate::debug_log::log(
                "ai::ollama",
                "readiness_poll",
                &format!(r#"{{"ready":true,"attempt":{attempt}}}"#),
            );
            return Ok(());
        }
    }

    crate::debug_log::log("ai::ollama", "readiness_poll", r#"{"ready":false}"#);
    Err("Couldn't start Ollama (server did not become ready)".to_string())
}

fn ask_ollama(
    config: &AiConfig,
    history: &[ChatMsg],
    image_b64: Option<&str>,
) -> Result<String, String> {
    let mut cfg = config.clone();
    // Make sure the local server is up before we list models or send the
    // request. This runs on the AI worker thread (see ask_chat callers), never
    // the main thread, and only for an actual submitted query - not while the
    // user is typing or browsing the AI setup page.
    ensure_ollama_running(&ollama_base(&cfg.endpoint))?;
    if cfg.model.is_empty() {
        cfg.model = list_ollama_models(&cfg.endpoint)?
            .into_iter()
            .next()
            .unwrap_or_else(|| "llama3.2".to_string());
    }
    if cfg.endpoint.is_empty() {
        cfg.endpoint = ollama_base("");
    }
    ask_openai(&cfg, None, history, image_b64)
}

fn ask_anthropic(
    config: &AiConfig,
    key: &str,
    history: &[ChatMsg],
    image_b64: Option<&str>,
) -> Result<String, String> {
    let last = history.len() - 1;
    let messages: Vec<Value> = history
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            let mut content = vec![json!({"type": "text", "text": m.content})];
            if i == last {
                if let Some(b64) = image_b64 {
                    content.push(json!({
                        "type": "image",
                        "source": {"type": "base64", "media_type": "image/png", "data": b64}
                    }));
                }
            }
            json!({"role": role, "content": content})
        })
        .collect();
    let body = json!({
        "model": config.model,
        "max_tokens": 1024,
        "messages": messages,
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
    key: Option<&str>,
    history: &[ChatMsg],
    image_b64: Option<&str>,
) -> Result<String, String> {
    let base = if config.provider == "ollama" {
        ollama_base(&config.endpoint)
    } else if config.endpoint.is_empty() {
        "https://api.openai.com".to_string()
    } else {
        config.endpoint.trim_end_matches('/').to_string()
    };
    let url = format!("{base}/v1/chat/completions");

    let last = history.len() - 1;
    let messages: Vec<Value> = history
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            let mut content = vec![json!({"type": "text", "text": m.content})];
            if i == last {
                if let Some(b64) = image_b64 {
                    content.push(json!({
                        "type": "image_url",
                        "image_url": {"url": format!("data:image/png;base64,{b64}")}
                    }));
                }
            }
            json!({"role": role, "content": content})
        })
        .collect();
    let body = json!({
        "model": config.model,
        "messages": messages,
    });

    let mut req = ureq::post(&url).header("content-type", "application/json");
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        req = req.header("authorization", format!("Bearer {key}"));
    }
    let mut resp = req.send_json(body).map_err(|e| e.to_string())?;

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

/// Default Gemini model when none is configured.
const GEMINI_DEFAULT_MODEL: &str = "gemini-2.5-flash";

fn ask_gemini(
    config: &AiConfig,
    key: &str,
    history: &[ChatMsg],
    image_b64: Option<&str>,
) -> Result<String, String> {
    let base = if config.endpoint.is_empty() {
        "https://generativelanguage.googleapis.com".to_string()
    } else {
        config.endpoint.trim_end_matches('/').to_string()
    };
    let model = if config.model.is_empty() {
        GEMINI_DEFAULT_MODEL
    } else {
        config.model.as_str()
    };
    let url = format!("{base}/v1beta/models/{model}:generateContent");

    let last = history.len() - 1;
    let contents: Vec<Value> = history
        .iter()
        .enumerate()
        .map(|(i, m)| {
            // Gemini uses "model" (not "assistant") for the AI role.
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "model",
            };
            let mut parts = vec![json!({"text": m.content})];
            if i == last {
                if let Some(b64) = image_b64 {
                    parts.push(json!({
                        "inline_data": {"mime_type": "image/png", "data": b64}
                    }));
                }
            }
            json!({"role": role, "parts": parts})
        })
        .collect();
    let body = json!({
        "contents": contents,
    });

    // Send the key in a header (not the URL query) so it never lands in logs.
    // Disable status-as-error so we can read Gemini's JSON error body (which
    // carries a human-readable message) instead of just a status code.
    let mut resp = ureq::post(&url)
        .config()
        .http_status_as_error(false)
        .build()
        .header("x-goog-api-key", key)
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|e| e.to_string())?;

    let value: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;

    if let Some(message) = value
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Err(format!("Gemini error: {message}"));
    }

    let candidate = value
        .get("candidates")
        .and_then(|c| c.get(0))
        .ok_or_else(|| {
            // No candidates usually means the prompt or response was blocked.
            match value
                .get("promptFeedback")
                .and_then(|f| f.get("blockReason"))
                .and_then(|r| r.as_str())
            {
                Some(reason) => format!("Gemini returned no answer (blocked: {reason})"),
                None => "Gemini returned no candidates".to_string(),
            }
        })?;

    // Concatenate every text part of the candidate's content.
    let text: String = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    if text.is_empty() {
        // A finishReason without text (e.g. SAFETY, MAX_TOKENS) is more useful
        // than an empty string.
        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
            return Err(format!("Gemini returned no text (finish reason: {reason})"));
        }
        return Err("Unexpected response from Gemini".to_string());
    }
    Ok(text)
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
