//! Preferences tab views and draft → `Config` collection.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::MainThreadOnly;
use objc2_app_kit::{NSButton, NSPopUpButton, NSScrollView, NSTextField, NSView};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

use super::list_editor::{build_list_editor, ColSpec, ListEditor};

use crate::config::{
    AiConfig, AppCommandConfig, ClipboardConfig, ColorConfig, CommandConfig, Config,
    ConversionConfig, DateTimeConfig, GitConfig, HotkeyConfig, MenuConfig, NewFileConfig,
    NotesConfig, PomodoroConfig, QuicklinkConfig, ScriptsConfig, SecurityConfig, SnippetConfig, SnippetsConfig,
    TimezoneConfig, ToggleHotkeyConfig, UiConfig, WindowConfig,
};
use crate::preferences::helpers::{
    self, bool_field, caption, checkbox, field, hotkey_recorder, label, popup, popup_selection,
    recorder_combo, scroll_wrap, str_field, HotkeyRecorder, LABEL_W, PAD, ROW_H,
};

/// All editable controls across tabs (filled when tabs are built).
pub struct TabControls {
    pub web_search: Retained<NSTextField>,
    pub launch_login: Retained<NSButton>,
    pub ui_playful: Retained<NSButton>,
    pub ui_critters: Retained<NSButton>,
    pub security_confirm_shell: Retained<NSButton>,
    pub hotkey_toggle: Retained<HotkeyRecorder>,
    pub hotkey_screenshot: Retained<HotkeyRecorder>,
    pub ai_provider: Retained<NSPopUpButton>,
    pub ai_model: Retained<NSTextField>,
    pub ai_endpoint: Retained<NSTextField>,
    pub ai_allow_private: Retained<NSButton>,
    pub clipboard_keep: Retained<NSButton>,
    pub clipboard_max: Retained<NSTextField>,
    pub clipboard_skip_secrets: Retained<NSButton>,
    pub conversion_ttl: Retained<NSTextField>,
    pub window_enabled: Retained<NSButton>,
    pub menu_enabled: Retained<NSButton>,
    pub notes_file: Retained<NSTextField>,
    pub notes_apple: Retained<NSButton>,
    pub scripts_dir: Retained<NSTextField>,
    pub git_scan: Retained<NSTextField>,
    pub git_depth: Retained<NSTextField>,
    pub newfile_base: Retained<NSTextField>,
    pub pom_work: Retained<NSTextField>,
    pub pom_break: Retained<NSTextField>,
    pub pom_long: Retained<NSTextField>,
    pub pom_cycles: Retained<NSTextField>,
    pub color_max: Retained<NSTextField>,
    pub commands: RefCell<Option<Retained<ListEditor>>>,
    pub app_commands: RefCell<Option<Retained<ListEditor>>>,
    pub quicklinks: RefCell<Option<Retained<ListEditor>>>,
    pub snippets: RefCell<Option<Retained<ListEditor>>>,
    pub timezones: RefCell<Option<Retained<ListEditor>>>,
    pub hotkeys_extra: RefCell<Option<Retained<ListEditor>>>,
    /// Raw initial values for each list, captured at build time (the editors are
    /// created lazily when their tab is first built).
    pub init_commands: Vec<Vec<String>>,
    pub init_app_commands: Vec<Vec<String>>,
    pub init_quicklinks: Vec<Vec<String>>,
    pub init_snippets: Vec<Vec<String>>,
    pub init_timezones: Vec<Vec<String>>,
    pub init_hotkeys: Vec<Vec<String>>,
}

