use std::path::PathBuf;

use crate::engine::Provider;
use crate::model::{Action, Item};
use crate::paths::support_dir;

/// Quick note capture. `note <text>` appends a timestamped line to a plain-text
/// notes file under the app support dir (the reliable default), and optionally
/// also creates an Apple Notes note when enabled in config. `note` / `notes`
/// with no argument opens the notes file.
pub struct NotesProvider {
    path: PathBuf,
    apple_notes: bool,
}

impl NotesProvider {
    pub fn new(file: &str, apple_notes: bool) -> Self {
        // A relative path is resolved under the support dir; absolute is used as-is.
        let path = if file.is_empty() {
            support_dir().join("notes.txt")
        } else {
            let p = PathBuf::from(file);
            if p.is_absolute() {
                p
            } else {
                support_dir().join(file)
            }
        };
        // Touch the file so "open notes" works even before the first capture.
        if !path.exists() {
            let _ = std::fs::write(&path, "");
        }
        Self { path, apple_notes }
    }
}

impl Provider for NotesProvider {
    fn name(&self) -> &'static str {
        "notes"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();
        let path_str = self.path.to_string_lossy().to_string();

        // Open the notes file.
        if matches!(lower.as_str(), "notes" | "note" | "open notes") {
            out.push(Item::new(
                "Open notes file",
                path_str.clone(),
                "Notes",
                9_000,
                Action::Open(path_str),
            ));
            return;
        }

        // Capture a note (keyword + original-case body).
        if let Some((kw, body)) = q.split_once(char::is_whitespace) {
            if matches!(kw.to_ascii_lowercase().as_str(), "note" | "n") {
                let body = body.trim();
                if !body.is_empty() {
                    out.push(Item::new(
                        format!("Add note: {body}"),
                        if self.apple_notes {
                            "Appends to your notes file and Apple Notes - Enter to save"
                        } else {
                            "Appends a timestamped line to your notes file - Enter to save"
                        },
                        "Notes",
                        9_100,
                        Action::RunShell(self.capture_command(body)),
                    ));
                }
            }
        }
    }
}

impl NotesProvider {
    fn capture_command(&self, body: &str) -> String {
        let path = self.path.to_string_lossy().to_string();
        let mut cmd = format!(
            "printf '[%s] %s\\n' \"$(date '+%Y-%m-%d %H:%M')\" {} >> {}",
            shell_quote(body),
            shell_quote(&path)
        );
        if self.apple_notes {
            let script = format!(
                "tell application \"Notes\" to make new note with properties {{name:{}, body:{}}}",
                applescript_quote(&first_words(body, 6)),
                applescript_quote(body)
            );
            cmd.push_str(&format!("; osascript -e {}", shell_quote(&script)));
        }
        cmd
    }
}

fn first_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
