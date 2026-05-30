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
mod window;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
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
    NSAnimationContext, NSBezierPath, NSBitmapImageFileType, NSBitmapImageRep, NSBox, NSBoxType,
    NSColor, NSControl, NSEvent, NSEventModifierFlags, NSFocusRingType, NSFont, NSFontWeight,
    NSFontWeightMedium, NSFontWeightRegular, NSImage, NSImageView, NSPanel, NSPasteboard,
    NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardTypeTIFF, NSScreen, NSScrollView, NSTableColumn, NSTableRowView, NSTableView,
    NSTextField, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSBaselineOffsetAttributeName, NSFontAttributeName, NSForegroundColorAttributeName,
    NSVisualEffectView, NSWindowCollectionBehavior, NSWindowDelegate, NSWindowStyleMask,
    NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSAttributedString, NSData, NSDictionary, NSIndexSet, NSNotification,
    NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer,
};

use ai::ChatMsg;
use clipboard::History;
use config::{AiConfig, CommandConfig, Config, HotkeyConfig};
use currency::CurrencyCache;
use engine::{Engine, Filter};
use frecency::Frecency;
use model::{Action, Item};
use providers::{
    AiCommandsProvider, AiProvider, AppsProvider, BookmarksProvider, CalcProvider,
    ClipboardProvider, CommandsProvider, ConvertProvider, EasterEggProvider, EmojiProvider,
    FilesProvider, PluginProvider, ProcessProvider, QuicklinksProvider, SnippetsProvider,
    SystemProvider, WebSearchProvider, WindowProvider,
};

type PendingResults = Arc<Mutex<Option<(u64, Vec<Item>)>>>;
type AiPending = Arc<Mutex<Option<(u64, Result<String, String>)>>>;

const PANEL_WIDTH: f64 = 720.0;
const SEARCH_AREA_H: f64 = 66.0;
const PLACEHOLDER_NORMAL: &str = "Search litecast...";
const PLACEHOLDER_SHOT: &str = "Ask about the screenshot, then press Enter...";
const PLACEHOLDER_FOLLOWUP: &str = "Follow up, or press Esc to exit chat...";
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
// Session-only recents ring buffer capacity (in-memory, reset on quit).
const RECENTS_CAP: usize = 12;
const ROW_ICON: f64 = 26.0;
const CORNER_RADIUS: f64 = 20.0;
// Shared left/right margin for the search field, separator, and result rows so
// everything aligns on a common edge.
const SIDE_INSET: f64 = 22.0;
// Vertical breathing room around the results list (bottom-left origin), so rows
// are never flush against the window edge / rounded corners.
const RESULTS_TOP_GAP: f64 = 6.0;
const RESULTS_BOTTOM_PAD: f64 = 10.0;
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

// Horizontal/vertical inset of the rounded selection highlight within a row.
const SELECTION_INSET_X: f64 = 8.0;
const SELECTION_INSET_Y: f64 = 4.0;

// Custom row view that draws a rounded, inset selection highlight (Raycast-style)
// instead of the default full-width table highlight.
define_class!(
    #[unsafe(super(NSTableRowView))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcRowView"]
    struct LcRowView;

    impl LcRowView {
        #[unsafe(method(drawSelectionInRect:))]
        fn draw_selection(&self, _dirty: NSRect) {
            if !self.isSelected() {
                return;
            }
            let b = self.bounds();
            let rect = NSRect::new(
                NSPoint::new(SELECTION_INSET_X, SELECTION_INSET_Y),
                NSSize::new(
                    (b.size.width - 2.0 * SELECTION_INSET_X).max(0.0),
                    (b.size.height - 2.0 * SELECTION_INSET_Y).max(0.0),
                ),
            );
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, 9.0, 9.0);
            let color = NSColor::controlAccentColor().colorWithAlphaComponent(0.20);
            color.set();
            path.fill();
        }
    }
);

