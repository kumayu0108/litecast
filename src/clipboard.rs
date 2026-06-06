use std::collections::VecDeque;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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

/// The nature of a clipboard entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ClipKind {
    Text,
    Link,
    Image,
}

/// A single clipboard history entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipEntry {
    pub kind: ClipKind,
    /// Text/link content, or a short label for images.
    pub text: String,
    /// Path to the stored PNG for image entries.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub ts: u64,
}

impl ClipEntry {
    /// Stable key used to identify an entry for pin toggling and dedup.
    pub fn key(&self) -> &str {
        self.path.as_deref().unwrap_or(&self.text)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn detect_kind(text: &str) -> ClipKind {
    let t = text.trim();
    let is_url = (t.starts_with("http://") || t.starts_with("https://"))
        && !t.chars().any(|c| c.is_whitespace());
    if is_url {
        ClipKind::Link
    } else {
        ClipKind::Text
    }
}

/// Thread-safe clipboard history shared between the watcher (main thread) and
/// the clipboard provider (worker thread). Pinned entries persist at the top and
/// are exempt from ring-buffer eviction.
#[derive(Clone)]
pub struct History {
    inner: Arc<Mutex<VecDeque<ClipEntry>>>,
    /// Max unpinned text/link entries kept.
    cap: usize,
    /// Max image entries kept (pinned images exempt).
    image_cap: usize,
    /// When true, skip recording clipboard text that looks like a secret.
    skip_secrets: bool,
}

impl History {
    pub fn new(cap: usize, image_cap: usize, skip_secrets: bool) -> Self {
        let items = load(cap);
        Self {
            inner: Arc::new(Mutex::new(items)),
            cap,
            image_cap,
            skip_secrets,
        }
    }

    /// Record a freshly copied text value. No-ops on empty/duplicate-of-most-recent.
    pub fn record(&self, text: String) {
        if text.is_empty() {
            return;
        }
        if self.skip_secrets && looks_like_secret(&text) {
            return;
        }
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if guard.front().map(|e| e.text.as_str()) == Some(text.as_str()) {
            return;
        }
        // Move an existing identical entry to the front (preserving its pin).
        let existing_pin = guard
            .iter()
            .position(|e| e.text == text && e.kind != ClipKind::Image)
            .map(|pos| guard.remove(pos).map(|e| e.pinned).unwrap_or(false))
            .unwrap_or(false);
        guard.push_front(ClipEntry {
            kind: detect_kind(&text),
            text,
            path: None,
            pinned: existing_pin,
            ts: now(),
        });
        self.evict(&mut guard);
        self.persist(&guard);
    }

    /// Record a freshly captured image stored at `path`.
    pub fn record_image(&self, path: String) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if guard
            .front()
            .and_then(|e| e.path.as_deref())
            == Some(path.as_str())
        {
            return;
        }
        let label = path
            .rsplit('/')
            .next()
            .map(|n| format!("Image ({n})"))
            .unwrap_or_else(|| "Image".to_string());
        guard.push_front(ClipEntry {
            kind: ClipKind::Image,
            text: label,
            path: Some(path),
            pinned: false,
            ts: now(),
        });
        self.evict(&mut guard);
        self.persist(&guard);
    }

    /// Toggle the pinned state of the entry identified by `key` (text or path).
    pub fn toggle_pin(&self, key: &str) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(entry) = guard.iter_mut().find(|e| e.key() == key) {
            entry.pinned = !entry.pinned;
        }
        self.persist(&guard);
    }

    pub fn snapshot(&self) -> Vec<ClipEntry> {
        self.inner
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Drop oldest unpinned entries beyond the caps. Deletes evicted image files.
    fn evict(&self, guard: &mut VecDeque<ClipEntry>) {
        // Text/link cap.
        loop {
            let unpinned_text = guard
                .iter()
                .filter(|e| !e.pinned && e.kind != ClipKind::Image)
                .count();
            if unpinned_text <= self.cap {
                break;
            }
            if let Some(pos) = guard
                .iter()
                .rposition(|e| !e.pinned && e.kind != ClipKind::Image)
            {
                guard.remove(pos);
            } else {
                break;
            }
        }
        // Image cap (delete the backing file on eviction).
        loop {
            let unpinned_images = guard
                .iter()
                .filter(|e| !e.pinned && e.kind == ClipKind::Image)
                .count();
            if unpinned_images <= self.image_cap {
                break;
            }
            if let Some(pos) = guard
                .iter()
                .rposition(|e| !e.pinned && e.kind == ClipKind::Image)
            {
                if let Some(removed) = guard.remove(pos) {
                    if let Some(p) = removed.path {
                        let _ = std::fs::remove_file(p);
                    }
                }
            } else {
                break;
            }
        }
    }

    fn persist(&self, guard: &VecDeque<ClipEntry>) {
        let snapshot: Vec<ClipEntry> = guard.iter().cloned().collect();
        if let Ok(json) = serde_json::to_string(&snapshot) {
            let _ = std::fs::write(support_file(HISTORY_FILE), json);
        }
    }
}

/// Load history from disk, migrating the legacy `Vec<String>` format if found.
fn load(cap: usize) -> VecDeque<ClipEntry> {
    let mut items = VecDeque::new();
    let Ok(data) = std::fs::read_to_string(support_file(HISTORY_FILE)) else {
        return items;
    };
    if let Ok(loaded) = serde_json::from_str::<Vec<ClipEntry>>(&data) {
        for entry in loaded {
            items.push_back(entry);
        }
    } else if let Ok(legacy) = serde_json::from_str::<Vec<String>>(&data) {
        // Migrate: wrap each old string as an (unpinned) text/link entry.
        for text in legacy.into_iter().take(cap) {
            items.push_back(ClipEntry {
                kind: detect_kind(&text),
                text,
                path: None,
                pinned: false,
                ts: 0,
            });
        }
    }
    items
}

fn looks_like_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let secret_markers = [
        "api_key",
        "api-key",
        "apikey",
        "secret",
        "password",
        "token",
        "bearer ",
    ];
    if secret_markers.iter().any(|marker| lower.contains(marker)) {
        return true;
    }
    let trimmed = text.trim();
    if trimmed.starts_with("sk-")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("AKIA")
    {
        return true;
    }
    if trimmed.len() > 40 && trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_secret_detects_api_key_line() {
        assert!(looks_like_secret("export API_KEY=abc123"));
    }

    #[test]
    fn looks_like_secret_detects_sk_prefix() {
        assert!(looks_like_secret("sk-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn looks_like_secret_allows_normal_text() {
        assert!(!looks_like_secret("hello world"));
    }
}
