use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::PomodoroConfig;
use crate::engine::Provider;
use crate::model::{notify, Action, Item};
use crate::paths::support_file;

/// Pomodoro / focus timer with detached countdown and notifications.
pub struct PomodoroProvider {
    config: PomodoroConfig,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct PomodoroState {
    phase: String,
    ends_at: u64,
    work_minutes: u64,
    break_minutes: u64,
    long_break_minutes: u64,
    cycles: u64,
    cycle_index: u64,
}

impl PomodoroProvider {
    pub fn new(config: PomodoroConfig) -> Self {
        Self { config }
    }

    fn state_path() -> std::path::PathBuf {
        support_file("pomodoro.json")
    }

    fn load() -> Option<PomodoroState> {
        let text = fs::read_to_string(Self::state_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save(state: &PomodoroState) {
        if let Ok(json) = serde_json::to_string(state) {
            let _ = fs::write(Self::state_path(), json);
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

impl Provider for PomodoroProvider {
    fn name(&self) -> &'static str {
        "pomodoro"
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

        if kw == "pomodoro" || kw == "pomo" || kw == "focus" {
            if let Some(state) = Self::load() {
                if state.ends_at > Self::now_secs() {
                    let left = state.ends_at - Self::now_secs();
                    let mins = left / 60;
                    let secs = left % 60;
                    out.push(Item::new(
                        format!("{} in progress", state.phase),
                        format!("{mins}m {secs}s remaining - Enter to cancel"),
                        "Focus",
                        8_800,
                        Action::Run {
                            program: "/bin/rm".to_string(),
                            args: vec![Self::state_path().to_string_lossy().to_string()],
                        },
                    ));
                    return;
                }
            }

            let work = arg
                .parse::<u64>()
                .ok()
                .filter(|&m| m > 0 && m <= 180)
                .unwrap_or(self.config.work_minutes);
            let ends = Self::now_secs() + work * 60;
            let state = PomodoroState {
                phase: "Work".to_string(),
                ends_at: ends,
                work_minutes: work,
                break_minutes: self.config.break_minutes,
                long_break_minutes: self.config.long_break_minutes,
                cycles: self.config.cycles,
                cycle_index: 1,
            };
            Self::save(&state);
            spawn_timer(state);
            out.push(Item::new(
                format!("Focus session started ({work} min)"),
                "You'll get a notification when it ends",
                "Focus",
                8_900,
                Action::None,
            ));
        }
    }
}

fn spawn_timer(_state: PomodoroState) {
    std::thread::spawn(move || {
        let path = PomodoroProvider::state_path();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let current = PomodoroProvider::load();
            let Some(cur) = current else {
                break;
            };
            if cur.ends_at > PomodoroProvider::now_secs() {
                continue;
            }
            let _ = fs::remove_file(&path);
            notify(
                &format!("{} complete", cur.phase),
                "Take a break or start another session with `pomodoro`",
            );
            break;
        }
    });
}
