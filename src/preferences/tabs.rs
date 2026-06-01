//! Preferences tab views and draft → `Config` collection.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, MainThreadOnly};
use objc2_app_kit::{NSButton, NSPopUpButton, NSScrollView, NSTextField, NSView};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

use crate::config::{
    AiConfig, AppCommandConfig, ClipboardConfig, ColorConfig, CommandConfig, Config,
    ConversionConfig, DateTimeConfig, GitConfig, HotkeyConfig, MenuConfig, NewFileConfig,
    NotesConfig, PomodoroConfig, QuicklinkConfig, ScriptsConfig, SnippetConfig, SnippetsConfig,
    TimezoneConfig, ToggleHotkeyConfig, UiConfig, WindowConfig,
};
use crate::preferences::helpers::{
    self, bool_field, button, checkbox, field, label, popup, popup_selection, scroll_wrap,
    str_field, LABEL_W, PAD, ROW_H,
};

/// All editable controls across tabs (filled when tabs are built).
pub struct TabControls {
    pub web_search: Retained<NSTextField>,
    pub ui_playful: Retained<NSButton>,
    pub ui_critters: Retained<NSButton>,
    pub hotkey_toggle: Retained<NSTextField>,
    pub hotkey_screenshot: Retained<NSTextField>,
    pub hotkeys_extra: Rc<RefCell<Vec<HotkeyRow>>>,
    pub ai_provider: Retained<NSPopUpButton>,
    pub ai_model: Retained<NSTextField>,
    pub ai_endpoint: Retained<NSTextField>,
    pub clipboard_keep: Retained<NSButton>,
    pub clipboard_max: Retained<NSTextField>,
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
    pub commands: Rc<RefCell<Vec<CommandRow>>>,
    pub app_commands: Rc<RefCell<Vec<AppCommandRow>>>,
    pub quicklinks: Rc<RefCell<Vec<QuicklinkRow>>>,
    pub snippets: Rc<RefCell<Vec<SnippetRow>>>,
    pub timezones: Rc<RefCell<Vec<TimezoneRow>>>,
}

pub struct HotkeyRow {
    pub key: Retained<NSTextField>,
    pub kind: Retained<NSTextField>,
    pub target: Retained<NSTextField>,
}

pub struct CommandRow {
    pub name: Retained<NSTextField>,
    pub keyword: Retained<NSTextField>,
    pub kind: Retained<NSTextField>,
    pub target: Retained<NSTextField>,
}

pub struct AppCommandRow {
    pub keyword: Retained<NSTextField>,
    pub name: Retained<NSTextField>,
    pub kind: Retained<NSTextField>,
    pub template: Retained<NSTextField>,
}

pub struct QuicklinkRow {
    pub name: Retained<NSTextField>,
    pub keyword: Retained<NSTextField>,
    pub url: Retained<NSTextField>,
}

pub struct SnippetRow {
    pub keyword: Retained<NSTextField>,
    pub name: Retained<NSTextField>,
    pub text: Retained<NSTextField>,
}

pub struct TimezoneRow {
    pub name: Retained<NSTextField>,
    pub tz: Retained<NSTextField>,
}

pub fn build_controls(mtm: MainThreadMarker, config: &Config) -> TabControls {
    TabControls {
        web_search: field(mtm, &config.web_search_url, 0.0, 0.0, 400.0),
        ui_playful: checkbox(mtm, "Playful placeholders", config.ui.playful_placeholders, 0.0, 0.0, 300.0),
        ui_critters: checkbox(mtm, "Wandering critters", config.ui.critters, 0.0, 0.0, 300.0),
        hotkey_toggle: field(mtm, &config.hotkey.toggle, 0.0, 0.0, 300.0),
        hotkey_screenshot: field(mtm, &config.hotkey.screenshot, 0.0, 0.0, 300.0),
        hotkeys_extra: Rc::new(RefCell::new(Vec::new())),
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
        clipboard_keep: checkbox(mtm, "Keep clipboard images", config.clipboard.keep_images, 0.0, 0.0, 300.0),
        clipboard_max: field(mtm, &config.clipboard.max_images.to_string(), 0.0, 0.0, 80.0),
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
        commands: Rc::new(RefCell::new(Vec::new())),
        app_commands: Rc::new(RefCell::new(Vec::new())),
        quicklinks: Rc::new(RefCell::new(Vec::new())),
        snippets: Rc::new(RefCell::new(Vec::new())),
        timezones: Rc::new(RefCell::new(Vec::new())),
    }
}

