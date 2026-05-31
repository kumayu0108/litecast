use std::sync::Mutex;

use crate::menu_ax::MenuEntry;

static CACHE: Mutex<Option<(i32, Vec<MenuEntry>)>> = Mutex::new(None);

/// Refresh cached menu items for `pid` (call from the main thread).
pub fn refresh(pid: i32, max: usize) {
    let entries = if pid > 0 {
        crate::menu_ax::list_menu_items(pid, max)
    } else {
        Vec::new()
    };
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((pid, entries));
    }
}

pub fn snapshot(pid: i32) -> Vec<MenuEntry> {
    CACHE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().filter(|(p, _)| *p == pid).map(|(_, e)| e.clone()))
        .unwrap_or_default()
}
