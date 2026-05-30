use keyring::Entry;

const SERVICE: &str = "litecast";

/// Fetch the stored API key for a backend (e.g. "anthropic", "openai", "cursor").
pub fn get_api_key(provider: &str) -> Option<String> {
    let entry = Entry::new(SERVICE, provider).ok()?;
    entry.get_password().ok()
}

/// Store an API key for a backend in the macOS Keychain.
pub fn set_api_key(provider: &str, key: &str) -> bool {
    match Entry::new(SERVICE, provider) {
        Ok(entry) => entry.set_password(key).is_ok(),
        Err(_) => false,
    }
}
