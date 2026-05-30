use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde_json::Value;

use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// How long browser history results are cached before a refresh (history is the
/// expensive path: it copies a locked SQLite DB and shells out to `sqlite3`).
const HISTORY_TTL_SECS: u64 = 300;
/// Cap on history rows pulled per profile.
const HISTORY_LIMIT: usize = 1500;
/// Max rows contributed to the result list (the engine truncates further).
const MAX_RESULTS: usize = 8;

/// Chromium-family browsers whose data lives under
/// `~/Library/Application Support/<dir>/<Profile>/{Bookmarks,History}`.
const BROWSER_DIRS: &[&str] = &[
    "Google/Chrome",
    "BraveSoftware/Brave-Browser",
    "Microsoft Edge",
    "Chromium",
    "Vivaldi",
];

struct Cache {
    bookmarks: Vec<(String, String)>,
    bm_sig: u64,
    bm_loaded: bool,
    history: Vec<(String, String)>,
    hist_at: Option<Instant>,
}

/// Searches Chromium-family browser bookmarks (`bm <query>`) and history
/// (`hist <query>`). Keyword-gated so it never touches disk unprompted; results
/// are cached. Safari is not supported (its data needs Full Disk Access).
pub struct BookmarksProvider {
    cache: Mutex<Cache>,
}

impl BookmarksProvider {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(Cache {
                bookmarks: Vec::new(),
                bm_sig: 0,
                bm_loaded: false,
                history: Vec::new(),
                hist_at: None,
            }),
        }
    }
}

impl Provider for BookmarksProvider {
    fn name(&self) -> &'static str {
        "bookmarks"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if let Some(filter) = strip_keyword(q, "bm") {
            let entries = self.bookmarks();
            push_matches(&entries, filter, "Bookmark", "bm:", out);
        } else if let Some(filter) = strip_keyword(q, "hist") {
            let entries = self.history();
            push_matches(&entries, filter, "History", "hist:", out);
        }
    }
}

impl BookmarksProvider {
    /// Bookmarks, reparsed only when a bookmark file's mtime changes.
    fn bookmarks(&self) -> Vec<(String, String)> {
        let files = collect_files("Bookmarks");
        let sig = mtime_signature(&files);
        let mut cache = match self.cache.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        if !cache.bm_loaded || cache.bm_sig != sig {
            let mut entries = Vec::new();
            for file in &files {
                if let Ok(text) = std::fs::read_to_string(file) {
                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                        if let Some(roots) = json.get("roots") {
                            for (_, node) in roots.as_object().into_iter().flatten() {
                                collect_bookmarks(node, &mut entries);
                            }
                        }
                    }
                }
            }
            cache.bookmarks = entries;
            cache.bm_sig = sig;
            cache.bm_loaded = true;
        }
        cache.bookmarks.clone()
    }

    /// History, refreshed at most once per `HISTORY_TTL_SECS`.
    fn history(&self) -> Vec<(String, String)> {
        let mut cache = match self.cache.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let fresh = cache
            .hist_at
            .map(|t| t.elapsed().as_secs() < HISTORY_TTL_SECS)
            .unwrap_or(false);
        if !fresh {
            cache.history = load_history();
            cache.hist_at = Some(Instant::now());
        }
        cache.history.clone()
    }
}

/// Returns the text after `keyword` (`keyword` alone yields ""), else `None`.
fn strip_keyword<'a>(q: &'a str, keyword: &str) -> Option<&'a str> {
    let lower = q.to_ascii_lowercase();
    if lower == keyword {
        Some("")
    } else if lower.starts_with(keyword) && q.as_bytes().get(keyword.len()) == Some(&b' ') {
        Some(q[keyword.len() + 1..].trim_start())
    } else {
        None
    }
}

/// Score, sort, and push the top matches as `Action::Open` rows.
fn push_matches(
    entries: &[(String, String)],
    filter: &str,
    source: &'static str,
    id_prefix: &str,
    out: &mut Vec<Item>,
) {
    let mut scored: Vec<(i64, &(String, String))> = Vec::new();
    for entry in entries {
        let (title, url) = entry;
        let score = if filter.is_empty() {
            7_000
        } else {
            let haystack = format!("{title} {url}");
            match fuzzy_score(filter, &haystack) {
                Some(s) => 7_000 + s as i64,
                None => continue,
            }
        };
        scored.push((score, entry));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    for (score, (title, url)) in scored.into_iter().take(MAX_RESULTS) {
        let display = if title.is_empty() { url.clone() } else { title.clone() };
        out.push(
            Item::new(display, url.clone(), source, score, Action::Open(url.clone()))
                .with_id(format!("{id_prefix}{url}")),
        );
    }
}

/// Recursively collect `{type:"url"}` nodes from a Chromium bookmark tree.
fn collect_bookmarks(node: &Value, out: &mut Vec<(String, String)>) {
    match node.get("type").and_then(|t| t.as_str()) {
        Some("url") => {
            if let Some(url) = node.get("url").and_then(|u| u.as_str()) {
                let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
                out.push((name.to_string(), url.to_string()));
            }
        }
        Some("folder") => {
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    collect_bookmarks(child, out);
                }
            }
        }
        _ => {}
    }
}

/// All existing profile files named `name` across the supported browsers.
fn collect_files(name: &str) -> Vec<PathBuf> {
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return Vec::new(),
    };
    let support = home.join("Library/Application Support");
    let mut files = Vec::new();
    for browser in BROWSER_DIRS {
        let base = support.join(browser);
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            // Profile dirs: "Default", "Profile 1", etc. Check for the file.
            let candidate = entry.path().join(name);
            if candidate.is_file() {
                files.push(candidate);
            }
        }
    }
    files
}

/// Sum of file modification times (seconds), used as a cheap change signature.
fn mtime_signature(files: &[PathBuf]) -> u64 {
    let mut sig = 0u64;
    for file in files {
        if let Ok(meta) = std::fs::metadata(file) {
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    sig = sig.wrapping_add(dur.as_secs());
                }
            }
        }
    }
    sig
}

/// Copy each (possibly locked) History DB to a temp file and query it with the
/// system `sqlite3`. Returns merged (title, url) rows. Empty if sqlite3 absent.
fn load_history() -> Vec<(String, String)> {
    if !PathBuf::from("/usr/bin/sqlite3").is_file() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let tmp_dir = std::env::temp_dir();
    for (i, db) in collect_files("History").into_iter().enumerate() {
        let tmp = tmp_dir.join(format!("litecast-hist-{i}.db"));
        if std::fs::copy(&db, &tmp).is_err() {
            continue;
        }
        let sql = format!(
            "SELECT IFNULL(title,''), url FROM urls ORDER BY last_visit_time DESC LIMIT {HISTORY_LIMIT}"
        );
        let output = std::process::Command::new("/usr/bin/sqlite3")
            .arg("-readonly")
            .arg("-separator")
            .arg("\t")
            .arg(&tmp)
            .arg(&sql)
            .output();
        let _ = std::fs::remove_file(&tmp);
        if let Ok(out) = output {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some((title, url)) = line.split_once('\t') {
                    if !url.is_empty() {
                        rows.push((title.to_string(), url.to_string()));
                    }
                }
            }
        }
    }
    rows
}