pub fn build_controls(mtm: MainThreadMarker, config: &Config) -> TabControls {
    TabControls {
        web_search: field(mtm, &config.web_search_url, 0.0, 0.0, 400.0),
        // Reflect the *actual* current login-item state, not just what the
        // config last recorded (the file on disk is the source of truth).
        launch_login: checkbox(
            mtm,
            "Launch litecast at login",
            crate::login_item::is_enabled(),
            0.0,
            0.0,
            300.0,
        ),
        ui_playful: checkbox(mtm, "Playful placeholders", config.ui.playful_placeholders, 0.0, 0.0, 300.0),
        ui_critters: checkbox(mtm, "Wandering critters", config.ui.critters, 0.0, 0.0, 300.0),
        security_confirm_shell: checkbox(
            mtm,
            "Confirm config shell commands",
            config.security.confirm_config_shell,
            0.0,
            0.0,
            360.0,
        ),
        hotkey_toggle: hotkey_recorder(mtm, &config.hotkey.toggle, 0.0, 0.0, 260.0),
        hotkey_screenshot: hotkey_recorder(mtm, &config.hotkey.screenshot, 0.0, 0.0, 260.0),
        ai_provider: popup(
            mtm,
            &["ollama", "anthropic", "openai", "gemini", "openai-compatible"],
            &config.ai.provider,
            0.0,
            0.0,
            200.0,
        ),
        ai_model: field(mtm, &config.ai.model, 0.0, 0.0, 300.0),
        ai_endpoint: field(mtm, &config.ai.endpoint, 0.0, 0.0, 400.0),
        ai_allow_private: checkbox(
            mtm,
            "Allow private AI endpoints",
            config.ai.allow_private_endpoint,
            0.0,
            0.0,
            300.0,
        ),
        clipboard_keep: checkbox(mtm, "Keep clipboard images", config.clipboard.keep_images, 0.0, 0.0, 300.0),
        clipboard_max: field(mtm, &config.clipboard.max_images.to_string(), 0.0, 0.0, 80.0),
        clipboard_skip_secrets: checkbox(
            mtm,
            "Skip likely secrets in clipboard history",
            config.clipboard.skip_secrets,
            0.0,
            0.0,
            360.0,
        ),
        conversion_ttl: field(
            mtm,
            &config.conversion.currency_ttl_hours.to_string(),
            0.0,
            0.0,
            80.0,
        ),
        window_enabled: checkbox(mtm, "Enable window management", config.window.enabled, 0.0, 0.0, 300.0),
        menu_enabled: checkbox(mtm, "Enable menu-bar search", config.menu.enabled, 0.0, 0.0, 300.0),
        notes_file: field(mtm, &config.notes.file, 0.0, 0.0, 300.0),
        notes_apple: checkbox(mtm, "Mirror to Apple Notes", config.notes.apple_notes, 0.0, 0.0, 300.0),
        scripts_dir: field(mtm, &config.scripts.dir, 0.0, 0.0, 300.0),
        git_scan: field(
            mtm,
            &config.git.scan_dirs.join(", "),
            0.0,
            0.0,
            400.0,
        ),
        git_depth: field(mtm, &config.git.max_depth.to_string(), 0.0, 0.0, 80.0),
        newfile_base: field(mtm, &config.newfile.base_dir, 0.0, 0.0, 300.0),
        pom_work: field(mtm, &config.pomodoro.work_minutes.to_string(), 0.0, 0.0, 80.0),
        pom_break: field(mtm, &config.pomodoro.break_minutes.to_string(), 0.0, 0.0, 80.0),
        pom_long: field(
            mtm,
            &config.pomodoro.long_break_minutes.to_string(),
            0.0,
            0.0,
            80.0,
        ),
        pom_cycles: field(mtm, &config.pomodoro.cycles.to_string(), 0.0, 0.0, 80.0),
        color_max: field(mtm, &config.color.max_recent.to_string(), 0.0, 0.0, 80.0),
        commands: RefCell::new(None),
        app_commands: RefCell::new(None),
        quicklinks: RefCell::new(None),
        snippets: RefCell::new(None),
        timezones: RefCell::new(None),
        hotkeys_extra: RefCell::new(None),
        init_commands: config
            .commands
            .iter()
            .map(|c| vec![c.name.clone(), c.keyword.clone(), c.kind.clone(), c.target.clone()])
            .collect(),
        init_app_commands: config
            .app_commands
            .iter()
            .map(|c| vec![c.keyword.clone(), c.name.clone(), c.kind.clone(), c.template.clone()])
            .collect(),
        init_quicklinks: config
            .quicklinks
            .iter()
            .map(|q| vec![q.name.clone(), q.keyword.clone(), q.url.clone()])
            .collect(),
        init_snippets: config
            .snippets
            .entries
            .iter()
            .map(|s| vec![s.keyword.clone(), s.name.clone(), s.text.clone()])
            .collect(),
        init_timezones: config
            .datetime
            .timezones
            .iter()
            .map(|t| vec![t.name.clone(), t.tz.clone()])
            .collect(),
        init_hotkeys: config
            .hotkeys
            .iter()
            .map(|h| vec![h.key.clone(), h.kind.clone(), h.target.clone()])
            .collect(),
    }
}

