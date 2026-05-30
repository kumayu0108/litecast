/// A window-management operation against the frontmost app's focused window.
/// Pure data; the actual Accessibility calls live in `crate::window` and run on
/// the main thread when the item is activated.
#[derive(Clone, Copy, Debug)]
pub enum WindowOp {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    LeftThird,
    RightThird,
    CenterTwoThirds,
    Maximize,
    Center,
    NextDisplay,
    PrevDisplay,
}

/// What happens when the user activates (presses Enter on) a result.
#[derive(Clone, Debug)]
pub enum Action {
    /// Open a file, folder, application bundle, or URL via `/usr/bin/open`.
    Open(String),
    /// Run a program directly with an argv array, WITHOUT a shell. This is the
    /// injection-safe way to run a command that includes user-derived text:
    /// arguments are passed verbatim to `execve`, so there is no shell word
    /// splitting, globbing, quoting, or metacharacter interpretation. Prefer
    /// this over `RunShell` for anything built from user input.
    Run { program: String, args: Vec<String> },
    /// Run a shell command via `sh -c`. Reserved for commands that genuinely
    /// need shell features (pipes, conditionals) AND contain no untrusted input,
    /// or for explicitly user-authored shell config (custom `kind = "shell"`
    /// commands/hotkeys/plugins). Never build this from runtime user text.
    RunShell(String),
    /// Append a timestamped line to a notes file (pure Rust, no shell), and
    /// optionally mirror it into Apple Notes via AppleScript passed as argv.
    AppendNote {
        path: String,
        text: String,
        apple_notes: bool,
    },
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
    /// Re-open the last AI interaction from the recents view, restoring its
    /// transcript and re-entering follow-up chat. Handled specially by the UI.
    ResumeAi,
    /// Accept an `@shortcut` autocomplete suggestion: complete the `@token` in
    /// the search field. Handled specially by the UI (keeps the panel open).
    Autocomplete { token: String },
    /// Move/resize the frontmost app's focused window. Handled specially by the
    /// UI (main thread + Accessibility), like AI actions.
    Window(WindowOp),
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
    /// When true, this row is rendered as a single rounded "answer card" (sized
    /// to its wrapped text height) rather than a fixed-height launcher row.
    /// Used for AI answers so a long reply reads as one polished card.
    pub multiline: bool,
    /// Precomputed pixel height for a multiline answer card, measured on the
    /// main thread from the wrapped text. `None` for normal rows.
    pub block_height: Option<f64>,
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
            multiline: false,
            block_height: None,
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
            Action::Run { program, args } => {
                let _ = std::process::Command::new(program).args(args).spawn();
                true
            }
            Action::RunShell(cmd) => {
                let _ = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(cmd)
                    .spawn();
                true
            }
            Action::AppendNote {
                path,
                text,
                apple_notes,
            } => {
                append_note_line(path, text);
                if *apple_notes {
                    // Pass the note title/body as argv parameters so no user
                    // text is interpolated into the AppleScript source.
                    let title = text.split_whitespace().take(6).collect::<Vec<_>>().join(" ");
                    let script = "on run argv\ntell application \"Notes\" to make new note with properties {name:(item 1 of argv), body:(item 2 of argv)}\nend run";
                    let _ = std::process::Command::new("/usr/bin/osascript")
                        .args(["-e", script, "--", &title, text])
                        .spawn();
                }
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
            Action::ResumeAi => false,
            Action::Autocomplete { .. } => false,
            // Handled by the UI (main thread + Accessibility); never run here.
            Action::Window(_) => false,
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

/// Append a `[YYYY-MM-DD HH:MM] <text>` line to a notes file, creating it if
/// needed. Done in pure Rust (no shell) so the note body can never be
/// interpreted as a command.
fn append_note_line(path: &str, text: &str) {
    use std::io::Write;
    let stamp = std::process::Command::new("/bin/date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        .unwrap_or_default();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[{stamp}] {text}");
    }
}

/// Build a shell-free `Action::Run` that invokes `osascript` with a literal
/// `script`. Use this for AppleScript that contains NO user-derived text.
pub fn osascript_action(script: impl Into<String>) -> Action {
    Action::Run {
        program: "/usr/bin/osascript".to_string(),
        args: vec!["-e".to_string(), script.into()],
    }
}

/// Build a shell-free `Action::Run` that invokes `osascript`, passing each
/// entry of `user_args` as an `on run argv` parameter (referenced inside the
/// script as `item 1 of argv`, `item 2 of argv`, ...). User text is delivered
/// verbatim via argv, so it is never parsed as AppleScript source — the
/// injection-safe way to script with user input.
pub fn osascript_action_with_args(script: impl Into<String>, user_args: &[&str]) -> Action {
    let mut args = vec!["-e".to_string(), script.into(), "--".to_string()];
    args.extend(user_args.iter().map(|s| s.to_string()));
    Action::Run {
        program: "/usr/bin/osascript".to_string(),
        args,
    }
}
