use std::process::Command;

use crate::engine::Provider;
use crate::model::{osascript_action, Action, Item};

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
                osascript_action(player_script(spotify_cmd, music_cmd)),
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

/// Build a single AppleScript (run without a shell) that dispatches to whichever
/// supported player is running. The control verbs (`spotify_cmd`/`music_cmd`)
/// are fixed strings chosen above, never user input.
fn player_script(spotify_cmd: &str, music_cmd: &str) -> String {
    format!(
        "if application \"Spotify\" is running then\ntell application \"Spotify\" to {spotify_cmd}\nelse if application \"Music\" is running then\ntell application \"Music\" to {music_cmd}\nend if"
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
