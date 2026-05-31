use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::GitConfig;
use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, CaptureMode, Item};

/// Git helper: scan configured directories for repositories and offer open,
/// status, pull, and fetch actions (argv-only, output via notification).
pub struct GitProvider {
    config: GitConfig,
    cache: Mutex<Option<(Instant, Vec<PathBuf>)>>,
}

impl GitProvider {
    pub fn new(config: GitConfig) -> Self {
        Self {
            config,
            cache: Mutex::new(None),
        }
    }
}

const CACHE_TTL: Duration = Duration::from_secs(60);

impl Provider for GitProvider {
    fn name(&self) -> &'static str {
        "git"
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
        if kw != "git" && kw != "repo" && kw != "repos" {
            return;
        }

        let repos = self.cached_repos();
        if repos.is_empty() {
            out.push(Item::new(
                "No git repositories found",
                "Add scan_dirs under [git] in config.toml",
                "Git",
                8_200,
                Action::None,
            ));
            return;
        }

        for path in repos {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("repo")
                .to_string();
            let label = path.display().to_string();
            let score = if arg.is_empty() {
                8_200
            } else {
                match fuzzy_score(arg, &name).or_else(|| fuzzy_score(arg, &label)) {
                    Some(s) => 8_000 + s as i64,
                    None => continue,
                }
            };
            let p = path.to_string_lossy().to_string();
            out.push(
                Item::new(
                    name.clone(),
                    format!("{label} - open in Terminal"),
                    "Git",
                    score + 30,
                    Action::Run {
                        program: "/usr/bin/open".to_string(),
                        args: vec![
                            "-a".to_string(),
                            "Terminal".to_string(),
                            path.to_string_lossy().to_string(),
                        ],
                    },
                )
                .with_id(format!("git-open:{p}")),
            );
            out.push(
                Item::new(
                    format!("{name} - status"),
                    "git status (output in notification)",
                    "Git",
                    score + 20,
                    git_capture(&p, &["status", "--short"], "git status"),
                )
                .with_id(format!("git-status:{p}")),
            );
            out.push(
                Item::new(
                    format!("{name} - pull"),
                    "git pull",
                    "Git",
                    score + 10,
                    git_capture(&p, &["pull", "--ff-only"], "git pull"),
                )
                .with_id(format!("git-pull:{p}")),
            );
            out.push(
                Item::new(
                    format!("{name} - reveal"),
                    "Reveal repository in Finder",
                    "Git",
                    score,
                    Action::Run {
                        program: "/usr/bin/open".to_string(),
                        args: vec!["-R".to_string(), p.clone()],
                    },
                )
                .with_id(format!("git-reveal:{p}")),
            );
        }
    }
}

impl GitProvider {
    fn cached_repos(&self) -> Vec<PathBuf> {
        if let Ok(guard) = self.cache.lock() {
            if let Some((at, repos)) = guard.as_ref() {
                if at.elapsed() < CACHE_TTL {
                    return repos.clone();
                }
            }
        }
        let mut repos = Vec::new();
        for dir in self.config.resolved_dirs() {
            scan_git_dir(&dir, self.config.max_depth, &mut repos);
        }
        repos.sort();
        repos.dedup();
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some((Instant::now(), repos.clone()));
        }
        repos
    }
}

fn scan_git_dir(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join(".git").exists() {
            out.push(path);
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') || matches!(name, "node_modules" | "target" | "build") {
            continue;
        }
        scan_git_dir(&path, depth - 1, out);
    }
}

fn git_capture(repo: &str, git_args: &[&str], title: &str) -> Action {
    let mut args = vec!["-C".to_string(), repo.to_string()];
    args.extend(git_args.iter().map(|s| s.to_string()));
    Action::RunCapture {
        program: "/usr/bin/git".to_string(),
        args,
        mode: CaptureMode::Notify,
        title: title.to_string(),
    }
}
