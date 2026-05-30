use crate::config::QuicklinkConfig;
use crate::engine::{fuzzy_score, keyword_matches, Provider};
use crate::model::{Action, Item};
use crate::providers::websearch::percent_encode;

/// Parameterized `{query}` URLs from `[[quicklinks]]`. Trigger with a keyword
/// plus an argument (`ghr rust-lang/rust`), or fuzzy-match the name to open the
/// link with no argument.
pub struct QuicklinksProvider {
    links: Vec<QuicklinkConfig>,
}

impl QuicklinksProvider {
    pub fn new(links: Vec<QuicklinkConfig>) -> Self {
        Self { links }
    }
}

impl Provider for QuicklinksProvider {
    fn name(&self) -> &'static str {
        "quicklinks"
    }

    fn query(&self, query: &str, out: &mut Vec<Item>) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        for link in &self.links {
            if !link.keyword.is_empty() {
                if let Some(arg) = match_keyword(q, &link.keyword) {
                    out.push(build_item(link, arg, 8_500));
                    continue;
                }
            }
            let mut best = fuzzy_score(q, &link.name);
            for alias in link.alias_list() {
                if let Some(s) = fuzzy_score(q, alias) {
                    best = Some(best.map_or(s, |b| b.max(s)));
                }
            }
            if let Some(score) = best {
                out.push(build_item(link, "", 200 + score as i64));
            }
        }
    }
}

/// If `q`'s first word matches the keyword (exactly or within a small typo
/// tolerance), returns the remaining argument text.
fn match_keyword<'a>(q: &'a str, keyword: &str) -> Option<&'a str> {
    let (first, rest) = match q.split_once(char::is_whitespace) {
        Some((f, r)) => (f, r.trim()),
        None => (q, ""),
    };
    keyword_matches(first, keyword).then_some(rest)
}

fn build_item(link: &QuicklinkConfig, arg: &str, score: i64) -> Item {
    let url = link.url.replace("{query}", &percent_encode(arg));
    Item::new(link.name.clone(), url.clone(), "Quicklink", score, Action::Open(url))
        .with_id(format!("ql:{}", link.name))
}
