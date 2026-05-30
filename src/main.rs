mod clipboard;
mod engine;
mod model;
mod providers;

use std::cell::{Cell, RefCell};

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSColor, NSControl, NSFont, NSPanel, NSScreen, NSScrollView, NSSearchField, NSTableColumn,
    NSTableView, NSTextField, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowDelegate,
    NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSIndexSet, NSNotification, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString,
};

use engine::Engine;
use model::Item;
use providers::AppsProvider;

const PANEL_WIDTH: f64 = 680.0;
const SEARCH_AREA_H: f64 = 64.0;
const ROW_H: f64 = 44.0;
const MAX_VISIBLE_ROWS: usize = 8;
// Fraction of the screen height where the panel's top edge sits.
const TOP_FRACTION: f64 = 0.80;

// NSPanel subclass allowed to become the key window despite being borderless.
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
    table: Retained<NSTableView>,
    scroll: Retained<NSScrollView>,
    visible: Cell<bool>,
    engine: Engine,
    results: RefCell<Vec<Item>>,
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
            self.hide_and_reset();
        }
    }

    impl AppDelegate {
        // Invoked on the main thread from the hotkey listener thread.
        #[unsafe(method(toggleFromHotkey))]
        fn toggle_from_hotkey(&self) {
            self.toggle();
        }

        // NSSearchField text changed: re-run the query.
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, _notification: &NSNotification) {
            let query = self.ivars().search.stringValue().to_string();
            self.run_query(&query);
        }

        // Intercept navigation keys while editing the search field.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn control_do_command(
            &self,
            _control: &NSControl,
            _text_view: &AnyObject,
            selector: Sel,
        ) -> bool {
            if selector == sel!(moveDown:) {
                self.move_selection(1);
                true
            } else if selector == sel!(moveUp:) {
                self.move_selection(-1);
                true
            } else if selector == sel!(insertNewline:) {
                self.activate_selection();
                true
            } else if selector == sel!(cancelOperation:) {
                self.hide_and_reset();
                true
            } else {
                false
            }
        }

        // NSTableViewDataSource
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table: &NSTableView) -> isize {
            self.ivars().results.borrow().len() as isize
        }

        // NSTableViewDelegate (view-based row)
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for_row(
            &self,
            _table: &NSTableView,
            _column: Option<&NSTableColumn>,
            row: isize,
        ) -> Option<Retained<NSTextField>> {
            let results = self.ivars().results.borrow();
            match results.get(row as usize) {
                Some(item) => Some(make_row_cell(self.mtm(), item)),
                None => None,
            }
        }
    }
);

impl AppDelegate {
    fn toggle(&self) {
        if self.ivars().visible.get() {
            self.hide_and_reset();
        } else {
            self.show();
        }
    }

    fn show(&self) {
        let mtm = self.mtm();
        let ivars = self.ivars();
        self.layout(self.ivars().results.borrow().len());
        let _ = mtm;
        let app = NSApplication::sharedApplication(self.mtm());
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        ivars.panel.makeKeyAndOrderFront(None);
        ivars.panel.makeFirstResponder(Some(&ivars.search));
        ivars.visible.set(true);
    }

    fn hide_and_reset(&self) {
        let ivars = self.ivars();
        if !ivars.visible.get() {
            return;
        }
        ivars.panel.orderOut(None);
        ivars.visible.set(false);
        // Clear state for the next invocation.
        ivars.search.setStringValue(&NSString::from_str(""));
        ivars.results.borrow_mut().clear();
        ivars.table.reloadData();
    }

    fn run_query(&self, query: &str) {
        let ivars = self.ivars();
        let results = ivars.engine.query(query);
        let n = results.len();
        *ivars.results.borrow_mut() = results;
        ivars.table.reloadData();
        self.layout(n);
        if n > 0 {
            self.select_row(0);
        }
    }

    fn move_selection(&self, delta: i64) {
        let ivars = self.ivars();
        let count = ivars.results.borrow().len() as i64;
        if count == 0 {
            return;
        }
        let current = ivars.table.selectedRow() as i64;
        let mut next = current + delta;
        if next < 0 {
            next = 0;
        }
        if next >= count {
            next = count - 1;
        }
        self.select_row(next as usize);
    }

    fn select_row(&self, row: usize) {
        let ivars = self.ivars();
        let set = NSIndexSet::indexSetWithIndex(row);
        ivars
            .table
            .selectRowIndexes_byExtendingSelection(&set, false);
        ivars.table.scrollRowToVisible(row as isize);
    }

    fn activate_selection(&self) {
        let ivars = self.ivars();
        let row = ivars.table.selectedRow();
        if row < 0 {
            return;
        }
        let action = {
            let results = ivars.results.borrow();
            match results.get(row as usize) {
                Some(item) => item.action.clone(),
                None => return,
            }
        };
        if action.execute() {
            self.hide_and_reset();
        }
    }

