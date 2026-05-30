mod ai;
mod clipboard;
mod config;
mod critters;
mod currency;
mod engine;
mod frecency;
mod model;
mod paths;
mod providers;
mod screenshot;
mod secrets;

use std::cell::{Cell, RefCell};
use std::sync::{mpsc, Arc, Mutex};

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSAnimationContext, NSBox, NSBoxType, NSColor, NSControl, NSEvent, NSEventModifierFlags,
    NSFocusRingType, NSFont, NSImage, NSImageView, NSPanel, NSPasteboard, NSPasteboardTypeString,
    NSScreen, NSScrollView, NSTableColumn, NSTableView, NSTextField, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowCollectionBehavior, NSWindowDelegate, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSIndexSet, NSNotification, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer,
};

use clipboard::History;
use config::{AiConfig, Config};
use currency::CurrencyCache;
use engine::{Engine, Filter};
use frecency::Frecency;
use model::{Action, Item};
use providers::{
    AiProvider, AppsProvider, CalcProvider, ClipboardProvider, CommandsProvider, ConvertProvider,
    EasterEggProvider, EmojiProvider, FilesProvider, PluginProvider, QuicklinksProvider,
    SnippetsProvider, SystemProvider, WebSearchProvider,
};

type PendingResults = Arc<Mutex<Option<(u64, Vec<Item>)>>>;
type AiPending = Arc<Mutex<Option<(u64, Result<String, String>)>>>;

const PANEL_WIDTH: f64 = 720.0;
const SEARCH_AREA_H: f64 = 66.0;
const PLACEHOLDER_NORMAL: &str = "Search litecast...";
const PLACEHOLDER_SHOT: &str = "Ask about the screenshot, then press Enter...";
const PLAYFUL_PLACEHOLDERS: &[&str] = &[
    "What are we launching today?",
    "Type to search, dream to launch...",
    "Ask me anything (try a ? prefix)",
    "Apps, files, math, the web - go on.",
    "I was just resting, honest.",
    "Your wish is my command... command.",
    "Tab to filter, Enter to fly.",
    "Faster than you can say Spotlight.",
    "Go ahead, type something brilliant.",
    "Less clicking, more launching.",
    "100 usd to eur? :rocket? I got you.",
    "The blank box of infinite potential.",
    "Searching is believing.",
    "Psst - try @apps or @calc.",
    "Tiny binary, big dreams.",
    "Name it and I'll find it.",
    "Keyboard warrior mode: engaged.",
    "What would Raycast do? This, but lighter.",
];
const ROW_H: f64 = 48.0;
const MAX_VISIBLE_ROWS: usize = 8;
const ROW_ICON: f64 = 26.0;
const CORNER_RADIUS: f64 = 18.0;
// Fraction of the screen height where the panel's top edge sits.
const TOP_FRACTION: f64 = 0.80;
// Built-in critters used when the user has supplied no GIFs.
const DEFAULT_CRITTERS: &[&str] = &["🐢", "🐈", "🐌", "🦆", "🐧", "🐞"];

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

        // A borderless accessory panel has no Edit menu, so the standard editing
        // key equivalents never reach the field editor. Route them manually
        // through the responder chain (to: nil targets the first responder).
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            let flags = event.modifierFlags();
            let only_cmd = flags
                .intersection(NSEventModifierFlags::DeviceIndependentFlagsMask)
                == NSEventModifierFlags::Command;
            if only_cmd {
                let chars = event
                    .charactersIgnoringModifiers()
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                let selector = match chars.as_str() {
                    "a" => Some(sel!(selectAll:)),
                    "c" => Some(sel!(copy:)),
                    "v" => Some(sel!(paste:)),
                    "x" => Some(sel!(cut:)),
                    "z" => Some(sel!(undo:)),
                    _ => None,
                };
                if let Some(selector) = selector {
                    let app = NSApplication::sharedApplication(self.mtm());
                    let to: *const AnyObject = std::ptr::null();
                    let handled: bool =
                        unsafe { msg_send![&app, sendAction: selector, to: to, from: self] };
                    if handled {
                        return true.into();
                    }
                }
            }
            let passed: bool = unsafe { msg_send![super(self), performKeyEquivalent: event] };
            passed.into()
        }
    }
);

