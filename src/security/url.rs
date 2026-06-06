/// Return true when `url` is safe to pass to `/usr/bin/open`.
pub fn is_safe_open_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.contains('\0') {
        return false;
    }
    match extract_scheme(url) {
        Some(scheme) => matches!(scheme.as_str(), "http" | "https"),
        None => true,
    }
}

/// Validate an AI provider endpoint URL. Returns the normalized base URL.
pub fn validate_ai_endpoint(
    provider: &str,
    endpoint: &str,
    allow_private_endpoint: bool,
) -> Result<String, String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Ok(String::new());
    }

    let normalized = endpoint.trim_end_matches('/').to_string();
    match provider {
        "anthropic" => validate_hosted_endpoint(&normalized, &["api.anthropic.com"]),
        "openai" => validate_hosted_endpoint(&normalized, &["api.openai.com"]),
        "gemini" | "google" => {
            validate_hosted_endpoint(&normalized, &["generativelanguage.googleapis.com"])
        }
        "ollama" => validate_ollama_endpoint(&normalized),
        "openai-compatible" | "cursor" => {
            validate_compatible_endpoint(&normalized, allow_private_endpoint)
        }
        other => Err(format!("Unknown AI provider: {other}")),
    }
}

fn split_scheme(url: &str) -> Option<(String, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), rest))
}

fn extract_scheme(url: &str) -> Option<String> {
    if let Some((scheme, _rest)) = split_scheme(url) {
        return Some(scheme);
    }
    let (scheme, _) = url.split_once(':')?;
    if scheme.is_empty() {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

fn parse_authority(url: &str) -> Result<(String, String, Option<u16>), String> {
    let Some((scheme, rest)) = split_scheme(url) else {
        return Err("endpoint must include a scheme (http:// or https://)".to_string());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.is_empty() {
        return Err("endpoint is missing a host".to_string());
    }

    let (host, port) = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| "invalid IPv6 authority".to_string())?;
        let host = authority[1..end].to_string();
        let port = authority[end + 1..]
            .strip_prefix(':')
            .map(parse_port)
            .transpose()?;
        (host, port)
    } else if let Some((host, port_str)) = authority.rsplit_once(':') {
        if port_str.chars().all(|c| c.is_ascii_digit()) && authority.matches(':').count() == 1 {
            (host.to_string(), Some(parse_port(port_str)?))
        } else {
            (authority.to_string(), None)
        }
    } else {
        (authority.to_string(), None)
    };

    Ok((scheme, host, port))
}

fn parse_port(port: &str) -> Result<u16, String> {
    port.parse::<u16>()
        .map_err(|_| format!("invalid port: {port}"))
}

fn validate_hosted_endpoint(url: &str, allowed_suffixes: &[&str]) -> Result<String, String> {
    let (scheme, host, port) = parse_authority(url)?;
    if scheme != "https" {
        return Err("hosted AI endpoints must use https://".to_string());
    }
    if let Some(port) = port {
        if port != 443 {
            return Err("hosted AI endpoints must use the default HTTPS port".to_string());
        }
    }
    if is_private_or_link_local_host(&host) {
        return Err("hosted AI endpoints must not point to private or link-local addresses".to_string());
    }
    let host_lower = host.to_ascii_lowercase();
    if !allowed_suffixes
        .iter()
        .any(|suffix| host_lower == *suffix || host_lower.ends_with(&format!(".{suffix}")))
    {
        return Err(format!(
            "endpoint host must be one of: {}",
            allowed_suffixes.join(", ")
        ));
    }
    Ok(url.to_string())
}

fn validate_ollama_endpoint(url: &str) -> Result<String, String> {
    let (scheme, host, _port) = parse_authority(url)?;
    if scheme != "http" {
        return Err("Ollama endpoints must use http://".to_string());
    }
    let host_lower = host.to_ascii_lowercase();
    if host_lower != "127.0.0.1" && host_lower != "localhost" {
        return Err("Ollama endpoints must be localhost only".to_string());
    }
    Ok(url.to_string())
}

fn validate_compatible_endpoint(url: &str, allow_private: bool) -> Result<String, String> {
    let (scheme, host, _port) = parse_authority(url)?;
    let host_lower = host.to_ascii_lowercase();
    let is_local = host_lower == "127.0.0.1" || host_lower == "localhost";
    if is_local {
        if scheme != "http" && scheme != "https" {
            return Err("local endpoints must use http:// or https://".to_string());
        }
        return Ok(url.to_string());
    }
    if scheme != "https" {
        return Err("remote OpenAI-compatible endpoints must use https://".to_string());
    }
    if !allow_private && is_private_or_link_local_host(&host) {
        return Err(
            "remote endpoints must not point to private or link-local addresses (set allow_private_endpoint = true to override)"
                .to_string(),
        );
    }
    Ok(url.to_string())
}

fn is_private_or_link_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        return is_private_or_link_local_ip(addr);
    }
    false
}

fn is_private_or_link_local_ip(addr: std::net::IpAddr) -> bool {
    match addr {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.octets()[0] == 0
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback() || is_unique_local_ipv6(v6),
    }
}

fn is_unique_local_ipv6(addr: std::net::Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xfe00) == 0xfc00
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_safe_open_url_allows_https() {
        assert!(is_safe_open_url("https://example.com"));
    }

    #[test]
    fn is_safe_open_url_blocks_javascript() {
        assert!(!is_safe_open_url("javascript:alert(1)"));
    }

    #[test]
    fn is_safe_open_url_blocks_data() {
        assert!(!is_safe_open_url("data:text/html,<script>alert(1)</script>"));
    }

    #[test]
    fn validate_ai_endpoint_anthropic_ok() {
        assert!(validate_ai_endpoint("anthropic", "https://api.anthropic.com", false).is_ok());
    }

    #[test]
    fn validate_ai_endpoint_blocks_metadata_ip() {
        assert!(validate_ai_endpoint("openai", "http://169.254.169.254", false).is_err());
    }

    #[test]
    fn validate_ai_endpoint_ollama_localhost() {
        assert!(validate_ai_endpoint("ollama", "http://127.0.0.1:11434", false).is_ok());
    }
}