/// Snapshot of the most recent AI interaction, kept session-only so the recents
/// view can re-open it and re-enter the existing follow-up chat thread.
#[derive(Clone)]
struct LastAi {
    prompt: String,
    answer: String,
    transcript: Vec<ChatMsg>,
}

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
    /// Whether to capture images copied to the clipboard.
    keep_images: bool,
    /// Last observed pasteboard change count.
    last_change: Cell<isize>,
    /// AI backend configuration (provider/model/endpoint).
    ai_config: AiConfig,
    /// Latest AI answer awaiting application on the main thread.
    ai_pending: AiPending,
    /// Monotonic AI request id; stale answers are discarded.
    ai_generation: Cell<u64>,
    /// Running conversation transcript for multi-turn follow-up chat. Reset when
    /// the panel is dismissed or a fresh top-level `?` question is asked.
    chat: RefCell<Vec<ChatMsg>>,
    /// Whether we are in follow-up chat mode (an answer is on screen and more
    /// typing continues the thread).
    chat_active: Cell<bool>,
    /// Session-only ring buffer of recently activated items, shown on an empty
    /// query. In-memory only; never persisted.
    recents: RefCell<VecDeque<Item>>,
    /// Session-only snapshot of the last AI interaction, pinned atop recents.
    last_ai: RefCell<Option<LastAi>>,
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
    /// PID of the app that was frontmost just before the panel opened. Window
    /// commands target this app's focused window (since opening the panel makes
    /// litecast itself frontmost). -1 when unknown.
    prev_app_pid: Cell<i32>,
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
            set_placeholder(&self.ivars().search, PLACEHOLDER_SHOT);
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
            } else if ivars.keep_images {
                // No text: capture an image off the pasteboard (if any).
                if let Some(path) = save_pasteboard_image(&pasteboard, count) {
                    ivars.clip_history.record_image(path);
                }
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
                // Esc escalates: exit chat, then clear an active filter, then close.
                let ivars = self.ivars();
                if ivars.chat_active.get() {
                    self.exit_chat();
                } else if ivars.active_filter.get() != Filter::All {
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

        // NSTableViewDelegate: supply our custom row view for inset selection.
        #[unsafe(method_id(tableView:rowViewForRow:))]
        fn row_view_for_row(
            &self,
            _table: &NSTableView,
            _row: isize,
        ) -> Option<Retained<NSTableRowView>> {
            let view = LcRowView::alloc(self.mtm());
            let view: Retained<LcRowView> = unsafe { msg_send![view, init] };
            Some(Retained::into_super(view))
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

    /// Leave follow-up chat mode and return to normal search (panel stays open).
    fn exit_chat(&self) {
        let ivars = self.ivars();
        ivars.chat_active.set(false);
        ivars.chat.borrow_mut().clear();
        // Invalidate any in-flight answer so it can't re-enter chat mode.
        ivars.ai_generation.set(ivars.ai_generation.get().wrapping_add(1));
        ivars.search.setStringValue(&NSString::from_str(""));
        set_placeholder(&ivars.search, PLACEHOLDER_NORMAL);
        ivars.results.borrow_mut().clear();
        ivars.table.reloadData();
        self.layout(0);
    }

    fn show(&self) {
        eprintln!("[litecast] show");
        let ivars = self.ivars();

        // Remember which app was frontmost before we steal focus, so window
        // commands can target its window rather than litecast's own panel.
        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(front) = workspace.frontmostApplication() {
            let pid = front.processIdentifier();
            if pid > 0 && pid != std::process::id() as i32 {
                ivars.prev_app_pid.set(pid);
            }
        }

        self.layout(ivars.results.borrow().len());

        // Normal launcher open with an empty field: surface session recents.
        // Skipped in screenshot mode (its own empty state) and chat mode.
        if ivars.screenshot_path.borrow().is_none()
            && !ivars.chat_active.get()
            && ivars.search.stringValue().to_string().trim().is_empty()
        {
            self.render_recents();
        }

        // Rotate a playful placeholder (unless in screenshot mode).
        if ivars.playful_placeholders && ivars.screenshot_path.borrow().is_none() {
            let idx = ivars.placeholder_idx.get();
            ivars.placeholder_idx.set(idx.wrapping_add(1));
            let text = PLAYFUL_PLACEHOLDERS[idx % PLAYFUL_PLACEHOLDERS.len()];
            set_placeholder(&ivars.search, text);
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
        ivars.chat_active.set(false);
        ivars.chat.borrow_mut().clear();
        // Bump the generations so any in-flight worker/AI results are discarded.
        ivars.generation.set(ivars.generation.get().wrapping_add(1));
        ivars.ai_generation.set(ivars.ai_generation.get().wrapping_add(1));
        // Clear state for the next invocation.
        ivars.search.setStringValue(&NSString::from_str(""));
        ivars.screenshot_path.borrow_mut().take();
        set_placeholder(&ivars.search, PLACEHOLDER_NORMAL);
        ivars.results.borrow_mut().clear();
        ivars.table.reloadData();
    }

    fn dispatch_query(&self) {
        let ivars = self.ivars();
        ivars.pending_confirm.set(-1);

        // Follow-up chat mode: while a conversation is open, typing composes the
        // next turn (Enter sends it) instead of running the normal providers.
        if ivars.chat_active.get() && ivars.screenshot_path.borrow().is_none() {
            self.render_chat();
            return;
        }

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
            // Empty query in normal launcher mode shows session recents instead
            // of a blank panel. No providers run.
            self.render_recents();
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

    /// Record an activated item into the session-only recents ring buffer. Only
    /// re-runnable launcher actions are kept; AI/confirm/pin actions are skipped.
    fn record_recent(&self, item: &Item) {
        let keep = matches!(
            item.action,
            Action::Open(_) | Action::RunShell(_) | Action::CopyText(_) | Action::Paste(_)
        );
        if !keep {
            return;
        }
        let mut entry = item.clone();
        entry.source = "Recent";
        entry.score = 0;
        let mut recents = self.ivars().recents.borrow_mut();
        recents.retain(|e| e.title != entry.title);
        recents.push_front(entry);
        while recents.len() > RECENTS_CAP {
            recents.pop_back();
        }
    }

    /// Build the empty-query "recents" view from the in-memory buffers: the last
    /// AI interaction pinned on top, then recently activated items. No providers
    /// run. Falls back to an empty list (just the search bar) when there's
    /// nothing yet.
    fn render_recents(&self) {
        let ivars = self.ivars();
        let mut items: Vec<Item> = Vec::new();
        if let Some(la) = ivars.last_ai.borrow().as_ref() {
            items.push(Item::new(
                format!("Last AI: {}", one_line(&la.prompt)),
                preview(&la.answer),
                "AI",
                0,
                Action::ResumeAi,
            ));
        }
        for entry in ivars.recents.borrow().iter() {
            items.push(entry.clone());
        }
        let n = items.len();
        *ivars.results.borrow_mut() = items;
        ivars.table.reloadData();
        self.layout(n);
        if n > 0 {
            self.select_row(0);
        }
    }

    /// Re-open the last AI interaction: restore its transcript, re-enter chat
    /// mode, and show the answer so the next keystroke continues the thread.
    fn resume_ai(&self) {
        let ivars = self.ivars();
        let Some(la) = ivars.last_ai.borrow().clone() else {
            return;
        };
        *ivars.chat.borrow_mut() = la.transcript;
        ivars.chat_active.set(true);
        ivars.search.setStringValue(&NSString::from_str(""));
        set_placeholder(&ivars.search, PLACEHOLDER_FOLLOWUP);
        let items = answer_to_items(&la.answer);
        let n = items.len();
        *ivars.results.borrow_mut() = items;
        ivars.table.reloadData();
        self.layout(n);
        if n > 0 {
            self.select_row(0);
        }
    }

    fn activate_selection(&self) {
        let ivars = self.ivars();
        let row = ivars.table.selectedRow();
        if row < 0 {
            return;
        }
        let item = {
            let results = ivars.results.borrow();
            match results.get(row as usize) {
                Some(item) => item.clone(),
                None => return,
            }
        };
        let action = item.action.clone();
        let id = item.id.clone();
        // AI requests run asynchronously and keep the panel open.
        if let Action::AskAi { prompt, image } = action {
            self.start_ai(prompt, image);
            return;
        }
        if let Action::AskAiFollowup { prompt } = action {
            self.start_ai_followup(prompt);
            return;
        }
        if let Action::ResumeAi = action {
            self.resume_ai();
            return;
        }
        // Window management: needs the main thread + Accessibility, handled here.
        if let Action::Window(op) = action {
            if let Some(id) = &id {
                ivars.frecency.record(id);
            }
            self.run_window_op(op);
            return;
        }
        // Toggle a clipboard pin, then refresh the list in place (panel stays open).
        if let Action::TogglePin { key } = action {
            ivars.clip_history.toggle_pin(&key);
            self.dispatch_query();
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
        self.record_recent(&item);
        if action.execute() {
            self.hide_and_reset();
        }
    }

    /// Run a window-management op against the previously-frontmost app's window.
    /// Verifies Accessibility trust lazily (prompting only on first use); if not
    /// trusted, shows a helpful row that opens the right System Settings pane
    /// instead of failing silently.
    fn run_window_op(&self, op: model::WindowOp) {
        let ivars = self.ivars();
        if !window::trusted(false) {
            // Trigger the standard system prompt the first time.
            window::trusted(true);
            self.show_status_row(
                "Accessibility permission needed",
                "Grant litecast access in System Settings \u{203a} Privacy & Security \u{203a} Accessibility, then run the command again",
                Action::Open(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
                        .to_string(),
                ),
            );
            return;
        }
        match window::apply(self.mtm(), ivars.prev_app_pid.get(), op) {
            Ok(()) => self.hide_and_reset(),
            Err(msg) => self.show_status_row("Window command failed", &msg, Action::None),
        }
    }

    /// Replace the results list with a single informational row (keeps the panel
    /// open). Used for window-management permission/error feedback.
    fn show_status_row(&self, title: &str, subtitle: &str, action: Action) {
        let ivars = self.ivars();
        *ivars.results.borrow_mut() = vec![Item::new(
            title.to_string(),
            subtitle.to_string(),
            "Window",
            0,
            action,
        )];
        ivars.table.reloadData();
        self.layout(1);
        self.select_row(0);
    }

    /// Start a fresh AI conversation with `prompt` (optionally about an image),
    /// resetting any prior transcript.
    fn start_ai(&self, prompt: String, image: Option<String>) {
        let ivars = self.ivars();
        ivars.chat.borrow_mut().clear();
        ivars.chat.borrow_mut().push(ChatMsg::user(prompt));
        self.send_ai_turn(image);
    }

    /// Continue the current conversation with a follow-up `prompt`.
    fn start_ai_followup(&self, prompt: String) {
        let ivars = self.ivars();
        ivars.chat.borrow_mut().push(ChatMsg::user(prompt));
        self.send_ai_turn(None);
    }

    /// Send the current transcript to the backend on a worker thread, showing a
    /// loading row. The reply is appended to the transcript by the main thread.
    fn send_ai_turn(&self, image: Option<String>) {
        let ivars = self.ivars();
        // Leaving screenshot mode now that the question has been sent.
        ivars.screenshot_path.borrow_mut().take();
        // Clear the field so the next keystroke composes a clean follow-up.
        ivars.search.setStringValue(&NSString::from_str(""));
        set_placeholder(&ivars.search, PLACEHOLDER_NORMAL);
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
        let history = ivars.chat.borrow().clone();
        let delegate_addr = self as *const AppDelegate as usize;
        std::thread::spawn(move || {
            let result = ai::ask_chat(&config, &history, image.as_deref());
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

    /// Render the conversation while in follow-up chat mode: a compose row (when
    /// the user has typed something) on top, followed by the latest answer.
    fn render_chat(&self) {
        let ivars = self.ivars();
        let typed = ivars.search.stringValue().to_string();
        let typed = typed.trim().to_string();

        let mut items: Vec<Item> = Vec::new();
        if !typed.is_empty() {
            items.push(Item::new(
                format!("Ask follow-up: {typed}"),
                "Press Enter to continue the conversation (Esc to exit chat)",
                "AI",
                0,
                Action::AskAiFollowup { prompt: typed },
            ));
        }
        // Show the most recent answer underneath for context.
        if let Some(answer) = ivars
            .chat
            .borrow()
            .iter()
            .rev()
            .find(|m| matches!(m.role, ai::Role::Assistant))
        {
            items.extend(answer_to_items(&answer.content));
        }
        if items.is_empty() {
            items.push(Item::new(
                "Type a follow-up, then Enter",
                "Esc to exit chat",
                "AI",
                0,
                Action::None,
            ));
        }
        let n = items.len();
        *ivars.results.borrow_mut() = items;
        ivars.table.reloadData();
        self.layout(n);
        self.select_row(0);
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
            Ok(answer) => {
                // Remember the reply and enter follow-up chat mode so the next
                // keystroke composes another turn.
                ivars.chat.borrow_mut().push(ChatMsg::assistant(&answer));
                ivars.chat_active.set(true);
                // Snapshot the interaction (session-only) for the recents view.
                let transcript = ivars.chat.borrow().clone();
                let prompt = transcript
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, ai::Role::User))
                    .map(|m| m.content.clone())
                    .unwrap_or_default();
                *ivars.last_ai.borrow_mut() = Some(LastAi {
                    prompt,
                    answer: answer.clone(),
                    transcript,
                });
                set_placeholder(&ivars.search, PLACEHOLDER_FOLLOWUP);
                answer_to_items(&answer)
            }
            Err(err) => {
                // Drop the failed turn so the transcript stays valid; leave chat.
                ivars.chat.borrow_mut().clear();
                ivars.chat_active.set(false);
                vec![Item::new("AI error", err, "AI", 0, Action::None)]
            }
        };
        let n = items.len();
        *ivars.results.borrow_mut() = items;
        ivars.table.reloadData();
        self.layout(n);
        if n > 0 {
            self.select_row(0);
        }
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
        // The results block adds symmetric breathing room: a small gap under the
        // separator and bottom padding so the last row clears the rounded corner.
        let results_block = if visible_rows > 0 {
            RESULTS_BOTTOM_PAD + results_h + RESULTS_TOP_GAP
        } else {
            0.0
        };
        let total_h = SEARCH_AREA_H + results_block;
        // Top of the results block / bottom of the search area.
        let band_bottom = results_block;

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

        // Filter chip on the right of the search area. Always visible: a faint
        // "Tab to filter" hint when unfiltered (discoverability), an accent pill
        // showing the category when a filter is active. Laid out first so the
        // search field can reserve room for it.
        let filter = ivars.active_filter.get();
        let chip = &ivars.chip;
        let (label, text_color, bg_color) = if filter == Filter::All {
            // Idle: a clearly-enabled (not greyed) affordance hinting the Tab key.
            (
                "\u{21e5} Filter".to_string(),
                NSColor::secondaryLabelColor(),
                NSColor::labelColor().colorWithAlphaComponent(0.10),
            )
        } else {
            // Active: a high-contrast accent pill with readable text.
            let accent = NSColor::controlAccentColor();
            (
                format!("\u{21e5} {}", filter.label()),
                NSColor::alternateSelectedControlTextColor(),
                accent.colorWithAlphaComponent(0.95),
            )
        };
        chip.setStringValue(&NSString::from_str(&label));
        chip.setTextColor(Some(&text_color));
        chip.setBackgroundColor(Some(&bg_color));
        chip.sizeToFit();
        let natural_w = chip.frame().size.width;
        let chip_h = (line_height(12.0, true) + 8.0).round();
        let chip_w = (natural_w + 22.0).round();
        let chip_x = PANEL_WIDTH - SIDE_INSET - chip_w;
        let chip_y = (band_bottom + (SEARCH_AREA_H - chip_h) / 2.0).round();
        chip.setFrame(NSRect::new(
            NSPoint::new(chip_x, chip_y),
            NSSize::new(chip_w, chip_h),
        ));
        chip.setHidden(false);
        let search_right = chip_x - 12.0;

        // Search field sized to its exact text height and centered in the top
        // search area, so the text sits on the vertical midline (a tall field
        // would top-align its text instead).
        let search_h = line_height(24.0, false);
        let search_y = (band_bottom + (SEARCH_AREA_H - search_h) / 2.0).round();
        let search_frame = NSRect::new(
            NSPoint::new(SIDE_INSET, search_y),
            NSSize::new((search_right - SIDE_INSET).max(40.0), search_h),
        );
        ivars.search.setFrame(search_frame);

        // Hairline separator on the boundary between the search area and results.
        let separator_frame = NSRect::new(
            NSPoint::new(SIDE_INSET, band_bottom),
            NSSize::new(PANEL_WIDTH - 2.0 * SIDE_INSET, 1.0),
        );
        ivars.separator.setFrame(separator_frame);
        ivars.separator.setHidden(visible_rows == 0);

        // Results scroll view, inset above the bottom padding so the last row
        // is fully visible and clears the rounded corners.
        let scroll_frame = NSRect::new(
            NSPoint::new(0.0, RESULTS_BOTTOM_PAD),
            NSSize::new(PANEL_WIDTH, results_h),
        );
        ivars.scroll.setFrame(scroll_frame);
        ivars.scroll.setHidden(visible_rows == 0);
    }
}

/// Turn an AI answer into wrapped result rows. Each row copies the full answer
/// on Enter, so a multi-line reply is readable and copyable.
/// Collapse text to a single line (whitespace runs -> one space), trimmed.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One-line preview of a longer answer, truncated with an ellipsis.
fn preview(text: &str) -> String {
    const MAX: usize = 96;
    let line = one_line(text);
    if line.chars().count() > MAX {
        let mut s: String = line.chars().take(MAX).collect();
        s.push('\u{2026}');
        s
    } else if line.is_empty() {
        "Reopen the conversation".to_string()
    } else {
        line
    }
}

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

/// Save an image from the pasteboard to `clip-images/clip-<change>.png` under the
/// support dir, returning the path. Prefers PNG data; converts TIFF when needed.
fn save_pasteboard_image(pasteboard: &NSPasteboard, change: isize) -> Option<String> {
    let png: Retained<NSData> = unsafe {
        if let Some(data) = pasteboard.dataForType(NSPasteboardTypePNG) {
            data
        } else {
            let tiff = pasteboard.dataForType(NSPasteboardTypeTIFF)?;
            let rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)?;
            let props = NSDictionary::new();
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props)?
        }
    };

    let dir = crate::paths::support_dir().join("clip-images");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("clip-{change}.png"));
    let path_str = path.to_str()?;
    let ok = png.writeToFile_atomically(&NSString::from_str(path_str), true);
    ok.then(|| path_str.to_string())
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

/// Placeholder point size. Smaller than the 24pt typed text so the hint reads as
/// secondary rather than dominating the search bar.
const PLACEHOLDER_FONT_SIZE: f64 = 15.0;

/// Set the search-field placeholder as a smaller, secondary hint via an
/// attributed string, so it doesn't render at the full 24pt input size while
/// typed input stays at the normal size. A baseline offset re-centers the
/// smaller text against the (taller) typed line.
fn set_placeholder(field: &NSTextField, text: &str) {
    fn anyobj<T: std::convert::AsRef<AnyObject>>(x: &T) -> &AnyObject {
        x.as_ref()
    }
    let s = NSString::from_str(text);
    let font = NSFont::systemFontOfSize(PLACEHOLDER_FONT_SIZE);
    let color = NSColor::placeholderTextColor();
    let delta = (line_height(24.0, false) - line_height(PLACEHOLDER_FONT_SIZE, false)) / 2.0;
    let offset = NSNumber::numberWithDouble(-delta);
    let keys: [&NSString; 3] = [
        unsafe { NSFontAttributeName },
        unsafe { NSForegroundColorAttributeName },
        unsafe { NSBaselineOffsetAttributeName },
    ];
    let objs: [&AnyObject; 3] = [anyobj(&*font), anyobj(&*color), anyobj(&*offset)];
    let attrs = NSDictionary::from_slices(&keys, &objs);
    let attr = unsafe { NSAttributedString::new_with_attributes(&s, &attrs) };
    field.setPlaceholderAttributedString(Some(&attr));
}

// Reading the AppKit font-weight statics is safe (immutable CGFloat constants).
fn weight_medium() -> NSFontWeight {
    unsafe { NSFontWeightMedium }
}
fn weight_regular() -> NSFontWeight {
    unsafe { NSFontWeightRegular }
}

fn make_label(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
    weight: NSFontWeight,
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
    field.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingTail);
    let font = NSFont::systemFontOfSize_weight(size, weight);
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
        "Bookmark" => "bookmark",
        "History" => "clock",
        "Clip" => "doc.on.clipboard",
        "Command" => "terminal",
        "Recent" => "clock.arrow.circlepath",
        "Plugin" => "puzzlepiece.extension",
        "Window" => "macwindow",
        "Proc" => "bolt.horizontal.circle",
        "?" => "wand.and.stars",
        _ => "magnifyingglass",
    };
    NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None)
}

