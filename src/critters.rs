use std::path::PathBuf;

use crate::paths::support_dir;

/// Discover critter animations: any `.gif` files placed in
/// `~/Library/Application Support/litecast/critters/`. If none exist, the
/// wandering-critter feature is simply dormant (zero cost).
pub fn discover() -> Vec<PathBuf> {
    let dir = support_dir().join("critters");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut gifs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("gif"))
            == Some(true)
        {
            gifs.push(path);
        }
    }
    gifs
}
