pub mod apps;
pub mod calc;
pub mod clipboard;
pub mod commands;
pub mod files;
pub mod plugins;
pub mod websearch;

pub use apps::AppsProvider;
pub use calc::CalcProvider;
pub use clipboard::ClipboardProvider;
pub use commands::CommandsProvider;
pub use files::FilesProvider;
pub use plugins::PluginProvider;
pub use websearch::WebSearchProvider;
