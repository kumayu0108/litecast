use crate::config::CommandConfig;
use crate::engine::{fuzzy_score, keyword_matches, Provider};
use crate::model::{Action, Item};

/// User-defined custom commands from the config file. Triggered by fuzzy name
/// match, or directly via an optional keyword (with `{}` argument substitution).
pub struct CommandsProvider {
    commands: Vec<CommandConfig>,
    confirm_config_shell: bool,
}

impl CommandsProvider {
    pub fn new(commands: Vec<CommandConfig>, confirm_config_shell: bool) -> Self {
        Self {
            commands,
            confirm_config_shell,
        }
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
                    out.push(build_item(cmd, arg, 8_500, self.confirm_config_shell));
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
                out.push(build_item(cmd, "", 200 + score as i64, self.confirm_config_shell));
            }
        }
    }
}

/// If `q`'s first word matches the keyword (exactly or within a small typo
/// tolerance), returns the remaining argument text. So `gh` and a close typo
/// both trigger the command, with or without an argument.
fn match_keyword<'a>(q: &'a str, keyword: &str) -> Option<&'a str> {
    let (first, rest) = match q.split_once(char::is_whitespace) {
        Some((f, r)) => (f, r.trim()),
        None => (q, ""),
    };
    keyword_matches(first, keyword).then_some(rest)
}

fn build_item(cmd: &CommandConfig, arg: &str, score: i64, confirm_config_shell: bool) -> Item {
    let target = if cmd.target.contains("{}") {
        cmd.target.replace("{}", arg)
    } else {
        cmd.target.clone()
    };
    let action = match cmd.kind.as_str() {
        "shell" => Action::RunShell(target.clone())
            .wrap_shell_confirm(format!("Run command: {}", cmd.name), confirm_config_shell),
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
