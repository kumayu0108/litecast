use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::support_file;

const CACHE_FILE: &str = "currency.json";
const BASE: &str = "USD";

/// USD-based exchange rates with the timestamp they were fetched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rates {
    /// 1 USD = `rate` units of the keyed currency.
    rates: HashMap<String, f64>,
    fetched_ts: u64,
    /// Human-readable date reported by the source (for the subtitle).
    date: String,
}

/// Cached currency rates with a short TTL and a background refresh. Reads are
/// cheap (in-memory); the network fetch never runs inline on the worker.
#[derive(Clone)]
pub struct CurrencyCache {
    rates: Arc<Mutex<Option<Rates>>>,
    refreshing: Arc<AtomicBool>,
    ttl_secs: u64,
}

impl CurrencyCache {
    pub fn new(ttl_hours: u64) -> Self {
        let loaded = std::fs::read_to_string(support_file(CACHE_FILE))
            .ok()
            .and_then(|data| serde_json::from_str::<Rates>(&data).ok());
        Self {
            rates: Arc::new(Mutex::new(loaded)),
            refreshing: Arc::new(AtomicBool::new(false)),
            ttl_secs: ttl_hours.max(1) * 3600,
        }
    }

    fn snapshot(&self) -> Option<Rates> {
        self.rates.lock().ok().and_then(|g| g.clone())
    }

    /// True when there is no cache or it is older than the TTL.
    pub fn is_stale(&self) -> bool {
        match self.snapshot() {
            Some(r) => now().saturating_sub(r.fetched_ts) > self.ttl_secs,
            None => true,
        }
    }

    pub fn has_rates(&self) -> bool {
        self.snapshot().map(|r| !r.rates.is_empty()).unwrap_or(false)
    }

    /// Kick a background refresh unless one is already in flight.
    pub fn refresh_async(&self) {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        let rates = self.rates.clone();
        let refreshing = self.refreshing.clone();
        std::thread::spawn(move || {
            if let Some(fresh) = fetch() {
                if let Ok(json) = serde_json::to_string(&fresh) {
                    let _ = std::fs::write(support_file(CACHE_FILE), json);
                }
                if let Ok(mut g) = rates.lock() {
                    *g = Some(fresh);
                }
            }
            refreshing.store(false, Ordering::SeqCst);
        });
    }

    /// Convert `amount` from one ISO code to another using cached USD rates.
    /// Returns the value and the cache date. `None` if a code is unknown or no
    /// rates are cached.
    pub fn convert(&self, amount: f64, from: &str, to: &str) -> Option<(f64, String)> {
        let r = self.snapshot()?;
        let from_rate = rate_for(&r, from)?;
        let to_rate = rate_for(&r, to)?;
        let usd = amount / from_rate;
        Some((usd * to_rate, r.date.clone()))
    }
}

fn rate_for(r: &Rates, code: &str) -> Option<f64> {
    let code = code.to_ascii_uppercase();
    if code == BASE {
        return Some(1.0);
    }
    // Reject a zero, negative, or non-finite rate: it would otherwise produce a
    // division-by-zero (inf/NaN) result in `convert`. Treat it as "unknown".
    r.rates
        .get(&code)
        .copied()
        .filter(|&v| v.is_finite() && v > 0.0)
}

/// Fetch rates from one of two key-less providers, chosen at random to spread
/// load, falling back to the other on error.
fn fetch() -> Option<Rates> {
    let er_api = || fetch_er_api();
    let frankfurter = || fetch_frankfurter();
    if now_nanos() % 2 == 0 {
        er_api().or_else(frankfurter)
    } else {
        frankfurter().or_else(er_api)
    }
}

fn fetch_er_api() -> Option<Rates> {
    let value = get_json("https://open.er-api.com/v6/latest/USD")?;
    if value.get("result").and_then(|v| v.as_str()) != Some("success") {
        return None;
    }
    let rates = parse_rates(value.get("rates")?)?;
    let date = value
        .get("time_last_update_utc")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(Rates {
        rates,
        fetched_ts: now(),
        date,
    })
}

fn fetch_frankfurter() -> Option<Rates> {
    let value = get_json("https://api.frankfurter.app/latest?from=USD")?;
    let rates = parse_rates(value.get("rates")?)?;
    let date = value
        .get("date")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(Rates {
        rates,
        fetched_ts: now(),
        date,
    })
}

fn parse_rates(value: &Value) -> Option<HashMap<String, f64>> {
    let obj = value.as_object()?;
    let mut rates = HashMap::with_capacity(obj.len());
    for (code, rate) in obj {
        if let Some(rate) = rate.as_f64() {
            rates.insert(code.to_ascii_uppercase(), rate);
        }
    }
    if rates.is_empty() {
        None
    } else {
        Some(rates)
    }
}

fn get_json(url: &str) -> Option<Value> {
    let mut resp = ureq::get(url).call().ok()?;
    resp.body_mut().read_json().ok()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
