use std::cell::Cell;

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSColor, NSPanel, NSScreen, NSSearchField, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowDelegate,
    NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObjectProtocol, NSPoint, NSRect, NSSize,
};

const PANEL_WIDTH: f64 = 680.0;
const PANEL_HEIGHT: f64 = 64.0;

// NSPanel subclass that is allowed to become the key window even though it is
// borderless (the default AppKit behavior refuses key status for borderless windows).
define_class!(
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcPanel"]
    struct LcPanel;

    impl LcPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }
    }
);

struct Ivars {
    panel: Retained<LcPanel>,
    search: Retained<NSSearchField>,
    visible: Cell<bool>,
    // Kept alive for the lifetime of the app so the hotkey stays registered.
    _hotkey_manager: GlobalHotKeyManager,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcAppDelegate"]
    #[ivars = Ivars]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {}
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.hide();
        }
    }

    impl AppDelegate {
        /// Called on the main thread from the hotkey listener via performSelectorOnMainThread.
        #[unsafe(method(toggleFromHotkey))]
        fn toggle_from_hotkey(&self) {
            self.toggle();
        }
    }
);

impl AppDelegate {
    fn toggle(&self) {
        if self.ivars().visible.get() {
            self.hide();
        } else {
            self.show();
        }
    }

    fn show(&self) {
        let mtm = self.mtm();
        let ivars = self.ivars();
        center_panel(&ivars.panel, mtm);
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        ivars.panel.makeKeyAndOrderFront(None);
        ivars.panel.makeFirstResponder(Some(&ivars.search));
        ivars.visible.set(true);
    }

    fn hide(&self) {
        let ivars = self.ivars();
        if !ivars.visible.get() {
            return;
        }
        ivars.panel.orderOut(None);
        ivars.visible.set(false);
    }
}

fn center_panel(panel: &LcPanel, mtm: MainThreadMarker) {
    if let Some(screen) = NSScreen::mainScreen(mtm) {
        let frame = screen.visibleFrame();
        let x = frame.origin.x + (frame.size.width - PANEL_WIDTH) / 2.0;
        // Position the panel in the upper third of the screen, Spotlight-style.
        let y = frame.origin.y + frame.size.height * 0.62;
        panel.setFrameOrigin(NSPoint::new(x, y));
    }
}

fn build_panel(mtm: MainThreadMarker) -> (Retained<LcPanel>, Retained<NSSearchField>) {
    let content_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(PANEL_WIDTH, PANEL_HEIGHT),
    );
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;

    let panel: Retained<LcPanel> = unsafe {
        msg_send![
            LcPanel::alloc(mtm),
            initWithContentRect: content_rect,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };

    panel.setLevel(25); // ~NSStatusWindowLevel: float above normal windows.
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setFloatingPanel(true);
    panel.setBecomesKeyOnlyIfNeeded(false);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );

    // Vibrancy background with rounded corners.
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_rect);
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setWantsLayer(true);
    if let Some(layer) = effect.layer() {
        unsafe {
            let _: () = msg_send![&*layer, setCornerRadius: 12.0_f64];
        }
    }

    let search_rect = NSRect::new(NSPoint::new(14.0, 14.0), NSSize::new(PANEL_WIDTH - 28.0, 36.0));
    let search = NSSearchField::initWithFrame(NSSearchField::alloc(mtm), search_rect);
    search.setPlaceholderString(Some(&objc2_foundation::NSString::from_str(
        "Search litecast...",
    )));

    effect.addSubview(&search);
    panel.setContentView(Some(&effect));

    (panel, search)
}

fn main() {
    let mtm = MainThreadMarker::new().expect("main() must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let (panel, search) = build_panel(mtm);

    // Register the global hotkey (Option/Alt + Space) to toggle the panel.
    let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
    let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
    manager.register(hotkey).expect("failed to register hotkey");

    let ivars = Ivars {
        panel,
        search,
        visible: Cell::new(false),
        _hotkey_manager: manager,
    };

    let delegate: Retained<AppDelegate> = {
        let this = AppDelegate::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    };

    // Wire the panel delegate back to the app delegate (for resign-key hiding).
    let ivars = delegate.ivars();
    ivars
        .panel
        .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    // Listen for hotkey events on a background thread (blocks when idle, so the
    // app uses zero CPU while hidden) and bounce the toggle onto the main thread.
    let delegate_addr = Retained::as_ptr(&delegate) as usize;
    std::thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.recv() {
            if event.state == HotKeyState::Pressed {
                let ptr = delegate_addr as *const AnyObject;
                unsafe {
                    let obj: &AnyObject = &*ptr;
                    let _: () = msg_send![
                        obj,
                        performSelectorOnMainThread: sel!(toggleFromHotkey),
                        withObject: std::ptr::null::<AnyObject>(),
                        waitUntilDone: false,
                    ];
                }
            }
        }
    });

    app.run();
}
