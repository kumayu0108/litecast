/// What happens when the user activates (presses Enter on) a result.
#[derive(Clone, Debug)]
pub enum Action {
    /// Open a file, folder, application bundle, or URL via `/usr/bin/open`.
    Open(String),
    /// Run a shell command via `sh -c`.
    RunShell(String),
    /// Copy text to the clipboard.
    CopyText(String),
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
        }
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
            Action::None => false,
        }
    }
}
