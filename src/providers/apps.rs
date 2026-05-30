use std::path::PathBuf;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::engine::Provider;
use crate::model::{Action, Item};

struct AppEntry {
    name: String,
    path: PathBuf,
}

pub struct AppsProvider {
    apps: Vec<AppEntry>,
}

impl AppsProvider {
    pub fn new() -> Self {
        Self { apps: scan_apps() }
    }
}

impl Provider for AppsProvider {
    fn name(&self) -> &'static str {
        "apps"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        for app in &self.apps {
            let haystack = Utf32Str::new(&app.name, &mut buf);
            if let Some(score) = pattern.score(haystack, &mut matcher) {
                // Apps are high-value launch targets; bias their score up.
                let ranked = score as i64 + 100;
                out.push(Item::new(
                    app.name.clone(),
                    app.path.display().to_string(),
                    "App",
                    ranked,
                    Action::Open(app.path.display().to_string()),
                ));
            }
        }
    }
}

fn scan_apps() -> Vec<AppEntry> {
    let mut roots: Vec<PathBuf> = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/Applications/Utilities"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(format!("{home}/Applications")));
    }

    let mut apps = Vec::new();
    for root in roots {
        collect_apps(&root, &mut apps);
    }
    apps
}

fn collect_apps(dir: &PathBuf, apps: &mut Vec<AppEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                apps.push(AppEntry {
                    name: name.to_string(),
                    path,
                });
            }
        }
    }
}
