use crate::config::MenuConfig;
use crate::engine::{fuzzy_score, Provider};
use crate::menu_cache;
use crate::model::{Action, Item};
use crate::target_pid;

/// Menu-bar search for the target app (Accessibility, opt-in via `[menu] enabled`).
/// Menu items are scanned on the main thread when the panel opens; this provider
/// only reads that cache.
pub struct MenuProvider {
    enabled: bool,
}

impl MenuProvider {
    pub fn new(config: MenuConfig) -> Self {
        Self {
            enabled: config.enabled,
        }
    }
}

impl Provider for MenuProvider {
    fn name(&self) -> &'static str {
        "menu"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        if !self.enabled {
            return;
        }
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let (kw, arg) = match q.split_once(char::is_whitespace) {
            Some((k, a)) => (k.to_ascii_lowercase(), a.trim()),
            None => (q.to_ascii_lowercase(), ""),
        };
        if kw != "menu" && kw != "menubar" {
            return;
        }

        let pid = target_pid::get();
        if pid <= 0 {
            out.push(Item::new(
                "Menu search",
                "Open the panel over an app first so litecast knows which menu bar to read",
                "Menu",
                8_200,
                Action::None,
            ));
            return;
        }

        let entries = menu_cache::snapshot(pid);
        if entries.is_empty() {
            out.push(Item::new(
                "No menu items found",
                "Grant Accessibility for litecast, then try again",
                "Menu",
                8_200,
                Action::Open(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
                        .to_string(),
                ),
            ));
            return;
        }

        for entry in entries {
            let label = entry.path.join(" › ");
            let score = if arg.is_empty() {
                8_200
            } else {
                match fuzzy_score(arg, &label) {
                    Some(s) => 8_000 + s as i64,
                    None => continue,
                }
            };
            out.push(
                Item::new(
                    label.clone(),
                    "Enter to trigger this menu item",
                    "Menu",
                    score,
                    Action::MenuPick {
                        pid,
                        path: entry.path.clone(),
                    },
                )
                .with_id(format!("menu:{}", entry.path.join("/"))),
            );
        }
    }
}
