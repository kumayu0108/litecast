use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::engine::Provider;
use crate::model::{Action, Item};

/// File power actions and recent-file listings. Keyword-gated so nothing scans
/// the disk on the default path:
///   `recent`               - recently modified files across Desktop/Downloads/Documents
///   `downloads`            - newest items in ~/Downloads
///   `reveal <path>`        - reveal in Finder (`open -R`)
///   `ql <path>`            - Quick Look preview (`qlmanage -p`)
///   `copypath <path>`      - copy the POSIX path to the clipboard
///   `folder <path>`        - open the enclosing folder
pub struct FileActionsProvider;

impl Provider for FileActionsProvider {
    fn name(&self) -> &'static str {
        "fileactions"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let (kw, arg) = match q.split_once(char::is_whitespace) {
            Some((k, a)) => (k.to_ascii_lowercase(), a.trim().to_string()),
            None => (q.to_ascii_lowercase(), String::new()),
        };

        match kw.as_str() {
            "recent" | "recents" => list_recent(out),
            "downloads" | "dls" => list_downloads(out),
            "reveal" if !arg.is_empty() => push_path_action(
                out,
                &arg,
                "Reveal in Finder",
                |p| Action::Run {
                    program: "/usr/bin/open".to_string(),
                    args: vec!["-R".to_string(), p.to_string()],
                },
            ),
            "ql" | "quicklook" | "preview" if !arg.is_empty() => push_path_action(
                out,
                &arg,
                "Quick Look preview",
                |p| Action::Run {
                    program: "/usr/bin/qlmanage".to_string(),
                    args: vec!["-p".to_string(), p.to_string()],
                },
            ),
            "copypath" | "cppath" if !arg.is_empty() => {
                let expanded = expand_tilde(&arg);
                out.push(Item::new(
                    format!("Copy path: {expanded}"),
                    "Enter to copy the POSIX path",
                    "File",
                    9_000,
                    Action::CopyText(expanded),
                ));
            }
            "folder" | "enclosing" if !arg.is_empty() => {
                let expanded = expand_tilde(&arg);
                let parent = Path::new(&expanded)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(expanded);
                out.push(Item::new(
                    format!("Open enclosing folder: {parent}"),
                    "Enter to open in Finder",
                    "File",
                    9_000,
                    Action::Open(parent),
                ));
            }
            _ => {}
        }
    }
}

fn list_downloads(out: &mut Vec<Item>) {
    if let Some(home) = home_dir() {
        let entries = newest_in(&home.join("Downloads"), 8);
        if entries.is_empty() {
            out.push(Item::new(
                "No recent downloads",
                "~/Downloads is empty or unreadable",
                "File",
                8_900,
                Action::None,
            ));
        }
        for (i, path) in entries.into_iter().enumerate() {
            out.push(file_item(&path, 9_000 - i as i64));
        }
    }
}

fn list_recent(out: &mut Vec<Item>) {
    let Some(home) = home_dir() else {
        return;
    };
    let mut all: Vec<(SystemTime, PathBuf)> = Vec::new();
    for sub in ["Desktop", "Downloads", "Documents"] {
        collect_with_mtime(&home.join(sub), &mut all);
    }
    all.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    if all.is_empty() {
        out.push(Item::new(
            "No recent files found",
            "Could not read Desktop/Downloads/Documents",
            "File",
            8_900,
            Action::None,
        ));
    }
    for (i, (_, path)) in all.into_iter().take(8).enumerate() {
        out.push(file_item(&path, 9_000 - i as i64));
    }
}

fn file_item(path: &Path, score: i64) -> Item {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();
    let path_str = path.to_string_lossy().to_string();
    Item::new(name, path_str.clone(), "File", score, Action::Open(path_str.clone()))
        .with_icon(path_str.clone())
        .with_id(format!("file:{path_str}"))
}

fn push_path_action(
    out: &mut Vec<Item>,
    arg: &str,
    label: &str,
    make_action: impl Fn(&str) -> Action,
) {
    let expanded = expand_tilde(arg);
    out.push(Item::new(
        format!("{label}: {expanded}"),
        "Press Enter",
        "File",
        9_000,
        make_action(&expanded),
    ));
}

/// Newest non-hidden entries in `dir`, by modification time.
fn newest_in(dir: &Path, limit: usize) -> Vec<PathBuf> {
    let mut entries: Vec<(SystemTime, PathBuf)> = Vec::new();
    collect_with_mtime(dir, &mut entries);
    entries.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    entries.into_iter().take(limit).map(|(_, p)| p).collect()
}

fn collect_with_mtime(dir: &Path, out: &mut Vec<(SystemTime, PathBuf)>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                out.push((mtime, path));
            }
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn expand_tilde(path: &str) -> String {
    let path = path.trim();
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{rest}", home.to_string_lossy());
        }
    }
    if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return home.to_string_lossy().to_string();
        }
    }
    path.to_string()
}

