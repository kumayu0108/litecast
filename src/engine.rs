use crate::frecency::Frecency;
use crate::model::Item;

/// A search-scope category. `All` runs every provider; any other value runs only
/// the providers tagged with it, so an active filter skips unrelated (and
/// potentially expensive, e.g. `mdfind`) providers entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Apps,
    Files,
    Clip,
    Calc,
    Web,
    Cmd,
    Emoji,
    Ai,
}

impl Filter {
    /// Cycle order for the Tab-driven chip (also the order shown to users).
    pub const CYCLE: [Filter; 9] = [
        Filter::All,
        Filter::Apps,
        Filter::Files,
        Filter::Clip,
        Filter::Calc,
        Filter::Web,
        Filter::Cmd,
        Filter::Emoji,
        Filter::Ai,
    ];

    pub fn next(self) -> Filter {
        let i = Self::CYCLE.iter().position(|f| *f == self).unwrap_or(0);
        Self::CYCLE[(i + 1) % Self::CYCLE.len()]
    }

    pub fn prev(self) -> Filter {
        let i = Self::CYCLE.iter().position(|f| *f == self).unwrap_or(0);
        Self::CYCLE[(i + Self::CYCLE.len() - 1) % Self::CYCLE.len()]
    }

    /// Short label shown on the filter chip (`All` has no chip).
    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Apps => "Apps",
            Filter::Files => "Files",
            Filter::Clip => "Clipboard",
            Filter::Calc => "Calc",
            Filter::Web => "Web",
            Filter::Cmd => "Commands",
            Filter::Emoji => "Emoji",
            Filter::Ai => "AI",
        }
    }

    /// Map a typed `@token` (without the `@`) to a filter.
    pub fn from_token(token: &str) -> Option<Filter> {
        match token {
            "apps" | "app" => Some(Filter::Apps),
            "files" | "file" => Some(Filter::Files),
            "clip" | "clipboard" => Some(Filter::Clip),
            "calc" | "conv" | "convert" => Some(Filter::Calc),
            "web" => Some(Filter::Web),
            "cmd" | "command" | "commands" => Some(Filter::Cmd),
            "emoji" => Some(Filter::Emoji),
            "ai" => Some(Filter::Ai),
            _ => None,
        }
    }
}

/// A source of results for a query. Providers are queried on each keystroke,
/// on a background worker thread, so they must be `Send + Sync`.
pub trait Provider: Send + Sync {
    /// Short label for the provider (used for diagnostics).
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    /// Append results for `query` into `out`.
    fn query(&self, query: &str, out: &mut Vec<Item>);
}

/// Aggregates results from all providers and ranks them.
pub struct Engine {
    providers: Vec<(Box<dyn Provider>, Filter)>,
    max_results: usize,
    frecency: Frecency,
}

impl Engine {
    pub fn new(frecency: Frecency) -> Self {
        Self {
            providers: Vec::new(),
            max_results: 8,
            frecency,
        }
    }

    /// Register a provider under a category. Providers tagged `Filter::All` are
    /// general and only run when no filter is active.
    pub fn add(&mut self, provider: Box<dyn Provider>, category: Filter) {
        self.providers.push((provider, category));
    }

    pub fn query(&self, query: &str, filter: Filter) -> Vec<Item> {
        let query = query.trim();
        let mut out = Vec::new();
        if query.is_empty() {
            return out;
        }
        for (provider, category) in &self.providers {
            // With no filter, run everything; otherwise run only matching
            // providers (skipping unrelated, possibly expensive, work).
            if filter != Filter::All && *category != filter {
                continue;
            }
            provider.query(query, &mut out);
        }
        // Nudge frequently/recently used items up (bounded, never overrides
        // intentful high-score results).
        for item in &mut out {
            if let Some(id) = &item.id {
                item.score += self.frecency.boost(id);
            }
        }
        // Highest score first; stable so providers keep insertion order on ties.
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out.truncate(self.max_results);
        out
    }
}

/// Convenience fuzzy scorer used by providers. Builds a matcher per call, which
/// is cheap relative to process/IO work and keeps providers `Sync`.
pub fn fuzzy_score(needle: &str, candidate: &str) -> Option<u32> {
    use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo_matcher::{Config, Matcher, Utf32Str};
    let pattern = Pattern::parse(needle, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let haystack = Utf32Str::new(candidate, &mut buf);
    pattern.score(haystack, &mut matcher)
}
