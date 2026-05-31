use keyring::Entry;

const SERVICE: &str = "litecast";

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
pub fn set_api_key(provider: &str, key: &str) -> bool {
    match Entry::new(SERVICE, provider) {
        Ok(entry) => entry.set_password(key).is_ok(),
        Err(_) => false,
    }
}
