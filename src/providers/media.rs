use std::process::Command;

use crate::engine::Provider;
use crate::model::{Action, Item};

/// Media controls for the active player. Commands target Spotify or Music via
/// AppleScript when one of them is running; if neither is running the action is
/// a graceful no-op. Keyword-gated so nothing runs on the default path.
pub struct MediaProvider;

impl Provider for MediaProvider {
    fn name(&self) -> &'static str {
        "media"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let lower = query.trim().to_ascii_lowercase();
        if lower.is_empty() {
            return;
        }

        let control = match lower.as_str() {
            "play" | "resume" => Some(("Play", "playpause", "play")),
            "pause" | "playpause" | "stop" => Some(("Pause / Play", "playpause", "playpause")),
            "next" | "next track" | "skip" => Some(("Next track", "next track", "next track")),
            "prev" | "previous" | "prev track" | "previous track" | "back" => {
                Some(("Previous track", "previous track", "previous track"))
            }
            _ => None,
        };

        if let Some((label, spotify_cmd, music_cmd)) = control {
            out.push(Item::new(
                label,
                "Controls Spotify or Music if running",
                "Media",
                9_000,
                Action::RunShell(player_command(spotify_cmd, music_cmd)),
            ));
            return;
        }

        if matches!(lower.as_str(), "now playing" | "nowplaying" | "track" | "current track") {
            if let Some(track) = now_playing() {
                out.push(Item::new(
                    track.clone(),
                    "Now playing - Enter to copy",
                    "Media",
                    9_100,
                    Action::CopyText(track),
                ));
            } else {
                out.push(Item::new(
                    "Nothing playing",
                    "Spotify and Music are not running",
                    "Media",
                    9_000,
                    Action::None,
                ));
            }
        }
    }
}

/// Build a command that dispatches to whichever supported player is running.
fn player_command(spotify_cmd: &str, music_cmd: &str) -> String {
    format!(
        "if pgrep -x Spotify >/dev/null; then osascript -e {}; elif pgrep -x Music >/dev/null; then osascript -e {}; fi",
        shell_quote(&format!("tell application \"Spotify\" to {spotify_cmd}")),
        shell_quote(&format!("tell application \"Music\" to {music_cmd}")),
    )
}

fn now_playing() -> Option<String> {
    let script = r#"
if application "Spotify" is running then
    tell application "Spotify"
        if player state is playing or player state is paused then
            return (name of current track) & " — " & (artist of current track)
        end if
    end tell
end if
if application "Music" is running then
    tell application "Music"
        if player state is playing or player state is paused then
            return (name of current track) & " — " & (artist of current track)
        end if
    end tell
end if
return ""
"#;
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
