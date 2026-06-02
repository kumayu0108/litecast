//! Global hotkey parsing and registration.

use std::collections::HashMap;

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyManager,
};

use crate::config::{CommandConfig, Config, HotkeyConfig};
use crate::debug_log;
use crate::model::Action;

/// Live hotkey ids used by the background listener thread.
#[derive(Default)]
pub struct HotkeyIds {
    pub toggle: u32,
    pub screenshot: u32,
    pub custom: HashMap<u32, Action>,
}

/// Parse a hotkey combo like "Cmd+Shift+S" into a `HotKey`.
pub fn parse_hotkey_combo(combo: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in combo.split('+') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        match token.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" | "win" | "meta" => mods |= Modifiers::META,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" | "opt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            other => {
                if code.is_some() {
                    return None;
                }
                code = parse_key_code(other);
                code?;
            }
        }
    }
    let code = code?;
    if mods.is_empty() {
        return None;
    }
    Some(HotKey::new(Some(mods), code))
}

fn parse_key_code(token: &str) -> Option<Code> {
    use Code::*;
    let upper = token.to_ascii_uppercase();
    Some(match upper.as_str() {
        "A" => KeyA,
        "B" => KeyB,
        "C" => KeyC,
        "D" => KeyD,
        "E" => KeyE,
        "F" => KeyF,
        "G" => KeyG,
        "H" => KeyH,
        "I" => KeyI,
        "J" => KeyJ,
        "K" => KeyK,
        "L" => KeyL,
        "M" => KeyM,
        "N" => KeyN,
        "O" => KeyO,
        "P" => KeyP,
        "Q" => KeyQ,
        "R" => KeyR,
        "S" => KeyS,
        "T" => KeyT,
        "U" => KeyU,
        "V" => KeyV,
        "W" => KeyW,
        "X" => KeyX,
        "Y" => KeyY,
        "Z" => KeyZ,
        "0" => Digit0,
        "1" => Digit1,
        "2" => Digit2,
        "3" => Digit3,
        "4" => Digit4,
        "5" => Digit5,
        "6" => Digit6,
        "7" => Digit7,
        "8" => Digit8,
        "9" => Digit9,
        "F1" => F1,
        "F2" => F2,
        "F3" => F3,
        "F4" => F4,
        "F5" => F5,
        "F6" => F6,
        "F7" => F7,
        "F8" => F8,
        "F9" => F9,
        "F10" => F10,
        "F11" => F11,
        "F12" => F12,
        "SPACE" => Space,
        "ENTER" | "RETURN" => Enter,
        "TAB" => Tab,
        "ESC" | "ESCAPE" => Escape,
        "UP" => ArrowUp,
        "DOWN" => ArrowDown,
        "LEFT" => ArrowLeft,
        "RIGHT" => ArrowRight,
        "MINUS" | "-" => Minus,
        "EQUAL" | "=" => Equal,
        "COMMA" | "," => Comma,
        "PERIOD" | "." => Period,
        "SLASH" | "/" => Slash,
        "BACKSLASH" | "\\" => Backslash,
        "SEMICOLON" | ";" => Semicolon,
        "QUOTE" | "'" => Quote,
        "BACKQUOTE" | "`" => Backquote,
        "LEFTBRACKET" | "[" => BracketLeft,
        "RIGHTBRACKET" | "]" => BracketRight,
        _ => return None,
    })
}

pub fn resolve_hotkey_action(hk: &HotkeyConfig, commands: &[CommandConfig]) -> Option<Action> {
    match hk.kind.as_str() {
        "open" => Some(Action::Open(hk.target.clone())),
        "shell" => Some(Action::RunShell(hk.target.clone())),
        "command" => {
            let cmd = commands.iter().find(|c| c.name == hk.target)?;
            let target = cmd.target.replace("{}", "");
            Some(match cmd.kind.as_str() {
                "shell" => Action::RunShell(target),
                _ => Action::Open(target),
            })
        }
        _ => None,
    }
}

/// Register toggle, screenshot, and custom hotkeys. Returns the manager and id map.
pub fn register_all(
    config: &Config,
) -> Result<(GlobalHotKeyManager, HotkeyIds), String> {
    let manager =
        GlobalHotKeyManager::new().map_err(|e| format!("failed to create hotkey manager: {e}"))?;
    let mut ids = HotkeyIds::default();

    let toggle_hotkey = parse_hotkey_combo(&config.hotkey.toggle).unwrap_or_else(|| {
        if !config.hotkey.toggle.is_empty() {
            eprintln!(
                "[litecast] invalid [hotkey] toggle {:?}; using Option+Space",
                config.hotkey.toggle
            );
        }
        HotKey::new(Some(Modifiers::ALT), Code::Space)
    });
    let shot_hotkey = parse_hotkey_combo(&config.hotkey.screenshot).unwrap_or_else(|| {
        if !config.hotkey.screenshot.is_empty() {
            eprintln!(
                "[litecast] invalid [hotkey] screenshot {:?}; using Option+Shift+Space",
                config.hotkey.screenshot
            );
        }
        HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space)
    });

    manager
        .register(toggle_hotkey)
        .map_err(|e| format!("toggle hotkey: {e}"))?;
    manager
        .register(shot_hotkey)
        .map_err(|e| format!("screenshot hotkey: {e}"))?;
    ids.toggle = toggle_hotkey.id();
    ids.screenshot = shot_hotkey.id();
    // DEBUG-TEMP
    debug_log::log(
        "hotkeys::register_all",
        "built-in registered",
        &format!(
            r#"{{"toggle_combo":"{}","toggle_id":{},"screenshot_combo":"{}","screenshot_id":{}}}"#,
            config.hotkey.toggle,
            ids.toggle,
            config.hotkey.screenshot,
            ids.screenshot,
        ),
    );

    for hk in &config.hotkeys {
        let Some(parsed) = parse_hotkey_combo(&hk.key) else {
            eprintln!("[litecast] skipping hotkey with invalid combo: {:?}", hk.key);
            continue;
        };
        let Some(action) = resolve_hotkey_action(hk, &config.commands) else {
            eprintln!(
                "[litecast] skipping hotkey {:?}: unknown kind/target ({}: {})",
                hk.key, hk.kind, hk.target
            );
            continue;
        };
        match manager.register(parsed) {
            Ok(()) => {
                ids.custom.insert(parsed.id(), action);
            }
            Err(e) => eprintln!("[litecast] failed to register custom hotkey {}: {e}", hk.key),
        }
    }

    Ok((manager, ids))
}
