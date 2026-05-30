/// What happens when the user activates (presses Enter on) a result.
#[derive(Clone, Debug)]
pub enum Action {
    /// Open a file, folder, application bundle, or URL via `/usr/bin/open`.
    Open(String),
    /// Run a shell command via `sh -c`.
    RunShell(String),
    /// Copy text to the clipboard.
    CopyText(String),
    /// Expand placeholders in a snippet template, then copy it to the clipboard
    /// so the user can paste it (paste-on-Enter; no Accessibility required).
    Paste(String),
    /// Send a prompt (optionally about a screenshot) to the AI backend. Handled
    /// specially by the UI (async, keeps the panel open), not via `execute`.
    AskAi { prompt: String, image: Option<String> },
    /// Continue the current AI conversation with `prompt`, threading prior turns.
    /// Handled specially by the UI (async, keeps the panel open).
    AskAiFollowup { prompt: String },
    /// Toggle the pinned state of a clipboard entry (identified by its key:
    /// text or image path). Handled by the UI so it can refresh the list.
    TogglePin { key: String },
    /// Store an API key for a backend in the Keychain.
    SetApiKey { provider: String, key: String },
    /// Two-step confirmation wrapper for destructive actions (empty trash,
    /// restart, shut down). Special-cased by the UI: the first Enter arms it,
    /// the second runs `inner`.
    Confirm { label: String, inner: Box<Action> },
    /// Nothing actionable (informational result).
    None,
}

/// A single result row shown in the panel.
#[derive(Clone, Debug)]
pub struct Item {
    pub title: String,
    pub subtitle: String,
    pub action: Action,
    /// Higher is better. Used to rank results across providers.
    pub score: i64,
    /// Human-readable source label (e.g. "App", "File", "Calc").
    pub source: &'static str,
    /// Optional path used to render a system icon to the left of the row.
    pub icon_path: Option<String>,
    /// Stable identity for frecency tracking (e.g. "app:/Applications/Safari.app").
    /// `None` for volatile results (calc, conversions, AI answers) that should
    /// not be learned.
    pub id: Option<String>,
}

impl Item {
    pub fn new(
        title: impl Into<String>,
        subtitle: impl Into<String>,
        source: &'static str,
        score: i64,
        action: Action,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            action,
            score,
            source,
            icon_path: None,
            id: None,
        }
    }

    /// Attach a filesystem path whose system icon should be shown for this row.
    pub fn with_icon(mut self, path: impl Into<String>) -> Self {
        self.icon_path = Some(path.into());
        self
    }

    /// Attach a stable identity so the item participates in frecency ranking.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Action {
    /// Execute the action. Returns true if the panel should close afterwards.
    pub fn execute(&self) -> bool {
        match self {
            Action::Open(target) => {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg(target)
                    .spawn();
                true
            }
            Action::RunShell(cmd) => {
                let _ = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(cmd)
                    .spawn();
                true
            }
            Action::CopyText(text) => {
                crate::clipboard::set_clipboard(text);
                true
            }
            Action::Paste(template) => {
                crate::clipboard::set_clipboard(&expand_placeholders(template));
                true
            }
            Action::SetApiKey { provider, key } => {
                crate::secrets::set_api_key(provider, key);
                true
            }
            // Handled by the UI (async); execution here is a no-op that keeps
            // the panel open.
            Action::AskAi { .. } => false,
            Action::AskAiFollowup { .. } => false,
            // Handled by the UI; never executed directly.
            Action::TogglePin { .. } => false,
            // Handled by the UI's two-step confirm flow; never executed directly.
            Action::Confirm { .. } => false,
            Action::None => false,
        }
    }
}

/// Expand snippet placeholders at activation time: `{date}`, `{time}`,
/// `{clipboard}`, and `{cursor}` (removed). Subprocesses run only when the
/// matching placeholder is present, and only on Enter (never per keystroke).
fn expand_placeholders(template: &str) -> String {
    let mut text = template.to_string();
    if text.contains("{date}") {
        text = text.replace("{date}", &shell_capture("date +%Y-%m-%d"));
    }
    if text.contains("{time}") {
        text = text.replace("{time}", &shell_capture("date +%H:%M"));
    }
    if text.contains("{clipboard}") {
        text = text.replace("{clipboard}", &shell_capture("pbpaste"));
    }
    text.replace("{cursor}", "")
}

fn shell_capture(cmd: &str) -> String {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        .unwrap_or_default()
}
