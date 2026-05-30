use std::process::Command;

use crate::engine::{fuzzy_score, Provider};
use crate::model::{osascript_action, Action, Item};

/// macOS system commands (lock, sleep, dark mode, volume, Wi-Fi, Bluetooth,
/// brightness, caffeinate, eject, Focus, etc.). Fuzzy-searchable by name;
/// destructive ones are wrapped in `Action::Confirm`. Parameterized controls
/// (`volume 50`, `brightness 70`) are matched by keyword. All routes are
/// permission-free or use AppleScript (which prompts for Automation on first
/// use); nothing here needs Accessibility.
///
/// Every command runs WITHOUT a shell (argv arrays / `osascript -e`); the only
/// remaining `sh -c` is the Wi-Fi toggle, which reads-then-sets state and
/// contains no user-derived text (the device name comes from the system).
pub struct SystemProvider {
    entries: Vec<Entry>,
    brightness_cli: Option<String>,
}

struct Entry {
    name: String,
    subtitle: String,
    action: Action,
}

impl SystemProvider {
    pub fn new() -> Self {
        let mut entries = vec![
            Entry::new("Lock Screen", "Lock the screen now", lock_action()),
            run_entry("Sleep", "Put the Mac to sleep", "/usr/bin/pmset", &["sleepnow"]),
            run_entry(
                "Sleep Displays",
                "Turn off the displays",
                "/usr/bin/pmset",
                &["displaysleepnow"],
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
            // Caffeinate: spawned detached (Run does not wait), stopped via pkill.
            run_entry(
                "Caffeinate",
                "Keep the Mac awake until you decaffeinate (or reboot)",
                "/usr/bin/caffeinate",
                &["-dimsu"],
            ),
            run_entry(
                "Decaffeinate",
                "Allow the Mac to sleep again (stops caffeinate)",
                "/usr/bin/pkill",
                &["-x", "caffeinate"],
            ),
            osascript_entry(
                "Eject All Disks",
                "Eject every ejectable disk (asks for Automation on first use)",
                "tell application \"Finder\" to eject (every disk whose ejectable is true)",
            ),
            // Do Not Disturb / Focus: no stable scriptable API on modern macOS.
            // Best-effort via a user-created Shortcut; a no-op if it is absent.
            run_entry(
                "Toggle Do Not Disturb",
                "Best-effort: runs a Shortcut named \"Toggle Do Not Disturb\" if present",
                "/usr/bin/shortcuts",
                &["run", "Toggle Do Not Disturb"],
            ),
            confirm_entry(
                "Empty Trash",
                "empty the Trash",
                "Permanently delete the Trash contents (asks for Automation on first use)",
                osascript_action("tell application \"Finder\" to empty trash"),
            ),
            confirm_entry(
                "Restart",
                "restart the Mac",
                "Restart the computer (asks for Automation on first use)",
                osascript_action("tell application \"System Events\" to restart"),
            ),
            confirm_entry(
                "Shut Down",
                "shut down the Mac",
                "Power off the computer (asks for Automation on first use)",
                osascript_action("tell application \"System Events\" to shut down"),
            ),
        ];

        if let Some(device) = wifi_device() {
            // Toggle reads-then-sets, so it keeps a small `sh -c`; `device` is a
            // system-provided BSD name (e.g. en0), never user input.
            entries.push(shell_entry(
                "Toggle Wi-Fi",
                "Turn Wi-Fi on or off",
                &toggle_wifi_command(&device),
            ));
            entries.push(run_entry(
                "Wi-Fi On",
                "Turn Wi-Fi on",
                "/usr/sbin/networksetup",
                &["-setairportpower", &device, "on"],
            ));
            entries.push(run_entry(
                "Wi-Fi Off",
                "Turn Wi-Fi off",
                "/usr/sbin/networksetup",
                &["-setairportpower", &device, "off"],
            ));
        }

        // Bluetooth has no permission-free CLI; only offer it when the optional
        // `blueutil` helper is installed (resolved to an absolute path).
        if let Some(blueutil) = cli_path("blueutil") {
            entries.push(run_entry(
                "Toggle Bluetooth",
                "Turn Bluetooth on or off (via blueutil)",
                &blueutil,
                &["-p", "toggle"],
            ));
            entries.push(run_entry(
                "Bluetooth On",
                "Turn Bluetooth on (via blueutil)",
                &blueutil,
                &["-p", "1"],
            ));
            entries.push(run_entry(
                "Bluetooth Off",
                "Turn Bluetooth off (via blueutil)",
                &blueutil,
                &["-p", "0"],
            ));
        }

        // Brightness has no permission-free CLI either; offer it only when the
        // optional `brightness` helper is installed.
        let brightness_cli = cli_path("brightness");
        if let Some(brightness) = &brightness_cli {
            entries.push(run_entry(
                "Brightness Up",
                "Raise display brightness (via brightness)",
                brightness,
                &["+0.1"],
            ));
            entries.push(run_entry(
                "Brightness Down",
                "Lower display brightness (via brightness)",
                brightness,
                &["-0.1"],
            ));
        }

        Self {
            entries,
            brightness_cli,
        }
    }
}

/// Lock the screen without a shell. The classic CGSession helper path contains
/// a space ("Menu Extras"), which a shell would split into two arguments; an
/// argv array passes it as one. Falls back to sleeping the displays when the
/// helper is absent (locks when "require password immediately" is set).
fn lock_action() -> Action {
    const CGSESSION: &str =
        "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession";
    if std::path::Path::new(CGSESSION).exists() {
        Action::Run {
            program: CGSESSION.to_string(),
            args: vec!["-suspend".to_string()],
        }
    } else {
        Action::Run {
            program: "/usr/bin/pmset".to_string(),
            args: vec!["displaysleepnow".to_string()],
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
                    // `level` is a parsed u32, so it is safe to embed literally.
                    let level = level.min(100);
                    let script = format!("set volume output volume {level}");
                    out.push(Item::new(
                        format!("Set volume to {level}%"),
                        "Press Enter to apply",
                        "System",
                        9_000,
                        osascript_action(script),
                    ));
                    return true;
                }
            }
        }

