use std::io::Write;
use std::process::{Command, Stdio};

/// Write text to the system clipboard via `pbcopy`.
pub fn set_clipboard(text: &str) {
    if let Ok(mut child) = Command::new("/usr/bin/pbcopy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}
