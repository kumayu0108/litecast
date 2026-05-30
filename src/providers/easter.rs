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
            "critter" => Some((
                "A critter appears!",
                "Watch the bottom edge.",
            )),
            "do a barrel roll" => Some(("Whee!", "Pretend the window just spun.")),
            "/coffee" | "make coffee" => Some((
                "Brewing... just kidding.",
                "I launch apps, not espresso. Yet.",
            )),
            "meaning of life" => Some(("42.", "You're welcome.")),
            "hello" | "hi" | "hey" => Some(("Hey there.", "What can I launch for you?")),
            "sudo" => Some((
                "Nice try.",
                "You already have my full cooperation.",
            )),
            "rm -rf /" | "rm -rf" => Some((
                "Absolutely not.",
                "We don't do that here.",
            )),
            "ping" => Some(("Pong.", "Latency: basically zero.")),
            "/flip" | "flip a coin" => Some((
                "Heads.",
                "Trust me, I'm a launcher.",
            )),
            "konami" | "up up down down" => Some((
                "30 lives granted.",
                "...metaphorically speaking.",
            )),
            "why" => Some(("Why not?", "Deep questions for a search box.")),
            "/dance" => Some(("\\o/ ... \\o/", "The critter approves.")),
            "thanks" | "thank you" => Some(("Anytime.", "That's what I'm here for.")),
            "open the pod bay doors" => Some((
                "I'm afraid I can't do that.",
                "But I can open almost anything else.",
            )),
            "is it friday" => Some(("It's always launch o'clock.", "Close enough.")),
            "/shrug" => Some(("¯\\_(ツ)_/¯", "Couldn't have said it better.")),
            _ => None,
        };
        if let Some((title, subtitle)) = response {
            out.push(Item::new(title, subtitle, "?", 12_000, Action::None));
        }
    }
}
