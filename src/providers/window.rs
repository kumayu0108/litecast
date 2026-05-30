use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item, WindowOp};

/// Window-management commands (move/resize the frontmost window). Registered
/// only when `[window] enabled = true`. Listing never touches Accessibility;
/// the AX calls (and the permission prompt) happen on activation in the UI.
pub struct WindowProvider;

/// (op, display name) — the name is what shows in the row and is fuzzy-matched.
const OPS: &[(WindowOp, &str)] = &[
    (WindowOp::LeftHalf, "Window: Left Half"),
    (WindowOp::RightHalf, "Window: Right Half"),
    (WindowOp::TopHalf, "Window: Top Half"),
    (WindowOp::BottomHalf, "Window: Bottom Half"),
    (WindowOp::LeftThird, "Window: Left Third"),
    (WindowOp::RightThird, "Window: Right Third"),
    (WindowOp::CenterTwoThirds, "Window: Center Two-Thirds"),
    (WindowOp::Maximize, "Maximize Window"),
    (WindowOp::Center, "Center Window"),
    (WindowOp::NextDisplay, "Window: Next Display"),
    (WindowOp::PrevDisplay, "Window: Previous Display"),
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
        // Keyword "win" (or "window") lists/filters the ops at a high score.
        let keyword_arg = match_keyword(q, "win").or_else(|| match_keyword(q, "window"));
        for &(op, name) in OPS {
            if let Some(arg) = keyword_arg {
                if arg.is_empty() || fuzzy_score(arg, name).is_some() {
                    out.push(build(op, name, 8_400));
                }
            } else if let Some(score) = fuzzy_score(q, name) {
                out.push(build(op, name, 200 + score as i64));
            }
        }
    }
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