/// Read an editor's rows, or fall back to the captured initial values when the
/// editor's tab was never opened (so unopened tabs still round-trip on Save).
fn editor_rows(
    slot: &RefCell<Option<Retained<ListEditor>>>,
    initial: &[Vec<String>],
) -> Vec<Vec<String>> {
    match slot.borrow().as_ref() {
        Some(ed) => ed.read_values(),
        None => initial.to_vec(),
    }
}

/// Get column `i` from a row, trimmed; empty if missing.
fn col(row: &[String], i: usize) -> String {
    row.get(i).cloned().unwrap_or_default()
}

pub fn collect_config(controls: &TabControls) -> Config {
    let commands_rows = editor_rows(&controls.commands, &controls.init_commands);
    let app_command_rows = editor_rows(&controls.app_commands, &controls.init_app_commands);
    let quicklink_rows = editor_rows(&controls.quicklinks, &controls.init_quicklinks);
    let snippet_rows = editor_rows(&controls.snippets, &controls.init_snippets);
    let timezone_rows = editor_rows(&controls.timezones, &controls.init_timezones);
    let hotkey_rows = editor_rows(&controls.hotkeys_extra, &controls.init_hotkeys);

    Config {
        web_search_url: str_field(&controls.web_search),
        launch_at_login: bool_field(&controls.launch_login),
        commands: commands_rows
            .iter()
            .filter(|r| !col(r, 0).is_empty())
            .map(|r| CommandConfig {
                name: col(r, 0),
                subtitle: String::new(),
                keyword: col(r, 1),
                alias: String::new(),
                aliases: Vec::new(),
                kind: col(r, 2),
                target: col(r, 3),
            })
            .collect(),
        app_commands: app_command_rows
            .iter()
            .filter(|r| !col(r, 0).is_empty())
            .map(|r| AppCommandConfig {
                keyword: col(r, 0),
                name: col(r, 1),
                subtitle: String::new(),
                kind: col(r, 2),
                template: col(r, 3),
            })
            .collect(),
        quicklinks: quicklink_rows
            .iter()
            .filter(|r| !col(r, 0).is_empty())
            .map(|r| QuicklinkConfig {
                name: col(r, 0),
                keyword: col(r, 1),
                alias: String::new(),
                aliases: Vec::new(),
                url: col(r, 2),
            })
            .collect(),
        snippets: SnippetsConfig {
            entries: snippet_rows
                .iter()
                .filter(|r| !col(r, 2).is_empty())
                .map(|r| SnippetConfig {
                    keyword: col(r, 0),
                    name: col(r, 1),
                    text: col(r, 2),
                    paste: false,
                })
                .collect(),
        },
        conversion: ConversionConfig {
            currency_ttl_hours: str_field(&controls.conversion_ttl)
                .parse()
                .unwrap_or(12),
        },
        ai: AiConfig {
            provider: popup_selection(&controls.ai_provider),
            model: str_field(&controls.ai_model),
            endpoint: str_field(&controls.ai_endpoint),
            allow_private_endpoint: bool_field(&controls.ai_allow_private),
        },
        ui: UiConfig {
            playful_placeholders: bool_field(&controls.ui_playful),
            critters: bool_field(&controls.ui_critters),
        },
        clipboard: ClipboardConfig {
            keep_images: bool_field(&controls.clipboard_keep),
            max_images: str_field(&controls.clipboard_max).parse().unwrap_or(20),
            skip_secrets: bool_field(&controls.clipboard_skip_secrets),
        },
        window: WindowConfig {
            enabled: bool_field(&controls.window_enabled),
        },
        hotkeys: hotkey_rows
            .iter()
            .filter(|r| !col(r, 0).is_empty())
            .map(|r| HotkeyConfig {
                key: col(r, 0),
                kind: col(r, 1),
                target: col(r, 2),
            })
            .collect(),
        hotkey: ToggleHotkeyConfig {
            toggle: recorder_combo(&controls.hotkey_toggle),
            screenshot: recorder_combo(&controls.hotkey_screenshot),
        },
        notes: NotesConfig {
            file: str_field(&controls.notes_file),
            apple_notes: bool_field(&controls.notes_apple),
        },
        datetime: DateTimeConfig {
            timezones: timezone_rows
                .iter()
                .filter(|r| !col(r, 0).is_empty())
                .map(|r| TimezoneConfig {
                    name: col(r, 0),
                    tz: col(r, 1),
                })
                .collect(),
        },
        scripts: ScriptsConfig {
            dir: str_field(&controls.scripts_dir),
        },
        git: GitConfig {
            scan_dirs: str_field(&controls.git_scan)
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            max_depth: str_field(&controls.git_depth).parse().unwrap_or(2),
        },
        newfile: NewFileConfig {
            base_dir: str_field(&controls.newfile_base),
            templates: Vec::new(),
        },
        pomodoro: PomodoroConfig {
            work_minutes: str_field(&controls.pom_work).parse().unwrap_or(25),
            break_minutes: str_field(&controls.pom_break).parse().unwrap_or(5),
            long_break_minutes: str_field(&controls.pom_long).parse().unwrap_or(15),
            cycles: str_field(&controls.pom_cycles).parse().unwrap_or(4),
        },
        color: ColorConfig {
            max_recent: str_field(&controls.color_max).parse().unwrap_or(12),
        },
        menu: MenuConfig {
            enabled: bool_field(&controls.menu_enabled),
        },
        security: SecurityConfig {
            confirm_config_shell: bool_field(&controls.security_confirm_shell),
        },
    }
}

