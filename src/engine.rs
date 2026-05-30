use crate::frecency::Frecency;
use crate::model::Item;

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
    providers: Vec<Box<dyn Provider>>,
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

    pub fn add(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    pub fn query(&self, query: &str) -> Vec<Item> {
        let query = query.trim();
        let mut out = Vec::new();
        if query.is_empty() {
            return out;
        }
        for provider in &self.providers {
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