/// Short right-aligned category tag shown on each row. `None` hides the tag
/// (e.g. for the playful easter-egg source).
fn source_tag(source: &str) -> Option<&'static str> {
    match source {
        "App" => Some("App"),
        "File" => Some("File"),
        "Calc" => Some("Calc"),
        "Convert" => Some("Convert"),
        "Web" => Some("Web"),
        "AI" => Some("AI"),
        "Clip" => Some("Clipboard"),
        "Command" => Some("Command"),
        "Snippet" => Some("Snippet"),
        "Quicklink" => Some("Quicklink"),
        "Emoji" => Some("Emoji"),
        "Bookmark" => Some("Bookmark"),
        "History" => Some("History"),
        "Plugin" => Some("Plugin"),
        "Recent" => Some("Recent"),
        "Window" => Some("Window"),
        "Proc" => Some("Process"),
        _ => None,
    }
}

fn make_row_cell(mtm: MainThreadMarker, item: &Item) -> Retained<NSView> {
    let width = PANEL_WIDTH;
    let container = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, ROW_H)),
    );

    // Icon left edge shares the search field's left margin so everything lines
    // up on a common left edge.
    let icon_view = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        NSRect::new(
            NSPoint::new(SIDE_INSET, (ROW_H - ROW_ICON) / 2.0),
            NSSize::new(ROW_ICON, ROW_ICON),
        ),
    );
    if let Some(image) = row_icon(item) {
        icon_view.setImage(Some(&image));
    }
    icon_view.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
    container.addSubview(&icon_view);

    let text_x = SIDE_INSET + ROW_ICON + 12.0;

    // Optional right-aligned source tag (e.g. "App", "Calc"), like Raycast.
    let mut right_edge = width - SIDE_INSET;
    if let Some(tag_text) = source_tag(item.source) {
        let tag = make_label(
            mtm,
            tag_text,
            11.0,
            weight_regular(),
            &NSColor::tertiaryLabelColor(),
        );
        tag.sizeToFit();
        let tag_w = tag.frame().size.width;
        let tag_h = line_height(11.0, false);
        let tag_x = width - SIDE_INSET - tag_w;
        let tag_y = ((ROW_H - tag_h) / 2.0).round();
        tag.setFrame(NSRect::new(
            NSPoint::new(tag_x, tag_y),
            NSSize::new(tag_w, tag_h),
        ));
        container.addSubview(&tag);
        right_edge = tag_x - 12.0;
    }
    let text_w = (right_edge - text_x).max(40.0);

    let title_h = line_height(15.0, true);
    if item.subtitle.is_empty() {
        // Single line: center the title band within the row.
        let y = ((ROW_H - title_h) / 2.0).round();
        let title = make_label(mtm, &item.title, 15.0, weight_medium(), &NSColor::labelColor());
        title.setFrame(NSRect::new(NSPoint::new(text_x, y), NSSize::new(text_w, title_h)));
        container.addSubview(&title);
    } else {
        // Two lines: center the (title + gap + subtitle) block within the row.
        const GAP: f64 = 2.0;
        let sub_h = line_height(12.0, false);
        let block = title_h + GAP + sub_h;
        let bottom = ((ROW_H - block) / 2.0).round();
        let subtitle = make_label(
            mtm,
            &item.subtitle,
            12.0,
            weight_regular(),
            &NSColor::secondaryLabelColor(),
        );
        subtitle.setFrame(NSRect::new(
            NSPoint::new(text_x, bottom),
            NSSize::new(text_w, sub_h),
        ));
        container.addSubview(&subtitle);
        let title = make_label(mtm, &item.title, 15.0, weight_medium(), &NSColor::labelColor());
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
    effect.setMaterial(NSVisualEffectMaterial::Menu);
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
        NSPoint::new(SIDE_INSET, 14.0),
        NSSize::new(PANEL_WIDTH - 2.0 * SIDE_INSET, 40.0),
    );
    let search = NSTextField::initWithFrame(NSTextField::alloc(mtm), search_rect);
    search.setBezeled(false);
    search.setBordered(false);
    search.setDrawsBackground(false);
    search.setEditable(true);
    search.setSelectable(true);
    search.setFocusRingType(NSFocusRingType::None);
    search.setFont(Some(&NSFont::systemFontOfSize(24.0)));
    set_placeholder(&search, PLACEHOLDER_NORMAL);

    // Results table inside a scroll view.
    let table = NSTableView::initWithFrame(
        NSTableView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, 0.0)),
    );
    let column =
        NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str("main"));
    column.setWidth(PANEL_WIDTH);
    table.addTableColumn(&column);
    table.setHeaderView(None);
    table.setRowHeight(ROW_H);
    // No inter-row spacing: each row is exactly ROW_H, so visible_rows * ROW_H
    // fits the scroll view exactly with no row clipped at the bottom.
    table.setIntercellSpacing(NSSize::new(0.0, 0.0));

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

