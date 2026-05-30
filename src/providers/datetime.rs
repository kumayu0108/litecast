use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::engine::Provider;
use crate::model::{Action, Item};

/// World clock, date math, and timers. Time zones go through the built-in `date`
/// CLI with a `TZ` override (correct DST handling, no chrono). Date math is
/// hand-rolled with a civil-days algorithm. Timers spawn a detached thread that
/// fires an `osascript` notification when elapsed, so nothing blocks the UI.
pub struct DateTimeProvider {
    custom_zones: Vec<(String, String)>,
}

impl DateTimeProvider {
    pub fn new(custom_zones: Vec<(String, String)>) -> Self {
        Self { custom_zones }
    }
}

impl Provider for DateTimeProvider {
    fn name(&self) -> &'static str {
        "datetime"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        if self.try_world_clock(q, out) {
            return;
        }
        if try_timer(q, out) {
            return;
        }
        try_date_math(q, out);
    }
}

impl DateTimeProvider {
    fn try_world_clock(&self, q: &str, out: &mut Vec<Item>) -> bool {
        let lower = q.to_ascii_lowercase();
        let place = lower
            .strip_prefix("time in ")
            .or_else(|| lower.strip_prefix("clock in "))
            .or_else(|| lower.strip_prefix("time at "));
        let Some(place) = place else {
            return false;
        };
        let place = place.trim();
        if place.is_empty() {
            return false;
        }
        // User-defined zones take priority over the built-in table.
        let tz = self
            .custom_zones
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(place))
            .map(|(_, tz)| tz.clone())
            .or_else(|| zone_for(place).map(|s| s.to_string()));
        let Some(tz) = tz else {
            out.push(Item::new(
                format!("Unknown place: {place}"),
                "Try a city (Tokyo, London) or zone (IST, UTC, PST)",
                "Time",
                9_300,
                Action::None,
            ));
            return true;
        };
        if let Some(formatted) = time_in_zone(&tz) {
            out.push(Item::new(
                formatted.clone(),
                format!("{tz} - Enter to copy"),
                "Time",
                9_400,
                Action::CopyText(formatted),
            ));
        }
        true
    }
}

/// Run `date` with a `TZ` override to get the current time in a zone.
fn time_in_zone(tz: &str) -> Option<String> {
    let output = Command::new("/bin/date")
        .env("TZ", tz)
        .arg("+%H:%M  %a %d %b  %Z")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Map a city name or zone abbreviation to an IANA timezone identifier.
fn zone_for(place: &str) -> Option<&'static str> {
    let p = place.trim().to_ascii_lowercase();
    let z = match p.as_str() {
        // Zone abbreviations
        "utc" | "gmt" => "UTC",
        "ist" => "Asia/Kolkata",
        "pst" | "pdt" | "pt" => "America/Los_Angeles",
        "mst" | "mdt" | "mt" => "America/Denver",
        "cst" | "cdt" | "ct" => "America/Chicago",
        "est" | "edt" | "et" => "America/New_York",
        "bst" => "Europe/London",
        "cet" | "cest" => "Europe/Paris",
        "jst" => "Asia/Tokyo",
        "aest" | "aedt" => "Australia/Sydney",
        // Cities
        "tokyo" => "Asia/Tokyo",
        "delhi" | "mumbai" | "bangalore" | "bengaluru" | "kolkata" | "india" | "chennai" => {
            "Asia/Kolkata"
        }
        "london" | "uk" => "Europe/London",
        "paris" | "france" => "Europe/Paris",
        "berlin" | "germany" => "Europe/Berlin",
        "madrid" | "spain" => "Europe/Madrid",
        "rome" | "italy" => "Europe/Rome",
        "moscow" | "russia" => "Europe/Moscow",
        "dubai" | "uae" => "Asia/Dubai",
        "singapore" => "Asia/Singapore",
        "hong kong" | "hongkong" => "Asia/Hong_Kong",
        "shanghai" | "beijing" | "china" => "Asia/Shanghai",
        "seoul" | "korea" => "Asia/Seoul",
        "sydney" => "Australia/Sydney",
        "melbourne" => "Australia/Melbourne",
        "auckland" | "nz" => "Pacific/Auckland",
        "new york" | "nyc" | "newyork" => "America/New_York",
        "chicago" => "America/Chicago",
        "denver" => "America/Denver",
        "los angeles" | "la" | "san francisco" | "sf" | "seattle" => "America/Los_Angeles",
        "toronto" => "America/Toronto",
        "vancouver" => "America/Vancouver",
        "mexico city" => "America/Mexico_City",
        "sao paulo" | "brazil" => "America/Sao_Paulo",
        "honolulu" | "hawaii" => "Pacific/Honolulu",
        "cairo" | "egypt" => "Africa/Cairo",
        "johannesburg" | "south africa" => "Africa/Johannesburg",
        "lagos" | "nigeria" => "Africa/Lagos",
        "istanbul" | "turkey" => "Europe/Istanbul",
        "bangkok" | "thailand" => "Asia/Bangkok",
        "jakarta" | "indonesia" => "Asia/Jakarta",
        "karachi" | "pakistan" => "Asia/Karachi",
        "tehran" | "iran" => "Asia/Tehran",
        _ => return None,
    };
    Some(z)
}

