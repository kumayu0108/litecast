use std::path::PathBuf;

use crate::config::{NewFileConfig, TemplateConfig};
use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};
use crate::security::path::safe_join;

/// Quick file/folder creation under a configured base directory.
pub struct NewFileProvider {
    config: NewFileConfig,
}

impl NewFileProvider {
    pub fn new(config: NewFileConfig) -> Self {
        Self { config }
    }
}

impl Provider for NewFileProvider {
    fn name(&self) -> &'static str {
        "newfile"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();
        let base = self.config.resolved_base();

        if lower.starts_with("new file ") {
            let name = q["new file ".len()..].trim();
            if !name.is_empty() {
                push_create(out, &base, name, false);
            }
            return;
        }
        if lower.starts_with("new folder ") || lower.starts_with("new dir ") {
            let name = q
                .strip_prefix("new folder ")
                .or_else(|| q.strip_prefix("new dir "))
                .map(str::trim)
                .unwrap_or("");
            if !name.is_empty() {
                push_create(out, &base, name, true);
            }
            return;
        }
        if lower == "new" || lower.starts_with("new ") {
            let rest = lower.strip_prefix("new ").unwrap_or("").trim();
            if rest.is_empty() {
                out.push(Item::new(
                    "Create file or folder",
                    "new file notes.md, new folder Projects, or pick a template",
                    "File",
                    8_200,
                    Action::None,
                ));
                for t in &self.config.templates {
                    push_template(out, &base, t);
                }
                return;
            }
            if let Some(t) = self.config.templates.iter().find(|t| t.name.eq_ignore_ascii_case(rest))
            {
                push_template(out, &base, t);
                return;
            }
            if fuzzy_score(rest, "file").is_some() {
                out.push(Item::new(
                    "new file <name>",
                    format!("Creates under {}", base.display()),
                    "File",
                    8_100,
                    Action::None,
                ));
            }
        }
    }
}

fn push_create(out: &mut Vec<Item>, base: &PathBuf, name: &str, directory: bool) {
    let path = match safe_join(base, name) {
        Ok(path) => path,
        Err(reason) => {
            out.push(Item::new(
                format!("Cannot create: {name}"),
                reason,
                "File",
                8_500,
                Action::None,
            ));
            return;
        }
    };
    let p = path.to_string_lossy().to_string();
    let kind = if directory { "folder" } else { "file" };
    out.push(Item::new(
        format!("Create {kind}: {name}"),
        format!("{} - Enter to create and reveal", p),
        "File",
        8_500,
        Action::CreatePath {
            path: p,
            directory,
            reveal: true,
            editor: !directory,
            contents: None,
        },
    ));
}

fn push_template(out: &mut Vec<Item>, base: &PathBuf, t: &TemplateConfig) {
    let path = match safe_join(base, &t.name) {
        Ok(path) => path,
        Err(reason) => {
            out.push(Item::new(
                format!("Cannot create template: {}", t.name),
                reason,
                "File",
                8_400,
                Action::None,
            ));
            return;
        }
    };
    let p = path.to_string_lossy().to_string();
    let contents = t.contents.clone();
    out.push(Item::new(
        format!("New {}", t.name),
        "Create from template - Enter to create and open in editor",
        "File",
        8_400,
        Action::CreatePath {
            path: p,
            directory: false,
            reveal: true,
            editor: true,
            contents: if contents.is_empty() {
                None
            } else {
                Some(contents)
            },
        },
    ));
}
