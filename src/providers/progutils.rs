use crate::engine::Provider;
use crate::model::{Action, CaptureMode, Item};

/// Programmer utilities: kill process on port, flush DNS cache, toggle hidden
/// files in Finder, restart Finder/Dock. All argv-only (no `sh -c`).
pub struct ProgUtilsProvider;

impl Provider for ProgUtilsProvider {
    fn name(&self) -> &'static str {
        "progutils"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();
        let (kw, arg) = match lower.split_once(char::is_whitespace) {
            Some((k, a)) => (k, a.trim()),
            None => (lower.as_str(), ""),
        };

        match kw {
            "killport" | "portkill" | "kill port" => {
                if let Ok(port) = arg.parse::<u16>() {
                    push_kill_port(out, port);
                } else if arg.is_empty() {
                    out.push(Item::new(
                        "Kill port",
                        "Usage: killport 3000",
                        "Dev",
                        8_200,
                        Action::None,
                    ));
                }
            }
            "flushdns" | "flush dns" | "dns flush" => {
                out.push(Item::new(
                    "Flush DNS cache",
                    "Runs dscacheutil and kills mDNSResponder (may need admin)",
                    "Dev",
                    8_500,
                    Action::RunCapture {
                        program: "/usr/bin/dscacheutil".to_string(),
                        args: vec!["-flushcache".to_string()],
                        mode: CaptureMode::Notify,
                        title: "Flush DNS".to_string(),
                    },
                ));
                out.push(Item::new(
                    "Restart mDNSResponder",
                    "Often required after flushing DNS",
                    "Dev",
                    8_400,
                    Action::Run {
                        program: "/usr/bin/killall".to_string(),
                        args: vec!["-HUP".to_string(), "mDNSResponder".to_string()],
                    },
                ));
            }
            "hidden" | "hiddenfiles" | "show hidden" => {
                out.push(Item::new(
                    "Toggle hidden files in Finder",
                    "Shows or hides dotfiles in Finder windows",
                    "Dev",
                    8_500,
                    Action::Run {
                        program: "/usr/bin/defaults".to_string(),
                        args: vec![
                            "write".to_string(),
                            "com.apple.finder".to_string(),
                            "AppleShowAllFiles".to_string(),
                            "-bool".to_string(),
                            "true".to_string(),
                        ],
                    },
                ));
                out.push(Item::new(
                    "Restart Finder (apply hidden-files toggle)",
                    "Relaunch Finder so the setting takes effect",
                    "Dev",
                    8_400,
                    restart_finder_action(),
                ));
            }
            "restart finder" | "finder restart" => {
                out.push(Item::new(
                    "Restart Finder",
                    "Relaunch Finder",
                    "Dev",
                    8_500,
                    restart_finder_action(),
                ));
            }
            "restart dock" | "dock restart" => {
                out.push(Item::new(
                    "Restart Dock",
                    "Relaunch the Dock",
                    "Dev",
                    8_500,
                    Action::Run {
                        program: "/usr/bin/killall".to_string(),
                        args: vec!["Dock".to_string()],
                    },
                ));
            }
            "dev" | "devutils" if arg.is_empty() => {
                out.push(Item::new(
                    "Developer utilities",
                    "killport, flushdns, hidden files, restart finder/dock",
                    "Dev",
                    8_200,
                    Action::None,
                ));
            }
            _ => {}
        }
    }
}

fn restart_finder_action() -> Action {
    Action::Run {
        program: "/usr/bin/killall".to_string(),
        args: vec!["Finder".to_string()],
    }
}

fn push_kill_port(out: &mut Vec<Item>, port: u16) {
    out.push(Item::new(
        format!("Kill process on port {port}"),
        "SIGTERM to the listener (via lsof + kill, argv-only)",
        "Dev",
        8_600,
        Action::KillPort(port),
    ));
}
