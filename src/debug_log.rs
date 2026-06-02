//! Temporary debug logging for hotkey / layout / panel diagnostics.
//! Marked DEBUG-TEMP — remove before pushing.
//!
//! All logging here is compiled out entirely in release builds: every public
//! function below is a no-op when `debug_assertions` is disabled (i.e. in
//! `cargo build --release`, which is what ships). Release builds therefore write
//! no log file and print nothing to stderr. Debug builds keep full logging.

#[cfg(debug_assertions)]
use std::path::PathBuf;

#[cfg(debug_assertions)]
use std::sync::OnceLock;

#[cfg(debug_assertions)]
static LOG_PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();
#[cfg(debug_assertions)]
static LOG_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Initialize log targets (stderr mirror always on). No-op in release builds.
///
/// Primary: `~/Library/Application Support/litecast/debug-litecast.log` (Finder/.app
/// launches use cwd `/`, so a repo-relative `.cursor/` path is unreliable).
/// Mirror: `<repo>/.cursor/debug-litecast.log` when cwd contains `Cargo.toml`.
#[cfg(debug_assertions)]
pub fn init() {
    use crate::paths;
    let mut paths = vec![paths::support_file("debug-litecast.log")];
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("Cargo.toml").exists() {
            paths.push(cwd.join(".cursor").join("debug-litecast.log"));
        }
    }
    for path in &paths {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let _ = LOG_PATHS.set(paths);
}

/// No-op in release builds.
#[cfg(not(debug_assertions))]
#[inline]
pub fn init() {}

/// Paths selected in [`init`] (for diagnostics / docs). Empty in release builds.
#[cfg(debug_assertions)]
pub fn paths() -> &'static [PathBuf] {
    LOG_PATHS.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Empty in release builds.
#[cfg(not(debug_assertions))]
#[inline]
#[allow(dead_code)]
pub fn paths() -> &'static [std::path::PathBuf] {
    &[]
}

#[cfg(debug_assertions)]
fn ts_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Write one JSON line to the log file and stderr. No-op in release builds.
#[cfg(debug_assertions)]
pub fn log(location: &str, message: &str, detail: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    let line = format!(
        r#"{{"ts":{},"location":"{}","message":"{}","detail":{}}}"#,
        ts_ms(),
        escape_json(location),
        escape_json(message),
        json_string_or_raw(detail),
    );
    eprintln!("[litecast-debug] {location} | {message} | {detail}");
    let _guard = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(paths) = LOG_PATHS.get() {
        for path in paths {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}

/// No-op in release builds: produces zero stderr output and no file writes.
#[cfg(not(debug_assertions))]
#[inline]
pub fn log(_location: &str, _message: &str, _detail: &str) {}

#[cfg(debug_assertions)]
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(debug_assertions)]
fn json_string_or_raw(detail: &str) -> String {
    let trimmed = detail.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || trimmed == "true"
        || trimmed == "false"
        || trimmed.parse::<f64>().is_ok()
    {
        detail.to_string()
    } else {
        format!("\"{}\"", escape_json(detail))
    }
}
