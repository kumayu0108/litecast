use crate::engine::Provider;
use crate::model::{Action, Item};

/// Text transforms: case changes, slugify, sort/dedupe lines, trim, reverse.
/// Keyword-gated (`text` / `transform`); Enter copies the result.
pub struct TextTransformProvider;

impl Provider for TextTransformProvider {
    fn name(&self) -> &'static str {
        "text"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let (kw, arg) = match q.split_once(char::is_whitespace) {
            Some((k, a)) => (k.to_ascii_lowercase(), a.trim()),
            None => (q.to_ascii_lowercase(), ""),
        };

        let (name, result) = match kw.as_str() {
            "upper" | "uppercase" | "upcase" if !arg.is_empty() => {
                ("Uppercase", arg.to_uppercase())
            }
            "lower" | "lowercase" | "downcase" if !arg.is_empty() => {
                ("Lowercase", arg.to_lowercase())
            }
            "title" | "titlecase" if !arg.is_empty() => ("Title case", title_case(arg)),
            "slug" | "slugify" if !arg.is_empty() => ("Slug", slugify(arg)),
            "sort" | "sortlines" if !arg.is_empty() => ("Sort lines", sort_lines(arg)),
            "dedupe" | "unique" | "uniq" if !arg.is_empty() => ("Dedupe lines", dedupe_lines(arg)),
            "trim" if !arg.is_empty() => ("Trim", arg.trim().to_string()),
            "reverse" | "rev" if !arg.is_empty() => ("Reverse", arg.chars().rev().collect()),
            "text" | "transform" | "case" => {
                if arg.is_empty() {
                    out.push(Item::new(
                        "Text transforms",
                        "Try: upper hello, slug My Title, sort lines, dedupe lines",
                        "Text",
                        8_200,
                        Action::None,
                    ));
                    return;
                }
                let inner = arg.split_once(char::is_whitespace);
                if let Some((sub, rest)) = inner {
                    let sub = sub.to_ascii_lowercase();
                    let rest = rest.trim();
                    if rest.is_empty() {
                        return;
                    }
                    if let Some((n, r)) = match sub.as_str() {
                        "upper" | "uppercase" => Some(("Uppercase", arg_to_upper(rest))),
                        "lower" | "lowercase" => Some(("Lowercase", rest.to_lowercase())),
                        "title" => Some(("Title case", title_case(rest))),
                        "slug" | "slugify" => Some(("Slug", slugify(rest))),
                        "sort" => Some(("Sort lines", sort_lines(rest))),
                        "dedupe" | "unique" => Some(("Dedupe lines", dedupe_lines(rest))),
                        "trim" => Some(("Trim", rest.trim().to_string())),
                        "reverse" => Some(("Reverse", rest.chars().rev().collect())),
                        _ => None,
                    } {
                        push_result(out, n, &r);
                    }
                }
                return;
            }
            _ => return,
        };

        push_result(out, name, &result);
    }
}

fn arg_to_upper(s: &str) -> String {
    s.to_uppercase()
}

fn push_result(out: &mut Vec<Item>, name: &str, text: &str) {
    let preview: String = text.lines().take(2).collect::<Vec<_>>().join(" / ");
    let sub = if preview.chars().count() > 72 {
        format!("{}… - Enter to copy", preview.chars().take(72).collect::<String>())
    } else {
        format!("{preview} - Enter to copy")
    };
    out.push(Item::new(
        format!("{name}"),
        sub,
        "Text",
        8_500,
        Action::CopyText(text.to_string()),
    ));
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn slugify(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn sort_lines(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().collect();
    lines.sort();
    lines.join("\n")
}

fn dedupe_lines(s: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in s.lines() {
        if seen.insert(line) {
            out.push(line);
        }
    }
    out.join("\n")
}
