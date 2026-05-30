use std::collections::VecDeque;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::paths::support_file;

const HISTORY_FILE: &str = "clipboard.json";

/// Write text to the system clipboard via `pbcopy`.
pub fn set_clipboard(text: &str) {
    if let Ok(mut child) = Command::new("/usr/bin/pbcopy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Thread-safe, capped clipboard history shared between the watcher (main
/// thread) and the clipboard provider (worker thread).
#[derive(Clone)]
pub struct History {
    inner: Arc<Mutex<VecDeque<String>>>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        let mut items = VecDeque::new();
        if let Ok(data) = std::fs::read_to_string(support_file(HISTORY_FILE)) {
            if let Ok(loaded) = serde_json::from_str::<Vec<String>>(&data) {
                for entry in loaded.into_iter().take(cap) {
                    items.push_back(entry);
                }
            }
        }
        Self {
            inner: Arc::new(Mutex::new(items)),
            cap,
        }
    }

    /// Record a freshly copied value. No-ops on empty/duplicate-of-most-recent.
    pub fn record(&self, text: String) {
        if text.is_empty() {
            return;
        }
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if guard.front().map(|s| s.as_str()) == Some(text.as_str()) {
            return;
        }
        // Move an existing identical entry to the front instead of duplicating.
        if let Some(pos) = guard.iter().position(|s| s == &text) {
            guard.remove(pos);
        }
        guard.push_front(text);
        while guard.len() > self.cap {
            guard.pop_back();
        }
        let snapshot: Vec<String> = guard.iter().cloned().collect();
        drop(guard);
        self.save(&snapshot);
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn save(&self, snapshot: &[String]) {
        if let Ok(json) = serde_json::to_string(snapshot) {
            let _ = std::fs::write(support_file(HISTORY_FILE), json);
        }
    }
}
