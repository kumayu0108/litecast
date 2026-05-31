use std::sync::atomic::{AtomicI32, Ordering};

/// PID of the app that was frontmost when the panel last opened (set in `show()`).
/// Used by menu-bar search and window ops so commands target the right app.
static TARGET: AtomicI32 = AtomicI32::new(-1);

pub fn set(pid: i32) {
    TARGET.store(pid, Ordering::SeqCst);
}

pub fn get() -> i32 {
    TARGET.load(Ordering::SeqCst)
}