struct Ivars {
    panel: Retained<LcPanel>,
    search: Retained<NSTextField>,
    table: Retained<NSTableView>,
    scroll: Retained<NSScrollView>,
    separator: Retained<NSBox>,
    visible: Cell<bool>,
    results: RefCell<Vec<Item>>,
    /// Monotonic query id; results tagged with a stale id are discarded.
    generation: Cell<u64>,
    /// Sends (generation, query, filter) to the background worker.
    query_tx: mpsc::Sender<(u64, String, Filter)>,
    /// Active category filter; driven by both `@prefix` typing and Tab cycling.
    active_filter: Cell<Filter>,
    /// Small pill near the search field showing the active filter (hidden on All).
    chip: Retained<NSTextField>,
    /// Latest computed results awaiting application on the main thread.
    pending: PendingResults,
    /// Clipboard history shared with the watcher timer.
    clip_history: History,
    /// Last observed pasteboard change count.
    last_change: Cell<isize>,
    /// AI backend configuration (provider/model/endpoint).
    ai_config: AiConfig,
    /// Latest AI answer awaiting application on the main thread.
    ai_pending: AiPending,
    /// Monotonic AI request id; stale answers are discarded.
    ai_generation: Cell<u64>,
    /// Active screenshot path while in "ask about screenshot" mode.
    screenshot_path: RefCell<Option<String>>,
    /// Captured screenshot path awaiting the main thread.
    shot_pending: Arc<Mutex<Option<String>>>,
    /// Whether to rotate playful placeholder text.
    playful_placeholders: bool,
    /// Rotating index into the playful placeholder list.
    placeholder_idx: Cell<usize>,
    /// Image view used for the wandering critter when GIFs are installed.
    critter_view: Retained<NSImageView>,
    /// Text-glyph critter used out of the box when no GIFs are installed.
    critter_label: Retained<NSTextField>,
    /// Loaded critter GIF images (empty = use the built-in glyph critter).
    critter_images: Vec<Retained<NSImage>>,
    /// Rotating index into the critter image list.
    critter_idx: Cell<usize>,
    /// Usage learner; records activations and boosts frequent/recent items.
    frecency: Frecency,
    /// Row index currently armed for a two-step destructive confirmation.
    pending_confirm: Cell<isize>,
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

        // One-shot safety net: guarantee the panel is fully opaque after the
        // fade-in window, even if the fade animation was a no-op.
        #[unsafe(method(ensurePanelVisible))]
        fn ensure_panel_visible(&self) {
            let ivars = self.ivars();
            if ivars.visible.get() {
                ivars.panel.setAlphaValue(1.0);
            }
        }

