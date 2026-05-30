use crate::clipboard::{ClipEntry, ClipKind, History};
use crate::engine::{fuzzy_score, Provider};
use crate::model::{osascript_action_with_args, Action, Item};

/// Clipboard history, surfaced when the query starts with the `clip` keyword
/// (e.g. "clip" to list recent items, or "clip foo" to filter). Pinned entries
/// appear first and persist. `clip pin <n>` / `clip unpin <n>` toggle a pin.
///
/// Enter on a text entry copies it; on a link, opens it; on an image, re-copies
/// the image to the clipboard.
pub struct ClipboardProvider {
    history: History,
}

impl ClipboardProvider {
    pub fn new(history: History) -> Self {
        Self { history }
    }
}

impl Provider for ClipboardProvider {
    fn name(&self) -> &'static str {
        "clipboard"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim_start();
        let Some(rest) = strip_keyword(q) else {
            return;
        };
        let rest = rest.trim();

        // Stable, pinned-first ordering used for both listing and pin numbering.
        let snap = self.history.snapshot();
        let mut ordered: Vec<ClipEntry> = snap.iter().filter(|e| e.pinned).cloned().collect();
        ordered.extend(snap.iter().filter(|e| !e.pinned).cloned());

        // `clip pin <n>` / `clip unpin <n>`: toggle the nth listed entry.
        let mut toks = rest.split_whitespace();
        if let Some(first) = toks.next() {
            if first.eq_ignore_ascii_case("pin") || first.eq_ignore_ascii_case("unpin") {
                match toks.next().and_then(|n| n.parse::<usize>().ok()) {
                    Some(n) if n >= 1 && n <= ordered.len() => {
                        let entry = &ordered[n - 1];
                        let verb = if entry.pinned { "Unpin" } else { "Pin" };
                        out.push(Item::new(
                            format!("{verb} #{n}: {}", preview(&entry.text)),
                            "Press Enter to toggle pin",
                            "Clip",
                            12_000,
                            Action::TogglePin {
                                key: entry.key().to_string(),
                            },
                        ));
                    }
                    _ => {
                        out.push(Item::new(
                            "Usage: clip pin <number>",
                            "Pin a history entry so it stays at the top",
                            "Clip",
                            12_000,
                            Action::None,
                        ));
                    }
                }
                return;
            }
        }

        for (i, entry) in ordered.iter().enumerate() {
            let n = i + 1;
            let score = if rest.is_empty() {
                let base = if entry.pinned { 11_000 } else { 10_000 };
                base - i as i64
            } else {
                match fuzzy_score(rest, &entry.text) {
                    Some(s) => {
                        let base = if entry.pinned { 9_500 } else { 9_000 };
                        base + s as i64
                    }
                    None => continue,
                }
            };
            out.push(make_item(entry, n, score));
        }
    }
}

fn make_item(entry: &ClipEntry, n: usize, score: i64) -> Item {
    let pin_mark = if entry.pinned { "[pin] " } else { "" };
    let title = format!("{pin_mark}{}", preview(&entry.text));
    match entry.kind {
        ClipKind::Link => Item::new(
            title,
            format!("#{n} - Link - Enter to open"),
            "Clip",
            score,
            Action::Open(entry.text.clone()),
        ),
        ClipKind::Image => {
            let path = entry.path.clone().unwrap_or_default();
            let item = Item::new(
                title,
                format!("#{n} - Image - Enter to copy"),
                "Clip",
                score,
                copy_image_action(&path),
            );
            if path.is_empty() {
                item
            } else {
                item.with_icon(path)
            }
        }
        ClipKind::Text => Item::new(
            title,
            format!("#{n} - Enter to copy"),
            "Clip",
            score,
            Action::CopyText(entry.text.clone()),
        ),
    }
}

/// Shell-free action that puts a stored PNG back on the clipboard as image
/// data. The file path is passed as an `on run argv` parameter, so it is never
/// interpreted as AppleScript source (even though it is an app-generated path).
fn copy_image_action(path: &str) -> Action {
    let script = "on run argv\nset the clipboard to (read (POSIX file (item 1 of argv)) as «class PNGf»)\nend run";
    osascript_action_with_args(script, &[path])
}

/// Returns the text after the `clip` keyword if the query uses it.
fn strip_keyword(q: &str) -> Option<&str> {
    let lower = q.to_ascii_lowercase();
    if lower == "clip" {
        Some("")
    } else if let Some(rest) = q.strip_prefix("clip ").or_else(|| q.strip_prefix("Clip ")) {
        Some(rest)
    } else {
        None
    }
}

/// Single-line, length-limited preview of a clipboard entry.
fn preview(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 80 {
        let truncated: String = one_line.chars().take(80).collect();
        format!("{truncated}...")
    } else if one_line.is_empty() {
        text.chars().take(80).collect()
    } else {
        one_line
    }
}
