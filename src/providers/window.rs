use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item, WindowOp};

/// Window-management commands (move/resize the frontmost window). Registered
/// only when `[window] enabled = true`. Listing never touches Accessibility;
/// the AX calls (and the permission prompt) happen on activation in the UI.
pub struct WindowProvider;

/// (op, display name, short phrase) — the name shows in the row and is
/// fuzzy-matched; the phrase is a natural keyboard-style trigger
/// (`left half`, `maximize`, `center`, `move to display`) matched
/// typo-tolerantly so the ops are discoverable without the `win` prefix.
const OPS: &[(WindowOp, &str, &str)] = &[
    (WindowOp::LeftHalf, "Window: Left Half", "left half"),
    (WindowOp::RightHalf, "Window: Right Half", "right half"),
    (WindowOp::TopHalf, "Window: Top Half", "top half"),
    (WindowOp::BottomHalf, "Window: Bottom Half", "bottom half"),
    (WindowOp::TopLeft, "Window: Top-Left Quarter", "top left"),
    (WindowOp::TopRight, "Window: Top-Right Quarter", "top right"),
    (WindowOp::BottomLeft, "Window: Bottom-Left Quarter", "bottom left"),
    (WindowOp::BottomRight, "Window: Bottom-Right Quarter", "bottom right"),
    (WindowOp::LeftThird, "Window: Left Third", "left third"),
    (WindowOp::CenterThird, "Window: Center Third", "center third"),
    (WindowOp::RightThird, "Window: Right Third", "right third"),
    (WindowOp::CenterTwoThirds, "Window: Center Two-Thirds", "center two thirds"),
    (WindowOp::Maximize, "Maximize Window", "maximize"),
    (WindowOp::Center, "Center Window", "center"),
    (WindowOp::NextDisplay, "Window: Next Display", "move to display"),
    (WindowOp::PrevDisplay, "Window: Previous Display", "move to previous display"),
];

impl WindowProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for WindowProvider {
    fn name(&self) -> &'static str {
        "window"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let q_lower = q.to_ascii_lowercase();
        // Keyword "win" (or "window") lists/filters the ops at a high score.
        let keyword_arg = match_keyword(q, "win").or_else(|| match_keyword(q, "window"));
        for &(op, name, phrase) in OPS {
            if let Some(arg) = keyword_arg {
                if arg.is_empty() || fuzzy_score(arg, name).is_some() || fuzzy_score(arg, phrase).is_some() {
                    out.push(build(op, name, 8_400));
                }
            } else if phrase_matches(&q_lower, phrase) {
                // Natural keyboard-style trigger (typo-tolerant), no prefix.
                out.push(build(op, name, 8_300));
            } else if let Some(score) = fuzzy_score(q, name) {
                out.push(build(op, name, 200 + score as i64));
            }
        }
    }
}

/// Does `q` match the op's natural phrase exactly or within a small typo
/// tolerance? Compared whole (so "left half" triggers, but stray text does not).
fn phrase_matches(q: &str, phrase: &str) -> bool {
    crate::engine::keyword_matches(q, phrase)
}

fn match_keyword<'a>(q: &'a str, keyword: &str) -> Option<&'a str> {
    if q == keyword {
        return Some("");
    }
    q.strip_prefix(&format!("{keyword} ")).map(|rest| rest.trim())
}

fn build(op: WindowOp, name: &str, score: i64) -> Item {
    Item::new(
        name.to_string(),
        "Move/resize the frontmost window".to_string(),
        "Window",
        score,
        Action::Window(op),
    )
    .with_id(format!("win:{name}"))
}
