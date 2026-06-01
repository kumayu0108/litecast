use std::process::Command;

use crate::engine::Provider;
use crate::model::{Action, Item};
use crate::paths::support_dir;

/// Color, number-base, and epoch/timestamp conversions. All offline and
/// hand-rolled; the only subprocess is `date` for epoch<->human formatting
/// (a built-in CLI), and it only runs when an epoch keyword matches.
pub struct ConvertersProvider;

impl Provider for ConvertersProvider {
    fn name(&self) -> &'static str {
        "converters"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        if try_color(q, out) {
            return;
        }
        if try_number_base(q, out) {
            return;
        }
        try_epoch(q, out);
    }
}

// --- Color --------------------------------------------------------------------

fn try_color(q: &str, out: &mut Vec<Item>) -> bool {
    let lower = q.to_ascii_lowercase();
    let lower = lower.trim();
    let Some((r, g, b)) = parse_color(lower) else {
        return false;
    };
    // Explicit forms (#hex, 0x, rgb(...), bare hex) are unambiguous and rank
    // high; a bare color name ("red") ranks modestly so it never buries apps.
    let explicit = lower.starts_with('#')
        || lower.starts_with("0x")
        || lower.starts_with("rgb(")
        || (lower.chars().all(|c| c.is_ascii_hexdigit()) && (lower.len() == 3 || lower.len() == 6));
    let base = if explicit { 9_400 } else { 240 };

    let hex = format!("#{r:02X}{g:02X}{b:02X}");
    let rgb = format!("rgb({r}, {g}, {b})");
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let hsl = format!("hsl({h}, {s}%, {l}%)");
    let detail = format!("{hex}  •  {rgb}  •  {hsl}");

    let mut item = Item::new(
        format!("{hex}   {rgb}"),
        format!("{hsl} - Enter to copy {hex}"),
        "Color",
        base,
        Action::CopyText(hex.clone()),
    );
    if let Some(path) = swatch_path(r, g, b) {
        item = item.with_icon(path);
    }
    out.push(item);
    // Secondary rows so each representation is copyable.
    out.push(Item::new(
        rgb.clone(),
        format!("RGB - Enter to copy  ({detail})"),
        "Color",
        base - 10,
        Action::CopyText(rgb),
    ));
    out.push(Item::new(
        hsl.clone(),
        "HSL - Enter to copy".to_string(),
        "Color",
        base - 20,
        Action::CopyText(hsl),
    ));
    true
}

/// Parse `#RRGGBB`, `#RGB`, `rgb(r,g,b)`, or a small set of named colors.
fn parse_color(input: &str) -> Option<(u8, u8, u8)> {
    let s = input.trim();
    if let Some(hex) = s.strip_prefix('#').or_else(|| s.strip_prefix("0x")) {
        return parse_hex(hex);
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some((r, g, b));
        }
        return None;
    }
    // Named color: only treat single bare words as colors to avoid hijacking
    // normal queries.
    if s.chars().all(|c| c.is_ascii_alphabetic()) {
        return named_color(s);
    }
    // Bare 6/3-digit hex without `#`.
    if (s.len() == 6 || s.len() == 3) && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return parse_hex(s);
    }
    None
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim();
    // Guard against multibyte / non-hex input (e.g. "#aé"): byte-slicing such a
    // string can land on a non-char boundary and panic. Operate on bytes only
    // once we've confirmed every byte is an ASCII hex digit.
    let bytes = hex.as_bytes();
    if !bytes.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match bytes.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn named_color(name: &str) -> Option<(u8, u8, u8)> {
    let c = match name {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "cyan" | "aqua" => (0, 255, 255),
        "magenta" | "fuchsia" => (255, 0, 255),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "maroon" => (128, 0, 0),
        "olive" => (128, 128, 0),
        "navy" => (0, 0, 128),
        "teal" => (0, 128, 128),
        "purple" => (128, 0, 128),
        "orange" => (255, 165, 0),
        "pink" => (255, 192, 203),
        "brown" => (165, 42, 42),
        "gold" => (255, 215, 0),
        "indigo" => (75, 0, 130),
        "violet" => (238, 130, 238),
        "coral" => (255, 127, 80),
        "salmon" => (250, 128, 114),
        "crimson" => (220, 20, 60),
        _ => return None,
    };
    Some(c)
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u32, u32, u32) {
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    let (h, s) = if (max - min).abs() < f64::EPSILON {
        (0.0, 0.0)
    } else {
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if max == rf {
            (gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }
        } else if max == gf {
            (bf - rf) / d + 2.0
        } else {
            (rf - gf) / d + 4.0
        };
        (h / 6.0, s)
    };
    (
        (h * 360.0).round() as u32,
        (s * 100.0).round() as u32,
        (l * 100.0).round() as u32,
    )
}

/// Write (or reuse) a small solid-color BMP under the support dir so the result
/// row can show a swatch via the normal file-icon path. BMP is uncompressed, so
/// no encoder/crate is needed and NSImage loads it natively.
fn swatch_path(r: u8, g: u8, b: u8) -> Option<String> {
    let dir = support_dir().join("swatches");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{r:02X}{g:02X}{b:02X}.bmp"));
    if !path.exists() {
        let bmp = solid_bmp(r, g, b, 24, 24);
        std::fs::write(&path, bmp).ok()?;
    }
    Some(path.to_string_lossy().to_string())
}