fn tab_view(
    mtm: MainThreadMarker,
    w: f64,
    h: f64,
    build: impl Fn(MainThreadMarker, f64) -> Retained<NSView>,
) -> Retained<NSScrollView> {
    let inner = build(mtm, w);
    scroll_wrap(mtm, &inner, w, h)
}

pub fn build_tab_views(
    mtm: MainThreadMarker,
    controls: &TabControls,
    content_w: f64,
    content_h: f64,
) -> Vec<(&'static str, Retained<NSScrollView>)> {
    let c = controls;
    vec![
        (
            "General",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                general_tab(mtm, c, w)
            }),
        ),
        (
            "Hotkeys",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                hotkeys_tab(mtm, c, w)
            }),
        ),
        (
            "AI",
            tab_view(mtm, content_w, content_h, |mtm, w| ai_tab(mtm, c, w)),
        ),
        (
            "Commands",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "Custom results you can fuzzy-search by name or trigger by keyword. kind = \"open\" (file/url/app) or \"shell\". If target has {} it is replaced by text typed after the keyword.",
                    vec![
                        ColSpec::text("name", 1.2),
                        ColSpec::text("keyword", 0.8),
                        ColSpec::choice("kind", &["open", "shell"], "open", 0.7),
                        ColSpec::text("target", 1.6),
                    ],
                    &c.commands,
                    &c.init_commands,
                )
            }),
        ),
        (
            "App commands",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "@keyword actions that take a free-text argument. kind = terminal / shell / applescript / open. For shell/open, {query} in the template is replaced by what you type after the keyword. For applescript, reference user input with item 1 of argv in the script body.",
                    vec![
                        ColSpec::text("keyword", 0.8),
                        ColSpec::text("name", 1.0),
                        ColSpec::choice("kind", &["terminal", "shell", "applescript", "open"], "shell", 1.0),
                        ColSpec::text("template", 1.6),
                    ],
                    &c.app_commands,
                    &c.init_app_commands,
                )
            }),
        ),
        (
            "Quicklinks",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "Keyword \u{2192} URL shortcuts. {query} in the url is URL-encoded and substituted. Example url: https://github.com/{query} (type \"ghr rust-lang/rust\").",
                    vec![
                        ColSpec::text("name", 1.0),
                        ColSpec::text("keyword", 0.8),
                        ColSpec::text("url", 2.0),
                    ],
                    &c.quicklinks,
                    &c.init_quicklinks,
                )
            }),
        ),
        (
            "Snippets",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "Reusable text snippets. Browse with the \"snip\" keyword; Enter copies the expanded text. Placeholders in text: {date} {time} {clipboard}.",
                    vec![
                        ColSpec::text("keyword", 0.8),
                        ColSpec::text("name", 1.0),
                        ColSpec::text("text", 2.0),
                    ],
                    &c.snippets,
                    &c.init_snippets,
                )
            }),
        ),
        (
            "Clipboard",
            tab_view(mtm, content_w, content_h, |mtm, w| clipboard_tab(mtm, c, w)),
        ),
        (
            "Conversion",
            tab_view(mtm, content_w, content_h, |mtm, w| conversion_tab(mtm, c, w)),
        ),
        (
            "Window",
            tab_view(mtm, content_w, content_h, |mtm, w| window_tab(mtm, c, w)),
        ),
        (
            "Menu",
            tab_view(mtm, content_w, content_h, |mtm, w| menu_tab(mtm, c, w)),
        ),
        (
            "Notes",
            tab_view(mtm, content_w, content_h, |mtm, w| notes_tab(mtm, c, w)),
        ),
        (
            "Date & time",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "Custom world-clock zones for \"time in <name>\". tz is an IANA identifier, e.g. America/New_York or Asia/Kolkata.",
                    vec![
                        ColSpec::text("name", 1.0),
                        ColSpec::text("tz", 1.8),
                    ],
                    &c.timezones,
                    &c.init_timezones,
                )
            }),
        ),
        (
            "Scripts",
            tab_view(mtm, content_w, content_h, |mtm, w| scripts_tab(mtm, c, w)),
        ),
        (
            "Git",
            tab_view(mtm, content_w, content_h, |mtm, w| git_tab(mtm, c, w)),
        ),
        (
            "New file",
            tab_view(mtm, content_w, content_h, |mtm, w| newfile_tab(mtm, c, w)),
        ),
        (
            "Pomodoro",
            tab_view(mtm, content_w, content_h, |mtm, w| pomodoro_tab(mtm, c, w)),
        ),
        (
            "Color",
            tab_view(mtm, content_w, content_h, |mtm, w| color_tab(mtm, c, w)),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Top-down form builder
//
// All tab content lives in a flipped view (see `helpers::flipped_view`), so we
// lay out from the top down with a simple cursor. Every section starts with a
// gray help caption, and individual controls can carry their own caption — this
// is the in-app, per-setting help text.
// ---------------------------------------------------------------------------

const ROW_GAP: f64 = 10.0;
const HELP_GAP: f64 = 2.0;
const CAP_LINE_H: f64 = 14.0;

/// Approximate wrapped height for a caption rendered at ~11px across `w` points.
fn caption_height(text: &str, w: f64) -> f64 {
    let chars_per_line = (w / 6.2).max(12.0);
    let lines = (text.chars().count() as f64 / chars_per_line).ceil().max(1.0);
    lines * CAP_LINE_H + 2.0
}

struct Form {
    mtm: MainThreadMarker,
    view: Retained<NSView>,
    w: f64,
    y: f64,
}

impl Form {
    fn new(mtm: MainThreadMarker, w: f64) -> Self {
        let view = helpers::flipped_view(mtm, w, 4000.0);
        Self { mtm, view, w, y: PAD }
    }

    fn add_caption(&mut self, text: &str, x: f64) {
        let cw = self.w - x - PAD;
        let h = caption_height(text, cw);
        let c = caption(self.mtm, text, x, self.y, cw, h);
        self.view.addSubview(&c);
        self.y += h + HELP_GAP;
    }

    /// Section-level help shown under the section heading.
    fn section_help(&mut self, text: &str) {
        self.add_caption(text, PAD);
        self.y += 6.0;
    }

    /// Labeled text field + optional per-field help caption.
    fn field_row(&mut self, lbl: &str, fld: &NSTextField, fw: f64, help: &str) {
        self.view.addSubview(&label(self.mtm, lbl, PAD, self.y, LABEL_W));
        let avail = self.w - PAD * 2.0 - LABEL_W;
        let width = if fw <= 0.0 { avail } else { fw.min(avail) };
        fld.setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, self.y),
            NSSize::new(width, ROW_H),
        ));
        self.view.addSubview(fld);
        self.y += ROW_H + HELP_GAP;
        if !help.is_empty() {
            self.add_caption(help, PAD + LABEL_W);
        }
        self.y += ROW_GAP;
    }

    /// Labeled control (popup / recorder / any NSView) + optional help.
    fn control_row(&mut self, lbl: &str, ctrl: &NSView, cw: f64, help: &str) {
        self.view.addSubview(&label(self.mtm, lbl, PAD, self.y, LABEL_W));
        ctrl.setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, self.y),
            NSSize::new(cw, ROW_H),
        ));
        self.view.addSubview(ctrl);
        self.y += ROW_H + HELP_GAP;
        if !help.is_empty() {
            self.add_caption(help, PAD + LABEL_W);
        }
        self.y += ROW_GAP;
    }

    /// Checkbox spanning the row + optional help indented under its label.
    fn checkbox_row(&mut self, cb: &NSButton, help: &str) {
        cb.setFrame(NSRect::new(
            NSPoint::new(PAD, self.y),
            NSSize::new(self.w - PAD * 2.0, ROW_H),
        ));
        self.view.addSubview(cb);
        self.y += ROW_H + HELP_GAP;
        if !help.is_empty() {
            self.add_caption(help, PAD + 22.0);
        }
        self.y += ROW_GAP;
    }

    fn finish(self) -> Retained<NSView> {
        self.view
            .setFrameSize(NSSize::new(self.w, self.y + PAD));
        self.view
    }
}

