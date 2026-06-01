use keyring::Entry;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "litecast";

/// Process-wide cache of API-key PRESENCE, keyed by provider. The per-keystroke
/// provider query path must never block on the (potentially slow / locked)
/// macOS Keychain, so we read presence from the Keychain at most once per
/// provider and serve every subsequent check from memory.
fn presence_cache() -> &'static Mutex<HashMap<String, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached check for whether an API key is stored for `provider`.
///
/// The first call for a given provider performs ONE Keychain read and caches
/// the boolean; all later calls return the cached value without touching the
/// Keychain. Safe to call on the per-keystroke worker thread. Poisoned-mutex
/// situations fall back to a direct (uncached) Keychain read rather than
/// panicking the worker.
pub fn has_api_key_cached(provider: &str) -> bool {
    if let Ok(cache) = presence_cache().lock() {
        if let Some(&present) = cache.get(provider) {
            return present;
        }
    }

    let present = get_api_key(provider).is_some();

    if let Ok(mut cache) = presence_cache().lock() {
        cache.insert(provider.to_string(), present);
    }
    present
}

/// Whether this provider expects a Keychain API key.
pub fn needs_api_key(provider: &str, endpoint: &str) -> bool {
    match provider {
        "ollama" => false,
        "openai-compatible" | "cursor" => {
            let ep = endpoint.trim().to_lowercase();
            !(ep.contains("127.0.0.1") || ep.contains("localhost"))
        }
        _ => true,
    }
}

/// Fetch the stored API key for a backend (e.g. "anthropic", "openai", "cursor").
pub fn get_api_key(provider: &str) -> Option<String> {
    if provider == "ollama" {
        return None;
    }
    let entry = Entry::new(SERVICE, provider).ok()?;
    entry.get_password().ok()
}

/// API key for chat requests: None when the provider does not use Keychain.
pub fn api_key_for_chat(provider: &str, endpoint: &str) -> Option<String> {
    if !needs_api_key(provider, endpoint) {
        return None;
    }
    get_api_key(provider)
}

/// Store an API key for a backend in the macOS Keychain.
///
/// On success, the presence cache is updated so a newly-saved key is reflected
/// immediately by `has_api_key_cached` without a fresh Keychain read.
pub fn set_api_key(provider: &str, key: &str) -> bool {
    let stored = match Entry::new(SERVICE, provider) {
        Ok(entry) => entry.set_password(key).is_ok(),
        Err(_) => false,
    };
    if stored {
        if let Ok(mut cache) = presence_cache().lock() {
            cache.insert(provider.to_string(), true);
        }
    }
    stored
}