pub fn init_list_rows(mtm: MainThreadMarker, controls: &TabControls, config: &Config) {
    for hk in &config.hotkeys {
        controls.hotkeys_extra.borrow_mut().push(HotkeyRow {
            key: field(mtm, &hk.key, 0.0, 0.0, 140.0),
            kind: field(mtm, &hk.kind, 0.0, 0.0, 80.0),
            target: field(mtm, &hk.target, 0.0, 0.0, 200.0),
        });
    }
    for c in &config.commands {
        controls.commands.borrow_mut().push(CommandRow {
            name: field(mtm, &c.name, 0.0, 0.0, 120.0),
            keyword: field(mtm, &c.keyword, 0.0, 0.0, 80.0),
            kind: field(mtm, &c.kind, 0.0, 0.0, 60.0),
            target: field(mtm, &c.target, 0.0, 0.0, 200.0),
        });
    }
    for c in &config.app_commands {
        controls.app_commands.borrow_mut().push(AppCommandRow {
            keyword: field(mtm, &c.keyword, 0.0, 0.0, 80.0),
            name: field(mtm, &c.name, 0.0, 0.0, 120.0),
            kind: field(mtm, &c.kind, 0.0, 0.0, 80.0),
            template: field(mtm, &c.template, 0.0, 0.0, 200.0),
        });
    }
    for q in &config.quicklinks {
        controls.quicklinks.borrow_mut().push(QuicklinkRow {
            name: field(mtm, &q.name, 0.0, 0.0, 120.0),
            keyword: field(mtm, &q.keyword, 0.0, 0.0, 80.0),
            url: field(mtm, &q.url, 0.0, 0.0, 260.0),
        });
    }
    for s in &config.snippets.entries {
        controls.snippets.borrow_mut().push(SnippetRow {
            keyword: field(mtm, &s.keyword, 0.0, 0.0, 80.0),
            name: field(mtm, &s.name, 0.0, 0.0, 120.0),
            text: field(mtm, &s.text, 0.0, 0.0, 260.0),
        });
    }
    for t in &config.datetime.timezones {
        controls.timezones.borrow_mut().push(TimezoneRow {
            name: field(mtm, &t.name, 0.0, 0.0, 120.0),
            tz: field(mtm, &t.tz, 0.0, 0.0, 200.0),
        });
    }
}

