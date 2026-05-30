use std::io::Read;

use crate::engine::Provider;
use crate::model::{Action, Item};

/// Developer utilities: base64, URL encode/decode, hashes (MD5/SHA-1/SHA-256),
/// UUID v4, random passwords, lorem ipsum, and JSON pretty/minify. Everything is
/// hand-rolled (or uses `serde_json`, already a dependency) so no new crates are
/// needed and nothing runs until a keyword matches.
pub struct DevToolsProvider;

impl Provider for DevToolsProvider {
    fn name(&self) -> &'static str {
        "devtools"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let (kw, arg) = split_keyword(q);
        match kw.as_str() {
            "base64" | "b64" => {
                // `base64 decode <text>` decodes; otherwise encode.
                if let Some(rest) = arg
                    .strip_prefix("decode ")
                    .or_else(|| arg.strip_prefix("d "))
                {
                    push_decode_base64(out, rest.trim());
                } else if !arg.is_empty() {
                    push_result(out, "Base64 encode", &base64_encode(arg.as_bytes()));
                }
            }
            "base64d" | "b64d" | "unbase64" => {
                if !arg.is_empty() {
                    push_decode_base64(out, &arg);
                }
            }
            "urlencode" | "urlenc" => {
                if !arg.is_empty() {
                    push_result(out, "URL encode", &url_encode(&arg));
                }
            }
            "urldecode" | "urldec" => {
                if !arg.is_empty() {
                    match url_decode(&arg) {
                        Some(s) => push_result(out, "URL decode", &s),
                        None => push_error(out, "URL decode", "Invalid percent-encoding"),
                    }
                }
            }
            "md5" => {
                if !arg.is_empty() {
                    push_result(out, "MD5", &hex(&md5(arg.as_bytes())));
                }
            }
            "sha1" => {
                if !arg.is_empty() {
                    push_result(out, "SHA-1", &hex(&sha1(arg.as_bytes())));
                }
            }
            "sha256" => {
                if !arg.is_empty() {
                    push_result(out, "SHA-256", &hex(&sha256(arg.as_bytes())));
                }
            }
            "uuid" | "guid" => {
                push_result(out, "UUID v4", &uuid_v4());
            }
            "password" | "pass" | "pwd" | "passwd" => {
                let len = arg.split_whitespace().next().and_then(|s| s.parse::<usize>().ok());
                let len = len.unwrap_or(20).clamp(4, 256);
                push_result(out, &format!("Random password ({len})"), &random_password(len));
            }
            "lorem" | "loremipsum" => {
                let words = arg
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(40)
                    .clamp(1, 500);
                push_result(out, &format!("Lorem ipsum ({words} words)"), &lorem(words));
            }
            "json" => {
                if !arg.is_empty() {
                    match pretty_json(&arg) {
                        Ok(s) => push_result(out, "JSON pretty-print", &s),
                        Err(e) => push_error(out, "JSON", &e),
                    }
                }
            }
            "jsonmin" | "minjson" if !arg.is_empty() => match minify_json(&arg) {
                Ok(s) => push_result(out, "JSON minify", &s),
                Err(e) => push_error(out, "JSON", &e),
            },
            _ => {}
        }
    }
}

/// Split the leading keyword token from the rest of the query (the argument).
fn split_keyword(q: &str) -> (String, String) {
    match q.split_once(char::is_whitespace) {
        Some((k, a)) => (k.to_ascii_lowercase(), a.trim().to_string()),
        None => (q.to_ascii_lowercase(), String::new()),
    }
}

fn push_result(out: &mut Vec<Item>, label: &str, value: &str) {
    let display = one_line(value, 120);
    out.push(Item::new(
        display,
        format!("{label} - Enter to copy"),
        "Dev",
        9_200,
        Action::CopyText(value.to_string()),
    ));
}

fn push_error(out: &mut Vec<Item>, label: &str, msg: &str) {
    out.push(Item::new(
        format!("{label}: {msg}"),
        "Could not process the input",
        "Dev",
        9_200,
        Action::None,
    ));
}

fn push_decode_base64(out: &mut Vec<Item>, input: &str) {
    match base64_decode(input) {
        Some(bytes) => match String::from_utf8(bytes) {
            Ok(s) => push_result(out, "Base64 decode", &s),
            Err(_) => push_error(out, "Base64 decode", "Result is not valid UTF-8"),
        },
        None => push_error(out, "Base64 decode", "Invalid base64 input"),
    }
}

