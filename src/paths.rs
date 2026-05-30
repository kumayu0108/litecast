use std::path::PathBuf;

/// `~/Library/Application Support/litecast`, created if missing.
pub fn support_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join("Library/Application Support/litecast");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Path to a file inside the support directory.
pub fn support_file(name: &str) -> PathBuf {
    support_dir().join(name)
}