pub fn collect_config(controls: &TabControls) -> Config {
    Config {
        web_search_url: str_field(&controls.web_search),
        commands: controls
            .commands
            .borrow()
            .iter()
            .filter(|r| !str_field(&r.name).is_empty())
            .map(|r| CommandConfig {
                name: str_field(&r.name),
                subtitle: String::new(),
                keyword: str_field(&r.keyword),
                alias: String::new(),
                aliases: Vec::new(),
                kind: str_field(&r.kind),
                target: str_field(&r.target),
            })
            .collect(),
        app_commands: controls
            .app_commands
            .borrow()
            .iter()
            .filter(|r| !str_field(&r.keyword).is_empty())
            .map(|r| AppCommandConfig {
                keyword: str_field(&r.keyword),
                name: str_field(&r.name),
                subtitle: String::new(),
                kind: str_field(&r.kind),
                template: str_field(&r.template),
            })
            .collect(),
        quicklinks: controls
            .quicklinks
            .borrow()
            .iter()
            .filter(|r| !str_field(&r.name).is_empty())
            .map(|r| QuicklinkConfig {
                name: str_field(&r.name),
                keyword: str_field(&r.keyword),
                alias: String::new(),
                aliases: Vec::new(),
                url: str_field(&r.url),
            })
            .collect(),
        snippets: SnippetsConfig {
            entries: controls
                .snippets
                .borrow()
                .iter()
                .filter(|r| !str_field(&r.text).is_empty())
                .map(|r| SnippetConfig {
                    keyword: str_field(&r.keyword),
                    name: str_field(&r.name),
                    text: str_field(&r.text),
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
        },
        ui: UiConfig {
            playful_placeholders: bool_field(&controls.ui_playful),
            critters: bool_field(&controls.ui_critters),
        },
        clipboard: ClipboardConfig {
            keep_images: bool_field(&controls.clipboard_keep),
            max_images: str_field(&controls.clipboard_max).parse().unwrap_or(20),
        },
        window: WindowConfig {
            enabled: bool_field(&controls.window_enabled),
        },
        hotkeys: controls
            .hotkeys_extra
            .borrow()
            .iter()
            .filter(|r| !str_field(&r.key).is_empty())
            .map(|r| HotkeyConfig {
                key: str_field(&r.key),
                kind: str_field(&r.kind),
                target: str_field(&r.target),
            })
            .collect(),
        hotkey: ToggleHotkeyConfig {
            toggle: str_field(&controls.hotkey_toggle),
            screenshot: str_field(&controls.hotkey_screenshot),
        },
        notes: NotesConfig {
            file: str_field(&controls.notes_file),
            apple_notes: bool_field(&controls.notes_apple),
        },
        datetime: DateTimeConfig {
            timezones: controls
                .timezones
                .borrow()
                .iter()
                .filter(|r| !str_field(&r.name).is_empty())
                .map(|r| TimezoneConfig {
                    name: str_field(&r.name),
                    tz: str_field(&r.tz),
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
    }
}

fn tab_view(
    mtm: MainThreadMarker,
    w: f64,
    h: f64,
    build: impl Fn(MainThreadMarker, f64) -> Retained<NSView>,
) -> Retained<NSScrollView> {
    let inner = build(mtm, w - PAD * 2.0);
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
                    "name",
                    "keyword",
                    "kind",
                    "target",
                    &c.commands,
                    |mtm| CommandRow {
                        name: field(mtm, "", 0.0, 0.0, 120.0),
                        keyword: field(mtm, "", 0.0, 0.0, 80.0),
                        kind: field(mtm, "open", 0.0, 0.0, 60.0),
                        target: field(mtm, "", 0.0, 0.0, 200.0),
                    },
                )
            }),
        ),
        (
            "App commands",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "keyword",
                    "name",
                    "kind",
                    "template",
                    &c.app_commands,
                    |mtm| AppCommandRow {
                        keyword: field(mtm, "", 0.0, 0.0, 80.0),
                        name: field(mtm, "", 0.0, 0.0, 120.0),
                        kind: field(mtm, "shell", 0.0, 0.0, 80.0),
                        template: field(mtm, "", 0.0, 0.0, 200.0),
                    },
                )
            }),
        ),
        (
            "Quicklinks",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "name",
                    "keyword",
                    "url",
                    "",
                    &c.quicklinks,
                    |mtm| QuicklinkRow {
                        name: field(mtm, "", 0.0, 0.0, 120.0),
                        keyword: field(mtm, "", 0.0, 0.0, 80.0),
                        url: field(mtm, "", 0.0, 0.0, 260.0),
                    },
                )
            }),
        ),
        (
            "Snippets",
            tab_view(mtm, content_w, content_h, |mtm, w| {
                list_tab(
                    mtm,
                    w,
                    "keyword",
                    "name",
                    "text",
                    "",
                    &c.snippets,
                    |mtm| SnippetRow {
                        keyword: field(mtm, "", 0.0, 0.0, 80.0),
                        name: field(mtm, "", 0.0, 0.0, 120.0),
                        text: field(mtm, "", 0.0, 0.0, 260.0),
                    },
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
                    "name",
                    "tz",
                    "",
                    "",
                    &c.timezones,
                    |mtm| TimezoneRow {
                        name: field(mtm, "", 0.0, 0.0, 120.0),
                        tz: field(mtm, "America/New_York", 0.0, 0.0, 200.0),
                    },
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

fn place_row(view: &NSView, fields: &[&NSTextField], y: f64, w: f64) {
    let cols = fields.len();
    let gap = 8.0;
    let col_w = (w - gap * (cols as f64 - 1.0)) / cols as f64;
    let mut x = PAD;
    for f in fields {
        f.setFrame(NSRect::new(
            NSPoint::new(x, y),
            NSSize::new(col_w, ROW_H),
        ));
        view.addSubview(f);
        x += col_w + gap;
    }
}

fn general_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 200.0);
    let mut y = 120.0;
    view.addSubview(&label(mtm, "Web search URL", PAD, y, LABEL_W));
    c.web_search
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, y),
            NSSize::new(w - PAD * 2.0 - LABEL_W, ROW_H),
        ));
    view.addSubview(&c.web_search);
    y -= ROW_H + 12.0;
    c.ui_playful
        .setFrame(NSRect::new(NSPoint::new(PAD, y), NSSize::new(300.0, ROW_H)));
    view.addSubview(&c.ui_playful);
    y -= ROW_H + 8.0;
    c.ui_critters
        .setFrame(NSRect::new(NSPoint::new(PAD, y), NSSize::new(300.0, ROW_H)));
    view.addSubview(&c.ui_critters);
    view.setFrameSize(NSSize::new(w, 160.0));
    view
}

