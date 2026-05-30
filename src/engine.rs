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

    /// Map a typed `@token` (without the `@`) to a filter. Exact aliases match
    /// first; otherwise a small typo tolerance lets `@aps`/`@clip`/`@emoij`
    /// still resolve to the right category.
    pub fn from_token(token: &str) -> Option<Filter> {
        match token {
            "apps" | "app" => return Some(Filter::Apps),
            "files" | "file" => return Some(Filter::Files),
            "clip" | "clipboard" => return Some(Filter::Clip),
            "calc" | "conv" | "convert" => return Some(Filter::Calc),
            "web" => return Some(Filter::Web),
            "cmd" | "command" | "commands" => return Some(Filter::Cmd),
            "emoji" => return Some(Filter::Emoji),
            "ai" => return Some(Filter::Ai),
            _ => {}
        }
        // Typo-tolerant fallback against each category's canonical alias.
        const ALIASES: &[(&str, Filter)] = &[
            ("apps", Filter::Apps),
            ("files", Filter::Files),
            ("clip", Filter::Clip),
            ("clipboard", Filter::Clip),
            ("calc", Filter::Calc),
            ("convert", Filter::Calc),
            ("web", Filter::Web),
            ("cmd", Filter::Cmd),
            ("commands", Filter::Cmd),
            ("emoji", Filter::Emoji),
            ("ai", Filter::Ai),
        ];
        ALIASES
            .iter()
            .find(|(alias, _)| keyword_matches(token, alias))
            .map(|(_, f)| *f)
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

/// Typo tolerance for a keyword of `len` characters: roughly one edit per 4-5
/// characters, capped low so matches stay tight (no edits for 1-2 char tokens).
fn keyword_threshold(len: usize) -> usize {
    match len {
        0..=2 => 0,
        3..=4 => 1,
        _ => (len / 4).min(2),
    }
}

/// Optimal String Alignment distance (Damerau-Levenshtein restricted to
/// adjacent transpositions) between two ASCII-lowercased strings, with an early
/// bound: returns `None` as soon as the best possible distance exceeds `max`.
/// Bounded and tiny (keywords are short), so it is cheap to call per keystroke.
fn osa_distance_within(a: &str, b: &str, max: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > max {
        return None;
    }
    // Three rolling rows are enough for OSA (current, previous, prev-previous).
    let mut prev2 = vec![0usize; m + 1];
    let mut prev1: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        let mut row_min = cur[0];
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut val = (prev1[j] + 1).min(cur[j - 1] + 1).min(prev1[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                val = val.min(prev2[j - 2] + 1);
            }
            cur[j] = val;
            row_min = row_min.min(val);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev2, &mut prev1);
        std::mem::swap(&mut prev1, &mut cur);
    }
    let dist = prev1[m];
    (dist <= max).then_some(dist)
}

/// Does `input` match `keyword` exactly or within a small, length-scaled edit
/// distance? Both are compared case-insensitively. Used so command/app-command
/// keywords and category tokens tolerate typos (e.g. `cliboard`, `@trm`,
/// `defien`) without loosening the precise fuzzy ranking of files/apps.
pub fn keyword_matches(input: &str, keyword: &str) -> bool {
    if input.eq_ignore_ascii_case(keyword) {
        return true;
    }
    let max = keyword_threshold(keyword.chars().count());
    if max == 0 {
        return false;
    }
    osa_distance_within(
        &input.to_ascii_lowercase(),
        &keyword.to_ascii_lowercase(),
        max,
    )
    .is_some()
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
