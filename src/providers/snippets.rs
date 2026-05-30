use crate::config::SnippetConfig;
use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// Reusable text snippets from `[[snippets.entries]]`. List them with the `snip`
/// keyword (`snip addr` to filter) or surface one directly via its own keyword.
/// Enter copies the expanded text to the clipboard.
pub struct SnippetsProvider {
    snippets: Vec<SnippetConfig>,
}

impl SnippetsProvider {
    pub fn new(snippets: Vec<SnippetConfig>) -> Self {
        Self { snippets }
    }
}

impl Provider for SnippetsProvider {
    fn name(&self) -> &'static str {
        "snippets"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }

        // A snippet's own keyword surfaces it directly (highest priority).
        for snip in &self.snippets {
            if !snip.keyword.is_empty() && (q == snip.keyword || q == format!("snip {}", snip.keyword)) {
                out.push(build_item(snip, 8_500));
            }
        }

        // `snip` / `snip <filter>` lists snippets, fuzzy-filtered by label.
        let Some(filter) = strip_keyword(q) else {
            return;
        };
        for snip in &self.snippets {
            let label = display_name(snip);
            let score = if filter.is_empty() {
                400
            } else {
                match fuzzy_score(filter, &label) {
                    Some(s) => 400 + s as i64,
                    None => continue,
                }
            };
            out.push(build_item(snip, score));
        }
    }
}

fn strip_keyword(q: &str) -> Option<&str> {
    if q == "snip" {
        Some("")
    } else {
        q.strip_prefix("snip ").map(|rest| rest.trim())
    }
}

fn display_name(snip: &SnippetConfig) -> String {
    if !snip.name.is_empty() {
        snip.name.clone()
    } else if !snip.keyword.is_empty() {
        snip.keyword.clone()
    } else {
        preview(&snip.text)
    }
}

fn build_item(snip: &SnippetConfig, score: i64) -> Item {
    let action = if snip.paste {
        Action::Paste(snip.text.clone())
    } else {
        Action::CopyText(snip.text.clone())
    };
    let label = display_name(snip);
    let id = if !snip.keyword.is_empty() {
        snip.keyword.clone()
    } else {
        label.clone()
    };
    Item::new(label, preview(&snip.text), "Snippet", score, action).with_id(format!("snip:{id}"))
}

fn preview(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 80 {
        let truncated: String = one_line.chars().take(80).collect();
        format!("{truncated}...")
    } else {
        one_line
    }
}
