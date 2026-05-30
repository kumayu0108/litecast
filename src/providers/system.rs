use std::process::Command;

use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// macOS system commands (lock, sleep, dark mode, Wi-Fi, etc.). Fuzzy-searchable
/// by name; destructive ones are wrapped in `Action::Confirm`. All routes are
/// permission-free or use AppleScript (which prompts for Automation on first
/// use); nothing here needs Accessibility.
pub struct SystemProvider {
    entries: Vec<Entry>,
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
        }

        // Bluetooth has no permission-free CLI; only offer it when the optional
        // `blueutil` helper is on PATH.
        if has_blueutil() {
            entries.push(shell_entry(
                "Toggle Bluetooth",
                "Turn Bluetooth on or off (via blueutil)",
                "blueutil -p toggle",
            ));
        }

        Self { entries }
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
        action: Action::RunShell(format!("osascript -e '{}'", script.replace('\'', "'\\''"))),
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

fn has_blueutil() -> bool {
    Command::new("/usr/bin/env")
        .args(["which", "blueutil"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}