    /// Resize the panel to fit `rows` results and reposition the subviews.
    fn layout(&self, rows: usize) {
        let ivars = self.ivars();
        let visible_rows = rows.min(MAX_VISIBLE_ROWS);
        let results_h = visible_rows as f64 * ROW_H;
        let total_h = SEARCH_AREA_H + results_h;

        let mtm = self.mtm();
        let (x, top) = if let Some(screen) = NSScreen::mainScreen(mtm) {
            let vf = screen.visibleFrame();
            (
                vf.origin.x + (vf.size.width - PANEL_WIDTH) / 2.0,
                vf.origin.y + vf.size.height * TOP_FRACTION,
            )
        } else {
            (200.0, 600.0)
        };
        let origin_y = top - total_h;

        let frame = NSRect::new(
            NSPoint::new(x, origin_y),
            NSSize::new(PANEL_WIDTH, total_h),
        );
        ivars.panel.setFrame_display(frame, true);

        // Search field pinned to the top of the content view.
        let search_frame = NSRect::new(
            NSPoint::new(14.0, total_h - SEARCH_AREA_H + 14.0),
            NSSize::new(PANEL_WIDTH - 28.0, 36.0),
        );
        ivars.search.setFrame(search_frame);

        // Results scroll view fills the area below the search field.
        let scroll_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(PANEL_WIDTH, results_h),
        );
        ivars.scroll.setFrame(scroll_frame);
        ivars.scroll.setHidden(visible_rows == 0);
    }
}

fn make_row_cell(mtm: MainThreadMarker, item: &Item) -> Retained<NSTextField> {
    let rect = NSRect::new(NSPoint::new(8.0, 0.0), NSSize::new(PANEL_WIDTH - 16.0, ROW_H));
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), rect);
    let text = if item.subtitle.is_empty() {
        format!("{}   [{}]", item.title, item.source)
    } else {
        format!("{}      {}   [{}]", item.title, item.subtitle, item.source)
    };
    field.setStringValue(&NSString::from_str(&text));
    field.setBezeled(false);
    field.setDrawsBackground(false);
    field.setEditable(false);
    field.setSelectable(false);
    field.setFont(Some(&NSFont::systemFontOfSize(15.0)));
    field
}

fn build_panel(
    mtm: MainThreadMarker,
) -> (
    Retained<LcPanel>,
    Retained<NSSearchField>,
    Retained<NSTableView>,
    Retained<NSScrollView>,
) {
    let content_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(PANEL_WIDTH, SEARCH_AREA_H),
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
    effect.setAutoresizingMask(
        objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
            | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    let search_rect = NSRect::new(
        NSPoint::new(14.0, 14.0),
        NSSize::new(PANEL_WIDTH - 28.0, 36.0),
    );
    let search = NSSearchField::initWithFrame(NSSearchField::alloc(mtm), search_rect);
    search.setPlaceholderString(Some(&NSString::from_str("Search litecast...")));

    // Results table inside a scroll view.
    let table = NSTableView::initWithFrame(
        NSTableView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, 0.0)),
    );
    let column =
        NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str("main"));
    column.setWidth(PANEL_WIDTH - 16.0);
    table.addTableColumn(&column);
    table.setHeaderView(None);
    table.setRowHeight(ROW_H);

    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, 0.0)),
    );
    scroll.setDocumentView(Some(&table));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);

    effect.addSubview(&search);
    effect.addSubview(&scroll);
    panel.setContentView(Some(&effect));

    (panel, search, table, scroll)
}

fn build_engine() -> Engine {
    let mut engine = Engine::new();
    engine.add(Box::new(AppsProvider::new()));
    engine
}

fn main() {
    let mtm = MainThreadMarker::new().expect("main() must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let (panel, search, table, scroll) = build_panel(mtm);

    let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
    let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
    manager.register(hotkey).expect("failed to register hotkey");

    let ivars = Ivars {
        panel,
        search,
        table,
        scroll,
        visible: Cell::new(false),
        engine: build_engine(),
        results: RefCell::new(Vec::new()),
        _hotkey_manager: manager,
    };

    let delegate: Retained<AppDelegate> = {
        let this = AppDelegate::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    };

    let ivars = delegate.ivars();
    ivars
        .panel
        .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    // Set search field + table delegates via msg_send to avoid protocol-narrowing.
    let obj: &AnyObject = &delegate;
    unsafe {
        let _: () = msg_send![&*ivars.search, setDelegate: obj];
        let _: () = msg_send![&*ivars.table, setDataSource: obj];
        let _: () = msg_send![&*ivars.table, setDelegate: obj];
    }

    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    // Hotkey listener: blocks when idle (zero CPU) and bounces onto the main thread.
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