fn general_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("General launcher behaviour and startup.");
    f.field_row(
        "Web search URL",
        &c.web_search,
        0.0,
        "Opened by the \"Search the web\" fallback. {} is replaced by your query. Example: https://duckduckgo.com/?q={}",
    );
    f.checkbox_row(
        &c.launch_login,
        "Start litecast automatically after you log in (installs a per-user LaunchAgent). Takes effect on next login.",
    );
    f.checkbox_row(
        &c.ui_playful,
        "Show rotating, playful placeholder text in the search field.",
    );
    f.checkbox_row(
        &c.ui_critters,
        "Occasionally let small animated critters wander across the panel. Purely cosmetic.",
    );
    f.checkbox_row(
        &c.security_confirm_shell,
        "Require confirmation before running shell commands from config [[commands]] or hotkeys.",
    );
    f.finish()
}

fn hotkeys_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help(
        "Global shortcuts. Click a recorder, then press your combo (at least one modifier: \u{2318} \u{2325} \u{2303} \u{21e7}). Esc cancels, \u{232b} clears.",
    );
    f.control_row(
        "Toggle panel",
        &c.hotkey_toggle,
        260.0,
        "Show/hide the launcher. Default \u{2325}Space. To use \u{2318}Space, first disable Spotlight's shortcut in System Settings \u{25b8} Keyboard \u{25b8} Keyboard Shortcuts \u{25b8} Spotlight.",
    );
    f.control_row(
        "Screenshot",
        &c.hotkey_screenshot,
        260.0,
        "Capture a screen region and ask the AI about it. Default \u{2325}\u{21e7}Space.",
    );
    f.add_caption(
        "Custom global hotkeys. key is a combo like Cmd+Shift+G. kind = open (url/app), shell, or command (a Commands entry). target is the url/command/command-name.",
        PAD,
    );
    f.y += 4.0;

    let (editor, doc) = build_list_editor(
        mtm,
        w,
        vec![
            ColSpec::text("key", 1.0),
            ColSpec::choice("kind", &["open", "shell", "command"], "open", 0.8),
            ColSpec::text("target", 2.0),
        ],
        c.init_hotkeys.clone(),
    );
    let band_h = content_band_height(c.init_hotkeys.len()).clamp(140.0, 380.0);
    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, f.y), NSSize::new(w, band_h)),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setDocumentView(Some(&doc));
    f.view.addSubview(&scroll);
    f.y += band_h + ROW_GAP;
    *c.hotkeys_extra.borrow_mut() = Some(editor);

    f.finish()
}