fn hotkeys_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 400.0);
    let mut y = 360.0;
    for (lbl, fld) in [
        ("Toggle panel", &c.hotkey_toggle),
        ("Screenshot", &c.hotkey_screenshot),
    ] {
        view.addSubview(&label(mtm, lbl, PAD, y, LABEL_W));
        fld.setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, y),
            NSSize::new(280.0, ROW_H),
        ));
        view.addSubview(fld);
        y -= ROW_H + 10.0;
    }
    view.addSubview(&label(
        mtm,
        "Custom hotkeys (key / kind / target)",
        PAD,
        y,
        w - PAD * 2.0,
    ));
    y -= ROW_H + 6.0;
    for row in c.hotkeys_extra.borrow().iter() {
        place_row(&view, &[&row.key, &row.kind, &row.target], y, w);
        y -= ROW_H + 6.0;
    }
    view.setFrameSize(NSSize::new(w, (360.0 - y + 40.0).max(120.0)));
    view
}

fn ai_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 220.0);
    let mut y = 180.0;
    view.addSubview(&label(mtm, "Provider", PAD, y, LABEL_W));
    c.ai_provider
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, y),
            NSSize::new(200.0, ROW_H),
        ));
    view.addSubview(&c.ai_provider);
    y -= ROW_H + 10.0;
    view.addSubview(&label(mtm, "Model", PAD, y, LABEL_W));
    c.ai_model
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, y),
            NSSize::new(280.0, ROW_H),
        ));
    view.addSubview(&c.ai_model);
    y -= ROW_H + 10.0;
    view.addSubview(&label(mtm, "Endpoint", PAD, y, LABEL_W));
    c.ai_endpoint
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, y),
            NSSize::new(w - PAD * 2.0 - LABEL_W, ROW_H),
        ));
    view.addSubview(&c.ai_endpoint);
    y -= ROW_H + 16.0;
    let hint = label(
        mtm,
        "API keys are stored in Keychain, not config.toml. Use litecast setkey or Set API key in the launcher.",
        PAD,
        y,
        w - PAD * 2.0,
    );
    view.addSubview(&hint);
    view.setFrameSize(NSSize::new(w, 200.0));
    view
}

fn clipboard_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    simple_two_row(mtm, w, &c.clipboard_keep, "Max images", &c.clipboard_max, 80.0)
}

fn conversion_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 80.0);
    view.addSubview(&label(mtm, "Currency cache TTL (hours)", PAD, 40.0, LABEL_W));
    c.conversion_ttl
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, 40.0),
            NSSize::new(80.0, ROW_H),
        ));
    view.addSubview(&c.conversion_ttl);
    view
}

fn window_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    checkbox_only(mtm, w, &c.window_enabled)
}

fn menu_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    checkbox_only(mtm, w, &c.menu_enabled)
}

fn notes_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 120.0);
    view.addSubview(&label(mtm, "Notes file", PAD, 80.0, LABEL_W));
    c.notes_file
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, 80.0),
            NSSize::new(280.0, ROW_H),
        ));
    view.addSubview(&c.notes_file);
    c.notes_apple
        .setFrame(NSRect::new(NSPoint::new(PAD, 44.0), NSSize::new(300.0, ROW_H)));
    view.addSubview(&c.notes_apple);
    view
}

fn scripts_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    single_field_tab(mtm, w, "Scripts directory", &c.scripts_dir)
}

fn git_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 100.0);
    view.addSubview(&label(mtm, "Scan dirs (comma-separated)", PAD, 60.0, LABEL_W));
    c.git_scan
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, 60.0),
            NSSize::new(w - PAD * 2.0 - LABEL_W, ROW_H),
        ));
    view.addSubview(&c.git_scan);
    view.addSubview(&label(mtm, "Max depth", PAD, 24.0, LABEL_W));
    c.git_depth
        .setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, 24.0),
            NSSize::new(80.0, ROW_H),
        ));
    view.addSubview(&c.git_depth);
    view
}

fn newfile_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    single_field_tab(mtm, w, "Base directory", &c.newfile_base)
}

fn pomodoro_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 160.0);
    let rows = [
        ("Work (min)", &c.pom_work),
        ("Break (min)", &c.pom_break),
        ("Long break (min)", &c.pom_long),
        ("Cycles", &c.pom_cycles),
    ];
    let mut y = 120.0;
    for (lbl, fld) in rows {
        view.addSubview(&label(mtm, lbl, PAD, y, LABEL_W));
        fld.setFrame(NSRect::new(
            NSPoint::new(PAD + LABEL_W, y),
            NSSize::new(80.0, ROW_H),
        ));
        view.addSubview(fld);
        y -= ROW_H + 8.0;
    }
    view
}