        // NSSearchField text changed: dispatch the query to the worker thread.
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, _notification: &NSNotification) {
            self.dispatch_query();
        }

        // Invoked on the main thread by the worker when results are ready.
        #[unsafe(method(applyResults))]
        fn apply_results(&self) {
            self.apply_pending_results();
        }

        // Invoked on the main thread when an AI answer is ready.
        #[unsafe(method(applyAiAnswer))]
        fn apply_ai_answer(&self) {
            self.apply_pending_ai_answer();
        }

        // Triggered by the screenshot hotkey. Captures off the main thread so
        // the interactive selection UI is not blocked.
        #[unsafe(method(captureScreenshot))]
        fn capture_screenshot(&self) {
            let pending = self.ivars().shot_pending.clone();
            let delegate_addr = self as *const AppDelegate as usize;
            std::thread::spawn(move || {
                if let Some(path) = screenshot::capture_interactive() {
                    if let Ok(mut slot) = pending.lock() {
                        *slot = Some(path);
                    }
                    let ptr = delegate_addr as *const AnyObject;
                    unsafe {
                        let obj: &AnyObject = &*ptr;
                        let _: () = msg_send![
                            obj,
                            performSelectorOnMainThread: sel!(showScreenshot),
                            withObject: std::ptr::null::<AnyObject>(),
                            waitUntilDone: false,
                        ];
                    }
                }
            });
        }

        // Enter "ask about screenshot" mode on the main thread.
        #[unsafe(method(showScreenshot))]
        fn show_screenshot(&self) {
            let path = self.ivars().shot_pending.lock().ok().and_then(|mut s| s.take());
            let Some(path) = path else { return };
            *self.ivars().screenshot_path.borrow_mut() = Some(path);
            self.ivars()
                .search
                .setPlaceholderString(Some(&NSString::from_str(PLACEHOLDER_SHOT)));
            self.ivars().search.setStringValue(&NSString::from_str(""));
            self.show();
            self.dispatch_query();
        }

        // Occasionally send a critter strolling across the bottom edge while the
        // panel is open. No-op when hidden, disabled, or no GIFs are installed.
        #[unsafe(method(walkCritter:))]
        fn walk_critter(&self, _timer: &AnyObject) {
            self.start_critter_walk();
        }

        // One-shot: park the critter off-screen and stop its animation.
        #[unsafe(method(hideCritter:))]
        fn hide_critter(&self, _timer: &AnyObject) {
            let ivars = self.ivars();
            ivars.critter_view.setHidden(true);
            ivars.critter_view.setAnimates(false);
            ivars.critter_label.setHidden(true);
        }

        // Repeating timer: record clipboard changes into history. Only does work
        // when the integer change count differs, so it is effectively free.
        #[unsafe(method(pollClipboard:))]
        fn poll_clipboard(&self, _timer: &AnyObject) {
            let pasteboard = NSPasteboard::generalPasteboard();
            let count = pasteboard.changeCount();
            let ivars = self.ivars();
            if count == ivars.last_change.get() {
                return;
            }
            ivars.last_change.set(count);
            if let Some(text) = unsafe { pasteboard.stringForType(NSPasteboardTypeString) } {
                ivars.clip_history.record(text.to_string());
            }
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
            } else if selector == sel!(insertTab:) {
                // Tab cycles the category filter instead of moving focus.
                self.cycle_filter(true);
                true
            } else if selector == sel!(insertBacktab:) {
                self.cycle_filter(false);
                true
            } else if selector == sel!(cancelOperation:) {
                // Esc clears an active filter first; only then closes the panel.
                if self.ivars().active_filter.get() != Filter::All {
                    self.set_filter(Filter::All);
                } else {
                    self.hide_and_reset();
                }
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
        ) -> Option<Retained<NSView>> {
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
        let visible = self.ivars().visible.get();
        eprintln!("[litecast] toggle (currently visible={visible})");
        if visible {
            self.hide_and_reset();
        } else {
            self.show();
        }
    }

    /// Advance the active filter forward (Tab) or backward (Shift+Tab).
    fn cycle_filter(&self, forward: bool) {
        let current = self.ivars().active_filter.get();
        let next = if forward { current.next() } else { current.prev() };
        self.set_filter(next);
    }

    /// Set the active filter, refresh the chip + layout, and re-run the query.
    fn set_filter(&self, filter: Filter) {
        let ivars = self.ivars();
        ivars.active_filter.set(filter);
        self.layout(ivars.results.borrow().len());
        self.dispatch_query();
    }

    fn show(&self) {
        eprintln!("[litecast] show");
        let ivars = self.ivars();
        self.layout(ivars.results.borrow().len());

        // Rotate a playful placeholder (unless in screenshot mode).
        if ivars.playful_placeholders && ivars.screenshot_path.borrow().is_none() {
            let idx = ivars.placeholder_idx.get();
            ivars.placeholder_idx.set(idx.wrapping_add(1));
            let text = PLAYFUL_PLACEHOLDERS[idx % PLAYFUL_PLACEHOLDERS.len()];
            ivars
                .search
                .setPlaceholderString(Some(&NSString::from_str(text)));
        }

        let app = NSApplication::sharedApplication(self.mtm());
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);

        ivars.panel.makeKeyAndOrderFront(None);
        ivars.panel.makeFirstResponder(Some(&ivars.search));
        ivars.visible.set(true);

        // Subtle fade-in: start transparent and animate up to fully opaque. The
        // animator's final value is 1.0, so in normal operation the panel ends
        // visible. A one-shot timer below re-asserts full opacity after the
        // animation window so the panel can never be left stranded transparent.
        ivars.panel.setAlphaValue(0.0);
        unsafe {
            NSAnimationContext::beginGrouping();
            let ctx = NSAnimationContext::currentContext();
            ctx.setDuration(0.12);
            let animator: Retained<AnyObject> = msg_send![&*ivars.panel, animator];
            let _: () = msg_send![&animator, setAlphaValue: 1.0_f64];
            NSAnimationContext::endGrouping();
        }
        let target: &AnyObject = self;
        let _alpha_guard = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.2,
                target,
                sel!(ensurePanelVisible),
                None,
                false,
            )
        };

        // Send a critter strolling shortly after the panel appears, so the
        // little creature is visible without waiting for the periodic timer.
        let _critter_kick = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.9,
                target,
                sel!(walkCritter:),
                None,
                false,
            )
        };
    }

    fn hide_and_reset(&self) {
        let ivars = self.ivars();
        if !ivars.visible.get() {
            return;
        }
        ivars.panel.orderOut(None);
        ivars.visible.set(false);
        ivars.pending_confirm.set(-1);
        ivars.active_filter.set(Filter::All);
        // Bump the generation so any in-flight worker results are discarded.
        ivars.generation.set(ivars.generation.get().wrapping_add(1));
        // Clear state for the next invocation.
        ivars.search.setStringValue(&NSString::from_str(""));
        ivars.screenshot_path.borrow_mut().take();
        ivars
            .search
            .setPlaceholderString(Some(&NSString::from_str(PLACEHOLDER_NORMAL)));
        ivars.results.borrow_mut().clear();
        ivars.table.reloadData();
    }

    fn dispatch_query(&self) {
        let ivars = self.ivars();
        ivars.pending_confirm.set(-1);
        let raw = ivars.search.stringValue().to_string();
        // A typed `@token ` prefix sets the same sticky filter the chip uses,
        // and the remainder becomes the actual query.
        let query = match parse_filter_prefix(&raw) {
            Some((filter, rest)) => {
                if ivars.active_filter.get() != filter {
                    ivars.active_filter.set(filter);
                    self.layout(ivars.results.borrow().len());
                }
                rest
            }
            None => raw,
        };
        let filter = ivars.active_filter.get();
        let generation = ivars.generation.get().wrapping_add(1);
        ivars.generation.set(generation);

        // In screenshot mode the whole query is a question about the image; we
        // bypass the providers and offer a single "ask" action.
        if let Some(path) = ivars.screenshot_path.borrow().clone() {
            let prompt = query.trim();
            let item = if prompt.is_empty() {
                Item::new(
                    "Ask about the screenshot...",
                    "Type a question, then press Enter",
                    "AI",
                    0,
                    Action::None,
                )
            } else {
                Item::new(
                    format!("Ask {}: {prompt}", ivars.ai_config.provider),
                    "Press Enter to send the screenshot + question",
                    "AI",
                    0,
                    Action::AskAi {
                        prompt: prompt.to_string(),
                        image: Some(path),
                    },
                )
            };
            *ivars.results.borrow_mut() = vec![item];
            ivars.table.reloadData();
            self.layout(1);
            self.select_row(0);
            return;
        }

        if query.trim().is_empty() {
            ivars.results.borrow_mut().clear();
            ivars.table.reloadData();
            self.layout(0);
            return;
        }
        let _ = ivars.query_tx.send((generation, query, filter));
    }

    fn apply_pending_results(&self) {
        let ivars = self.ivars();
        let taken = ivars.pending.lock().ok().and_then(|mut slot| slot.take());
        if let Some((generation, items)) = taken {
            if generation != ivars.generation.get() {
                return; // Stale results for an older query.
            }
            let n = items.len();
            *ivars.results.borrow_mut() = items;
            ivars.table.reloadData();
            self.layout(n);
            if n > 0 {
                self.select_row(0);
            }
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
        let (action, id) = {
            let results = ivars.results.borrow();
            match results.get(row as usize) {
                Some(item) => (item.action.clone(), item.id.clone()),
                None => return,
            }
        };
        // AI requests run asynchronously and keep the panel open.
        if let Action::AskAi { prompt, image } = action {
            self.start_ai(prompt, image);
            return;
        }

        // Two-step confirmation for destructive actions: the first Enter arms
        // the row, the second (on the same row) runs the wrapped action.
        if let Action::Confirm { label, inner } = action {
            if ivars.pending_confirm.get() == row {
                ivars.pending_confirm.set(-1);
                if let Some(id) = &id {
                    ivars.frecency.record(id);
                }
                if inner.execute() {
                    self.hide_and_reset();
                }
            } else {
                ivars.pending_confirm.set(row);
                if let Some(item) = ivars.results.borrow_mut().get_mut(row as usize) {
                    item.subtitle = format!("Press Enter again to {label}");
                }
                ivars.table.reloadData();
                self.select_row(row as usize);
            }
            return;
        }
        ivars.pending_confirm.set(-1);

        if let Some(id) = &id {
            ivars.frecency.record(id);
        }
        if action.execute() {
            self.hide_and_reset();
        }
    }

    /// Kick off an AI request on a background thread and show a loading state.
    fn start_ai(&self, prompt: String, image: Option<String>) {
        let ivars = self.ivars();
        // Leaving screenshot mode now that the question has been sent.
        ivars.screenshot_path.borrow_mut().take();
        ivars
            .search
            .setPlaceholderString(Some(&NSString::from_str(PLACEHOLDER_NORMAL)));
        let generation = ivars.ai_generation.get().wrapping_add(1);
        ivars.ai_generation.set(generation);

        // Show a loading row immediately.
        *ivars.results.borrow_mut() = vec![Item::new(
            "Thinking...",
            "Contacting the AI backend",
            "AI",
            0,
            Action::None,
        )];
        ivars.table.reloadData();
        self.layout(1);

        let config = ivars.ai_config.clone();
        let pending = ivars.ai_pending.clone();
        let delegate_addr = self as *const AppDelegate as usize;
        std::thread::spawn(move || {
            let result = ai::ask(&config, &prompt, image.as_deref());
            if let Ok(mut slot) = pending.lock() {
                *slot = Some((generation, result));
            }
            let ptr = delegate_addr as *const AnyObject;
            unsafe {
                let obj: &AnyObject = &*ptr;
                let _: () = msg_send![
                    obj,
                    performSelectorOnMainThread: sel!(applyAiAnswer),
                    withObject: std::ptr::null::<AnyObject>(),
                    waitUntilDone: false,
                ];
            }
        });
    }

    fn apply_pending_ai_answer(&self) {
        let ivars = self.ivars();
        let taken = ivars.ai_pending.lock().ok().and_then(|mut slot| slot.take());
        let Some((generation, result)) = taken else {
            return;
        };
        if generation != ivars.ai_generation.get() {
            return; // Stale answer.
        }
        let items = match result {
            Ok(answer) => answer_to_items(&answer),
            Err(err) => vec![Item::new(
                "AI error",
                err,
                "AI",
                0,
                Action::None,
            )],
        };
        let n = items.len();
        *ivars.results.borrow_mut() = items;
        ivars.table.reloadData();
        self.layout(n);
    }

    /// Send a critter walking from the left edge to the right edge. Uses an
    /// installed GIF when present, otherwise a built-in text-glyph creature so
    /// something always strolls across the panel out of the box.
    fn start_critter_walk(&self) {
        let ivars = self.ivars();
        if !ivars.visible.get() {
            return;
        }
        let idx = ivars.critter_idx.get();
        ivars.critter_idx.set(idx.wrapping_add(1));

        const SIZE: f64 = 30.0;
        let start = NSRect::new(NSPoint::new(-SIZE, 2.0), NSSize::new(SIZE, SIZE));
        let view: *const AnyObject = if ivars.critter_images.is_empty() {
            let glyph = DEFAULT_CRITTERS[idx % DEFAULT_CRITTERS.len()];
            let label = &ivars.critter_label;
            label.setStringValue(&NSString::from_str(glyph));
            label.setFrame(start);
            label.setHidden(false);
            &*ivars.critter_label as *const NSTextField as *const AnyObject
        } else {
            let image = &ivars.critter_images[idx % ivars.critter_images.len()];
            let view = &ivars.critter_view;
            view.setImage(Some(image));
            view.setAnimates(true);
            view.setHidden(false);
            view.setFrame(start);
            &*ivars.critter_view as *const NSImageView as *const AnyObject
        };

        let duration = 6.0;
        unsafe {
            let obj: &AnyObject = &*view;
            NSAnimationContext::beginGrouping();
            let ctx = NSAnimationContext::currentContext();
            ctx.setDuration(duration);
            let animator: Retained<AnyObject> = msg_send![obj, animator];
            let _: () = msg_send![&animator, setFrameOrigin: NSPoint::new(PANEL_WIDTH + SIZE, 2.0)];
            NSAnimationContext::endGrouping();
        }

        // Park it off-screen and stop animating shortly after it exits.
        let delegate_obj: &AnyObject = self;
        let _hide_timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                duration + 0.2,
                delegate_obj,
                sel!(hideCritter:),
                None,
                false,
            )
        };
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

        // Filter chip on the right of the search area (hidden when unfiltered).
        // It is laid out first so the search field can reserve room for it.
        let filter = ivars.active_filter.get();
        let mut search_right = PANEL_WIDTH - 22.0;
        if filter == Filter::All {
            ivars.chip.setHidden(true);
        } else {
            let chip = &ivars.chip;
            chip.setStringValue(&NSString::from_str(filter.label()));
            chip.sizeToFit();
            let natural_w = chip.frame().size.width;
            let chip_h = (line_height(12.0, true) + 6.0).round();
            let chip_w = (natural_w + 18.0).round();
            let chip_x = PANEL_WIDTH - 22.0 - chip_w;
            let chip_y = (results_h + (SEARCH_AREA_H - chip_h) / 2.0).round();
            chip.setFrame(NSRect::new(
                NSPoint::new(chip_x, chip_y),
                NSSize::new(chip_w, chip_h),
            ));
            chip.setHidden(false);
            search_right = chip_x - 12.0;
        }

        // Search field sized to its exact text height and centered in the top
        // search area, so the text sits on the vertical midline (a tall field
        // would top-align its text instead).
        let search_h = line_height(24.0, false);
        let search_y = (results_h + (SEARCH_AREA_H - search_h) / 2.0).round();
        let search_frame = NSRect::new(
            NSPoint::new(22.0, search_y),
            NSSize::new((search_right - 22.0).max(40.0), search_h),
        );
        ivars.search.setFrame(search_frame);

        // Hairline separator on the boundary between the search area and results.
        let separator_frame = NSRect::new(
            NSPoint::new(18.0, results_h),
            NSSize::new(PANEL_WIDTH - 36.0, 1.0),
        );
        ivars.separator.setFrame(separator_frame);
        ivars.separator.setHidden(visible_rows == 0);

        // Results scroll view fills the area below the search field.
        let scroll_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(PANEL_WIDTH, results_h),
        );
        ivars.scroll.setFrame(scroll_frame);
        ivars.scroll.setHidden(visible_rows == 0);
    }
}

