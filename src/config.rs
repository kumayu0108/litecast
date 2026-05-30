use serde::Deserialize;

use crate::paths::support_file;

const CONFIG_FILE: &str = "config.toml";

const DEFAULT_CONFIG_TOML: &str = r#"# litecast configuration

# Search engine used by the "Search the web" fallback. Use {} for the query.
web_search_url = "https://www.google.com/search?q={}"

# Custom commands. Each appears as a result you can fuzzy-search by name, or
# trigger directly by typing its optional keyword. If `target` contains {} and
# a keyword is set, the text after the keyword is substituted in. Optional
# `alias`/`aliases` are extra search terms folded into name matching, so a short
# token can surface the command (e.g. typing "gh" finds "Open GitHub").
#
# [[commands]]
# name = "Open GitHub"
# keyword = "gh"
# alias = "git"                       # one extra search term
# aliases = ["hub", "repo"]           # or several
# kind = "open"                       # "open" (file/url/app) or "shell"
# target = "https://github.com/{}"

# Quicklinks: parameterized URLs opened in your browser. Type the keyword plus
# an argument (e.g. "ghr rust-lang/rust"); {query} is URL-encoded and
# substituted. Without an argument, the link is fuzzy-matched by name (or by any
# alias).
#
# [[quicklinks]]
# name = "GitHub repo"
# keyword = "ghr"
# alias = "repo"
# url = "https://github.com/{query}"

# Process manager. Type "kill" or "ps" (optionally with a filter, e.g.
# "kill safari") to list your running processes by name, pid, and %CPU. Enter
# asks you to confirm ("Press Enter again to kill <name> (pid)") and then sends
# SIGTERM (graceful). Critical system processes are hidden to avoid foot-guns.
# No configuration or permissions are required; nothing runs until you type the
# keyword.

# Custom global hotkeys. Each binds a key combo to an action, registered system
# wide alongside the built-in Option+Space (toggle) and Option+Shift+Space
# (screenshot) hotkeys. Combo syntax: modifiers + a key joined by "+", e.g.
# "Cmd+Shift+S". Modifiers: Cmd (or Command/Super/Win), Ctrl, Alt (or Option),
# Shift. Keys: letters A-Z, digits 0-9, F1-F12, Space, Enter, Tab, Esc, arrow
# keys (Up/Down/Left/Right), and common punctuation. At least one modifier is
# required. Registration failures (e.g. a combo already taken by another app)
# are logged and ignored.
#
# kind = "open"   -> target is a URL/path/app opened via `open`
# kind = "shell"  -> target is a shell command run via `sh -c`
# kind = "command"-> target is the name of a [[commands]] entry above
#
# [[hotkeys]]
# key = "Cmd+Shift+G"
# kind = "open"
# target = "https://github.com"
#
# [[hotkeys]]
# key = "Ctrl+Alt+T"
# kind = "shell"
# target = "open -a Terminal"

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
# Backend: "anthropic", "openai", "gemini", or "openai-compatible"
# ("cursor" still works as a legacy alias for "openai-compatible").
# For Gemini, use e.g. model = "gemini-2.5-flash" (no endpoint needed).
provider = "anthropic"
model = "claude-3-5-sonnet-latest"
# Optional base-URL override for "openai-compatible"/"gemini" proxies.
endpoint = ""

[ui]
playful_placeholders = true
critters = true

# Clipboard history. Pin entries with "clip pin <number>" so they persist at the
# top. Images copied to the clipboard are captured and stored under the support
# directory; set keep_images = false to disable, or cap how many are kept.
[clipboard]
keep_images = true
max_images = 20

# Window management. This is the ONE litecast feature that needs the macOS
# Accessibility permission, so it is OFF by default. Set enabled = true to show
# window commands (type "win", or search e.g. "Maximize Window"). The first time
# you run a window command, macOS asks you to grant litecast Accessibility access
# in System Settings > Privacy & Security > Accessibility. Nothing here runs (or
# prompts) until you actually trigger a window command.
[window]
enabled = false
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
    pub clipboard: ClipboardConfig,
    pub window: WindowConfig,
    pub hotkeys: Vec<HotkeyConfig>,
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
            clipboard: ClipboardConfig::default(),
            window: WindowConfig::default(),
            hotkeys: Vec::new(),
        }
    }
}

/// A user-defined global hotkey: a key combo (modifiers + key) bound to an
/// action. Registered alongside the built-in toggle/screenshot hotkeys;
/// registration failures are logged and non-fatal.
#[derive(Debug, Clone, Deserialize)]
pub struct HotkeyConfig {
    /// Combo like "Cmd+Shift+S" (modifiers: Cmd/Ctrl/Alt/Shift + a key).
    pub key: String,
    /// "open" (file/url/app), "shell" (run a shell command), or "command"
    /// (run a named `[[commands]]` entry).
    pub kind: String,
    /// The URL/path (open), shell command (shell), or command name (command).
    pub target: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Window management uses the Accessibility permission, so it is opt-in and
    /// OFF by default. When false, the window commands are not registered at all.
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig {
    /// Capture images copied to the clipboard (stored under the support dir).
    pub keep_images: bool,
    /// Max image entries kept in history (pinned images are exempt).
    pub max_images: usize,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            keep_images: true,
            max_images: 20,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuicklinkConfig {
    pub name: String,
    /// Optional keyword that triggers the link with a `{query}` argument.
    #[serde(default)]
    pub keyword: String,
    /// Optional single alias folded into name matching (`alias = "gh"`).
    #[serde(default)]
    pub alias: String,
    /// Optional extra aliases folded into name matching.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// URL template; `{query}` is replaced with the (URL-encoded) argument.
    pub url: String,
}

impl QuicklinkConfig {
    /// All alias strings (singular `alias` plus the `aliases` list), skipping empties.
    pub fn alias_list(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.alias.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .filter(|s| !s.is_empty())
    }
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
    /// Optional single alias folded into name matching (`alias = "ss"`).
    #[serde(default)]
    pub alias: String,
    /// Optional extra aliases folded into name matching.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// "open" (file/url/app) or "shell".
    pub kind: String,
    pub target: String,
}

impl CommandConfig {
    /// All alias strings (singular `alias` plus the `aliases` list), skipping empties.
    pub fn alias_list(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.alias.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .filter(|s| !s.is_empty())
    }
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
