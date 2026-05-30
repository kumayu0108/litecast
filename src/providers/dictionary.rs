use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

use crate::engine::Provider;
use crate::model::{Action, Item};

/// Offline dictionary lookups and spell suggestions.
///   `define <word>` - inline definition via macOS Dictionary Services (if
///                     reachable through python3), plus a Dictionary.app fallback
///   `spell <word>`  - nearest words from /usr/share/dict/words
///
/// No network. Definitions are cached per word; the system word list is loaded
/// once on first use.
pub struct DictionaryProvider {
    def_cache: Mutex<HashMap<String, Option<String>>>,
    words: Mutex<Option<Vec<String>>>,
}

impl DictionaryProvider {
    pub fn new() -> Self {
        Self {
            def_cache: Mutex::new(HashMap::new()),
            words: Mutex::new(None),
        }
    }
}

impl Provider for DictionaryProvider {
    fn name(&self) -> &'static str {
        "dictionary"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();

        if let Some(rest) = lower.strip_prefix("define ").or_else(|| lower.strip_prefix("def ")) {
            let word = rest.trim();
            if word.is_empty() || !word.chars().all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '-') {
                return;
            }
            self.define(word, out);
            return;
        }

        if let Some(rest) = lower.strip_prefix("spell ").or_else(|| lower.strip_prefix("spellcheck ")) {
            let word = rest.trim();
            if word.is_empty() || !word.chars().all(|c| c.is_ascii_alphabetic()) {
                return;
            }
            self.spell(word, out);
        }
    }
}

impl DictionaryProvider {
    fn define(&self, word: &str, out: &mut Vec<Item>) {
        if let Some(def) = self.lookup_definition(word) {
            let one_line = def.split_whitespace().collect::<Vec<_>>().join(" ");
            let display = truncate(&one_line, 140);
            out.push(Item::new(
                display,
                format!("Definition of \"{word}\" - Enter to copy"),
                "Dictionary",
                9_200,
                Action::CopyText(one_line),
            ));
        }
        // Reliable fallback that always works: open Dictionary.app.
        out.push(Item::new(
            format!("Look up \"{word}\" in Dictionary"),
            "Enter to open the macOS Dictionary",
            "Dictionary",
            9_100,
            Action::Open(format!("dict://{}", word.replace(' ', "%20"))),
        ));
    }

    /// Try macOS Dictionary Services via python3 (PyObjC). Returns `None` when
    /// python3/the framework is unavailable. Cached per word.
    fn lookup_definition(&self, word: &str) -> Option<String> {
        if let Some(cached) = self.def_cache.lock().unwrap().get(word) {
            return cached.clone();
        }
        let script = "import sys\nfrom DictionaryServices import DCSCopyTextDefinition\nw=sys.argv[1]\nd=DCSCopyTextDefinition(None,w,(0,len(w)))\nsys.stdout.write(d or '')";
        let result = Command::new("/usr/bin/python3")
            .arg("-c")
            .arg(script)
            .arg(word)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                }
            });
        self.def_cache
            .lock()
            .unwrap()
            .insert(word.to_string(), result.clone());
        result
    }

    fn spell(&self, word: &str, out: &mut Vec<Item>) {
        self.ensure_words();
        let guard = self.words.lock().unwrap();
        let Some(words) = guard.as_ref() else {
            out.push(Item::new(
                "Word list unavailable",
                "/usr/share/dict/words could not be read",
                "Dictionary",
                9_000,
                Action::None,
            ));
            return;
        };
        let lower = word.to_ascii_lowercase();
        if words.binary_search(&lower).is_ok() {
            out.push(Item::new(
                format!("\"{word}\" is spelled correctly"),
                "Found in the system word list",
                "Dictionary",
                9_200,
                Action::CopyText(word.to_string()),
            ));
            return;
        }
        // Candidate set: same first letter and length within ±2, then rank by
        // edit distance. Keeps the scan small and fast.
        let first = lower.chars().next().unwrap_or('a');
        let mut scored: Vec<(usize, &String)> = words
            .iter()
            .filter(|w| {
                w.starts_with(first) && (w.len() as i64 - lower.len() as i64).abs() <= 2
            })
            .map(|w| (levenshtein(&lower, w), w))
            .collect();
        scored.sort_by_key(|(d, _)| *d);
        if scored.is_empty() {
            out.push(Item::new(
                format!("No suggestions for \"{word}\""),
                "Not found in the system word list",
                "Dictionary",
                9_000,
                Action::None,
            ));
            return;
        }
        for (i, (dist, suggestion)) in scored.into_iter().take(6).enumerate() {
            out.push(Item::new(
                suggestion.clone(),
                format!("Suggestion (edit distance {dist}) - Enter to copy"),
                "Dictionary",
                9_100 - i as i64,
                Action::CopyText(suggestion.clone()),
            ));
        }
    }

    fn ensure_words(&self) {
        let mut guard = self.words.lock().unwrap();
        if guard.is_some() {
            return;
        }
        let loaded = std::fs::read_to_string("/usr/share/dict/words").ok().map(|text| {
            let mut v: Vec<String> = text
                .lines()
                .map(|l| l.trim().to_ascii_lowercase())
                .filter(|l| !l.is_empty())
                .collect();
            v.sort();
            v.dedup();
            v
        });
        *guard = loaded;
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
