use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::engine::Provider;
use crate::model::{Action, Item};
use crate::paths::support_dir;

const PLUGIN_TIMEOUT: Duration = Duration::from_millis(800);

struct PluginEntry {
    keyword: String,
    path: PathBuf,
}

/// External plugins: executables in `~/Library/Application Support/litecast/plugins/`.
/// A plugin is invoked only when the first word of the query matches its file
/// name (its keyword), so we never spawn processes on every keystroke. The
/// plugin receives the rest of the query as its single argument and prints a
/// JSON document of results to stdout. See docs/plugins.md.
pub struct PluginProvider {
    plugins: Vec<PluginEntry>,
}

impl PluginProvider {
    pub fn new() -> Self {
        Self {
            plugins: discover(),
        }
    }
}

impl Provider for PluginProvider {
    fn name(&self) -> &'static str {
        "plugins"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let mut parts = q.splitn(2, char::is_whitespace);
        let keyword = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        let Some(plugin) = self
            .plugins
            .iter()
            .find(|p| p.keyword.eq_ignore_ascii_case(keyword))
        else {
            return;
        };

        let Some(stdout) = run_plugin(&plugin.path, arg) else {
            return;
        };
        let Ok(parsed) = serde_json::from_str::<PluginOutput>(&stdout) else {
            return;
        };

        for (index, item) in parsed.items.into_iter().enumerate() {
            let action = match item.action.as_str() {
                "shell" => Action::RunShell(item.target.clone()),
                "copy" => Action::CopyText(item.target.clone()),
                "none" => Action::None,
                _ => Action::Open(item.target.clone()),
            };
            out.push(Item::new(
                item.title,
                item.subtitle,
                "Plugin",
                // Keyword-triggered, so rank highly; preserve plugin ordering.
                9_000 - index as i64,
                action,
            ));
        }
    }
}

#[derive(Deserialize)]
struct PluginOutput {
    #[serde(default)]
    items: Vec<PluginItem>,
}

#[derive(Deserialize)]
struct PluginItem {
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default = "default_action")]
    action: String,
    #[serde(default)]
    target: String,
}

fn default_action() -> String {
    "open".to_string()
}

fn discover() -> Vec<PluginEntry> {
    let dir = support_dir().join("plugins");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        // Must have an executable bit set.
        if meta.permissions().mode() & 0o111 == 0 {
            continue;
        }
        if let Some(keyword) = path.file_stem().and_then(|n| n.to_str()) {
            plugins.push(PluginEntry {
                keyword: keyword.to_string(),
                path,
            });
        }
    }
    plugins
}

/// Run a plugin with a timeout, returning its stdout on success.
fn run_plugin(path: &PathBuf, arg: &str) -> Option<String> {
    let mut child = Command::new(path)
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > PLUGIN_TIMEOUT {
                    let _ = child.kill();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }

    let output = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