fn ai_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Backend for ? questions, screenshot questions, and quick AI commands (translate / summarize / fix / improve).");
    f.control_row(
        "Provider",
        &c.ai_provider,
        200.0,
        "anthropic, openai, gemini, openai-compatible, or ollama (local, no key needed).",
    );
    f.field_row(
        "Model",
        &c.ai_model,
        280.0,
        "Model id for the provider, e.g. claude-3-5-sonnet, gpt-4o, gemini-2.5-flash, or llama3.2 (Ollama).",
    );
    f.field_row(
        "Endpoint",
        &c.ai_endpoint,
        0.0,
        "Override base URL. Leave empty for hosted providers; for Ollama use http://127.0.0.1:11434. Endpoints are validated to block SSRF.",
    );
    f.checkbox_row(
        &c.ai_allow_private,
        "Allow private/link-local endpoints for openai-compatible providers (advanced).",
    );
    f.add_caption(
        "API keys are stored in the macOS Keychain, never in config.toml. Set one with \"setkey <key>\" (or \"setup\") in the launcher.",
        PAD,
    );
    f.finish()
}

fn clipboard_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Clipboard history (\"clip\" keyword). Pin entries with \"clip pin <n>\".");
    f.checkbox_row(
        &c.clipboard_keep,
        "Capture images copied to the clipboard (stored under the support directory).",
    );
    f.field_row(
        "Max images",
        &c.clipboard_max,
        80.0,
        "How many captured images to keep before old ones are dropped (pinned images are exempt).",
    );
    f.checkbox_row(
        &c.clipboard_skip_secrets,
        "Skip recording clipboard text that looks like an API key, password, or token.",
    );
    f.finish()
}

