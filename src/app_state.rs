//! Shared runtime state: config, query engine, and hotkey reload.

use std::sync::{Arc, Mutex, RwLock};

use crate::clipboard::History;
use crate::config::Config;
use crate::engine::Engine;
use crate::engine_setup;
use crate::frecency::Frecency;
use crate::hotkeys::HotkeyIds;

pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub engine: Arc<RwLock<Engine>>,
    pub history: History,
    pub frecency: Frecency,
    /// Live hotkey ids used by the background listener thread.
    pub hotkey_ids: Arc<Mutex<HotkeyIds>>,
    /// Last global-hotkey registration error (set by the delegate on each
    /// register attempt; `None` on success). The Preferences window reads this
    /// right after a Save to surface combo conflicts (e.g. Cmd+Space vs Spotlight).
    pub last_hotkey_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new(config: Config, history: History, frecency: Frecency) -> Result<Self, String> {
        let config = Arc::new(RwLock::new(config));
        let cfg = config.read().map_err(|e| e.to_string())?;
        let engine = Arc::new(RwLock::new(engine_setup::build_engine(
            history.clone(),
            &cfg,
            frecency.clone(),
        )));
        drop(cfg);

        Ok(Self {
            config,
            engine,
            history,
            frecency,
            hotkey_ids: Arc::new(Mutex::new(HotkeyIds::default())),
            last_hotkey_error: Arc::new(Mutex::new(None)),
        })
    }

    /// Rebuild the query engine from the current config.
    /// Hotkey re-registration is handled by the app delegate (`ensure_hotkeys_registered`).
    pub fn apply_config(&self) -> Result<(), String> {
        let cfg = self.config.read().map_err(|e| e.to_string())?;
        *self.engine.write().map_err(|e| e.to_string())? =
            engine_setup::build_engine(self.history.clone(), &cfg, self.frecency.clone());
        Ok(())
    }
}
