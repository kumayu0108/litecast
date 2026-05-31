use crate::color_pick::{format_color_detail, load_recent};
use crate::config::ColorConfig;
use crate::engine::Provider;
use crate::model::{Action, Item};

/// Screen color picker and recent palette (`pick color`, `colors`).
pub struct ColorProvider {
    config: ColorConfig,
}

impl ColorProvider {
    pub fn new(config: ColorConfig) -> Self {
        Self { config }
    }
}

impl Provider for ColorProvider {
    fn name(&self) -> &'static str {
        "color"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();

        if lower == "colors" || lower == "palette" || lower.starts_with("colors ") {
            for hex in load_recent(self.config.max_recent) {
                out.push(Item::new(
                    hex.clone(),
                    format!("{} - Enter to copy", format_color_detail(&hex)),
                    "Color",
                    8_300,
                    Action::CopyText(hex),
                ));
            }
            if out.is_empty() {
                out.push(Item::new(
                    "No recent colors",
                    "Use `pick color` to sample from the screen",
                    "Color",
                    8_200,
                    Action::None,
                ));
            }
            return;
        }

        if lower == "pick color"
            || lower == "color pick"
            || lower == "pick"
            || lower.starts_with("pick color")
            || lower.starts_with("color pick")
        {
            out.push(Item::new(
                "Pick color from screen",
                "Click anywhere to sample a pixel (panel hides first)",
                "Color",
                8_800,
                Action::PickColor,
            ));
        }
    }
}