/// Turn an AI answer into wrapped result rows. Each row copies the full answer
/// on Enter, so a multi-line reply is readable and copyable.
fn answer_to_items(answer: &str) -> Vec<Item> {
    const WRAP: usize = 88;
    let full = answer.trim().to_string();
    let mut lines: Vec<String> = Vec::new();
    for raw_line in full.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > WRAP {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push("(empty response)".to_string());
    }

    lines
        .into_iter()
        .take(MAX_VISIBLE_ROWS)
        .enumerate()
        .map(|(i, line)| {
            let subtitle = if i == 0 { "Enter to copy full answer" } else { "" };
            Item::new(line, subtitle, "AI", 0, Action::CopyText(full.clone()))
        })
        .collect()
}

/// Parse a leading `@token` filter prefix. Returns the matched filter and the
/// remaining query (the text after the token), or `None` if there is no valid
/// `@token`. Examples: `@apps safari` -> (Apps, "safari"), `@clip` -> (Clip, "").
fn parse_filter_prefix(raw: &str) -> Option<(Filter, String)> {
    let trimmed = raw.trim_start();
    let rest = trimmed.strip_prefix('@')?;
    let (token, after) = match rest.split_once(char::is_whitespace) {
        Some((t, a)) => (t, a.trim_start()),
        None => (rest, ""),
    };
    let filter = Filter::from_token(&token.to_ascii_lowercase())?;
    Some((filter, after.to_string()))
}

