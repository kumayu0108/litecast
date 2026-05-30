use crate::engine::Provider;
use crate::model::{Action, Item};

/// Hidden fun responses. Cheap (just a few string comparisons) and entirely
/// optional in spirit.
pub struct EasterEggProvider;

impl Provider for EasterEggProvider {
    fn name(&self) -> &'static str {
        "easter"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim().to_ascii_lowercase();
        let response = match q.as_str() {
            "/party" | "party" => Some((
                "It is now a party.",
                "Confetti is in the mail. Probably.",
            )),
            "litecast" => Some((
                "Built in Rust, light as a feather.",
                "You typed my name. I'm flattered.",
            )),
            "do a barrel roll" => Some(("Whee!", "Pretend the window just spun.")),
            "/coffee" | "make coffee" => Some((
                "Brewing... just kidding.",
                "I launch apps, not espresso. Yet.",
            )),
            "meaning of life" => Some(("42.", "You're welcome.")),
            _ => None,
        };
        if let Some((title, subtitle)) = response {
            out.push(Item::new(title, subtitle, "?", 12_000, Action::None));
        }
    }
}