fn conversion_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Unit & currency conversion, e.g. \"10 km in mi\", \"100 usd to eur\".");
    f.field_row(
        "Currency cache TTL (hours)",
        &c.conversion_ttl,
        80.0,
        "How long cached exchange rates are reused before refreshing from the public rates API.",
    );
    f.finish()
}

fn window_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Window management (\"win\" commands, e.g. \"win left\", \"win max\").");
    f.checkbox_row(
        &c.window_enabled,
        "Enable window snapping/resizing. This is the only feature that needs the Accessibility permission; macOS prompts on first use. Off by default.",
    );
    f.finish()
}

fn menu_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Menu-bar search (\"menu\" keyword): list and trigger the frontmost app's menu items.");
    f.checkbox_row(
        &c.menu_enabled,
        "Enable menu-bar search. Needs the Accessibility permission (prompts on first use). Off by default.",
    );
    f.finish()
}

fn notes_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Quick notes: \"note <text>\" appends a timestamped line; \"note\" opens the file.");
    f.field_row(
        "Notes file",
        &c.notes_file,
        280.0,
        "Relative paths resolve under the support dir; absolute paths are used as-is. Empty = notes.txt in the support dir.",
    );
    f.checkbox_row(
        &c.notes_apple,
        "Also create a note in Apple Notes on each capture (asks for Automation permission the first time).",
    );
    f.finish()
}

