//! Assemble the query engine from config.

use crate::clipboard::History;
use crate::config::{self, Config};
use crate::currency::CurrencyCache;
use crate::engine::{Engine, Filter};
use crate::frecency::Frecency;
use crate::providers::{
    AiCommandsProvider, AiProvider, AppCommandsProvider, AppsProvider, BookmarksProvider,
    CalcProvider, CalendarProvider, ClipboardProvider, CommandsProvider, ConvertProvider,
    ConvertersProvider, ColorProvider, DateTimeProvider, DevToolsProvider, DictionaryProvider,
    EasterEggProvider, EmojiProvider, FileActionsProvider, FilesProvider, GitProvider,
    MediaProvider, MenuProvider, NetworkProvider, NewFileProvider, NotesProvider,
    PluginProvider, PomodoroProvider, ProcessProvider, ProgUtilsProvider, QuicklinksProvider,
    ScriptsProvider, SnippetsProvider, SwitcherProvider, SystemProvider, TextTransformProvider,
    WebSearchProvider, WindowProvider,
};

pub fn build_engine(history: History, config: &Config, frecency: Frecency) -> Engine {
    let mut engine = Engine::new(frecency);
    engine.add(Box::new(EasterEggProvider), Filter::All);
    engine.add(Box::new(AiProvider::new(config.ai.clone())), Filter::Ai);
    engine.add(
        Box::new(AiCommandsProvider::new(&config.ai, history.clone())),
        Filter::Ai,
    );
    engine.add(Box::new(CalcProvider), Filter::Calc);
    let currency = CurrencyCache::new(config.conversion.currency_ttl_hours);
    currency.refresh_async();
    engine.add(Box::new(ConvertProvider::new(currency)), Filter::Calc);
    engine.add(Box::new(DevToolsProvider), Filter::Calc);
    engine.add(Box::new(ConvertersProvider), Filter::Calc);
    engine.add(
        Box::new(DateTimeProvider::new(config.datetime.pairs())),
        Filter::Calc,
    );
    engine.add(Box::new(EmojiProvider), Filter::Emoji);
    engine.add(Box::new(ClipboardProvider::new(history)), Filter::Clip);
    engine.add(
        Box::new(CommandsProvider::new(
            config.commands.clone(),
            config.security.confirm_config_shell,
        )),
        Filter::Cmd,
    );
    let app_commands = config::merged_app_commands(&config.app_commands);
    engine.add(
        Box::new(AppCommandsProvider::new(
            app_commands.clone(),
            config.web_search_url.clone(),
        )),
        Filter::All,
    );
    engine.add(
        Box::new(AppCommandsProvider::new(
            app_commands,
            config.web_search_url.clone(),
        )),
        Filter::Cmd,
    );
    engine.add(
        Box::new(QuicklinksProvider::new(config.quicklinks.clone())),
        Filter::Cmd,
    );
    engine.add(
        Box::new(SnippetsProvider::new(config.snippets.entries.clone())),
        Filter::Cmd,
    );
    engine.add(
        Box::new(ScriptsProvider::new(&config.scripts.dir)),
        Filter::Cmd,
    );
    engine.add(Box::new(SystemProvider::new()), Filter::Cmd);
    engine.add(Box::new(GitProvider::new(config.git.clone())), Filter::Cmd);
    engine.add(Box::new(TextTransformProvider), Filter::Cmd);
    engine.add(Box::new(ProgUtilsProvider), Filter::Cmd);
    engine.add(
        Box::new(PomodoroProvider::new(config.pomodoro.clone())),
        Filter::Cmd,
    );
    engine.add(
        Box::new(NewFileProvider::new(config.newfile.clone())),
        Filter::Cmd,
    );
    engine.add(Box::new(ColorProvider::new(config.color.clone())), Filter::Calc);
    if config.menu.enabled {
        engine.add(Box::new(MenuProvider::new(config.menu.clone())), Filter::Cmd);
    }
    engine.add(Box::new(SwitcherProvider::new()), Filter::Cmd);
    if config.window.enabled {
        engine.add(Box::new(WindowProvider::new()), Filter::Cmd);
    }
    engine.add(Box::new(PluginProvider::new()), Filter::Cmd);
    engine.add(Box::new(ProcessProvider::new()), Filter::Cmd);
    engine.add(Box::new(CalendarProvider::new()), Filter::Cmd);
    engine.add(Box::new(NetworkProvider::new()), Filter::Cmd);
    engine.add(
        Box::new(NotesProvider::new(
            &config.notes.file,
            config.notes.apple_notes,
        )),
        Filter::Cmd,
    );
    engine.add(Box::new(DictionaryProvider::new()), Filter::Cmd);
    engine.add(Box::new(MediaProvider), Filter::Cmd);
    engine.add(Box::new(AppsProvider::new()), Filter::Apps);
    engine.add(Box::new(FilesProvider::new()), Filter::Files);
    engine.add(Box::new(FileActionsProvider), Filter::Files);
    engine.add(Box::new(BookmarksProvider::new()), Filter::Web);
    engine.add(
        Box::new(WebSearchProvider::new(config.web_search_url.clone())),
        Filter::Web,
    );
    engine
}
