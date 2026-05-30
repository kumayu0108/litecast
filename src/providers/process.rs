use std::process::Command;

use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// Process manager. Keyword-gated (`kill <query>` or `ps <query>`) so it never
/// spawns `ps` unprompted. Lists the current user's running processes by name +
/// pid + %CPU; Enter sends SIGTERM, but only through the two-step
/// `Action::Confirm` so an accidental Enter can't kill anything.
pub struct ProcessProvider;

impl ProcessProvider {
    pub fn new() -> Self {
        Self
    }
}

/// Processes we refuse to list, to avoid a foot-gun of killing the session.
const DENYLIST: &[&str] = &[
    "kernel_task",
    "launchd",
    "WindowServer",
    "loginwindow",
    "logind",
    "Dock",
    "SystemUIServer",
    "Finder",
    "litecast",
];

impl Provider for ProcessProvider {
    fn name(&self) -> &'static str {
        "process"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        let Some(arg) = match_keyword(q, "kill").or_else(|| match_keyword(q, "ps")) else {
            return;
        };

        for proc in list_processes() {
            if DENYLIST.contains(&proc.name.as_str()) {
                continue;
            }
            // Empty arg lists everything (highest-CPU first); otherwise fuzzy
            // filter on the process name.
            let score = if arg.is_empty() {
                // Float busy processes up, but keep them under intentful hits.
                8_000 + (proc.cpu.min(99.0) as i64)
            } else {
                match fuzzy_score(arg, &proc.name) {
                    Some(s) => 8_000 + s as i64,
                    None => continue,
                }
            };
            out.push(build(&proc, score));
        }
    }
}

struct Proc {
    pid: i32,
    cpu: f64,
    name: String,
}

/// Snapshot the user's processes via `ps`. Cheap, and only called once the
/// `kill`/`ps` keyword matched.
fn list_processes() -> Vec<Proc> {
    let output = match Command::new("/bin/ps")
        .args(["-axco", "pid=,pcpu=,comm="])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output);
    let mut procs = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let mut parts = line.splitn(3, char::is_whitespace);
        let (Some(pid), Some(cpu), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(pid), Ok(cpu)) = (pid.parse::<i32>(), cpu.parse::<f64>()) else {
            continue;
        };
        let name = name.trim();
        if pid <= 1 || name.is_empty() {
            continue;
        }
        procs.push(Proc {
            pid,
            cpu,
            name: name.to_string(),
        });
    }
    procs
}

fn build(proc: &Proc, score: i64) -> Item {
    Item::new(
        proc.name.clone(),
        format!("pid {} \u{2022} {:.1}% CPU \u{2022} sends SIGTERM", proc.pid, proc.cpu),
        "Proc",
        score,
        Action::Confirm {
            label: format!("kill {} (pid {})", proc.name, proc.pid),
            // SIGTERM (graceful): plain `kill`, not `kill -9`.
            inner: Box::new(Action::RunShell(format!("kill {}", proc.pid))),
        },
    )
}

fn match_keyword<'a>(q: &'a str, keyword: &str) -> Option<&'a str> {
    if q == keyword {
        return Some("");
    }
    q.strip_prefix(&format!("{keyword} ")).map(|rest| rest.trim())
}
