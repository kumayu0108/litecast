use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};
use crate::providers::emoji_data::EMOJI;

/// Fuzzy emoji + symbol search. Triggered by the `emoji` keyword or a `:` prefix
/// (`emoji fire`, `:fire`). Enter copies the glyph to the clipboard.
pub struct EmojiProvider;

impl Provider for EmojiProvider {
    fn name(&self) -> &'static str {
        "emoji"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let Some(filter) = trigger(query.trim()) else {
            return;
        };

        for (glyph, name, keywords) in EMOJI {
            let score = if filter.is_empty() {
                // No filter yet: show a small, stable starter set.
                7_000
            } else {
                let haystack = format!("{name} {keywords}");
                match fuzzy_score(filter, &haystack) {
                    Some(s) => 7_000 + s as i64,
                    None => continue,
                }
            };
            out.push(Item::new(
                format!("{glyph}  {name}"),
                "Emoji - Enter to copy",
                "Emoji",
                score,
                Action::CopyText((*glyph).to_string()),
            ));
            if filter.is_empty() && out.len() >= 12 {
                break;
            }
        }
    }
}

/// Returns the search text after the `emoji` keyword or a leading `:`.
fn trigger(q: &str) -> Option<&str> {
    if q == "emoji" {
        return Some("");
    }
    if let Some(rest) = q.strip_prefix("emoji ") {
        return Some(rest.trim());
    }
    // `:fire` style. Require a following character so a lone ":" stays inert.
    if let Some(rest) = q.strip_prefix(':') {
        if !rest.is_empty() {
            return Some(rest.trim());
        }
    }
    None
}
