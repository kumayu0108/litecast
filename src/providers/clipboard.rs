use crate::clipboard::History;
use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// Clipboard history, surfaced when the query starts with the `clip` keyword
/// (e.g. "clip" to list recent items, or "clip foo" to filter). Pressing Enter
/// copies the entry back to the clipboard so it can be pasted anywhere.
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
        let filter = rest.trim();

        for (index, entry) in self.history.snapshot().into_iter().enumerate() {
            let score = if filter.is_empty() {
                // Preserve recency order; newer entries score higher.
                10_000 - index as i64
            } else {
                match fuzzy_score(filter, &entry) {
                    Some(s) => 9_000 + s as i64,
                    None => continue,
                }
            };
            out.push(Item::new(
                preview(&entry),
                "Clipboard history - Enter to copy",
                "Clip",
                score,
                Action::CopyText(entry),
            ));
        }
    }
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