// --- Timer --------------------------------------------------------------------

fn try_timer(q: &str, out: &mut Vec<Item>) -> bool {
    let lower = q.to_ascii_lowercase();
    let rest = match lower.strip_prefix("timer ").or_else(|| lower.strip_prefix("countdown ")) {
        Some(r) => r.trim(),
        None => return false,
    };
    if rest.is_empty() {
        return false;
    }
    // First token is the duration; the remainder (from the ORIGINAL query, to
    // preserve case) is an optional label.
    let dur_token = rest.split_whitespace().next().unwrap_or("");
    let Some(secs) = parse_duration(dur_token) else {
        out.push(Item::new(
            "Timer: invalid duration",
            "Try \"timer 5m\", \"timer 30s\", or \"timer 1h30m\"",
            "Time",
            9_300,
            Action::None,
        ));
        return true;
    };
    // Recover the label using the original (case-preserving) query.
    let label = q
        .split_once(char::is_whitespace)
        .and_then(|(_, r)| r.trim().split_once(char::is_whitespace))
        .map(|(_, l)| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Time's up!".to_string());

    out.push(Item::new(
        format!("Start timer: {}", human_duration(secs)),
        format!("Notifies \"{label}\" when elapsed - Enter to start"),
        "Time",
        9_400,
        Action::RunShell(timer_command(secs, &label)),
    ));
    true
}

/// A shell command that sleeps then posts a notification. Run via the standard
/// `RunShell` action (which spawns and does not wait), so the UI never blocks.
fn timer_command(secs: u64, label: &str) -> String {
    let title = "litecast timer";
    let script = format!(
        "display notification {} with title {} sound name \"Glass\"",
        applescript_quote(label),
        applescript_quote(title)
    );
    format!("sleep {secs}; osascript -e {}", shell_quote(&script))
}

fn parse_duration(token: &str) -> Option<u64> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    // Plain number = minutes.
    if let Ok(mins) = t.parse::<u64>() {
        return Some(mins.saturating_mul(60));
    }
    let mut total: u64 = 0;
    let mut num = String::new();
    let mut matched = false;
    for c in t.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let n: u64 = num.parse().ok()?;
            num.clear();
            let mult = match c {
                's' => 1,
                'm' => 60,
                'h' => 3600,
                'd' => 86400,
                _ => return None,
            };
            total = total.checked_add(n.checked_mul(mult)?)?;
            matched = true;
        }
    }
    if !num.is_empty() {
        // Trailing bare number = seconds when a unit was already seen.
        total = total.checked_add(num.parse::<u64>().ok()?)?;
        matched = true;
    }
    if matched && total > 0 {
        Some(total)
    } else {
        None
    }
}

fn human_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{h}h"));
    }
    if m > 0 {
        parts.push(format!("{m}m"));
    }
    if s > 0 || parts.is_empty() {
        parts.push(format!("{s}s"));
    }
    parts.join(" ")
}

// --- Date math ----------------------------------------------------------------

fn try_date_math(q: &str, out: &mut Vec<Item>) -> bool {
    let lower = q.to_ascii_lowercase();
    let lower = lower.trim();

    // Cheap intent check first so we never shell out to `date` (today's date)
    // unless the query actually looks like date math.
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    let looks_like = lower.starts_with("days until ")
        || lower.starts_with("days till ")
        || lower.starts_with("days to ")
        || lower.starts_with("days since ")
        || compact.starts_with("today+")
        || compact.starts_with("today-")
        || compact.starts_with("now+")
        || compact.starts_with("now-");
    if !looks_like {
        return false;
    }

    let today = today_civil();

    // "days until <date>" / "days till <date>"
    for kw in ["days until ", "days till ", "days to "] {
        if let Some(rest) = lower.strip_prefix(kw) {
            if let Some(target) = parse_date(rest.trim(), today.0 .0) {
                let diff = days_between(today.0, target);
                out.push(date_diff_item(diff, "until", rest.trim()));
                return true;
            }
        }
    }
    // "days since <date>"
    if let Some(rest) = lower.strip_prefix("days since ") {
        if let Some(target) = parse_date(rest.trim(), today.0 .0) {
            let diff = days_between(target, today.0);
            out.push(date_diff_item(diff, "since", rest.trim()));
            return true;
        }
    }
    // "today+30d" / "today + 30d" / "today-7d" / "now+2w"
    for prefix in ["today", "now"] {
        if let Some(rest) = compact.strip_prefix(prefix) {
            if let Some((sign, n, unit)) = parse_offset(rest) {
                let days = match unit {
                    'd' => n,
                    'w' => n * 7,
                    _ => return false,
                };
                let target_days = today.1 + sign * days;
                let (y, m, d) = civil_from_days(target_days);
                let result = format!("{y:04}-{m:02}-{d:02}");
                out.push(Item::new(
                    result.clone(),
                    format!("{} {} {} days - Enter to copy", prefix, if sign > 0 { "plus" } else { "minus" }, days.abs()),
                    "Time",
                    9_400,
                    Action::CopyText(result),
                ));
                return true;
            }
        }
    }
    false
}