fn solid_bmp(r: u8, g: u8, b: u8, w: u32, h: u32) -> Vec<u8> {
    let row_raw = (w * 3) as usize;
    let pad = (4 - (row_raw % 4)) % 4;
    let row = row_raw + pad;
    let pixel_data = row * h as usize;
    let file_size = 54 + pixel_data;
    let mut out = Vec::with_capacity(file_size);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_data as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..h {
        for _ in 0..w {
            out.push(b); // BGR order
            out.push(g);
            out.push(r);
        }
        out.resize(out.len() + pad, 0);
    }
    out
}

// --- Number base --------------------------------------------------------------

fn try_number_base(q: &str, out: &mut Vec<Item>) -> bool {
    let lower = q.to_ascii_lowercase();
    let (left, target) = match split_sep(&lower, &[" to ", " in ", " as "]) {
        Some(x) => x,
        None => return false,
    };
    let target = target.trim();
    let base = match target {
        "dec" | "decimal" | "base10" => 10u32,
        "hex" | "hexadecimal" | "base16" => 16,
        "bin" | "binary" | "base2" => 2,
        "oct" | "octal" | "base8" => 8,
        _ => return false,
    };
    let Some(value) = parse_integer(left.trim()) else {
        return false;
    };
    let formatted = match base {
        10 => value.to_string(),
        16 => format!("0x{value:X}"),
        2 => format!("0b{value:b}"),
        8 => format!("0o{value:o}"),
        _ => return false,
    };
    out.push(Item::new(
        format!("= {formatted}"),
        format!("{value} (decimal) - Enter to copy"),
        "Convert",
        9_450,
        Action::CopyText(formatted),
    ));
    true
}

/// Parse an integer with optional `0x`/`0b`/`0o` prefix (decimal otherwise).
fn parse_integer(s: &str) -> Option<i128> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    let value = if let Some(h) = s.strip_prefix("0x") {
        i128::from_str_radix(h, 16).ok()?
    } else if let Some(b) = s.strip_prefix("0b") {
        i128::from_str_radix(b, 2).ok()?
    } else if let Some(o) = s.strip_prefix("0o") {
        i128::from_str_radix(o, 8).ok()?
    } else {
        s.parse::<i128>().ok()?
    };
    Some(if neg { -value } else { value })
}

fn split_sep<'a>(q: &'a str, seps: &[&str]) -> Option<(&'a str, &'a str)> {
    for sep in seps {
        if let Some(idx) = q.find(sep) {
            return Some((&q[..idx], &q[idx + sep.len()..]));
        }
    }
    None
}

// --- Epoch / timestamp --------------------------------------------------------

fn try_epoch(q: &str, out: &mut Vec<Item>) -> bool {
    let lower = q.to_ascii_lowercase();
    let lower = lower.trim();

    // Current epoch.
    if matches!(
        lower,
        "now epoch" | "epoch now" | "now unix" | "unix now" | "epoch" | "now timestamp" | "timestamp now"
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(Item::new(
            format!("= {now}"),
            "Current Unix time (seconds) - Enter to copy",
            "Time",
            9_450,
            Action::CopyText(now.to_string()),
        ));
        return true;
    }

    // `epoch <n>` / `timestamp <n>` / `unix <n>` -> human readable.
    for kw in ["epoch ", "timestamp ", "unix "] {
        if let Some(rest) = lower.strip_prefix(kw) {
            let digits: String = rest.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                continue;
            }
            if let Ok(mut secs) = digits.parse::<i64>() {
                // Heuristic: 13-digit values are milliseconds.
                if digits.len() >= 13 {
                    secs /= 1000;
                }
                let local = date_format(secs, false);
                let utc = date_format(secs, true);
                if let Some(local) = local {
                    let copy = local.clone();
                    out.push(Item::new(
                        local,
                        match &utc {
                            Some(u) => format!("UTC: {u} - Enter to copy"),
                            None => "Local time - Enter to copy".to_string(),
                        },
                        "Time",
                        9_450,
                        Action::CopyText(copy),
                    ));
                    return true;
                }
            }
        }
    }
    false
}

/// Format a Unix timestamp using the built-in `date` CLI (handles the local TZ
/// and DST correctly). Returns `None` if `date` is unavailable.
fn date_format(secs: i64, utc: bool) -> Option<String> {
    let mut cmd = Command::new("/bin/date");
    if utc {
        cmd.arg("-u");
    }
    let output = cmd
        .arg("-r")
        .arg(secs.to_string())
        .arg("+%Y-%m-%d %H:%M:%S %Z")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_never_panics_on_bad_input() {
        // Multibyte / non-hex inputs must return None rather than panicking on a
        // non-char-boundary byte slice.
        assert_eq!(parse_hex("aé"), None);
        assert_eq!(parse_hex("zz"), None);
        assert_eq!(parse_hex(""), None);
        // Valid forms still parse.
        assert_eq!(parse_hex("fff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("000000"), Some((0, 0, 0)));
    }

    #[test]
    fn parse_color_handles_multibyte_input() {
        // The full color path with the `#` prefix used to panic on `#aé`.
        assert_eq!(parse_color("#aé"), None);
        assert_eq!(parse_color("#zz"), None);
        assert_eq!(parse_color("#"), None);
        assert_eq!(parse_color("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_color("#ffffff"), Some((255, 255, 255)));
    }
}