/// Exact line height for the system font at `size`. A single-line NSTextField
/// draws its text at the top of the cell, so we size label frames to this height
/// and center those frames to achieve true vertical centering.
fn line_height(size: f64, bold: bool) -> f64 {
    let font = if bold {
        NSFont::boldSystemFontOfSize(size)
    } else {
        NSFont::systemFontOfSize(size)
    };
    (font.ascender() - font.descender() + font.leading()).ceil()
}

fn make_label(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
    bold: bool,
    color: &NSColor,
) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 10.0)),
    );
    field.setStringValue(&NSString::from_str(text));
    field.setBezeled(false);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setEditable(false);
    field.setSelectable(false);
    let font = if bold {
        NSFont::boldSystemFontOfSize(size)
    } else {
        NSFont::systemFontOfSize(size)
    };
    field.setFont(Some(&font));
    field.setTextColor(Some(color));
    field
}

fn row_icon(item: &Item) -> Option<Retained<NSImage>> {
    if let Some(path) = &item.icon_path {
        let workspace = NSWorkspace::sharedWorkspace();
        return Some(workspace.iconForFile(&NSString::from_str(path)));
    }
    let symbol = match item.source {
        "Calc" => "function",
        "AI" => "sparkles",
        "Web" => "globe",
        "Clip" => "doc.on.clipboard",
        "Command" => "terminal",
        "Plugin" => "puzzlepiece.extension",
        "?" => "wand.and.stars",
        _ => "magnifyingglass",
    };
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None)
}

