//! Native Preferences window (Settings).

mod helpers;
mod list_editor;
mod tabs;

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, DeclaredClass, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSScrollView, NSTextField, NSView,
    NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::app_state::AppState;
use crate::config::{self, Config};
use crate::paths::support_dir;
use tabs::{build_controls, build_tab_views, collect_config, TabControls};

const PREFS_W: f64 = 800.0;
const PREFS_H: f64 = 600.0;
const MIN_W: f64 = 680.0;
const MIN_H: f64 = 480.0;
const SIDEBAR_W: f64 = 196.0;
const FOOTER_H: f64 = 64.0;
const NAV_ROW_H: f64 = 28.0;

struct PrefsIvars {
    window: Retained<NSWindow>,
    controls: TabControls,
    error_label: Retained<NSTextField>,
    app_state: Arc<AppState>,
    snapshot: RefCell<Config>,
    apply_delegate: usize,
    /// Section content views (one NSScrollView per sidebar item), and the pane
    /// that hosts the currently-selected one.
    sections: RefCell<Vec<Retained<NSScrollView>>>,
    content_pane: Retained<NSView>,
    current: Cell<usize>,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcPrefsController"]
    #[ivars = PrefsIvars]
    struct PrefsController;

    unsafe impl NSObjectProtocol for PrefsController {}

    unsafe impl NSWindowDelegate for PrefsController {
        /// Red close button hides Settings instead of quitting litecast.
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            self.ivars().window.orderOut(None);
            false
        }
    }

    impl PrefsController {
        #[unsafe(method(prefsSave:))]
        fn prefs_save(&self, _sender: Option<&AnyObject>) {
            let ivars = self.ivars();
            let draft = collect_config(&ivars.controls);
            match config::save(&draft) {
                Ok(()) => {
                    if let Ok(mut cfg) = ivars.app_state.config.write() {
                        *cfg = draft.clone();
                    }
                    if let Err(e) = ivars.app_state.apply_config() {
                        self.set_error(&e);
                        return;
                    }

                    let mut login_err: Option<String> = None;
                    // Apply the login-item toggle immediately.
                    if let Err(e) = crate::login_item::set_enabled(draft.launch_at_login) {
                        login_err = Some(format!("Launch at login: {e}"));
                    }

                    *ivars.snapshot.borrow_mut() = draft;

                    // Re-register global hotkeys (synchronous, on the main thread).
                    self.notify_delegate_apply();
                    let hotkey_err = ivars
                        .app_state
                        .last_hotkey_error
                        .lock()
                        .ok()
                        .and_then(|g| g.clone());

                    match (&hotkey_err, &login_err) {
                        (Some(hk), _) => {
                            // Full, fully-readable modal alert for the hotkey
                            // failure (the message is long and was being clipped
                            // by the inline label). Keep a short inline marker.
                            self.set_error("\u{26a0} Couldn't register the global hotkey \u{2014} see details.");
                            let extra = login_err.as_deref();
                            self.present_error_alert("Shortcut not registered", hk, extra);
                        }
                        (None, Some(le)) => {
                            self.set_error(le);
                        }
                        (None, None) => {
                            self.set_error("");
                        }
                    }
                }
                Err(e) => {
                    self.set_error(&e);
                    self.present_error_alert("Couldn't save settings", &e, None);
                }
            }
        }

        #[unsafe(method(selectSection:))]
        fn select_section(&self, sender: Option<&AnyObject>) {
            if let Some(s) = sender {
                let tag: isize = unsafe { msg_send![s, tag] };
                if tag >= 0 {
                    self.show_section(tag as usize);
                }
            }
        }

        #[unsafe(method(prefsCancel:))]
        fn prefs_cancel(&self, _sender: Option<&AnyObject>) {
            let ivars = self.ivars();
            if let Ok(mut cfg) = ivars.app_state.config.write() {
                *cfg = ivars.snapshot.borrow().clone();
            }
            ivars.error_label.setStringValue(&NSString::from_str(""));
            self.ivars().window.orderOut(None);
        }

        #[unsafe(method(prefsOpenConfigFolder:))]
        fn prefs_open_folder(&self, _sender: Option<&AnyObject>) {
            let dir = support_dir();
            let url = objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(
                &dir.to_string_lossy(),
            ));
            let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
            ws.openURL(&url);
        }

        #[unsafe(method(prefsReloadFromDisk:))]
        fn prefs_reload(&self, _sender: Option<&AnyObject>) {
            // Keep `self` alive across `rebuild_ui` (which clears the only strong
            // ref in PREFS) so the rest of this method can't touch freed memory.
            let _keep_alive = PREFS.with(|p| p.borrow().clone());
            match config::load_result() {
                Ok(cfg) => {
                    if let Ok(mut c) = self.ivars().app_state.config.write() {
                        *c = cfg.clone();
                    }
                    let app_state = self.ivars().app_state.clone();
                    let delegate_ptr = self.ivars().apply_delegate;
                    self.rebuild_ui(cfg);
                    if let Some(mtm) = MainThreadMarker::new() {
                        // Re-open with the freshly-loaded config so the UI mirrors disk.
                        show(mtm, app_state, delegate_ptr);
                    }
                }
                Err(e) => {
                    self.ivars()
                        .error_label
                        .setStringValue(&NSString::from_str(&e));
                }
            }
        }
    }
);

