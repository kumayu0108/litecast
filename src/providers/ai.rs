use crate::ai;
use crate::config::AiConfig;
use crate::engine::Provider;
use crate::model::{Action, Item};
use crate::secrets;

/// Surfaces AI actions. Typing `? <question>` offers to ask the configured
/// backend (the request only fires when you press Enter, never per-keystroke).
/// Typing `setkey <api-key>` stores the key for the active backend.
pub struct AiProvider {
    config: AiConfig,
}

impl AiProvider {
    pub fn new(config: AiConfig) -> Self {
        Self { config }
    }
}

impl Provider for AiProvider {
    fn name(&self) -> &'static str {
        "ai"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let provider = &self.config.provider;
        let q = query.trim();
        let lower = q.to_ascii_lowercase();

        // `setkey <key>` (and the friendlier `set key <key>`) stores a key.
        let key_input = q
            .strip_prefix("setkey ")
            .or_else(|| q.strip_prefix("set key "))
            .map(str::trim);
        if let Some(key) = key_input {
            if key.is_empty() {
                self.push_setup_guide(out);
            } else {
                out.push(Item::new(
                    format!("Save {} API key", friendly_name(provider)),
                    format!("Press Enter to store it securely in the Keychain (service \"litecast\", provider \"{provider}\")"),
                    "AI",
                    11_000,
                    Action::SetApiKey {
                        provider: provider.clone(),
                        key: key.to_string(),
                    },
                ));
            }
            return;
        }

        // `setup` / `setkey` / `api key` open the guided key-setup flow.
        if matches!(lower.as_str(), "setup" | "setkey" | "set key" | "api key" | "apikey") {
            self.push_setup_guide(out);
            return;
        }

        let Some(prompt) = query.strip_prefix('?') else {
            return;
        };
        let prompt = prompt.trim();
        if prompt.is_empty() {
            out.push(Item::new(
                format!("Ask {}...", friendly_name(provider)),
                "Type your question after ? then press Enter",
                "AI",
                10_500,
                Action::None,
            ));
            return;
        }

        // Friendly first-run hint: no key configured but the user is trying to
        // ask. Point them at `setup` and offer to open the key page directly.
        if secrets::needs_api_key(provider, &self.config.endpoint)
            && secrets::get_api_key(provider).is_none()
        {
            out.push(Item::new(
                format!("Set up {} first - no API key yet", friendly_name(provider)),
                "Type `setup` for a guided walkthrough, or Enter to open the key page".to_string(),
                "AI",
                10_600,
                match key_page(provider) {
                    Some(url) => Action::Open(url.to_string()),
                    None => Action::None,
                },
            ));
            return;
        }

        out.push(Item::new(
            format!("Ask {}: {prompt}", friendly_name(provider)),
            "Press Enter to send",
            "AI",
            10_500,
            Action::AskAi {
                prompt: prompt.to_string(),
                image: None,
            },
        ));
    }
}

impl AiProvider {
    /// A short guided key-setup flow shown for `setup` / `setkey` with no key:
    /// where to get a key for the active provider, then how to store it.
    fn push_setup_guide(&self, out: &mut Vec<Item>) {
        let provider = &self.config.provider;
        let name = friendly_name(provider);
        let needs_key = secrets::needs_api_key(provider, &self.config.endpoint);
        let has_key = secrets::get_api_key(provider).is_some();

        let status = if provider == "ollama" {
            match ai::list_ollama_models(&self.config.endpoint) {
                Ok(models) => format!(
                    "Ollama is running. Installed models: {}. No API key needed.",
                    models.join(", ")
                ),
                Err(e) => format!("Ollama: {e}"),
            }
        } else if has_key {
            format!("A key is already saved for {name}. Run `setkey <new-key>` to replace it.")
        } else if needs_key {
            format!("No key saved yet for {name}.")
        } else {
            format!("{name} uses endpoint {} (no Keychain key required).", self.config.endpoint)
        };
        out.push(Item::new(
            format!("AI setup - active provider: {name}"),
            status,
            "AI",
            11_050,
            Action::None,
        ));

        if provider == "ollama" {
            out.push(Item::new(
                "1. Install & start Ollama",
                "Download from ollama.com, then run `ollama serve`",
                "AI",
                11_040,
                Action::Open("https://ollama.com".to_string()),
            ));
            out.push(Item::new(
                "2. Pull a model",
                "Example: `ollama pull llama3.2` or `ollama pull hf.co/<user>/<model>` for Hugging Face",
                "AI",
                11_035,
                Action::None,
            ));
        } else if let Some(url) = key_page(provider) {
            out.push(Item::new(
                format!("1. Get a {name} API key"),
                format!("Press Enter to open {url}"),
                "AI",
                11_040,
                Action::Open(url.to_string()),
            ));
        }
        if needs_key {
            out.push(Item::new(
                "Store API key in litecast",
                "Type `setkey <your-api-key>` then Enter - saved to the macOS Keychain",
                "AI",
                11_030,
                Action::None,
            ));
        }
        out.push(Item::new(
            "3. Ask away",
            "Type `? your question` and press Enter (Esc exits chat)",
            "AI",
            11_020,
            Action::None,
        ));
        out.push(Item::new(
            "Change provider/model",
            "Edit the [ai] section of config.toml (provider/model/endpoint)",
            "AI",
            11_010,
            Action::None,
        ));
    }
}

/// Human-friendly provider name for prompts and setup copy.
fn friendly_name(provider: &str) -> &str {
    match provider {
        "anthropic" => "Anthropic Claude",
        "openai" => "OpenAI",
        "gemini" | "google" => "Google Gemini",
        "ollama" => "Ollama (local)",
        "openai-compatible" | "cursor" => "your OpenAI-compatible endpoint",
        other => other,
    }
}

/// The page where a user can create an API key for the given provider.
fn key_page(provider: &str) -> Option<&'static str> {
    match provider {
        "anthropic" => Some("https://console.anthropic.com/settings/keys"),
        "openai" => Some("https://platform.openai.com/api-keys"),
        "gemini" | "google" => Some("https://aistudio.google.com/app/apikey"),
        _ => None,
    }
}