/// Parse a hotkey combo like "Cmd+Shift+S" into a `HotKey`. Modifiers accept
/// Cmd/Command/Super/Win, Ctrl/Control, Alt/Option/Opt, and Shift; the final
/// token is the key. Requires at least one modifier (global hotkeys without one
/// are a bad idea) and exactly one key. Returns `None` on anything unrecognized.
fn parse_hotkey_combo(combo: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in combo.split('+') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        match token.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" | "win" | "meta" => mods |= Modifiers::META,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" | "opt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            other => {
                if code.is_some() {
                    return None;
                }
                code = parse_key_code(other);
                code?;
            }
        }
    }
    let code = code?;
    if mods.is_empty() {
        return None;
    }
    Some(HotKey::new(Some(mods), code))
}

/// Map a single key token (a letter, digit, or named key) to a `Code`.
fn parse_key_code(token: &str) -> Option<Code> {
    use Code::*;
    let upper = token.to_ascii_uppercase();
    Some(match upper.as_str() {
        "A" => KeyA, "B" => KeyB, "C" => KeyC, "D" => KeyD, "E" => KeyE, "F" => KeyF,
        "G" => KeyG, "H" => KeyH, "I" => KeyI, "J" => KeyJ, "K" => KeyK, "L" => KeyL,
        "M" => KeyM, "N" => KeyN, "O" => KeyO, "P" => KeyP, "Q" => KeyQ, "R" => KeyR,
        "S" => KeyS, "T" => KeyT, "U" => KeyU, "V" => KeyV, "W" => KeyW, "X" => KeyX,
        "Y" => KeyY, "Z" => KeyZ,
        "0" => Digit0, "1" => Digit1, "2" => Digit2, "3" => Digit3, "4" => Digit4,
        "5" => Digit5, "6" => Digit6, "7" => Digit7, "8" => Digit8, "9" => Digit9,
        "F1" => F1, "F2" => F2, "F3" => F3, "F4" => F4, "F5" => F5, "F6" => F6,
        "F7" => F7, "F8" => F8, "F9" => F9, "F10" => F10, "F11" => F11, "F12" => F12,
        "SPACE" => Space,
        "ENTER" | "RETURN" => Enter,
        "TAB" => Tab,
        "ESC" | "ESCAPE" => Escape,
        "UP" => ArrowUp,
        "DOWN" => ArrowDown,
        "LEFT" => ArrowLeft,
        "RIGHT" => ArrowRight,
        "MINUS" | "-" => Minus,
        "EQUAL" | "=" => Equal,
        "COMMA" | "," => Comma,
        "PERIOD" | "." => Period,
        "SLASH" | "/" => Slash,
        "BACKSLASH" | "\\" => Backslash,
        "SEMICOLON" | ";" => Semicolon,
        "QUOTE" | "'" => Quote,
        "BACKQUOTE" | "`" => Backquote,
        "LEFTBRACKET" | "[" => BracketLeft,
        "RIGHTBRACKET" | "]" => BracketRight,
        _ => return None,
    })
}

