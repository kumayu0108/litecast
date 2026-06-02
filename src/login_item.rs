//! "Launch at login" support for a directly-distributed (non-sandboxed) app.
//!
//! Implementation choice: a per-user **LaunchAgent** plist at
//! `~/Library/LaunchAgents/com.litecast.app.plist` with `RunAtLoad = true`.
//!
//! Why this over `SMAppService` / `LSSharedFileList`:
//! - There are no `objc2` ServiceManagement bindings in our dependency set, so
//!   `SMAppService.mainApp` is not available without adding a framework crate.
//! - `LSSharedFileList` is deprecated.
//! - For a directly distributed (Developer-ID / ad-hoc, non-sandboxed) app, a
//!   LaunchAgent plist is the most robust, dependency-free mechanism and is fully
//!   supported by launchd. It is plain to read back (the file either exists or
//!   not), which lets us reflect the real system state in the UI.
//!
//! The agent points `ProgramArguments` at the currently running executable
//! (resolved via `current_exe`), so a bundled `litecast.app` re-launches itself
//! at login. We deliberately do NOT `launchctl bootstrap` on enable: the app is
//! already running, and `RunAtLoad` would otherwise spawn a duplicate instance
//! immediately. Writing the plist is the persistent registration; it takes
//! effect on the next login. On disable we remove the plist and best-effort
//! `bootout` any loaded instance.

use std::path::PathBuf;

const LABEL: &str = "com.litecast.app";

/// `~/Library/LaunchAgents`.
fn launch_agents_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents"))
}

/// Path to our LaunchAgent plist.
pub fn plist_path() -> Option<PathBuf> {
    launch_agents_dir().map(|d| d.join(format!("{LABEL}.plist")))
}

/// True when the login item is currently registered (plist present on disk).
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

/// Executable to launch at login. Prefer the current executable path; this is
/// the bundled `litecast.app/Contents/MacOS/litecast` for a bundled app.
fn target_executable() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("cannot resolve executable path: {e}"))
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn plist_body(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
</dict>
</plist>
"#,
        label = LABEL,
        exe = escape_xml(exe),
    )
}

/// Register (`true`) or unregister (`false`) the login item, applying the
/// change immediately. Returns a user-facing error string on failure.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let path = plist_path().ok_or_else(|| "cannot locate ~/Library/LaunchAgents".to_string())?;
    if enabled {
        let exe = target_executable()?;
        let exe = exe.to_string_lossy();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create LaunchAgents dir: {e}"))?;
        }
        std::fs::write(&path, plist_body(&exe))
            .map_err(|e| format!("cannot write login item: {e}"))?;
        Ok(())
    } else {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("cannot remove login item: {e}"))?;
        }
        // Best-effort: drop any currently-loaded agent so it does not relaunch.
        if let Some(uid) = current_uid() {
            let _ = std::process::Command::new("launchctl")
                .args(["bootout", &format!("gui/{uid}/{LABEL}")])
                .output();
        }
        Ok(())
    }
}

fn current_uid() -> Option<u32> {
    // SAFETY: getuid() is always safe and never fails.
    Some(unsafe { libc_getuid() })
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}
