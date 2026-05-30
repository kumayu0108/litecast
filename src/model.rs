/// What happens when the user activates (presses Enter on) a result.
#[derive(Clone, Debug)]
pub enum Action {
    /// Open a file, folder, application bundle, or URL via `/usr/bin/open`.
    Open(String),
    /// Run a shell command via `sh -c`.
    RunShell(String),
    /// Copy text to the clipboard.
    CopyText(String),
    /// Send a prompt (optionally about a screenshot) to the AI backend. Handled
    /// specially by the UI (async, keeps the panel open), not via `execute`.
    AskAi { prompt: String, image: Option<String> },
    /// Store an API key for a backend in the Keychain.
    SetApiKey { provider: String, key: String },
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
        }
    }

    /// Attach a filesystem path whose system icon should be shown for this row.
    pub fn with_icon(mut self, path: impl Into<String>) -> Self {
        self.icon_path = Some(path.into());
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
            Action::SetApiKey { provider, key } => {
                crate::secrets::set_api_key(provider, key);
                true
            }
            // Handled by the UI (async); execution here is a no-op that keeps
            // the panel open.
            Action::AskAi { .. } => false,
            Action::None => false,
        }
    }
}