/// Collapse newlines/whitespace runs to a single space for the display title.
fn one_line(s: &str, max: usize) -> String {
    let collapsed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > max {
        let truncated: String = collapsed.chars().take(max).collect();
        format!("{truncated}…")
    } else {
        collapsed
    }
}

// --- Random bytes (no crate; reads /dev/urandom) ------------------------------

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    // Fallback: derive from the system clock (only used if /dev/urandom fails).
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15);
    for b in buf.iter_mut() {
        // xorshift64
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        *b = (seed & 0xff) as u8;
    }
    buf
}

fn uuid_v4() -> String {
    let mut b = random_bytes(16);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 10xx
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn random_password(len: usize) -> String {
    const ALPHABET: &[u8] =
        b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_=+";
    // Draw extra bytes and use rejection sampling to avoid modulo bias.
    let mut out = String::with_capacity(len);
    let n = ALPHABET.len() as u8;
    let max = 256u16 / ALPHABET.len() as u16 * ALPHABET.len() as u16;
    while out.len() < len {
        for byte in random_bytes((len - out.len()).max(8)) {
            if (byte as u16) < max {
                out.push(ALPHABET[(byte % n) as usize] as char);
                if out.len() == len {
                    break;
                }
            }
        }
    }
    out
}

// --- Lorem ipsum --------------------------------------------------------------

fn lorem(words: usize) -> String {
    const WORDS: &[&str] = &[
        "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do",
        "eiusmod", "tempor", "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua",
        "enim", "ad", "minim", "veniam", "quis", "nostrud", "exercitation", "ullamco", "laboris",
        "nisi", "aliquip", "ex", "ea", "commodo", "consequat", "duis", "aute", "irure", "in",
        "reprehenderit", "voluptate", "velit", "esse", "cillum", "fugiat", "nulla", "pariatur",
        "excepteur", "sint", "occaecat", "cupidatat", "non", "proident", "sunt", "culpa", "qui",
        "officia", "deserunt", "mollit", "anim", "id", "est", "laborum",
    ];
    let mut out = String::new();
    for i in 0..words {
        let word = WORDS[i % WORDS.len()];
        if i == 0 {
            let mut c = word.chars();
            if let Some(first) = c.next() {
                out.push_str(&first.to_uppercase().to_string());
                out.push_str(c.as_str());
            }
        } else {
            out.push(' ');
            out.push_str(word);
        }
    }
    out.push('.');
    out
}

// --- JSON ---------------------------------------------------------------------

fn pretty_json(input: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

fn minify_json(input: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

// --- Base64 -------------------------------------------------------------------

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut bits: u32 = 0;
    let mut nbits = 0;
    let mut out = Vec::new();
    for c in input.chars() {
        if c == '=' || c.is_whitespace() {
            continue;
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Some(out)
}

// --- URL encode/decode --------------------------------------------------------

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for &byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => {
                out.push('%');
                out.push(hex_upper(byte >> 4));
                out.push(hex_upper(byte & 0x0f));
            }
        }
    }
    out
}

fn url_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hi = hex_val(bytes[i + 1])?;
                let lo = hex_val(bytes[i + 2])?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_upper(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        _ => (b'A' + (v - 10)) as char,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(hex_lower(b >> 4));
        s.push(hex_lower(b & 0x0f));
    }
    s
}

fn hex_lower(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        _ => (b'a' + (v - 10)) as char,
    }
}

// --- MD5 ----------------------------------------------------------------------

fn md5(message: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    let mut msg = message.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    for chunk in msg.chunks(64) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f
                .wrapping_add(a)
                .wrapping_add(K[i])
                .wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

// --- SHA-1 --------------------------------------------------------------------

fn sha1(message: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = message.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// --- SHA-256 ------------------------------------------------------------------

fn sha256(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = message.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }
        for (hi, vi) in h.iter_mut().zip(v.iter()) {
            *hi = hi.wrapping_add(*vi);
        }
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_vectors() {
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn base64_roundtrip() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
    }

    #[test]
    fn url_roundtrip() {
        assert_eq!(url_encode("a b&c"), "a%20b%26c");
        assert_eq!(url_decode("a%20b%26c").unwrap(), "a b&c");
    }
}
