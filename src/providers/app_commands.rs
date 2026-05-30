use crate::config::AppCommandConfig;
use crate::engine::Provider;
use crate::model::{Action, Item};
use crate::providers::websearch::percent_encode;

/// `@keyword`-triggered app commands (e.g. `@term ls`, `@finder ~/Downloads`).
/// These are namespaced under `@` (which is NOT a category-filter token, so the
/// raw `@keyword arg` query reaches this provider untouched) and run a templated
/// shell/AppleScript/open action with the typed argument.
pub struct AppCommandsProvider {
    commands: Vec<AppCommandConfig>,
    web_search_url: String,
}

impl AppCommandsProvider {
    pub fn new(commands: Vec<AppCommandConfig>, web_search_url: String) -> Self {
        Self {
            commands,
            web_search_url,
        }
    }
}

impl Provider for AppCommandsProvider {
    fn name(&self) -> &'static str {
        "app_commands"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        let Some(rest) = q.strip_prefix('@') else {
            return;
        };
        let (token, arg) = match rest.split_once(char::is_whitespace) {
            Some((t, a)) => (t, a.trim()),
            None => (rest, ""),
        };
        let token = token.to_ascii_lowercase();
        for cmd in &self.commands {
            if cmd.keyword.eq_ignore_ascii_case(&token) {
                out.push(self.build_item(cmd, arg));
            }
        }
    }
}

impl AppCommandsProvider {
    fn build_item(&self, cmd: &AppCommandConfig, arg: &str) -> Item {
        let name = if cmd.name.is_empty() {
            format!("@{}", cmd.keyword)
        } else {
            cmd.name.clone()
        };
        match cmd.kind.as_str() {
            "terminal" => {
                let (title, subtitle, action) = if arg.is_empty() {
                    (
                        "Open Terminal".to_string(),
                        "Press Enter to open Terminal.app".to_string(),
                        Action::RunShell("open -a Terminal".to_string()),
                    )
                } else {
                    (
                        format!("Run in Terminal: {arg}"),
                        "Opens Terminal.app and runs the command".to_string(),
                        Action::RunShell(terminal_script(arg)),
                    )
                };
                Item::new(title, subtitle, "Command", 9_000, action)
            }
            "finder" => {
                let path = if arg.is_empty() { "~" } else { arg };
                let expanded = expand_tilde(path);
                Item::new(
                    format!("Reveal in Finder: {path}"),
                    "Opens the path in Finder".to_string(),
                    "Command",
                    9_000,
                    Action::Open(expanded),
                )
            }
            "safari" => {
                let (title, target) = if arg.is_empty() {
                    ("Open browser".to_string(), "https://".to_string())
                } else if looks_like_url(arg) {
                    let url = normalize_url(arg);
                    (format!("Open {url}"), url)
                } else {
                    let url = self.web_search_url.replace("{}", &percent_encode(arg));
                    (format!("Search the web for \"{arg}\""), url)
                };
                Item::new(
                    title,
                    "Opens in your default browser".to_string(),
                    "Command",
                    9_000,
                    Action::Open(target),
                )
            }
            other => {
                let filled = fill_template(&cmd.template, arg);
                let action = match other {
                    "open" => Action::Open(filled.clone()),
                    "applescript" => Action::RunShell(format!(
                        "osascript -e {}",
                        shell_quote(&filled)
                    )),
                    // "shell" and anything unrecognized run via the shell.
                    _ => Action::RunShell(filled.clone()),
                };
                let subtitle = if cmd.subtitle.is_empty() {
                    filled
                } else {
                    cmd.subtitle.clone()
                };
                Item::new(name, subtitle, "Command", 9_000, action)
            }
        }
    }
}

/// Substitute `{query}` / `{arg}` in a template with the typed argument.
fn fill_template(template: &str, arg: &str) -> String {
    template.replace("{query}", arg).replace("{arg}", arg)
}

/// Build an osascript invocation that opens Terminal.app and runs `cmd`.
fn terminal_script(cmd: &str) -> String {
    let script = format!(
        "tell application \"Terminal\"\nactivate\ndo script {}\nend tell",
        applescript_quote(cmd)
    );
    format!("osascript -e {}", shell_quote(&script))
}

/// Quote a string as a single-quoted shell argument (safe against embedded `'`).
fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Quote a string as an AppleScript string literal.
fn applescript_quote(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{rest}", home.to_string_lossy());
        }
    }
    if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return home.to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// Heuristic: does `arg` look like a URL/domain rather than a search phrase?
fn looks_like_url(arg: &str) -> bool {
    if arg.contains(char::is_whitespace) {
        return false;
    }
    arg.contains("://") || (arg.contains('.') && !arg.starts_with('.') && !arg.ends_with('.'))
}

/// Prepend `https://` when no scheme is present.
fn normalize_url(arg: &str) -> String {
    if arg.contains("://") {
        arg.to_string()
    } else {
        format!("https://{arg}")
    }
}
