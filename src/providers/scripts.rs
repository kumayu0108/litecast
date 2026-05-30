use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::engine::{fuzzy_score, keyword_matches, Provider};
use crate::model::{Action, CaptureMode, Item};
use crate::paths::support_dir;

/// Script commands: executable scripts in a watched directory surfaced as
/// runnable launcher commands. Each script may declare metadata in a leading
/// comment header (`# @litecast.title:` etc., tolerant of `#` and `//` comment
/// styles). Running a script executes the file directly via argv (no shell), and
/// its `mode` decides whether stdout is ignored, copied, or shown in a
/// notification.
///
/// Listing is keyword-gated (`scripts`/`script`) plus each script's own keyword,
/// and the parsed set is cached and only re-scanned when the directory's
/// modification time changes, so the default path stays cheap.
pub struct ScriptsProvider {
    dir: PathBuf,
    cache: Mutex<Option<(SystemTime, Vec<Script>)>>,
}

#[derive(Clone)]
struct Script {
    path: String,
    title: String,
    description: String,
    keyword: String,
    mode: CaptureMode,
}

impl ScriptsProvider {
    pub fn new(dir_cfg: &str) -> Self {
        let dir = if dir_cfg.is_empty() {
            support_dir().join("scripts")
        } else {
            let p = PathBuf::from(dir_cfg);
            if p.is_absolute() {
                p
            } else {
                support_dir().join(dir_cfg)
            }
        };
        Self {
            dir,
            cache: Mutex::new(None),
        }
    }

    /// Parsed scripts, re-scanning only when the directory mtime changed.
    fn scripts(&self) -> Vec<Script> {
        let mtime = std::fs::metadata(&self.dir)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if let Ok(guard) = self.cache.lock() {
            if let Some((cached_mtime, scripts)) = guard.as_ref() {
                if *cached_mtime == mtime {
                    return scripts.clone();
                }
            }
        }
        let scripts = scan(&self.dir);
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some((mtime, scripts.clone()));
        }
        scripts
    }
}

impl Provider for ScriptsProvider {
    fn name(&self) -> &'static str {
        "scripts"
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

        let listing = kw == "scripts" || kw == "script";
        let scripts = self.scripts();
        for s in &scripts {
            // A script's own keyword surfaces it directly (highest priority).
            if !s.keyword.is_empty() && keyword_matches(&kw, &s.keyword) {
                out.push(build(s, 9_000));
                continue;
            }
            if listing {
                if arg.is_empty() || fuzzy_score(arg, &s.title).is_some() {
                    out.push(build(s, 8_500));
                }
            } else if let Some(score) = fuzzy_score(q, &s.title) {
                out.push(build(s, 250 + score as i64));
            }
        }
    }
}

fn build(s: &Script, score: i64) -> Item {
    let subtitle = if s.description.is_empty() {
        match s.mode {
            CaptureMode::Silent => "Run script".to_string(),
            CaptureMode::Clipboard => "Run script - copies output".to_string(),
            CaptureMode::Notify => "Run script - notifies with output".to_string(),
        }
    } else {
        s.description.clone()
    };
    let action = match s.mode {
        CaptureMode::Silent => Action::Run {
            program: s.path.clone(),
            args: Vec::new(),
        },
        mode => Action::RunCapture {
            program: s.path.clone(),
            args: Vec::new(),
            mode,
            title: s.title.clone(),
        },
    };
    Item::new(s.title.clone(), subtitle, "Script", score, action)
        .with_id(format!("script:{}", s.path))
}

/// Scan `dir` for executable files and parse each one's metadata header.
fn scan(dir: &PathBuf) -> Vec<Script> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || !is_executable(&meta) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        let meta_hdr = parse_metadata(&path);
        out.push(Script {
            path: path.to_string_lossy().to_string(),
            title: meta_hdr.title.unwrap_or(name),
            description: meta_hdr.description.unwrap_or_default(),
            keyword: meta_hdr.keyword.unwrap_or_default(),
            mode: meta_hdr.mode,
        });
    }
    out.sort_by(|a, b| a.title.to_ascii_lowercase().cmp(&b.title.to_ascii_lowercase()));
    out
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

#[derive(Default)]
struct MetaHeader {
    title: Option<String>,
    description: Option<String>,
    keyword: Option<String>,
    mode: CaptureMode,
}

impl Default for CaptureMode {
    fn default() -> Self {
        CaptureMode::Silent
    }
}

/// Read the leading comment header and pull out `@litecast.*` directives. Reads
/// only the first lines, stopping after a short budget so this never reads large
/// script bodies.
fn parse_metadata(path: &PathBuf) -> MetaHeader {
    let mut hdr = MetaHeader::default();
    let Ok(contents) = std::fs::read_to_string(path) else {
        return hdr;
    };
    for line in contents.lines().take(40) {
        let trimmed = line.trim();
        // Strip a leading comment marker (`#` or `//`); skip the shebang.
        let body = if let Some(rest) = trimmed.strip_prefix("#!") {
            let _ = rest;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("//") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix('#') {
            rest.trim()
        } else if trimmed.is_empty() {
            continue;
        } else {
            // First real code line: metadata header is over.
            break;
        };
        if let Some(v) = body.strip_prefix("@litecast.title:") {
            hdr.title = Some(v.trim().to_string());
        } else if let Some(v) = body.strip_prefix("@litecast.description:") {
            hdr.description = Some(v.trim().to_string());
        } else if let Some(v) = body.strip_prefix("@litecast.keyword:") {
            hdr.keyword = Some(v.trim().to_string());
        } else if let Some(v) = body.strip_prefix("@litecast.mode:") {
            hdr.mode = match v.trim().to_ascii_lowercase().as_str() {
                "clipboard" | "copy" => CaptureMode::Clipboard,
                "notify" | "notification" => CaptureMode::Notify,
                _ => CaptureMode::Silent,
            };
        }
    }
    hdr
}
