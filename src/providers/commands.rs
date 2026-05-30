use crate::config::CommandConfig;
use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};

/// User-defined custom commands from the config file. Triggered by fuzzy name
/// match, or directly via an optional keyword (with `{}` argument substitution).
pub struct CommandsProvider {
    commands: Vec<CommandConfig>,
}

impl CommandsProvider {
    pub fn new(commands: Vec<CommandConfig>) -> Self {
        Self { commands }
    }
}

impl Provider for CommandsProvider {
    fn name(&self) -> &'static str {
        "commands"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        for cmd in &self.commands {
            // Keyword trigger takes priority and supports an argument.
            if !cmd.keyword.is_empty() {
                if let Some(arg) = match_keyword(q, &cmd.keyword) {
                    out.push(build_item(cmd, arg, 8_500));
                    continue;
                }
            }
            // Otherwise fuzzy-match the command name or any of its aliases,
            // keeping the best score.
            let mut best = fuzzy_score(q, &cmd.name);
            for alias in cmd.alias_list() {
                if let Some(s) = fuzzy_score(q, alias) {
                    best = Some(best.map_or(s, |b| b.max(s)));
                }
            }
            if let Some(score) = best {
                out.push(build_item(cmd, "", 200 + score as i64));
            }
        }
    }
}

/// If `q` is exactly the keyword or starts with "keyword ", returns the argument.
fn match_keyword<'a>(q: &'a str, keyword: &str) -> Option<&'a str> {
    if q == keyword {
        return Some("");
    }
    let prefix = format!("{keyword} ");
    q.strip_prefix(&prefix).map(|rest| rest.trim())
}

fn build_item(cmd: &CommandConfig, arg: &str, score: i64) -> Item {
    let target = if cmd.target.contains("{}") {
        cmd.target.replace("{}", arg)
    } else {
        cmd.target.clone()
    };
    let action = match cmd.kind.as_str() {
        "shell" => Action::RunShell(target.clone()),
        _ => Action::Open(target.clone()),
    };
    let subtitle = if cmd.subtitle.is_empty() {
        target
    } else {
        cmd.subtitle.clone()
    };
    Item::new(cmd.name.clone(), subtitle, "Command", score, action)
        .with_id(format!("cmd:{}", cmd.name))
}