fn color_tab(mtm: MainThreadMarker, c: &TabControls, w: f64) -> Retained<NSView> {
    single_field_tab(mtm, w, "Max recent colors", &c.color_max)
}

fn checkbox_only(mtm: MainThreadMarker, w: f64, cb: &NSButton) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 60.0);
    cb.setFrame(NSRect::new(NSPoint::new(PAD, 20.0), NSSize::new(400.0, ROW_H)));
    view.addSubview(cb);
    view
}

fn single_field_tab(
    mtm: MainThreadMarker,
    w: f64,
    lbl: &str,
    fld: &NSTextField,
) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 60.0);
    view.addSubview(&label(mtm, lbl, PAD, 20.0, LABEL_W));
    fld.setFrame(NSRect::new(
        NSPoint::new(PAD + LABEL_W, 20.0),
        NSSize::new(w - PAD * 2.0 - LABEL_W, ROW_H),
    ));
    view.addSubview(fld);
    view
}

fn simple_two_row(
    mtm: MainThreadMarker,
    w: f64,
    cb: &NSButton,
    lbl2: &str,
    fld2: &NSTextField,
    fw: f64,
) -> Retained<NSView> {
    let view = helpers::container(mtm, w, 90.0);
    cb.setFrame(NSRect::new(NSPoint::new(PAD, 50.0), NSSize::new(400.0, ROW_H)));
    view.addSubview(cb);
    view.addSubview(&label(mtm, lbl2, PAD, 16.0, LABEL_W));
    fld2.setFrame(NSRect::new(
        NSPoint::new(PAD + LABEL_W, 16.0),
        NSSize::new(fw, ROW_H),
    ));
    view.addSubview(fld2);
    view
}

/// Generic list editor tab (3–4 columns).
fn list_tab<T>(
    mtm: MainThreadMarker,
    w: f64,
    h1: &str,
    h2: &str,
    h3: &str,
    h4: &str,
    rows: &Rc<RefCell<Vec<T>>>,
    _new_row: impl Fn(MainThreadMarker) -> T,
) -> Retained<NSView>
where
    T: ListRow,
{
    let mut height = 40.0 + rows.borrow().len() as f64 * (ROW_H + 6.0);
    height = height.max(80.0);
    let view = helpers::container(mtm, w, height);
    let mut y = height - 30.0;
    let headers = [h1, h2, h3, h4].into_iter().filter(|s| !s.is_empty());
    let ncol = [h1, h2, h3, h4].iter().filter(|s| !s.is_empty()).count();
    if ncol > 0 {
        let gap = 8.0;
        let col_w = (w - PAD * 2.0 - gap * (ncol as f64 - 1.0)) / ncol as f64;
        let mut x = PAD;
        for h in headers {
            let lbl = label(mtm, h, x, y, col_w);
            view.addSubview(&lbl);
            x += col_w + gap;
        }
        y -= ROW_H + 8.0;
    }
    for row in rows.borrow().iter() {
        row.place(&view, y, w);
        y -= ROW_H + 6.0;
    }
    view.setFrameSize(NSSize::new(w, height));
    view
}

pub trait ListRow {
    fn place(&self, view: &NSView, y: f64, w: f64);
}

impl ListRow for CommandRow {
    fn place(&self, view: &NSView, y: f64, w: f64) {
        place_row(view, &[&self.name, &self.keyword, &self.kind, &self.target], y, w);
    }
}
impl ListRow for AppCommandRow {
    fn place(&self, view: &NSView, y: f64, w: f64) {
        place_row(
            view,
            &[&self.keyword, &self.name, &self.kind, &self.template],
            y,
            w,
        );
    }
}
impl ListRow for QuicklinkRow {
    fn place(&self, view: &NSView, y: f64, w: f64) {
        place_row(view, &[&self.name, &self.keyword, &self.url], y, w);
    }
}
impl ListRow for SnippetRow {
    fn place(&self, view: &NSView, y: f64, w: f64) {
        place_row(view, &[&self.keyword, &self.name, &self.text], y, w);
    }
}
impl ListRow for TimezoneRow {
    fn place(&self, view: &NSView, y: f64, w: f64) {
        place_row(view, &[&self.name, &self.tz], y, w);
    }
}