fn make_row_cell(mtm: MainThreadMarker, item: &Item) -> Retained<NSView> {
    let width = PANEL_WIDTH - 16.0;
    let container = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, ROW_H)),
    );

    let icon_view = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        NSRect::new(
            NSPoint::new(12.0, (ROW_H - ROW_ICON) / 2.0),
            NSSize::new(ROW_ICON, ROW_ICON),
        ),
    );
    if let Some(image) = row_icon(item) {
        icon_view.setImage(Some(&image));
    }
    icon_view.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
    container.addSubview(&icon_view);

    let text_x = 12.0 + ROW_ICON + 12.0;
    let text_w = width - text_x - 14.0;

    let title_h = line_height(15.0, true);
    if item.subtitle.is_empty() {
        // Single line: center the title band within the row.
        let y = ((ROW_H - title_h) / 2.0).round();
        let title = make_label(mtm, &item.title, 15.0, true, &NSColor::labelColor());
        title.setFrame(NSRect::new(NSPoint::new(text_x, y), NSSize::new(text_w, title_h)));
        container.addSubview(&title);
    } else {
        // Two lines: center the (title + gap + subtitle) block within the row.
        const GAP: f64 = 2.0;
        let sub_h = line_height(12.0, false);
        let block = title_h + GAP + sub_h;
        let bottom = ((ROW_H - block) / 2.0).round();
        let subtitle =
            make_label(mtm, &item.subtitle, 12.0, false, &NSColor::secondaryLabelColor());
        subtitle.setFrame(NSRect::new(
            NSPoint::new(text_x, bottom),
            NSSize::new(text_w, sub_h),
        ));
        container.addSubview(&subtitle);
        let title = make_label(mtm, &item.title, 15.0, true, &NSColor::labelColor());
        title.setFrame(NSRect::new(
            NSPoint::new(text_x, bottom + sub_h + GAP),
            NSSize::new(text_w, title_h),
        ));
        container.addSubview(&title);
    }

    container
}

struct PanelViews {
    panel: Retained<LcPanel>,
    search: Retained<NSTextField>,
    table: Retained<NSTableView>,
    scroll: Retained<NSScrollView>,
    separator: Retained<NSBox>,
    chip: Retained<NSTextField>,
    critter: Retained<NSImageView>,
    critter_label: Retained<NSTextField>,
}

