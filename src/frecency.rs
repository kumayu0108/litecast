use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::support_file;

const USAGE_FILE: &str = "usage.json";
/// Upper bound on the ranking nudge so frecency never overrides intentful,
/// high-score results (calc = 10_000, keyword hits = 8_500+).
const MAX_BOOST: i64 = 400;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Stat {
    count: u32,
    last_ts: u64,
}

/// Learns how often and how recently items are activated and folds a bounded
/// boost into ranking. Cheap, shareable handle backed by a small JSON file.
#[derive(Clone)]
pub struct Frecency {
    inner: Arc<Mutex<HashMap<String, Stat>>>,
}

impl Frecency {
    pub fn load() -> Self {
        let map = std::fs::read_to_string(support_file(USAGE_FILE))
            .ok()
            .and_then(|data| serde_json::from_str::<HashMap<String, Stat>>(&data).ok())
            .unwrap_or_default();
        Self {
            inner: Arc::new(Mutex::new(map)),
        }
    }

    /// Record an activation of `id` and persist the updated table.
    pub fn record(&self, id: &str) {
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        let stat = map.entry(id.to_string()).or_default();
        stat.count = stat.count.saturating_add(1);
        stat.last_ts = now();
        if let Ok(json) = serde_json::to_string(&*map) {
            let _ = std::fs::write(support_file(USAGE_FILE), json);
        }
    }

    /// Bounded ranking boost for `id` from usage frequency and recency.
    pub fn boost(&self, id: &str) -> i64 {
        let Ok(map) = self.inner.lock() else {
            return 0;
        };
        let Some(stat) = map.get(id) else {
            return 0;
        };
        let frequency = ((stat.count as f64).ln_1p() * 60.0) as i64;
        let age_days = now().saturating_sub(stat.last_ts) / 86_400;
        let recency = match age_days {
            0 => 120,
            1..=2 => 80,
            3..=6 => 50,
            7..=29 => 20,
            _ => 0,
        };
        (frequency + recency).min(MAX_BOOST)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
