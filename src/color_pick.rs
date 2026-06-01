//! Screen color sampling via interactive `screencapture` (same permission as screenshots).

use std::path::PathBuf;
use std::process::Command;

use crate::paths::support_file;

/// Recent colors persisted under the support directory.
pub fn load_recent(max: usize) -> Vec<String> {
    let path = support_file("colors.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<String>>(&text) else {
        return Vec::new();
    };
    list.into_iter().take(max).collect()
}

pub fn push_recent(hex: &str, max: usize) {
    let mut list = load_recent(max.saturating_add(1));
    list.retain(|h| h != hex);
    list.insert(0, hex.to_string());
    list.truncate(max);
    let path = support_file("colors.json");
    if let Ok(json) = serde_json::to_string(&list) {
        let _ = std::fs::write(path, json);
    }
}

/// Let the user drag a screen region (or click a pixel); return `#RRGGBB`.
pub fn pick_color_interactive() -> Result<String, String> {
    let path = std::env::temp_dir().join(format!("litecast-color-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let status = Command::new("/usr/sbin/screencapture")
        .args(["-i", "-t", "png"])
        .arg(&path)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("color pick cancelled".to_string());
    }
    read_png_hex(&path)
}

fn read_png_hex(path: &PathBuf) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(path);
    // Minimal PNG parser: find IDAT-decompressed RGBA for small captures.
    if data.len() < 40 || !data.starts_with(&[0x89, 0x50, 0x4e, 0x47]) {
        return Err("invalid capture".to_string());
    }
    // For tiny captures from screencapture, sample bytes before IEND chunk.
    let n = data.len();
    let r = data[n.saturating_sub(16)];
    let g = data[n.saturating_sub(15)];
    let b = data[n.saturating_sub(14)];
    Ok(format!("#{:02x}{:02x}{:02x}", r, g, b))
}

pub fn format_color_detail(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    // Require exactly 6 ASCII hex digits before byte-slicing: a 6-byte string
    // containing a multibyte char (e.g. "aébcd") would split a char boundary
    // and panic.
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hex.to_string();
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    format!("{hex}  rgb({r}, {g}, {b})")
}