impl PrefsController {
    fn notify_delegate_apply(&self) {
        let ptr = self.ivars().apply_delegate;
        if ptr == 0 {
            return;
        }
        let obj: &AnyObject = unsafe { &*(ptr as *const AnyObject) };
        unsafe {
            let _: () = msg_send![
                obj,
                applyConfigFromPreferences: std::ptr::null::<AnyObject>()
            ];
        }
    }

    fn rebuild_ui(&self, cfg: Config) {
        *self.ivars().snapshot.borrow_mut() = cfg;
        self.ivars().window.orderOut(None);
        PREFS.with(|p| *p.borrow_mut() = None);
    }

    fn set_error(&self, text: &str) {
        self.ivars()
            .error_label
            .setStringValue(&NSString::from_str(text));
    }

    /// Present the full, fully-readable error as a modal NSAlert. `title` is the
    /// bold headline; `body` is the (possibly long) explanation shown in full.
    /// When the message looks shortcut-related, also offer to open the macOS
    /// Keyboard settings pane so the user can free a conflicting shortcut.
    fn present_error_alert(&self, title: &str, body: &str, extra: Option<&str>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let full = match extra {
            Some(e) if !e.is_empty() => format!("{body}\n\n{e}"),
            _ => body.to_string(),
        };
        let alert: Retained<objc2_app_kit::NSAlert> =
            unsafe { msg_send![objc2_app_kit::NSAlert::alloc(mtm), init] };
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(&full));
        alert.setAlertStyle(objc2_app_kit::NSAlertStyle::Warning);

        // Offer a shortcut to the Keyboard settings when the failure is about a
        // global shortcut conflict.
        let offer_keyboard = body.contains("hotkey") || body.contains("shortcut");
        alert.addButtonWithTitle(&NSString::from_str("OK"));
        if offer_keyboard {
            alert.addButtonWithTitle(&NSString::from_str("Open Keyboard Settings\u{2026}"));
        }

        let response = alert.runModal();
        // NSAlertFirstButtonReturn = 1000 (OK), second = 1001 (Open Keyboard).
        if offer_keyboard && response == 1001 {
            let url = objc2_foundation::NSURL::URLWithString(&NSString::from_str(
                "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
            ));
            if let Some(url) = url {
                objc2_app_kit::NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }
    }

    /// Swap the visible section content view in the right-hand pane.
    fn show_section(&self, idx: usize) {
        let ivars = self.ivars();
        let pane = &ivars.content_pane;
        for sub in pane.subviews().iter() {
            sub.removeFromSuperview();
        }
        if let Some(view) = ivars.sections.borrow().get(idx) {
            view.setFrame(pane.bounds());
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            pane.addSubview(view);
        }
        ivars.current.set(idx);
    }
}

thread_local! {
    static PREFS: RefCell<Option<Retained<PrefsController>>> = RefCell::new(None);
}

