use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::engine::{fuzzy_score, Provider};
use crate::model::{osascript_action_with_args, Action, Item};

/// Window & tab switcher. Keyword-gated so nothing runs on the default path:
///   `windows` / `switch`  - list open windows across apps; Enter focuses one
///   `tabs`                - list tabs of running browsers; Enter activates one
///
/// Window listing goes through `System Events` (AppleScript), and tab listing
/// through each browser's own scripting interface; both prompt for the
/// **Automation** (and, for windows, **Accessibility**) permission on first use.
/// Results are cached briefly so repeated keystrokes don't re-run osascript.
/// All scripts are literal; window/app names and tab indices are passed as
/// `on run argv` parameters, never interpolated into AppleScript source.
pub struct SwitcherProvider {
    windows: Mutex<Option<(Instant, Vec<WinEntry>)>>,
    tabs: Mutex<Option<(Instant, Vec<TabEntry>)>>,
}

#[derive(Clone)]
struct WinEntry {
    app: String,
    title: String,
}

#[derive(Clone)]
struct TabEntry {
    browser: String,
    window_index: u32,
    tab_index: u32,
    title: String,
}

const CACHE_TTL: Duration = Duration::from_secs(3);
/// Chromium-family browsers share the same scripting model.
const CHROMIUM: &[&str] = &["Google Chrome", "Arc", "Brave Browser", "Microsoft Edge"];

impl SwitcherProvider {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(None),
            tabs: Mutex::new(None),
        }
    }
}

impl Provider for SwitcherProvider {
    fn name(&self) -> &'static str {
        "switcher"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let (kw, arg) = match q.split_once(char::is_whitespace) {
            Some((k, a)) => (k.to_ascii_lowercase(), a.trim()),
            None => (q.to_ascii_lowercase(), ""),
        };

        match kw.as_str() {
            "windows" | "switch" | "switcher" => self.list_windows(arg, out),
            "tabs" | "tab" => self.list_tabs(arg, out),
            _ => {}
        }
    }
}

impl SwitcherProvider {
    fn list_windows(&self, filter: &str, out: &mut Vec<Item>) {
        let entries = self.cached_windows();
        if entries.is_empty() {
            out.push(Item::new(
                "No windows found",
                "Needs Accessibility/Automation for System Events, or no windows are open",
                "Switch",
                8_200,
                Action::None,
            ));
            return;
        }
        for entry in entries {
            let label = if entry.title.is_empty() {
                entry.app.clone()
            } else {
                entry.title.clone()
            };
            let score = if filter.is_empty() {
                8_200
            } else {
                match fuzzy_score(filter, &label).or_else(|| fuzzy_score(filter, &entry.app)) {
                    Some(s) => 8_000 + s as i64,
                    None => continue,
                }
            };
            out.push(
                Item::new(
                    label,
                    format!("{} - Enter to focus", entry.app),
                    "Switch",
                    score,
                    raise_window_action(&entry.app, &entry.title),
                )
                .with_id(format!("win-switch:{}:{}", entry.app, entry.title)),
            );
        }
    }

    fn list_tabs(&self, filter: &str, out: &mut Vec<Item>) {
        let entries = self.cached_tabs();
        if entries.is_empty() {
            out.push(Item::new(
                "No browser tabs found",
                "No supported browser is running (Safari, Chrome, Arc, Brave, Edge)",
                "Switch",
                8_200,
                Action::None,
            ));
            return;
        }
        for entry in entries {
            let score = if filter.is_empty() {
                8_200
            } else {
                match fuzzy_score(filter, &entry.title) {
                    Some(s) => 8_000 + s as i64,
                    None => continue,
                }
            };
            out.push(
                Item::new(
                    entry.title.clone(),
                    format!("{} tab - Enter to activate", entry.browser),
                    "Switch",
                    score,
                    activate_tab_action(&entry),
                )
                .with_id(format!(
                    "tab-switch:{}:{}:{}",
                    entry.browser, entry.window_index, entry.tab_index
                )),
            );
        }
    }

    fn cached_windows(&self) -> Vec<WinEntry> {
        if let Ok(guard) = self.windows.lock() {
            if let Some((at, entries)) = guard.as_ref() {
                if at.elapsed() < CACHE_TTL {
                    return entries.clone();
                }
            }
        }
        let entries = scan_windows();
        if let Ok(mut guard) = self.windows.lock() {
            *guard = Some((Instant::now(), entries.clone()));
        }
        entries
    }

