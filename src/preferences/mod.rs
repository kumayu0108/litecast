//! Native Preferences window (Settings).

mod helpers;
mod tabs;

use std::cell::RefCell;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, AnyThread, DeclaredClass, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSBezelStyle, NSButton, NSButtonType, NSTabView, NSTabViewItem, NSTextField,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::app_state::AppState;
use crate::config::{self, Config};
use crate::paths::support_dir;
use tabs::{build_controls, build_tab_views, collect_config, init_list_rows, TabControls};

const PREFS_W: f64 = 640.0;
const PREFS_H: f64 = 480.0;
const FOOTER_H: f64 = 44.0;
const TAB_H: f64 = PREFS_H - FOOTER_H - 28.0;

struct PrefsIvars {
    window: Retained<NSWindow>,
    tab_view: Retained<NSTabView>,
    controls: TabControls,
    error_label: Retained<NSTextField>,
    app_state: Arc<AppState>,
    snapshot: RefCell<Config>,
    apply_delegate: usize,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcPrefsController"]
    #[ivars = PrefsIvars]
    struct PrefsController;

    unsafe impl NSObjectProtocol for PrefsController {}

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
                        ivars.error_label.setStringValue(&NSString::from_str(&e));
                        return;
                    }
                    ivars.error_label.setStringValue(&NSString::from_str(""));
                    *ivars.snapshot.borrow_mut() = draft;
                    self.notify_delegate_apply();
                }
                Err(e) => {
                    ivars.error_label.setStringValue(&NSString::from_str(&e));
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
            match config::load_result() {
                Ok(cfg) => {
                    if let Ok(mut c) = self.ivars().app_state.config.write() {
                        *c = cfg.clone();
                    }
                    self.rebuild_ui(cfg);
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
        self.ivars().window.close();
        PREFS.with(|p| *p.borrow_mut() = None);
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
    if let Some(ctrl) = PREFS.with(|p| p.borrow().clone()) {
        ctrl.ivars().window.makeKeyAndOrderFront(None);
        ctrl.ivars().window.center();
        return;
    }

    let cfg = app_state.config.read().ok().map(|c| c.clone()).unwrap_or_default();
    let controls = build_controls(mtm, &cfg);
    init_list_rows(mtm, &controls, &cfg);

    let content_h = TAB_H;
    let content_w = PREFS_W - 24.0;
    let tab_views = build_tab_views(mtm, &controls, content_w, content_h);

    let tab_view = NSTabView::initWithFrame(
        NSTabView::alloc(mtm),
        NSRect::new(
            NSPoint::new(12.0, FOOTER_H),
            NSSize::new(PREFS_W - 24.0, TAB_H),
        ),
    );

    for (title, view) in tab_views {
        let item = NSTabViewItem::new();
        item.setLabel(&NSString::from_str(title));
        item.setView(Some(&view));
        tab_view.addTabViewItem(&item);
    }

    let error_label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    error_label.setFrame(NSRect::new(
        NSPoint::new(12.0, FOOTER_H + 4.0),
        NSSize::new(PREFS_W - 24.0, 18.0),
    ));
    error_label.setTextColor(Some(&objc2_app_kit::NSColor::systemRedColor()));

    let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PREFS_W, PREFS_H));
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
    let window: Retained<NSWindow> = unsafe {
        msg_send![
            NSWindow::alloc(mtm),
            initWithContentRect: content_rect,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    window.setTitle(&NSString::from_str("litecast Settings"));
    if let Some(content) = window.contentView() {
        content.addSubview(&tab_view);
        content.addSubview(&error_label);
    }

    let ctrl: Retained<PrefsController> = {
        let this = PrefsController::alloc(mtm).set_ivars(PrefsIvars {
            window: window.clone(),
            tab_view,
            controls,
            error_label: error_label.clone(),
            app_state: app_state.clone(),
            snapshot: RefCell::new(cfg.clone()),
            apply_delegate: delegate_ptr,
        });
        unsafe { msg_send![super(this), init] }
    };
    let target: &AnyObject = &ctrl;

    let footer_y = 8.0;
    let btn_w = 100.0;
    let cancel = make_footer_btn(mtm, "Cancel", 12.0, footer_y, btn_w, sel!(prefsCancel:), target);
    let save = make_footer_btn(mtm, "Save", 12.0 + btn_w + 8.0, footer_y, btn_w, sel!(prefsSave:), target);
    let folder = make_footer_btn(
        mtm,
        "Open config folder",
        PREFS_W - 12.0 - 160.0,
        footer_y,
        160.0,
        sel!(prefsOpenConfigFolder:),
        target,
    );
    let reload = make_footer_btn(
        mtm,
        "Reload from disk",
        PREFS_W - 12.0 - 160.0 - 8.0 - 130.0,
        footer_y,
        130.0,
        sel!(prefsReloadFromDisk:),
        target,
    );
    if let Some(content) = window.contentView() {
        content.addSubview(&cancel);
        content.addSubview(&save);
        content.addSubview(&folder);
        content.addSubview(&reload);
    }

    PREFS.with(|p| *p.borrow_mut() = Some(ctrl.clone()));
    window.center();
    window.makeKeyAndOrderFront(None);
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