        if let Some(brightness) = &self.brightness_cli {
            if let Some(rest) = lower.strip_prefix("brightness ") {
                if let Ok(level) = rest.trim().parse::<u32>() {
                    let level = level.min(100);
                    let frac = level as f64 / 100.0;
                    out.push(Item::new(
                        format!("Set brightness to {level}%"),
                        "Press Enter to apply (via brightness)",
                        "System",
                        9_000,
                        Action::Run {
                            program: brightness.clone(),
                            args: vec![format!("{frac}")],
                        },
                    ));
                    return true;
                }
            }
        }
        false
    }
}

impl Entry {
    fn new(name: &str, subtitle: &str, action: Action) -> Self {
        Self {
            name: name.to_string(),
            subtitle: subtitle.to_string(),
            action,
        }
    }
}

/// A shell-free entry that runs `program` with a fixed argv array.
fn run_entry(name: &str, subtitle: &str, program: &str, args: &[&str]) -> Entry {
    Entry::new(
        name,
        subtitle,
        Action::Run {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        },
    )
}

fn shell_entry(name: &str, subtitle: &str, command: &str) -> Entry {
    Entry::new(name, subtitle, Action::RunShell(command.to_string()))
}

fn osascript_entry(name: &str, subtitle: &str, script: &str) -> Entry {
    Entry::new(name, subtitle, osascript_action(script))
}

fn confirm_entry(name: &str, label: &str, subtitle: &str, inner: Action) -> Entry {
    Entry::new(
        name,
        subtitle,
        Action::Confirm {
            label: label.to_string(),
            inner: Box::new(inner),
        },
    )
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

/// Resolve an optional helper CLI to its absolute path via `which`, so we can
/// invoke it without a shell (and without depending on the GUI process PATH).
fn cli_path(name: &str) -> Option<String> {
    let output = Command::new("/usr/bin/env").args(["which", name]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}
