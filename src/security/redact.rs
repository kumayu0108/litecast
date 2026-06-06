/// Mask a secret for display, showing only the last four characters.
pub fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.len() <= 4 {
        return "••••".to_string();
    }
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("••••••••{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_secret_short() {
        assert_eq!(mask_secret("abc"), "••••");
    }

    #[test]
    fn mask_secret_long() {
        assert_eq!(mask_secret("sk-abcdefghijklmnop"), "••••••••mnop");
    }
}
