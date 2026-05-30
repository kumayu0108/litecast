use crate::clipboard::{ClipKind, History};
use crate::config::AiConfig;
use crate::engine::{fuzzy_score, Provider};
use crate::model::{Action, Item};
use crate::secrets;

/// A quick one-shot AI action over text: either a typed argument or, with no
/// argument, the most recent clipboard entry.
struct AiCommand {
    name: &'static str,
    /// Keywords that trigger the command directly (first word of the query).
    keywords: &'static [&'static str],
    /// Prompt template; `{input}` is replaced with the argument/clipboard text.
    template: &'static str,
}

const COMMANDS: &[AiCommand] = &[
    AiCommand {
        name: "Translate to English",
        keywords: &["translate", "tr"],
        template: "Translate the following text to English. Output only the translation:\n\n{input}",
    },
    AiCommand {
        name: "Summarize",
        keywords: &["summarize", "summary", "sum"],
        template: "Summarize the following text concisely:\n\n{input}",
    },
    AiCommand {
        name: "Fix Grammar",
        keywords: &["fixgrammar", "fix", "grammar"],
        template: "Fix the spelling and grammar of the following text. Output only the corrected text:\n\n{input}",
    },
    AiCommand {
        name: "Improve Writing",
        keywords: &["improve", "improvewriting", "rewrite"],
        template: "Improve the clarity and flow of the following text while preserving its meaning. Output only the rewritten text:\n\n{input}",
    },
];

/// Quick AI commands (`translate`, `summarize`, `fixgrammar`, `improve`) that act
/// on a typed argument or, with none, the latest clipboard entry. The request
/// only fires on Enter (reusing the AI flow); nothing heavy runs per keystroke.
pub struct AiCommandsProvider {
    provider: String,
    history: History,
}

impl AiCommandsProvider {
    pub fn new(config: &AiConfig, history: History) -> Self {
        Self {
            provider: config.provider.clone(),
            history,
        }
    }

    /// Latest clipboard text (in-memory; no process spawn), if any.
    fn latest_clip(&self) -> Option<String> {
        self.history
            .snapshot()
            .into_iter()
            .filter(|e| e.kind != ClipKind::Image)
            .map(|e| e.text)
            .find(|t| !t.trim().is_empty())
    }

    fn emit(&self, cmd: &AiCommand, arg: &str, score: i64, out: &mut Vec<Item>) {
        let (input, source_note) = if arg.trim().is_empty() {
            match self.latest_clip() {
                Some(text) => (text, "latest clipboard text"),
                None => {
                    out.push(Item::new(
                        cmd.name,
                        "Copy some text first, or type an argument",
                        "AI",
                        score,
                        Action::None,
                    ));
                    return;
                }
            }
        } else {
            (arg.trim().to_string(), "typed text")
        };

        if secrets::get_api_key(&self.provider).is_none() {
            out.push(Item::new(
                format!("{} (no API key)", cmd.name),
                "Type: setkey <your-api-key> then Enter",
                "AI",
                score,
                Action::None,
            ));
            return;
        }

        let prompt = cmd.template.replace("{input}", &input);
        out.push(Item::new(
            format!("{}: {}", cmd.name, preview(&input)),
            format!("Press Enter to ask {} ({source_note})", self.provider),
            "AI",
            score,
            Action::AskAi {
                prompt,
                image: None,
            },
        ));
    }
}

impl Provider for AiCommandsProvider {
    fn name(&self) -> &'static str {
        "ai-commands"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_ascii_lowercase();
        let (first, rest) = match lower.split_once(char::is_whitespace) {
            Some((f, _)) => {
                let arg = q[first_word_len(q)..].trim_start();
                (f.to_string(), arg.to_string())
            }
            None => (lower.clone(), String::new()),
        };

        // Direct keyword match: highest priority, uses the typed argument.
        for cmd in COMMANDS {
            if cmd.keywords.contains(&first.as_str()) {
                self.emit(cmd, &rest, 10_400, out);
                return;
            }
        }

        // Otherwise, make commands fuzzy-discoverable by name (uses clipboard).
        for cmd in COMMANDS {
            if let Some(s) = fuzzy_score(&lower, &cmd.name.to_ascii_lowercase()) {
                self.emit(cmd, "", 8_400 + s as i64, out);
            }
        }
    }
}

fn first_word_len(s: &str) -> usize {
    s.find(char::is_whitespace).unwrap_or(s.len())
}

fn preview(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 60 {
        let truncated: String = one_line.chars().take(60).collect();
        format!("{truncated}...")
    } else {
        one_line
    }
}
