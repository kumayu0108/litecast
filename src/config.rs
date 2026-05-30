use serde::Deserialize;

use crate::paths::support_file;

const CONFIG_FILE: &str = "config.toml";

const DEFAULT_CONFIG_TOML: &str = r#"# litecast configuration

# Search engine used by the "Search the web" fallback. Use {} for the query.
web_search_url = "https://www.google.com/search?q={}"

# Custom commands. Each appears as a result you can fuzzy-search by name, or
# trigger directly by typing its optional keyword. If `target` contains {} and
# a keyword is set, the text after the keyword is substituted in.
#
# [[commands]]
# name = "Open GitHub"
# keyword = "gh"
# kind = "open"                       # "open" (file/url/app) or "shell"
# target = "https://github.com/{}"

[ai]
# Backend: "anthropic", "openai", or "cursor" (OpenAI-compatible endpoint).
provider = "anthropic"
model = "claude-3-5-sonnet-latest"
# Optional override for OpenAI-compatible endpoints (used by "cursor"/custom).
endpoint = ""

[ui]
playful_placeholders = true
critters = true
"#;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub web_search_url: String,
    pub commands: Vec<CommandConfig>,
    pub ai: AiConfig,
    pub ui: UiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            web_search_url: "https://www.google.com/search?q={}".to_string(),
            commands: Vec::new(),
            ai: AiConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandConfig {
    pub name: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub keyword: String,
    /// "open" (file/url/app) or "shell".
    pub kind: String,
    pub target: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-3-5-sonnet-latest".to_string(),
            endpoint: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub playful_placeholders: bool,
    pub critters: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            playful_placeholders: true,
            critters: true,
        }
    }
}

/// Load the config from disk, writing a commented default on first run.
pub fn load() -> Config {
    let path = support_file(CONFIG_FILE);
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => {
            let _ = std::fs::write(&path, DEFAULT_CONFIG_TOML);
            Config::default()
        }
    }
}