    fn cached_tabs(&self) -> Vec<TabEntry> {
        if let Ok(guard) = self.tabs.lock() {
            if let Some((at, entries)) = guard.as_ref() {
                if at.elapsed() < CACHE_TTL {
                    return entries.clone();
                }
            }
        }
        let entries = scan_tabs();
        if let Ok(mut guard) = self.tabs.lock() {
            *guard = Some((Instant::now(), entries.clone()));
        }
        entries
    }
}

/// Enumerate visible windows of foreground apps via System Events. Output rows
/// are `app\twindowtitle`.
fn scan_windows() -> Vec<WinEntry> {
    const SCRIPT: &str = "tell application \"System Events\"\nset acc to \"\"\nrepeat with p in (every process whose background only is false)\nset pn to name of p\nrepeat with w in (windows of p)\nset acc to acc & pn & \"\\t\" & (name of w) & \"\\n\"\nend repeat\nend repeat\nend tell\nreturn acc";
    let text = run_osascript(SCRIPT);
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(2, '\t');
        let (Some(app), Some(title)) = (parts.next(), parts.next()) else {
            continue;
        };
        let app = app.trim();
        if app.is_empty() {
            continue;
        }
        out.push(WinEntry {
            app: app.to_string(),
            title: title.trim().to_string(),
        });
    }
    out
}

/// Enumerate tabs of any running supported browser.
fn scan_tabs() -> Vec<TabEntry> {
    let mut out = Vec::new();
    for &browser in CHROMIUM {
        if !app_running(browser) {
            continue;
        }
        let script = format!(
            "tell application \"{browser}\"\nset acc to \"\"\nset wi to 0\nrepeat with w in windows\nset wi to wi + 1\nset ti to 0\nrepeat with t in tabs of w\nset ti to ti + 1\nset acc to acc & wi & \"\\t\" & ti & \"\\t\" & (title of t) & \"\\n\"\nend repeat\nend repeat\nend tell\nreturn acc"
        );
        parse_tabs(browser, &run_osascript(&script), &mut out);
    }
    if app_running("Safari") {
        const SAFARI: &str = "tell application \"Safari\"\nset acc to \"\"\nset wi to 0\nrepeat with w in windows\nset wi to wi + 1\nset ti to 0\nrepeat with t in tabs of w\nset ti to ti + 1\nset acc to acc & wi & \"\\t\" & ti & \"\\t\" & (name of t) & \"\\n\"\nend repeat\nend repeat\nend tell\nreturn acc";
        parse_tabs("Safari", &run_osascript(SAFARI), &mut out);
    }
    out
}

fn parse_tabs(browser: &str, text: &str, out: &mut Vec<TabEntry>) {
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(wi), Some(ti), Some(title)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(window_index), Ok(tab_index)) = (wi.trim().parse::<u32>(), ti.trim().parse::<u32>())
        else {
            continue;
        };
        let title = title.trim();
        if title.is_empty() {
            continue;
        }
        out.push(TabEntry {
            browser: browser.to_string(),
            window_index,
            tab_index,
            title: title.to_string(),
        });
    }
}

/// Is `name` running? Checked without launching it (`pgrep -x`).
fn app_running(name: &str) -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", name])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

fn run_osascript(script: &str) -> String {
    Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// Focus an app and raise its window with the given title (argv-safe).
fn raise_window_action(app: &str, title: &str) -> Action {
    if title.is_empty() {
        let script = "on run argv\ntell application (item 1 of argv) to activate\nend run";
        return osascript_action_with_args(script, &[app]);
    }
    let script = "on run argv\ntell application (item 1 of argv) to activate\ntell application \"System Events\" to tell process (item 1 of argv)\nperform action \"AXRaise\" of (first window whose name is (item 2 of argv))\nend tell\nend run";
    osascript_action_with_args(script, &[app, title])
}

/// Activate a browser tab by window/tab index (argv-safe).
fn activate_tab_action(entry: &TabEntry) -> Action {
    let wi = entry.window_index.to_string();
    let ti = entry.tab_index.to_string();
    if entry.browser == "Safari" {
        let script = "on run argv\nset wi to (item 2 of argv) as integer\nset ti to (item 3 of argv) as integer\ntell application \"Safari\"\nactivate\ntell window wi to set current tab to tab ti\nend tell\nend run";
        return osascript_action_with_args(script, &[&entry.browser, &wi, &ti]);
    }
    let script = "on run argv\nset wi to (item 2 of argv) as integer\nset ti to (item 3 of argv) as integer\ntell application (item 1 of argv)\nactivate\nset active tab index of window wi to ti\nset index of window wi to 1\nend tell\nend run";
    osascript_action_with_args(script, &[&entry.browser, &wi, &ti])
}
