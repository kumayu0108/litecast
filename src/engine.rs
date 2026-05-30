use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::Item;

/// A source of results for a query. Providers are queried on each keystroke.
pub trait Provider: Send {
    /// Short label for the provider.
    fn name(&self) -> &'static str;
    /// Append results for `query` into `out`.
    fn query(&self, query: &str, out: &mut Vec<Item>);
}

/// Aggregates results from all providers and ranks them.
pub struct Engine {
    providers: Vec<Box<dyn Provider>>,
    max_results: usize,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            max_results: 8,
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
        // Highest score first; stable so providers keep insertion order on ties.
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out.truncate(self.max_results);
        out
    }
}

/// Shared fuzzy-matching helper built on nucleo-matcher.
pub struct Fuzzy {
    matcher: Matcher,
}

impl Fuzzy {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Score `candidate` against `needle`. Returns None if it does not match.
    pub fn score(&mut self, needle: &str, candidate: &str) -> Option<u32> {
        let pattern = Pattern::parse(needle, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();
        let haystack = Utf32Str::new(candidate, &mut buf);
        pattern.score(haystack, &mut self.matcher)
    }
}
