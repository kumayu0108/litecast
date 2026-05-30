use std::path::Path;
use std::process::Command;

use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// File search backed by the macOS Spotlight index via `mdfind`. We never build
/// our own index, so this is cheap on disk/CPU. Runs on the worker thread.
pub struct FilesProvider {
    max_results: usize,
}

impl FilesProvider {
    pub fn new() -> Self {
        Self { max_results: 6 }
    }
}

impl Provider for FilesProvider {
    fn name(&self) -> &'static str {
        "files"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        // Avoid hammering Spotlight on very short queries.
        if q.len() < 3 {
            return;
        }

        let output = Command::new("/usr/bin/mdfind")
            .arg("-name")
            .arg(q)
            .output();
        let Ok(output) = output else {
            return;
        };
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut scored: Vec<(i64, String, String)> = Vec::new();
        for line in stdout.lines().take(200) {
            let path = line.trim();
            if path.is_empty() {
                continue;
            }
            let file_name = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path);
            if let Some(score) = fuzzy_score(q, file_name) {
                scored.push((score as i64, file_name.to_string(), path.to_string()));
            }
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        for (score, name, path) in scored.into_iter().take(self.max_results) {
            out.push(Item::new(
                name,
                path.clone(),
                "File",
                // Below apps (which get a +100 bias) but above the web fallback.
                score,
                Action::Open(path),
            ));
        }
    }
}