fn build_panel(mtm: MainThreadMarker) -> PanelViews {
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

    // Frosted translucent background with clipped rounded corners. Sidebar is a
    // clean, system-adaptive material that reads well as a Spotlight-style panel.
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_rect);
    effect.setMaterial(NSVisualEffectMaterial::Sidebar);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setWantsLayer(true);
    if let Some(layer) = effect.layer() {
        unsafe {
            let _: () = msg_send![&*layer, setCornerRadius: CORNER_RADIUS];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
        }
    }
    effect.setAutoresizingMask(
        objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
            | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    // Borderless, transparent, large text field for a native Spotlight feel.
    let search_rect = NSRect::new(
        NSPoint::new(22.0, 14.0),
        NSSize::new(PANEL_WIDTH - 44.0, 40.0),
    );
    let search = NSTextField::initWithFrame(NSTextField::alloc(mtm), search_rect);
    search.setBezeled(false);
    search.setBordered(false);
    search.setDrawsBackground(false);
    search.setEditable(true);
    search.setSelectable(true);
    search.setFocusRingType(NSFocusRingType::None);
    search.setFont(Some(&NSFont::systemFontOfSize(24.0)));
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

    // Native hairline separator between the search field and results.
    let separator = NSBox::initWithFrame(
        NSBox::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, 1.0)),
    );
    separator.setBoxType(NSBoxType::Separator);
    separator.setHidden(true);

    // Filter chip: a small rounded accent pill showing the active category.
    let chip = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(40.0, 20.0)),
    );
    chip.setBezeled(false);
    chip.setBordered(false);
    chip.setEditable(false);
    chip.setSelectable(false);
    chip.setDrawsBackground(true);
    chip.setAlignment(objc2_app_kit::NSTextAlignment::Center);
    chip.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    let accent = NSColor::controlAccentColor();
    chip.setTextColor(Some(&accent));
    chip.setBackgroundColor(Some(&accent.colorWithAlphaComponent(0.18)));
    chip.setWantsLayer(true);
    if let Some(layer) = chip.layer() {
        unsafe {
            let _: () = msg_send![&*layer, setCornerRadius: 9.0_f64];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
        }
    }
    chip.setHidden(true);

    // Critter image view, kept on top and hidden until it walks.
    let critter = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        NSRect::new(NSPoint::new(-30.0, 2.0), NSSize::new(30.0, 30.0)),
    );
    critter.setHidden(true);
    critter.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);

    // Built-in glyph critter (used when no GIFs are installed).
    let critter_label = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(-30.0, 2.0), NSSize::new(30.0, 30.0)),
    );
    critter_label.setBezeled(false);
    critter_label.setBordered(false);
    critter_label.setDrawsBackground(false);
    critter_label.setEditable(false);
    critter_label.setSelectable(false);
    critter_label.setHidden(true);
    critter_label.setFont(Some(&NSFont::systemFontOfSize(22.0)));

    effect.addSubview(&search);
    effect.addSubview(&scroll);
    effect.addSubview(&separator);
    effect.addSubview(&chip);
    effect.addSubview(&critter);
    effect.addSubview(&critter_label);
    panel.setContentView(Some(&effect));

    PanelViews {
        panel,
        search,
        table,
        scroll,
        separator,
        chip,
        critter,
        critter_label,
    }
}

