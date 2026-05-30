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

        if let Some(key) = query.trim().strip_prefix("setkey ") {
            let key = key.trim();
            if !key.is_empty() {
                out.push(Item::new(
                    format!("Save {provider} API key"),
                    "Press Enter to store it securely in the Keychain",
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

        let Some(prompt) = query.strip_prefix('?') else {
            return;
        };
        let prompt = prompt.trim();
        if prompt.is_empty() {
            out.push(Item::new(
                format!("Ask {provider}..."),
                "Type your question after ? then press Enter",
                "AI",
                10_500,
                Action::None,
            ));
            return;
        }

        if secrets::get_api_key(provider).is_none() {
            out.push(Item::new(
                format!("No API key set for {provider}"),
                "Type: setkey <your-api-key> then Enter".to_string(),
                "AI",
                10_500,
                Action::None,
            ));
            return;
        }

        out.push(Item::new(
            format!("Ask {provider}: {prompt}"),
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
