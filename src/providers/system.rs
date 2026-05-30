use std::process::Command;

use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// macOS system commands (lock, sleep, dark mode, volume, Wi-Fi, Bluetooth,
/// brightness, caffeinate, eject, Focus, etc.). Fuzzy-searchable by name;
/// destructive ones are wrapped in `Action::Confirm`. Parameterized controls
/// (`volume 50`, `brightness 70`) are matched by keyword. All routes are
/// permission-free or use AppleScript (which prompts for Automation on first
/// use); nothing here needs Accessibility.
pub struct SystemProvider {
    entries: Vec<Entry>,
    has_brightness_cli: bool,
}

struct Entry {
    name: String,
    subtitle: String,
    action: Action,
}

impl SystemProvider {
    pub fn new() -> Self {
        let mut entries = vec![
            shell_entry(
                "Lock Screen",
                "Lock the screen now",
                "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession -suspend",
            ),
            shell_entry("Sleep", "Put the Mac to sleep", "pmset sleepnow"),
            shell_entry(
                "Sleep Displays",
                "Turn off the displays",
                "pmset displaysleepnow",
            ),
            osascript_entry(
                "Toggle Dark Mode",
                "Switch between light and dark appearance (asks for Automation on first use)",
                "tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode",
            ),
            // Volume controls (osascript `set volume`; no permissions needed).
            osascript_entry(
                "Volume Up",
                "Raise the output volume by 10%",
                "set volume output volume ((output volume of (get volume settings)) + 10)",
            ),
            osascript_entry(
                "Volume Down",
                "Lower the output volume by 10%",
                "set volume output volume ((output volume of (get volume settings)) - 10)",
            ),
            osascript_entry(
                "Mute",
                "Mute the output volume",
                "set volume with output muted",
            ),
            osascript_entry(
                "Unmute",
                "Unmute the output volume",
                "set volume without output muted",
            ),
            // Caffeinate: spawn detached (RunShell does not wait), stop via pkill.
            shell_entry(
                "Caffeinate",
                "Keep the Mac awake until you decaffeinate (or reboot)",
                "caffeinate -dimsu",
            ),
            shell_entry(
                "Decaffeinate",
                "Allow the Mac to sleep again (stops caffeinate)",
                "pkill -x caffeinate",
            ),
            osascript_entry(
                "Eject All Disks",
                "Eject every ejectable disk (asks for Automation on first use)",
                "tell application \"Finder\" to eject (every disk whose ejectable is true)",
            ),
            // Do Not Disturb / Focus: no stable scriptable API on modern macOS.
            // Best-effort via a user-created Shortcut; degrades to a no-op error
            // if the shortcut does not exist.
            shell_entry(
                "Toggle Do Not Disturb",
                "Best-effort: runs a Shortcut named \"Toggle Do Not Disturb\" if present",
                "shortcuts run \"Toggle Do Not Disturb\" 2>/dev/null || true",
            ),
            confirm_entry(
                "Empty Trash",
                "empty the Trash",
                "Permanently delete the Trash contents (asks for Automation on first use)",
                Action::RunShell(
                    "osascript -e 'tell application \"Finder\" to empty trash'".to_string(),
                ),
            ),
            confirm_entry(
                "Restart",
                "restart the Mac",
                "Restart the computer (asks for Automation on first use)",
                Action::RunShell(
                    "osascript -e 'tell application \"System Events\" to restart'".to_string(),
                ),
            ),
            confirm_entry(
                "Shut Down",
                "shut down the Mac",
                "Power off the computer (asks for Automation on first use)",
                Action::RunShell(
                    "osascript -e 'tell application \"System Events\" to shut down'".to_string(),
                ),
            ),
        ];

        if let Some(device) = wifi_device() {
            entries.push(shell_entry(
                "Toggle Wi-Fi",
                "Turn Wi-Fi on or off",
                &toggle_wifi_command(&device),
            ));
            entries.push(shell_entry(
                "Wi-Fi On",
                "Turn Wi-Fi on",
                &format!("networksetup -setairportpower {device} on"),
            ));
            entries.push(shell_entry(
                "Wi-Fi Off",
                "Turn Wi-Fi off",
                &format!("networksetup -setairportpower {device} off"),
            ));
        }

        // Bluetooth has no permission-free CLI; only offer it when the optional
        // `blueutil` helper is on PATH.
        if has_cli("blueutil") {
            entries.push(shell_entry(
                "Toggle Bluetooth",
                "Turn Bluetooth on or off (via blueutil)",
                "blueutil -p toggle",
            ));
            entries.push(shell_entry(
                "Bluetooth On",
                "Turn Bluetooth on (via blueutil)",
                "blueutil -p 1",
            ));
            entries.push(shell_entry(
                "Bluetooth Off",
                "Turn Bluetooth off (via blueutil)",
                "blueutil -p 0",
            ));
        }

        // Brightness has no permission-free CLI either; offer it only when the
        // optional `brightness` helper is installed.
        let has_brightness_cli = has_cli("brightness");
        if has_brightness_cli {
            entries.push(shell_entry(
                "Brightness Up",
                "Raise display brightness (via brightness)",
                "brightness +0.1",
            ));
            entries.push(shell_entry(
                "Brightness Down",
                "Lower display brightness (via brightness)",
                "brightness -0.1",
            ));
        }

        Self {
            entries,
            has_brightness_cli,
        }
    }
}