fn build_engine(history: History, config: &Config, frecency: Frecency) -> Engine {
    let mut engine = Engine::new(frecency);
    // EasterEgg is general fun: only shown when no filter is active.
    engine.add(Box::new(EasterEggProvider), Filter::All);
    engine.add(Box::new(AiProvider::new(config.ai.clone())), Filter::Ai);
    engine.add(Box::new(CalcProvider), Filter::Calc);
    let currency = CurrencyCache::new(config.conversion.currency_ttl_hours);
    // Warm the rate cache in the background so the first currency query is fast.
    currency.refresh_async();
    engine.add(Box::new(ConvertProvider::new(currency)), Filter::Calc);
    engine.add(Box::new(EmojiProvider), Filter::Emoji);
    engine.add(Box::new(ClipboardProvider::new(history)), Filter::Clip);
    // The "Commands" category groups user commands, quicklinks, snippets,
    // plugins, and system actions.
    engine.add(
        Box::new(CommandsProvider::new(config.commands.clone())),
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
    engine.add(Box::new(SystemProvider::new()), Filter::Cmd);
    engine.add(Box::new(PluginProvider::new()), Filter::Cmd);
    engine.add(Box::new(AppsProvider::new()), Filter::Apps);
    engine.add(Box::new(FilesProvider::new()), Filter::Files);
    engine.add(
        Box::new(WebSearchProvider::new(config.web_search_url.clone())),
        Filter::Web,
    );
    engine
}

fn main() {
    eprintln!("[litecast] starting; press Option+Space to toggle the panel");
    let mtm = MainThreadMarker::new().expect("main() must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let views = build_panel(mtm);
    let PanelViews {
        panel,
        search,
        table,
        scroll,
        separator,
        chip,
        critter,
        critter_label,
    } = views;

    // Load critter GIFs (if any) only when the feature is enabled.
    let config = config::load();
    let critter_images: Vec<Retained<NSImage>> = if config.ui.critters {
        critters::discover()
            .into_iter()
            .filter_map(|path| {
                NSImage::initWithContentsOfFile(
                    NSImage::alloc(),
                    &NSString::from_str(&path.to_string_lossy()),
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
    let toggle_hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
    let shot_hotkey = HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space);
    match manager.register(toggle_hotkey) {
        Ok(()) => eprintln!(
            "[litecast] registered toggle hotkey Option+Space (id={})",
            toggle_hotkey.id()
        ),
        Err(e) => eprintln!("[litecast] FAILED to register toggle hotkey Option+Space: {e}"),
    }
    match manager.register(shot_hotkey) {
        Ok(()) => eprintln!(
            "[litecast] registered screenshot hotkey Option+Shift+Space (id={})",
            shot_hotkey.id()
        ),
        Err(e) => {
            eprintln!("[litecast] FAILED to register screenshot hotkey Option+Shift+Space: {e}")
        }
    }
    let toggle_id = toggle_hotkey.id();
    let shot_id = shot_hotkey.id();

    let (query_tx, query_rx) = mpsc::channel::<(u64, String, Filter)>();
    let pending: PendingResults = Arc::new(Mutex::new(None));
    let history = History::new(50);
    let frecency = Frecency::load();

    let ivars = Ivars {
        panel,
        search,
        table,
        scroll,
        separator,
        visible: Cell::new(false),
        results: RefCell::new(Vec::new()),
        generation: Cell::new(0),
        query_tx,
        active_filter: Cell::new(Filter::All),
        chip,
        pending: pending.clone(),
        clip_history: history.clone(),
        last_change: Cell::new(-1),
        ai_config: config.ai.clone(),
        ai_pending: Arc::new(Mutex::new(None)),
        ai_generation: Cell::new(0),
        screenshot_path: RefCell::new(None),
        shot_pending: Arc::new(Mutex::new(None)),
        playful_placeholders: config.ui.playful_placeholders,
        placeholder_idx: Cell::new(0),
        critter_view: critter,
        critter_label,
        critter_images,
        critter_idx: Cell::new(0),
        frecency: frecency.clone(),
        pending_confirm: Cell::new(-1),
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

    let delegate_addr = Retained::as_ptr(&delegate) as usize;

    // Background query worker: runs providers off the main thread (so slow
    // sources like mdfind never block typing) and signals the main thread when
    // results are ready. Blocks on recv when idle, so it uses zero CPU.
    {
        let engine = Arc::new(build_engine(history.clone(), &config, frecency.clone()));
        let pending = pending.clone();
        std::thread::spawn(move || {
            while let Ok((mut generation, mut query, mut filter)) = query_rx.recv() {
                // Coalesce: skip to the most recent queued query.
                while let Ok((g, q, f)) = query_rx.try_recv() {
                    generation = g;
                    query = q;
                    filter = f;
                }
                let items = engine.query(&query, filter);
                if let Ok(mut slot) = pending.lock() {
                    *slot = Some((generation, items));
                }
                let ptr = delegate_addr as *const AnyObject;
                unsafe {
                    let obj: &AnyObject = &*ptr;
                    let _: () = msg_send![
                        obj,
                        performSelectorOnMainThread: sel!(applyResults),
                        withObject: std::ptr::null::<AnyObject>(),
                        waitUntilDone: false,
                    ];
                }
            }
        });
    }

    // Hotkey listener: blocks when idle (zero CPU) and bounces onto the main thread.
    std::thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.recv() {
            eprintln!(
                "[litecast] hotkey event id={} state={:?}",
                event.id, event.state
            );
            if event.state != HotKeyState::Pressed {
                continue;
            }
            let selector = if event.id == shot_id {
                sel!(captureScreenshot)
            } else if event.id == toggle_id {
                sel!(toggleFromHotkey)
            } else {
                continue;
            };
            let ptr = delegate_addr as *const AnyObject;
            unsafe {
                let obj: &AnyObject = &*ptr;
                let _: () = msg_send![
                    obj,
                    performSelectorOnMainThread: selector,
                    withObject: std::ptr::null::<AnyObject>(),
                    waitUntilDone: false,
                ];
            }
        }
    });

    // Clipboard watcher: a 1s repeating timer that only acts when the pasteboard
    // change count moves. Retained by the run loop.
    let target: &AnyObject = &delegate;
    let _clip_timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            1.0,
            target,
            sel!(pollClipboard:),
            None,
            true,
        )
    };

    // Wandering-critter timer: fires occasionally; a no-op unless the panel is
    // open and critter GIFs are installed.
    let _critter_timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            14.0,
            target,
            sel!(walkCritter:),
            None,
            true,
        )
    };

    app.run();
}