/// Resolve a `[[hotkeys]]` entry into the `Action` to run on press. For
/// `kind = "command"`, the target names a `[[commands]]` entry, whose own
/// kind/target define the action (with `{}` stripped, since a hotkey carries
/// no argument).
fn resolve_hotkey_action(hk: &HotkeyConfig, commands: &[CommandConfig]) -> Option<Action> {
    match hk.kind.as_str() {
        "open" => Some(Action::Open(hk.target.clone())),
        "shell" => Some(Action::RunShell(hk.target.clone())),
        "command" => {
            let cmd = commands.iter().find(|c| c.name == hk.target)?;
            let target = cmd.target.replace("{}", "");
            Some(match cmd.kind.as_str() {
                "shell" => Action::RunShell(target),
                _ => Action::Open(target),
            })
        }
        _ => None,
    }
}

fn build_engine(history: History, config: &Config, frecency: Frecency) -> Engine {
    let mut engine = Engine::new(frecency);
    // EasterEgg is general fun: only shown when no filter is active.
    engine.add(Box::new(EasterEggProvider), Filter::All);
    engine.add(Box::new(AiProvider::new(config.ai.clone())), Filter::Ai);
    engine.add(
        Box::new(AiCommandsProvider::new(&config.ai, history.clone())),
        Filter::Ai,
    );
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
    // Window management is opt-in (needs Accessibility); only surface its
    // commands when explicitly enabled in the config.
    if config.window.enabled {
        engine.add(Box::new(WindowProvider::new()), Filter::Cmd);
    }
    engine.add(Box::new(PluginProvider::new()), Filter::Cmd);
    // Process manager: keyword-gated (`kill`/`ps`); kills go through a confirm.
    engine.add(Box::new(ProcessProvider::new()), Filter::Cmd);
    engine.add(Box::new(AppsProvider::new()), Filter::Apps);
    engine.add(Box::new(FilesProvider::new()), Filter::Files);
    engine.add(Box::new(BookmarksProvider::new()), Filter::Web);
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

    // User-defined global hotkeys from `[[hotkeys]]`. Each binds a key combo to
    // an action (open/shell/named command). Registration is best-effort and
    // non-fatal, mirroring the built-in hotkeys above.
    let mut hotkey_actions: HashMap<u32, Action> = HashMap::new();
    for hk in &config.hotkeys {
        let Some(parsed) = parse_hotkey_combo(&hk.key) else {
            eprintln!("[litecast] skipping hotkey with invalid combo: {:?}", hk.key);
            continue;
        };
        let Some(action) = resolve_hotkey_action(hk, &config.commands) else {
            eprintln!(
                "[litecast] skipping hotkey {:?}: unknown kind/target ({}: {})",
                hk.key, hk.kind, hk.target
            );
            continue;
        };
        match manager.register(parsed) {
            Ok(()) => {
                eprintln!(
                    "[litecast] registered custom hotkey {} (id={})",
                    hk.key,
                    parsed.id()
                );
                hotkey_actions.insert(parsed.id(), action);
            }
            Err(e) => eprintln!("[litecast] FAILED to register custom hotkey {}: {e}", hk.key),
        }
    }

    let (query_tx, query_rx) = mpsc::channel::<(u64, String, Filter)>();
    let pending: PendingResults = Arc::new(Mutex::new(None));
    let history = History::new(50, config.clipboard.max_images);
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
        keep_images: config.clipboard.keep_images,
        last_change: Cell::new(-1),
        ai_config: config.ai.clone(),
        ai_pending: Arc::new(Mutex::new(None)),
        ai_generation: Cell::new(0),
        chat: RefCell::new(Vec::new()),
        chat_active: Cell::new(false),
        recents: RefCell::new(VecDeque::with_capacity(RECENTS_CAP)),
        last_ai: RefCell::new(None),
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
        prev_app_pid: Cell::new(-1),
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
            // Custom hotkeys map directly to an Action (open/shell). These only
            // spawn subprocesses, so they run here without touching the main
            // thread or the panel.
            if let Some(action) = hotkey_actions.get(&event.id) {
                action.execute();
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