/// Show or raise the Preferences window.
pub fn show(
    mtm: MainThreadMarker,
    app_state: Arc<AppState>,
    delegate_ptr: usize,
) {
    crate::debug_log::log("preferences::show", "called", "{}"); // DEBUG-TEMP
    if let Some(ctrl) = PREFS.with(|p| p.borrow().clone()) {
        // DEBUG-TEMP
        crate::debug_log::log("preferences::show", "raising existing window", "{}");
        ctrl.ivars()
            .window
            .setDelegate(Some(ProtocolObject::from_ref(&*ctrl)));
        objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        ctrl.ivars().window.makeKeyAndOrderFront(None);
        ctrl.ivars().window.center();
        // DEBUG-TEMP
        crate::debug_log::log(
            "preferences::show",
            "existing window isVisible",
            &format!("{{\"visible\":{}}}", ctrl.ivars().window.isVisible()),
        );
        return;
    }

    let cfg = app_state.config.read().ok().map(|c| c.clone()).unwrap_or_default();
    let controls = build_controls(mtm, &cfg);

    // Each section is an independent, vertically-scrolling content view sized to
    // the right-hand pane; the sidebar swaps which one is shown.
    let content_w = PREFS_W - SIDEBAR_W;
    let content_h = PREFS_H - FOOTER_H;
    let tab_views = build_tab_views(mtm, &controls, content_w, content_h);
    let mut titles: Vec<&'static str> = Vec::with_capacity(tab_views.len());
    let mut sections: Vec<Retained<NSScrollView>> = Vec::with_capacity(tab_views.len());
    for (title, view) in tab_views {
        titles.push(title);
        sections.push(view);
    }

    // Right-hand content pane (resizes with the window).
    let content_pane = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(SIDEBAR_W, FOOTER_H),
            NSSize::new(PREFS_W - SIDEBAR_W, PREFS_H - FOOTER_H),
        ),
    );
    content_pane.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    // Sidebar: a scrollable column of radio items (one per section). Radio
    // buttons in one superview sharing an action form a single-selection group,
    // so every section is always reachable no matter how many there are.
    let n = titles.len();
    let sidebar_doc = helpers::flipped_view(mtm, SIDEBAR_W, (n as f64) * NAV_ROW_H + 8.0);
    let sidebar_scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, FOOTER_H),
            NSSize::new(SIDEBAR_W, PREFS_H - FOOTER_H),
        ),
    );
    sidebar_scroll.setHasVerticalScroller(true);
    sidebar_scroll.setAutohidesScrollers(true);
    sidebar_scroll.setDrawsBackground(false);
    sidebar_scroll.setDocumentView(Some(&sidebar_doc));
    sidebar_scroll.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewHeightSizable | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );

    let error_label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    error_label.setFrame(NSRect::new(
        NSPoint::new(12.0, FOOTER_H - 20.0),
        NSSize::new(PREFS_W - 24.0, 16.0),
    ));
    error_label.setTextColor(Some(&objc2_app_kit::NSColor::systemRedColor()));
    error_label.setFont(Some(&objc2_app_kit::NSFont::systemFontOfSize(11.0)));
    error_label.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );

    let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PREFS_W, PREFS_H));
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    crate::debug_log::log("preferences::show", "alloc window", "{}"); // DEBUG-TEMP
    let window: Retained<NSWindow> = unsafe {
        msg_send![
            NSWindow::alloc(mtm),
            initWithContentRect: content_rect,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // NSWindow created via `initWithContentRect:` defaults to
    // `releasedWhenClosed = true`: AppKit sends an extra `release` to the window
    // when the user clicks the red close button. That extra release is invisible
    // to our `Retained<NSWindow>`, so closing the Settings window would
    // over-release it and leave a dangling pointer. PREFS keeps the controller
    // (and thus this `window` ivar) alive after a close, so the next Dock/Finder
    // reopen would call `makeKeyAndOrderFront` on freed memory and crash with an
    // `objc_msgSend` use-after-free. Opt out so the window's lifetime is owned
    // solely by our `Retained` handle.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("litecast Settings"));
    window.setContentMinSize(NSSize::new(MIN_W, MIN_H));
    if let Some(content) = window.contentView() {
        content.addSubview(&sidebar_scroll);
        content.addSubview(&content_pane);
        content.addSubview(&error_label);
    }

    let ctrl: Retained<PrefsController> = {
        let this = PrefsController::alloc(mtm).set_ivars(PrefsIvars {
            window: window.clone(),
            controls,
            error_label: error_label.clone(),
            app_state: app_state.clone(),
            snapshot: RefCell::new(cfg.clone()),
            apply_delegate: delegate_ptr,
            sections: RefCell::new(sections),
            content_pane: content_pane.clone(),
            current: Cell::new(0),
        });
        unsafe { msg_send![super(this), init] }
    };
    let target: &AnyObject = &ctrl;

    // Sidebar items (target the controller, so build after it exists).
    for (i, title) in titles.iter().enumerate() {
        let y = (i as f64) * NAV_ROW_H + 4.0;
        let item = helpers::nav_item(
            mtm,
            title,
            i as isize,
            8.0,
            y,
            SIDEBAR_W - 16.0,
            sel!(selectSection:),
            target,
        );
        if i == 0 {
            item.setState(1);
        }
        sidebar_doc.addSubview(&item);
    }

    // Footer buttons, pinned to the bottom and anchored left/right.
    let footer_y = 14.0;
    let btn_w = 92.0;
    let left_mask =
        NSAutoresizingMaskOptions::ViewMaxXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin;
    let right_mask =
        NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin;
    let cancel = make_footer_btn(mtm, "Cancel", 12.0, footer_y, btn_w, sel!(prefsCancel:), target);
    cancel.setAutoresizingMask(left_mask);
    let save = make_footer_btn(mtm, "Save", 12.0 + btn_w + 8.0, footer_y, btn_w, sel!(prefsSave:), target);
    save.setAutoresizingMask(left_mask);
    let folder = make_footer_btn(
        mtm,
        "Open config folder",
        PREFS_W - 12.0 - 160.0,
        footer_y,
        160.0,
        sel!(prefsOpenConfigFolder:),
        target,
    );
    folder.setAutoresizingMask(right_mask);
    let reload = make_footer_btn(
        mtm,
        "Reload from disk",
        PREFS_W - 12.0 - 160.0 - 8.0 - 130.0,
        footer_y,
        130.0,
        sel!(prefsReloadFromDisk:),
        target,
    );
    reload.setAutoresizingMask(right_mask);
    if let Some(content) = window.contentView() {
        content.addSubview(&cancel);
        content.addSubview(&save);
        content.addSubview(&folder);
        content.addSubview(&reload);
    }

    // Show the first section.
    ctrl.show_section(0);

    window.setDelegate(Some(ProtocolObject::from_ref(&*ctrl)));
    PREFS.with(|p| *p.borrow_mut() = Some(ctrl.clone()));
    // DEBUG-TEMP: ensure the app is frontmost so the new window is not created
    // behind the (borderless, floating) launcher panel or other apps.
    objc2_app_kit::NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    window.center();
    crate::debug_log::log("preferences::show", "makeKeyAndOrderFront", "{}"); // DEBUG-TEMP
    window.makeKeyAndOrderFront(None);
    window.orderFrontRegardless();
    // DEBUG-TEMP
    crate::debug_log::log(
        "preferences::show",
        "new window shown",
        &format!(
            "{{\"visible\":{},\"level\":{}}}",
            window.isVisible(),
            window.level()
        ),
    );
}

fn make_footer_btn(
    mtm: MainThreadMarker,
    title: &str,
    x: f64,
    y: f64,
    w: f64,
    action: objc2::runtime::Sel,
    target: &AnyObject,
) -> Retained<NSButton> {
    helpers::button(mtm, title, x, y, w, action, target)
}

/// About window (version from bundle).
pub fn show_about(mtm: MainThreadMarker) {
    let version = objc2_foundation::NSBundle::mainBundle()
        .objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString"))
        .and_then(|v| v.downcast::<NSString>().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "0.1.0".to_string());

    let alert: Retained<objc2_app_kit::NSAlert> =
        unsafe { msg_send![objc2_app_kit::NSAlert::alloc(mtm), init] };
    alert.setMessageText(&NSString::from_str("litecast"));
    alert.setInformativeText(&NSString::from_str(&format!(
        "Version {version}\n\nA lightweight native launcher for macOS.\nhttps://github.com/litecast/litecast"
    )));
    alert.runModal();
}
