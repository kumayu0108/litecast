use crate::engine::Provider;
use crate::model::{Action, Item};

/// Fallback provider that offers a web search for the current query, opening the
/// default browser. Low score so local results (apps, files, calc) rank higher.
pub struct WebSearchProvider {
    /// URL template containing `{}` where the encoded query is inserted.
    template: String,
}

impl WebSearchProvider {
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }
}

impl Provider for WebSearchProvider {
    fn name(&self) -> &'static str {
        "web"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let url = self.template.replace("{}", &percent_encode(q));
        out.push(Item::new(
            format!("Search the web for \"{q}\""),
            "Open in your default browser",
            "Web",
            // Always available, but should sit below meaningful local matches.
            5,
            Action::Open(url),
        ));
    }
}

/// Minimal percent-encoding for query strings (RFC 3986 unreserved kept as-is).
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for &byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + (value - 10)) as char,
    }
}
