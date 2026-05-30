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

# Quicklinks: parameterized URLs opened in your browser. Type the keyword plus
# an argument (e.g. "ghr rust-lang/rust"); {query} is URL-encoded and
# substituted. Without an argument, the link is fuzzy-matched by name.
#
# [[quicklinks]]
# name = "GitHub repo"
# keyword = "ghr"
# url = "https://github.com/{query}"

# Reusable text snippets. List them with the "snip" keyword (or "snip <filter>"),
# or surface one directly via its own keyword. Enter copies the expanded text to
# the clipboard so you can paste it. Supported placeholders in `text`:
# {date} {time} {clipboard} {cursor}.
#
# [[snippets.entries]]
# keyword = "addr"
# name = "Home address"
# text = "1 Main St, Springfield"
# paste = false

# Unit & currency conversion. Type natural queries like "10 km in mi",
# "100 f to c", or "100 usd to eur". Currency rates are fetched from key-less
# public APIs and cached on disk; this controls how long the cache is reused.
[conversion]
currency_ttl_hours = 12

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
    pub quicklinks: Vec<QuicklinkConfig>,
    pub snippets: SnippetsConfig,
    pub conversion: ConversionConfig,
    pub ai: AiConfig,
    pub ui: UiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            web_search_url: "https://www.google.com/search?q={}".to_string(),
            commands: Vec::new(),
            quicklinks: Vec::new(),
            snippets: SnippetsConfig::default(),
            conversion: ConversionConfig::default(),
            ai: AiConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuicklinkConfig {
    pub name: String,
    /// Optional keyword that triggers the link with a `{query}` argument.
    #[serde(default)]
    pub keyword: String,
    /// URL template; `{query}` is replaced with the (URL-encoded) argument.
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ConversionConfig {
    /// Hours before cached currency rates are considered stale and refreshed.
    pub currency_ttl_hours: u64,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            currency_ttl_hours: 12,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SnippetsConfig {
    pub entries: Vec<SnippetConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnippetConfig {
    /// Optional keyword that surfaces this snippet directly.
    #[serde(default)]
    pub keyword: String,
    /// Display name; falls back to the keyword when empty.
    #[serde(default)]
    pub name: String,
    /// Snippet body. Supports `{date}`, `{time}`, `{clipboard}` placeholders.
    pub text: String,
    /// When true, the entry uses the paste-on-Enter action (copies to the
    /// clipboard). Currently identical to a plain copy (no synthetic paste).
    #[serde(default)]
    pub paste: bool,
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