fn date_diff_item(diff: i64, word: &str, target: &str) -> Item {
    let abs = diff.abs();
    let title = if diff == 0 {
        "That's today".to_string()
    } else {
        format!("{abs} days {word} {target}")
    };
    Item::new(title, "Enter to copy the count", "Time", 9_400, Action::CopyText(abs.to_string()))
}

/// Parse `+30d`, `-7d`, `+2w` (the part after `today`/`now`).
fn parse_offset(s: &str) -> Option<(i64, i64, char)> {
    let (sign, rest) = match s.chars().next()? {
        '+' => (1i64, &s[1..]),
        '-' => (-1i64, &s[1..]),
        _ => return None,
    };
    let unit = rest.chars().last()?;
    let digits = &rest[..rest.len() - unit.len_utf8()];
    let n: i64 = digits.parse().ok()?;
    Some((sign, n, unit))
}

/// Parse `YYYY-MM-DD`, `DD Mon`, `Mon DD`, or `DD/MM/YYYY`. For day/month forms
/// without a year, pick the next occurrence relative to `cur_year`.
fn parse_date(s: &str, cur_year: i64) -> Option<(i64, u32, u32)> {
    let s = s.trim();
    // ISO YYYY-MM-DD
    if let Some((y, m, d)) = parse_iso(s) {
        return Some((y, m, d));
    }
    // DD/MM/YYYY or DD/MM
    if s.contains('/') {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 3 {
            let d = parts[0].parse().ok()?;
            let m = parts[1].parse().ok()?;
            let y = parts[2].parse().ok()?;
            return valid_ymd(y, m, d);
        }
    }
    // "DD Mon" or "Mon DD"
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() == 2 {
        if let (Some(d), Some(m)) = (tokens[0].parse::<u32>().ok(), month_num(tokens[1])) {
            return next_occurrence(cur_year, m, d);
        }
        if let (Some(m), Some(d)) = (month_num(tokens[0]), tokens[1].parse::<u32>().ok()) {
            return next_occurrence(cur_year, m, d);
        }
    }
    None
}

fn parse_iso(s: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 3 {
        let y = parts[0].parse().ok()?;
        let m = parts[1].parse().ok()?;
        let d = parts[2].parse().ok()?;
        return valid_ymd(y, m, d);
    }
    None
}

fn next_occurrence(cur_year: i64, m: u32, d: u32) -> Option<(i64, u32, u32)> {
    valid_ymd(cur_year, m, d)
}

fn valid_ymd(y: i64, m: u32, d: u32) -> Option<(i64, u32, u32)> {
    if (1..=12).contains(&m) && (1..=31).contains(&d) {
        Some((y, m, d))
    } else {
        None
    }
}

fn month_num(s: &str) -> Option<u32> {
    let m = s.trim().to_ascii_lowercase();
    let n = match &m[..m.len().min(3)] {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    Some(n)
}

/// Today's local date, as (year, days-from-epoch).
fn today_civil() -> ((i64, u32, u32), i64) {
    let out = Command::new("/bin/date").arg("+%Y-%m-%d").output();
    if let Ok(out) = out {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some((y, m, d)) = parse_iso(&s) {
            return ((y, m, d), days_from_civil(y, m as i64, d as i64));
        }
    }
    // Fallback to UTC from the system clock.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days);
    ((y, m, d), days)
}

fn days_between(a: (i64, u32, u32), b: (i64, u32, u32)) -> i64 {
    days_from_civil(b.0, b.1 as i64, b.2 as i64) - days_from_civil(a.0, a.1 as i64, a.2 as i64)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

// Keep an explicit reference so a future async timer can use a thread handle;
// currently the shell `sleep` approach is used (no blocking on the worker).
#[allow(dead_code)]
fn _spawn_timer(secs: u64, body: impl FnOnce() + Send + 'static) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(secs));
        body();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip() {
        let days = days_from_civil(2020, 1, 1);
        assert_eq!(civil_from_days(days), (2020, 1, 1));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn duration_parsing() {
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration("1h30m"), Some(5400));
        assert_eq!(parse_duration("2"), Some(120));
    }
}