impl Provider for SystemProvider {
    fn name(&self) -> &'static str {
        "system"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }

        // Parameterized controls first (e.g. "volume 50", "brightness 70").
        if self.try_parameterized(q, out) {
            return;
        }

        for entry in &self.entries {
            if let Some(score) = fuzzy_score(q, &entry.name) {
                out.push(
                    Item::new(
                        entry.name.clone(),
                        entry.subtitle.clone(),
                        "System",
                        300 + score as i64,
                        entry.action.clone(),
                    )
                    .with_id(format!("sys:{}", entry.name)),
                );
            }
        }
    }
}

impl SystemProvider {
    /// Handle `volume <0-100>` / `set volume <n>` and `brightness <0-100>`.
    fn try_parameterized(&self, q: &str, out: &mut Vec<Item>) -> bool {
        let lower = q.to_ascii_lowercase();
        let lower = lower.trim();

        for kw in ["set volume ", "volume "] {
            if let Some(rest) = lower.strip_prefix(kw) {
                if let Ok(level) = rest.trim().parse::<u32>() {
                    let level = level.min(100);
                    let script =
                        format!("set volume output volume {level}");
                    out.push(Item::new(
                        format!("Set volume to {level}%"),
                        "Press Enter to apply",
                        "System",
                        9_000,
                        Action::RunShell(osascript(&script)),
                    ));
                    return true;
                }
            }
        }

        if self.has_brightness_cli {
            if let Some(rest) = lower.strip_prefix("brightness ") {
                if let Ok(level) = rest.trim().parse::<u32>() {
                    let level = level.min(100);
                    let frac = level as f64 / 100.0;
                    out.push(Item::new(
                        format!("Set brightness to {level}%"),
                        "Press Enter to apply (via brightness)",
                        "System",
                        9_000,
                        Action::RunShell(format!("brightness {frac}")),
                    ));
                    return true;
                }
            }
        }
        false
    }
}

fn osascript(script: &str) -> String {
    format!("osascript -e '{}'", script.replace('\'', "'\\''"))
}

fn shell_entry(name: &str, subtitle: &str, command: &str) -> Entry {
    Entry {
        name: name.to_string(),
        subtitle: subtitle.to_string(),
        action: Action::RunShell(command.to_string()),
    }
}

fn osascript_entry(name: &str, subtitle: &str, script: &str) -> Entry {
    Entry {
        name: name.to_string(),
        subtitle: subtitle.to_string(),
        action: Action::RunShell(osascript(script)),
    }
}

fn confirm_entry(name: &str, label: &str, subtitle: &str, inner: Action) -> Entry {
    Entry {
        name: name.to_string(),
        subtitle: subtitle.to_string(),
        action: Action::Confirm {
            label: label.to_string(),
            inner: Box::new(inner),
        },
    }
}

/// Detect the Wi-Fi hardware port's BSD device name (usually `en0`).
fn wifi_device() -> Option<String> {
    let output = Command::new("/usr/sbin/networksetup")
        .arg("-listallhardwareports")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.contains("Wi-Fi") || line.contains("AirPort") {
            for next in lines.by_ref() {
                if let Some(dev) = next.strip_prefix("Device: ") {
                    return Some(dev.trim().to_string());
                }
                if next.trim().is_empty() {
                    break;
                }
            }
        }
    }
    None
}

/// Toggle command that reads the current power state and flips it.
fn toggle_wifi_command(device: &str) -> String {
    format!(
        "if networksetup -getairportpower {dev} | grep -q On; then networksetup -setairportpower {dev} off; else networksetup -setairportpower {dev} on; fi",
        dev = device
    )
}

fn has_cli(name: &str) -> bool {
    Command::new("/usr/bin/env")
        .args(["which", name])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}
