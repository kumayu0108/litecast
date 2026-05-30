use std::process::Command;

/// Interactively capture a screen region/window via the built-in
/// `screencapture` tool, returning the PNG path on success. Returns None if the
/// user cancelled or nothing was captured. Requires the Screen Recording
/// permission (macOS will prompt the first time).
pub fn capture_interactive() -> Option<String> {
    let path = std::env::temp_dir().join("litecast-shot.png");
    let _ = std::fs::remove_file(&path);

    let status = Command::new("/usr/sbin/screencapture")
        .arg("-i") // interactive selection (region or window)
        .arg(&path)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > 0 => Some(path.to_string_lossy().into_owned()),
        _ => None, // user pressed Esc / no capture
    }
}
