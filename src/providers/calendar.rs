use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::engine::Provider;
use crate::model::{osascript_action_with_args, Action, Item};

/// Calendar & Reminders via AppleScript bridges (no entitlement-heavy linking).
///   `today` / `agenda`            - list today's calendar events
///   `remind <text> [at <time>]`   - quick-add a reminder
///   `event <text> [at <time>]`    - quick-add a calendar event
///
/// Listing today's events shells out to Calendar (slow), so results are cached
/// briefly; nothing runs until a keyword matches, and create actions only run
/// on Enter. macOS prompts for Automation permission on first use.
/// (fetched-at, list of (summary, when)).
type EventCache = Option<(Instant, Vec<(String, String)>)>;

pub struct CalendarProvider {
    cache: Mutex<EventCache>,
}

impl CalendarProvider {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
        }
    }
}

const CACHE_TTL: Duration = Duration::from_secs(120);

impl Provider for CalendarProvider {
    fn name(&self) -> &'static str {
        "calendar"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();

        if matches!(lower.as_str(), "today" | "agenda" | "events") {
            self.today(out);
            return;
        }

        if let Some(rest) = lower.strip_prefix("remind ") {
            // Use the original-case text after "remind ".
            let text = q[q.len() - rest.len()..].trim();
            push_reminder(out, text);
            return;
        }

        if let Some(rest) = lower
            .strip_prefix("event ")
            .or_else(|| lower.strip_prefix("addevent "))
            .or_else(|| lower.strip_prefix("cal "))
        {
            let text = q[q.len() - rest.len()..].trim();
            push_event(out, text);
        }
    }
}

impl CalendarProvider {
    fn today(&self, out: &mut Vec<Item>) {
        let events = {
            let mut guard = self.cache.lock().unwrap();
            let fresh = guard
                .as_ref()
                .is_some_and(|(t, _)| t.elapsed() < CACHE_TTL);
            if !fresh {
                let events = fetch_today_events();
                *guard = Some((Instant::now(), events));
            }
            guard.as_ref().map(|(_, e)| e.clone()).unwrap_or_default()
        };

        if events.is_empty() {
            out.push(Item::new(
                "No events today",
                "Calendar returned nothing (or access was denied) - Enter to open Calendar",
                "Calendar",
                9_000,
                Action::Open("/System/Applications/Calendar.app".to_string()),
            ));
            return;
        }
        for (i, (summary, when)) in events.into_iter().enumerate() {
            out.push(Item::new(
                summary,
                when,
                "Calendar",
                9_000 - i as i64,
                Action::Open("/System/Applications/Calendar.app".to_string()),
            ));
        }
    }
}

fn fetch_today_events() -> Vec<(String, String)> {
    let script = r#"
set out to ""
tell application "Calendar"
    set dayStart to (current date)
    set hours of dayStart to 0
    set minutes of dayStart to 0
    set seconds of dayStart to 0
    set dayEnd to dayStart + (24 * 60 * 60)
    repeat with c in calendars
        try
            set evs to (every event of c whose start date >= dayStart and start date < dayEnd)
            repeat with e in evs
                set out to out & (summary of e) & "\t" & (start date of e as string) & linefeed
            end repeat
        end try
    end repeat
end tell
return out
"#;
    let output = Command::new("/usr/bin/osascript").arg("-e").arg(script).output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut events: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((summary, when)) = line.split_once('\t') {
            events.push((summary.trim().to_string(), when.trim().to_string()));
        } else {
            events.push((line.to_string(), String::new()));
        }
    }
    events
}

fn push_reminder(out: &mut Vec<Item>, text: &str) {
    if text.is_empty() {
        return;
    }
    let (name, time) = split_at_time(text);
    // The reminder name is passed as an `on run argv` parameter (item 1 of
    // argv), never interpolated into the AppleScript source. Numeric h/m are
    // parsed u32s, so embedding them literally is safe.
    let script = match time {
        Some((h, m)) => format!(
            "on run argv\ntell application \"Reminders\"\nset d to (current date)\nset hours of d to {h}\nset minutes of d to {m}\nset seconds of d to 0\nmake new reminder with properties {{name:(item 1 of argv), remind me date:d}}\nend tell\nend run"
        ),
        None => "on run argv\ntell application \"Reminders\"\nmake new reminder with properties {name:(item 1 of argv)}\nend tell\nend run".to_string(),
    };
    let subtitle = match time {
        Some((h, m)) => format!("Reminder at {h:02}:{m:02} - Enter to add (asks for Automation)"),
        None => "Enter to add a reminder (asks for Automation on first use)".to_string(),
    };
    out.push(Item::new(
        format!("Add reminder: {name}"),
        subtitle,
        "Reminders",
        9_100,
        osascript_action_with_args(&script, &[&name]),
    ));
}

fn push_event(out: &mut Vec<Item>, text: &str) {
    if text.is_empty() {
        return;
    }
    let (summary, time) = split_at_time(text);
    let (h, m) = time.unwrap_or((9, 0));
    // Summary passed via argv; h/m are parsed u32s.
    let script = format!(
        "on run argv\ntell application \"Calendar\"\nset startDate to (current date)\nset hours of startDate to {h}\nset minutes of startDate to {m}\nset seconds of startDate to 0\nset endDate to startDate + (60 * 60)\ntell calendar 1\nmake new event with properties {{summary:(item 1 of argv), start date:startDate, end date:endDate}}\nend tell\nend tell\nend run"
    );
    out.push(Item::new(
        format!("Add event: {summary}"),
        format!("Today {h:02}:{m:02} (1h) - Enter to add (asks for Automation)"),
        "Calendar",
        9_100,
        osascript_action_with_args(&script, &[&summary]),
    ));
}

/// Split a trailing `at <time>` clause from the text, returning the name and an
/// optional (hour, minute).
fn split_at_time(text: &str) -> (String, Option<(u32, u32)>) {
    if let Some(idx) = text.to_ascii_lowercase().rfind(" at ") {
        let (name, rest) = (&text[..idx], &text[idx + 4..]);
        if let Some(hm) = parse_time(rest.trim()) {
            return (name.trim().to_string(), Some(hm));
        }
    }
    (text.trim().to_string(), None)
}

/// Parse `5pm`, `5:30pm`, `17:00`, `9am`.
fn parse_time(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().to_ascii_lowercase();
    let (s, ampm) = if let Some(rest) = s.strip_suffix("am") {
        (rest.trim().to_string(), Some(false))
    } else if let Some(rest) = s.strip_suffix("pm") {
        (rest.trim().to_string(), Some(true))
    } else {
        (s, None)
    };
    let (h, m) = if let Some((h, m)) = s.split_once(':') {
        (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?)
    } else {
        (s.parse::<u32>().ok()?, 0)
    };
    if m > 59 {
        return None;
    }
    let h = match ampm {
        Some(true) => {
            if h == 12 {
                12
            } else {
                h + 12
            }
        }
        Some(false) => {
            if h == 12 {
                0
            } else {
                h
            }
        }
        None => h,
    };
    if h > 23 {
        return None;
    }
    Some((h, m))
}
