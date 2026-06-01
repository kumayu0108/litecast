//! Shared runtime state: config, query engine, and hotkey reload.

use std::sync::{Arc, Mutex, RwLock};

use global_hotkey::GlobalHotKeyManager;

use crate::clipboard::History;
use crate::config::Config;
use crate::engine::Engine;
use crate::frecency::Frecency;
use crate::engine_setup;
use crate::hotkeys::{self, HotkeyIds};

pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub engine: Arc<RwLock<Engine>>,
    pub history: History,
    pub frecency: Frecency,
    pub hotkey_ids: Arc<Mutex<HotkeyIds>>,
    pub hotkey_manager: Mutex<Option<GlobalHotKeyManager>>,
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
        let (manager, ids) = hotkeys::register_all(&cfg)?;
        drop(cfg);

        Ok(Self {
            config,
            engine,
            history,
            frecency,
            hotkey_ids: Arc::new(Mutex::new(ids)),
            hotkey_manager: Mutex::new(Some(manager)),
        })
    }

    /// Rebuild the query engine and re-register global hotkeys from the current config.
    pub fn apply_config(&self) -> Result<(), String> {
        let cfg = self.config.read().map_err(|e| e.to_string())?;
        *self.engine.write().map_err(|e| e.to_string())? =
            engine_setup::build_engine(self.history.clone(), &cfg, self.frecency.clone());

        let (manager, ids) = hotkeys::register_all(&cfg)?;
        *self.hotkey_manager.lock().map_err(|e| e.to_string())? = Some(manager);
        *self.hotkey_ids.lock().map_err(|e| e.to_string())? = ids;
        Ok(())
    }
}