fn scripts_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Drop executable scripts in this folder and they appear as runnable commands (with optional @litecast.* header metadata).");
    f.field_row(
        "Scripts directory",
        &c.scripts_dir,
        0.0,
        "Relative paths resolve under the support dir; absolute used as-is. Empty = scripts/ in the support dir.",
    );
    f.finish()
}

fn git_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Git helper (\"git\" / \"repo\"): list and open recent repositories.");
    f.field_row(
        "Scan dirs (comma-separated)",
        &c.git_scan,
        0.0,
        "Folders scanned for repositories, e.g. ~/Developer, ~/work. Empty = ~/Developer ~/Projects ~/Code ~/src.",
    );
    f.field_row(
        "Max depth",
        &c.git_depth,
        80.0,
        "How many directory levels deep to look for a .git folder.",
    );
    f.finish()
}

fn newfile_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Quick file/folder creation: \"new file <name>\", \"new folder <name>\".");
    f.field_row(
        "Base directory",
        &c.newfile_base,
        0.0,
        "Where relative names are created. Empty = ~/Desktop (else your home folder).",
    );
    f.finish()
}

fn pomodoro_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Pomodoro / focus timer (\"pomodoro\", or \"focus 50\" to override work length). Durations in minutes.");
    f.field_row("Work (min)", &c.pom_work, 80.0, "Length of each focus session.");
    f.field_row("Break (min)", &c.pom_break, 80.0, "Short break after a work session.");
    f.field_row("Long break (min)", &c.pom_long, 80.0, "Longer break after a full set of cycles.");
    f.field_row("Cycles", &c.pom_cycles, 80.0, "Work sessions to complete before a long break.");
    f.finish()
}

fn color_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help("Screen color picker (\"pick color\"). Recently picked colors are listed under \"colors\".");
    f.field_row(
        "Max recent colors",
        &c.color_max,
        80.0,
        "How many recently picked colors to remember.",
    );
    f.finish()
}

/// Generic editable list section: help caption + an embedded `ListEditor` with
/// ＋ Add / Remove. The editor is stored back into `slot` so Save can read it.
fn list_tab(
    mtm: MainThreadMarker,
    w: f64,
    help: &str,
    cols: Vec<ColSpec>,
    slot: &RefCell<Option<Retained<ListEditor>>>,
    initial: &[Vec<String>],
) -> Retained<NSView> {
    let mut f = Form::new(mtm, w);
    f.section_help(help);

    let (editor, doc) = build_list_editor(mtm, w, cols, initial.to_vec());

    // Give the editor a generous fixed-height band. The editor's own `doc` is the
    // scroll's documentView, so as Add/Remove resizes `doc` the scroll updates
    // its content automatically — rows never clip and the section stays stable.
    let band_h = content_band_height(initial.len()).clamp(160.0, 460.0);
    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, f.y), NSSize::new(w, band_h)),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setDocumentView(Some(&doc));
    f.view.addSubview(&scroll);
    f.y += band_h + ROW_GAP;

    *slot.borrow_mut() = Some(editor);
    f.finish()
}

/// Visible height for a list editor band given its initial row count.
fn content_band_height(rows: usize) -> f64 {
    // add button + header + rows, with headroom for a few added rows.
    let base = ROW_H * 2.0 + 24.0;
    base + ((rows + 3) as f64) * (ROW_H + 6.0)
}
