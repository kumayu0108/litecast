mod ai;
mod clipboard;
mod color_pick;
mod config;
mod critters;
mod currency;
mod engine;
mod frecency;
mod menu_ax;
mod menu_cache;
mod model;
mod paths;
mod providers;
mod screenshot;
mod secrets;
mod target_pid;
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
    NSAnimationContext, NSBezelStyle, NSBezierPath, NSBitmapImageFileType, NSBitmapImageRep, NSBox,
    NSBoxType, NSButton, NSButtonType,
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
    NSNumber, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString, NSTimer,
};

use ai::ChatMsg;
use clipboard::History;
use config::{AiConfig, AppCommandConfig, CommandConfig, Config, HotkeyConfig};
use currency::CurrencyCache;
use engine::{fuzzy_score, Engine, Filter};
use frecency::Frecency;
use model::{Action, Item};
use providers::{
    AiCommandsProvider, AiProvider, AppCommandsProvider, AppsProvider, BookmarksProvider,
    CalcProvider, CalendarProvider, ClipboardProvider, CommandsProvider, ConvertProvider,
    ConvertersProvider, DateTimeProvider, DevToolsProvider, DictionaryProvider, EasterEggProvider,
    EmojiProvider, FileActionsProvider, FilesProvider, MediaProvider, NetworkProvider,
    ColorProvider, GitProvider, MenuProvider, NewFileProvider, NotesProvider, PluginProvider,
    PomodoroProvider, ProcessProvider, ProgUtilsProvider, QuicklinksProvider, ScriptsProvider,
    SnippetsProvider, SwitcherProvider, SystemProvider, TextTransformProvider, WebSearchProvider,
    WindowProvider,
};

type PendingResults = Arc<Mutex<Option<(u64, Vec<Item>)>>>;
type AiPending = Arc<Mutex<Option<(u64, Result<String, String>)>>>;

const PANEL_WIDTH: f64 = 720.0;
const SEARCH_AREA_H: f64 = 72.0;
// Spotlight-style rounded "pill" search field: a soft capsule with a leading
// magnifier glyph and generous padding around the typed text.
const SEARCH_PILL_H: f64 = 48.0;
const SEARCH_ICON: f64 = 19.0;
// Point size of the typed query text.
const SEARCH_FONT_SIZE: f64 = 22.0;
// Inset of the magnifier glyph from the pill's left edge, and the gap from the
// glyph to where the text begins.
const SEARCH_PILL_PAD_X: f64 = 16.0;
const SEARCH_ICON_GAP: f64 = 11.0;
// Spotlight-style category chip row that sits in its own band under the search
// field. Chips are clickable and stay in sync with Tab-cycle and @prefix.
const CHIP_ROW_H: f64 = 40.0;
const CHIP_H: f64 = 24.0;
const CHIP_GAP: f64 = 7.0;
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
    "Faster than you can say launcher.",
    "Go ahead, type something brilliant.",
    "Less clicking, more launching.",
    "100 usd to eur? :rocket? I got you.",
    "The blank box of infinite potential.",
    "Searching is believing.",
    "Psst - try @apps or @calc.",
    "Tiny binary, big dreams.",
    "Name it and I'll find it.",
    "Keyboard warrior mode: engaged.",
    "Lighter than the rest, by design.",
];
const ROW_H: f64 = 52.0;
const MAX_VISIBLE_ROWS: usize = 8;
// AI answer "card" rendering: the whole reply lives in one rounded container
// with a wrapping text view, a leading sparkle accent, and a copy button. Sized
// to the measured wrapped-text height.
const ANSWER_FONT_SIZE: f64 = 13.0;
// Outer margin of the card within its table row (horizontal aligns with rows).
const CARD_MARGIN_X: f64 = SIDE_INSET;
const CARD_MARGIN_Y: f64 = 7.0;
// Internal padding between the card edge and its content.
const CARD_PAD: f64 = 14.0;
// Minimum card height so a one-line answer still reads as a card.
const CARD_MIN_H: f64 = 46.0;
// Leading accent (sparkle) glyph box.
const ANSWER_ACCENT: f64 = 16.0;
// Left edge of the wrapping text inside the card (pad + accent + gap).
const ANSWER_TEXT_LEFT: f64 = CARD_PAD + ANSWER_ACCENT + 10.0;
// Copy button box reserved in the top-right corner.
const ANSWER_COPY: f64 = 22.0;
// Horizontal space reserved on the right so wrapped text clears the copy button.
const ANSWER_TEXT_RIGHT_RESERVE: f64 = ANSWER_COPY + 8.0;
// Cap the results area height (matches MAX_VISIBLE_ROWS normal rows); taller
// content scrolls within this band so the panel never grows without bound.
const MAX_RESULTS_H: f64 = MAX_VISIBLE_ROWS as f64 * ROW_H;
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
                // Cmd+1..9 selects the Nth category chip (Spotlight-style),
                // routed to the window delegate (the AppDelegate).
                if let Some(c) = chars.chars().next() {
                    if ('1'..='9').contains(&c) {
                        let delegate: Option<Retained<AnyObject>> =
                            unsafe { msg_send![self, delegate] };
                        if let Some(delegate) = delegate {
                            let idx = (c as u8 - b'1') as isize;
                            unsafe {
                                let _: () = msg_send![&*delegate, selectChipByIndex: idx];
                            }
                            return true.into();
                        }
                    }
                }
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
const SELECTION_INSET_X: f64 = 15.0;
const SELECTION_INSET_Y: f64 = 7.0;
/// Extra inset so icons/text/tags sit clearly inside the selection pill.
const ROW_CONTENT_INSET: f64 = 5.0;
const ROW_CONTENT_LEFT: f64 = SELECTION_INSET_X + ROW_CONTENT_INSET;
// Gap between the row icon and the title/subtitle text, and between the text and
// the right-side source tag.
const ROW_TEXT_GAP: f64 = 14.0;

// Custom row view that draws a rounded, inset selection highlight (Raycast-style)
// instead of the default full-width table highlight.
define_class!(
    #[unsafe(super(NSTableRowView))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcRowView"]
    #[ivars = Cell<bool>]
    struct LcRowView;

    impl LcRowView {
        #[unsafe(method(drawSelectionInRect:))]
        fn draw_selection(&self, _dirty: NSRect) {
            // AI answer cards draw their own container, so skip the row highlight.
            if self.ivars().get() {
                return;
            }
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
            let color = accent_selection_fill();
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

/// A single `@`-shortcut suggestion: the token typed after `@` plus a short
/// description. Covers category filters (`@apps`, `@calc`, …) and app commands
/// (`@term`, `@finder`, plus any user-defined `[[app_commands]]`).
struct ShortcutEntry {
    token: String,
    desc: String,
}

struct Ivars {
    panel: Retained<LcPanel>,
    search: Retained<NSTextField>,
    /// Rounded "pill" background behind the search field (Spotlight-style).
    search_bg: Retained<NSView>,
    /// Leading magnifier glyph inside the search pill.
    search_icon: Retained<NSImageView>,
    table: Retained<NSTableView>,
    scroll: Retained<NSScrollView>,
    separator: Retained<NSBox>,
    visible: Cell<bool>,
    results: RefCell<Vec<Item>>,
    /// Monotonic query id; results tagged with a stale id are discarded.
    generation: Cell<u64>,
    /// Sends (generation, query, filter) to the background worker.
    query_tx: mpsc::Sender<(u64, String, Filter)>,
    /// Active category filter; driven by `@prefix` typing, Tab cycling, and
    /// clicking a chip in the category row.
    active_filter: Cell<Filter>,
    /// Spotlight-style clickable category chips (one per `Filter::CYCLE` entry,
    /// in order). The active one is highlighted; clicking one sets the filter.
    chips: Vec<Retained<NSButton>>,
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
    /// Whether the wandering-critter feature is enabled (`[ui] critters`).
    critters_enabled: bool,
    /// Loaded critter GIF images (empty = use the built-in glyph critter).
    critter_images: Vec<Retained<NSImage>>,
    /// Rotating index into the critter image list.
    critter_idx: Cell<usize>,
    /// Usage learner; records activations and boosts frequent/recent items.
    frecency: Frecency,
    /// Row index currently armed for a two-step destructive confirmation.
    pending_confirm: Cell<isize>,
    /// Available `@`-shortcut suggestions (filters + app commands), shown while
    /// the user is typing an `@token`.
    autocomplete: Vec<ShortcutEntry>,
    /// Set when the last edit was a Backspace/Delete, so the `@token`
    /// autocomplete does NOT re-fill the inline ghost suffix on that change
    /// (otherwise deleting a char would be immediately re-completed, making
    /// Backspace appear to do nothing). Consumed once per text change.
    suppress_ghost: Cell<bool>,
    /// PID of the app that was frontmost just before the panel opened. Window
    /// commands target this app's focused window (since opening the panel makes
    /// litecast itself frontmost). -1 when unknown.
    prev_app_pid: Cell<i32>,
    menu_enabled: bool,
    color_max_recent: usize,
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

        // When the panel becomes key, select the whole (unsubmitted) query so the
        // next non-arrow keystroke overwrites it, like a fresh search.
        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            if self.ivars().visible.get() {
                self.select_all_search();
            }
        }

        #[unsafe(method(windowDidChangeScreen:))]
        fn window_did_change_screen(&self, _notification: &NSNotification) {
            self.layout(self.ivars().results.borrow().len());
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
            // Backspace/Delete: never intercept (always edit text normally), but
            // remember that this edit was a deletion so the `@token` autocomplete
            // does not immediately re-complete the char we just removed.
            if selector == sel!(deleteBackward:)
                || selector == sel!(deleteForward:)
                || selector == sel!(deleteWordBackward:)
                || selector == sel!(deleteWordForward:)
                || selector == sel!(deleteToBeginningOfLine:)
            {
                self.ivars().suppress_ghost.set(true);
                false
            } else if selector == sel!(moveDown:) {
                self.move_selection(1);
                true
            } else if selector == sel!(moveUp:) {
                self.move_selection(-1);
                true
            } else if selector == sel!(insertNewline:) {
                self.activate_selection();
                true
            } else if selector == sel!(insertTab:) {
                // While typing an `@token`, Tab accepts the nearest autocomplete
                // match; otherwise it cycles the category filter.
                if !self.accept_nearest_autocomplete() {
                    self.cycle_filter(true);
                }
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

        // NSTableViewDelegate: variable row height. AI answer "block" rows are
        // sized to their wrapped text height; everything else is a normal row.
        #[unsafe(method(tableView:heightOfRow:))]
        fn height_of_row(&self, _table: &NSTableView, row: isize) -> f64 {
            let results = self.ivars().results.borrow();
            results.get(row as usize).map(row_height_for).unwrap_or(ROW_H)
        }

        // NSTableViewDelegate: supply our custom row view for inset selection.
        #[unsafe(method_id(tableView:rowViewForRow:))]
        fn row_view_for_row(
            &self,
            _table: &NSTableView,
            row: isize,
        ) -> Option<Retained<NSTableRowView>> {
            let suppress = self
                .ivars()
                .results
                .borrow()
                .get(row as usize)
                .map(|i| i.multiline)
                .unwrap_or(false);
            let view = LcRowView::alloc(self.mtm()).set_ivars(Cell::new(suppress));
            let view: Retained<LcRowView> = unsafe { msg_send![super(view), init] };
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
            results
                .get(row as usize)
                .map(|item| make_row_cell(self.mtm(), item, row, self))
        }

        // Copy an AI answer card's full text to the clipboard (the card's copy
        // button). The button's tag carries its row index.
        #[unsafe(method(copyAnswer:))]
        fn copy_answer(&self, sender: &NSButton) {
            let row = sender.tag() as usize;
            let text = {
                let results = self.ivars().results.borrow();
                match results.get(row).map(|i| &i.action) {
                    Some(Action::CopyText(text)) => Some(text.clone()),
                    _ => None,
                }
            };
            if let Some(text) = text {
                clipboard::set_clipboard(&text);
            }
        }

        // A category chip was clicked: activate its filter (same state the Tab
        // cycle and @prefix drive) and return focus to the search field.
        #[unsafe(method(chipClicked:))]
        fn chip_clicked(&self, sender: &NSButton) {
            let idx = sender.tag() as usize;
            if let Some(filter) = Filter::CYCLE.get(idx).copied() {
                self.set_filter(filter);
                let ivars = self.ivars();
                ivars.panel.makeFirstResponder(Some(&ivars.search));
            }
        }

        // Cmd+1..9: select the Nth category chip by its displayed order (0-based
        // index). No-op when hidden or out of range, and never reached while
        // editing text (Cmd+digit is not a text-input command).
        #[unsafe(method(selectChipByIndex:))]
        fn select_chip_by_index(&self, index: isize) {
            let ivars = self.ivars();
            if !ivars.visible.get() || index < 0 {
                return;
            }
            if let Some(filter) = Filter::CYCLE.get(index as usize).copied() {
                self.set_filter(filter);
                ivars.panel.makeFirstResponder(Some(&ivars.search));
            }
        }
    }
);

impl AppDelegate {
    fn toggle(&self) {
        let visible = self.ivars().visible.get();
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
        let ivars = self.ivars();

        // Remember which app was frontmost before we steal focus, so window
        // commands can target its window rather than litecast's own panel.
        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(front) = workspace.frontmostApplication() {
            let pid = front.processIdentifier();
            if pid > 0 && pid != std::process::id() as i32 {
                ivars.prev_app_pid.set(pid);
                target_pid::set(pid);
            }
        }
        if ivars.menu_enabled {
            menu_cache::refresh(ivars.prev_app_pid.get(), 80);
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
        // Select the entire existing query so typing replaces it (Spotlight-style),
        // while arrow keys still navigate results without clearing it.
        self.select_all_search();

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

        // Consume the "last edit was a deletion" flag exactly once per change.
        let suppress_ghost = ivars.suppress_ghost.replace(false);

        let raw = ivars.search.stringValue().to_string();
        // While the user is still typing an `@token` (no space yet), show the
        // available shortcuts as autocomplete suggestions instead of results.
        if ivars.screenshot_path.borrow().is_none() {
            if let Some(partial) = parse_autocomplete_token(&raw) {
                self.render_autocomplete(&partial, suppress_ghost);
                return;
            }
        }
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
        // On-demand critter: typing the easter-egg keyword sends one strolling
        // immediately (the EasterEgg provider also returns a fun row).
        if query.trim().eq_ignore_ascii_case("critter") {
            self.start_critter_walk();
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

    /// Select the entire text in the search field (Spotlight-style), so the next
    /// non-arrow keystroke overwrites the existing query. Safe on empty text.
    fn select_all_search(&self) {
        let ivars = self.ivars();
        let sender: *const AnyObject = std::ptr::null();
        unsafe {
            let _: () = msg_send![&*ivars.search, selectText: sender];
        }
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
            Action::Open(_)
                | Action::Run { .. }
                | Action::RunShell(_)
                | Action::CopyText(_)
                | Action::Paste(_)
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

    /// Suggestions matching the partial `@token` the user has typed, best first.
    /// An empty partial lists every shortcut in declaration order; otherwise they
    /// are fuzzy-scored against the token.
    fn autocomplete_matches(&self, partial: &str) -> Vec<(String, String)> {
        let entries = &self.ivars().autocomplete;
        if partial.is_empty() {
            return entries
                .iter()
                .map(|e| (e.token.clone(), e.desc.clone()))
                .collect();
        }
        let mut scored: Vec<(u32, &ShortcutEntry)> = entries
            .iter()
            .filter_map(|e| fuzzy_score(partial, &e.token).map(|s| (s, e)))
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored
            .into_iter()
            .map(|(_, e)| (e.token.clone(), e.desc.clone()))
            .collect()
    }

    /// Render the `@`-shortcut autocomplete list for the partial token, with the
    /// nearest match selected on top and shown as an inline ghost in the field.
    /// When `suppress_ghost` is set (the edit was a Backspace/Delete), the inline
    /// ghost is skipped so the deletion is not instantly re-completed.
    fn render_autocomplete(&self, partial: &str, suppress_ghost: bool) {
        let ivars = self.ivars();
        let matches = self.autocomplete_matches(partial);

        // Inline ghost: complete the field to the nearest match and select the
        // appended suffix, so the next keystroke replaces it (Spotlight-style).
        if let Some((best, _)) = matches.first() {
            if !suppress_ghost
                && !partial.is_empty()
                && best.len() > partial.len()
                && best.starts_with(partial)
            {
                let full = format!("@{best}");
                ivars.search.setStringValue(&NSString::from_str(&full));
                if let Some(editor) = ivars.search.currentEditor() {
                    let start = partial.chars().count() + 1; // past '@' + typed chars
                    let len = best.chars().count() - partial.chars().count();
                    unsafe {
                        let _: () =
                            msg_send![&*editor, setSelectedRange: NSRange::new(start, len)];
                    }
                }
            }
        }

        let items: Vec<Item> = matches
            .into_iter()
            .map(|(token, desc)| {
                Item::new(
                    format!("@{token}"),
                    desc,
                    "Shortcut",
                    0,
                    Action::Autocomplete { token },
                )
            })
            .collect();
        let n = items.len();
        *ivars.results.borrow_mut() = items;
        ivars.table.reloadData();
        self.layout(n);
        if n > 0 {
            self.select_row(0);
        }
    }

    /// Complete the search field to `@token ` (ready for an argument) and re-run.
    fn accept_autocomplete(&self, token: &str) {
        let ivars = self.ivars();
        let text = format!("@{token} ");
        ivars.search.setStringValue(&NSString::from_str(&text));
        if let Some(editor) = ivars.search.currentEditor() {
            let end = text.chars().count();
            unsafe {
                let _: () = msg_send![&*editor, setSelectedRange: NSRange::new(end, 0)];
            }
        }
        self.dispatch_query();
    }

    /// If an `@token` is being typed, accept its nearest match and return true.
    /// Otherwise return false so the caller can fall back to Tab's normal action.
    fn accept_nearest_autocomplete(&self) -> bool {
        let raw = self.ivars().search.stringValue().to_string();
        let Some(partial) = parse_autocomplete_token(&raw) else {
            return false;
        };
        match self.autocomplete_matches(&partial).into_iter().next() {
            Some((token, _)) => {
                self.accept_autocomplete(&token);
                true
            }
            None => false,
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
        let items = answer_to_items(self.mtm(), &la.answer);
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
        if let Action::Autocomplete { token } = action {
            self.accept_autocomplete(&token);
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
        if let Action::PickColor = action {
            self.run_pick_color();
            return;
        }
        if let Action::MenuPick { pid, path } = action {
            if let Some(id) = &id {
                ivars.frecency.record(id);
            }
            self.run_menu_pick(pid, &path);
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

    fn run_pick_color(&self) {
        let max = self.ivars().color_max_recent;
        self.hide_and_reset();
        match color_pick::pick_color_interactive() {
            Ok(hex) => {
                color_pick::push_recent(&hex, max);
                clipboard::set_clipboard(&hex);
                model::notify("Color picked", &color_pick::format_color_detail(&hex));
            }
            Err(msg) => model::notify("Color pick", &msg),
        }
    }

    fn run_menu_pick(&self, pid: i32, path: &[String]) {
        if !window::trusted(false) {
            window::trusted(true);
            self.show_status_row(
                "Accessibility permission needed",
                "Grant litecast access in System Settings, then try again",
                Action::Open(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
                        .to_string(),
                ),
            );
            return;
        }
        match menu_ax::press_menu_path(pid, path) {
            Ok(()) => self.hide_and_reset(),
            Err(msg) => self.show_status_row("Menu command failed", &msg, Action::None),
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
            items.extend(answer_to_items(self.mtm(), &answer.content));
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
                answer_to_items(self.mtm(), &answer)
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
        if !ivars.critters_enabled || !ivars.visible.get() {
            return;
        }
        let idx = ivars.critter_idx.get();
        ivars.critter_idx.set(idx.wrapping_add(1));

        const SIZE: f64 = 30.0;
        // Sit just above the rounded bottom corners so the critter is never
        // clipped and reads clearly along the bottom band.
        const WALK_Y: f64 = 6.0;
        let start = NSRect::new(NSPoint::new(-SIZE, WALK_Y), NSSize::new(SIZE, SIZE));
        let view: *const AnyObject = if ivars.critter_images.is_empty() {
            let glyph = DEFAULT_CRITTERS[idx % DEFAULT_CRITTERS.len()];
            let label = &ivars.critter_label;
            label.setStringValue(&NSString::from_str(glyph));
            label.setFrame(start);
            label.setHidden(false);
            // Bring to the front so it renders over the results area.
            if let Some(sv) = unsafe { label.superview() } {
                sv.addSubview(label);
            }
            &*ivars.critter_label as *const NSTextField as *const AnyObject
        } else {
            let image = &ivars.critter_images[idx % ivars.critter_images.len()];
            let view = &ivars.critter_view;
            view.setImage(Some(image));
            view.setAnimates(true);
            view.setHidden(false);
            view.setFrame(start);
            if let Some(sv) = unsafe { view.superview() } {
                sv.addSubview(view);
            }
            &*ivars.critter_view as *const NSImageView as *const AnyObject
        };

        let duration = 6.0;
        unsafe {
            let obj: &AnyObject = &*view;
            NSAnimationContext::beginGrouping();
            let ctx = NSAnimationContext::currentContext();
            ctx.setDuration(duration);
            let animator: Retained<AnyObject> = msg_send![obj, animator];
            let _: () =
                msg_send![&animator, setFrameOrigin: NSPoint::new(PANEL_WIDTH + SIZE, WALK_Y)];
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
    fn layout(&self, _rows: usize) {
        let ivars = self.ivars();
        // Sum the actual per-row heights (AI answer blocks are taller than a
        // normal row), capped so the panel never grows unbounded; overflow
        // scrolls within the results band.
        let (results_h, has_rows) = {
            let results = ivars.results.borrow();
            if results.is_empty() {
                (0.0, false)
            } else {
                let total: f64 = results.iter().map(row_height_for).sum();
                (total.min(MAX_RESULTS_H), true)
            }
        };
        // The results block adds symmetric breathing room: a small gap under the
        // separator and bottom padding so the last row clears the rounded corner.
        let results_block = if has_rows {
            RESULTS_BOTTOM_PAD + results_h + RESULTS_TOP_GAP
        } else {
            0.0
        };
        // Vertical bands, top to bottom: search area, category chip row, the
        // hairline separator, then the results block. The chip row is always
        // present, so it always contributes to the panel height.
        let total_h = SEARCH_AREA_H + CHIP_ROW_H + results_block;
        // Top of the results block / where the separator and chip row sit.
        let band_bottom = results_block;

        let mtm = self.mtm();
        let (x, top) = if let Some(screen) = screen_for_panel(mtm, ivars.prev_app_pid.get()) {
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

        // Spotlight-style category chip row in its own band, just under the
        // search field. Highlight reflects the active filter (driven equally by
        // clicks, Tab cycling, and @prefix typing).
        let active = ivars.active_filter.get();
        let chip_y = (band_bottom + (CHIP_ROW_H - CHIP_H) / 2.0).round();
        let mut chip_x = SIDE_INSET;
        for (i, chip) in ivars.chips.iter().enumerate() {
            let filter = Filter::CYCLE[i];
            style_chip(chip, filter.label(), filter == active);
            chip.sizeToFit();
            let chip_w = chip.frame().size.width.round();
            chip.setFrame(NSRect::new(
                NSPoint::new(chip_x, chip_y),
                NSSize::new(chip_w, CHIP_H),
            ));
            chip_x += chip_w + CHIP_GAP;
        }

        // Spotlight-style search pill, centered in the top search band (which
        // sits above the chip row). The pill spans the shared side margins; the
        // magnifier glyph and the text field are vertically centered inside it.
        let search_area_bottom = band_bottom + CHIP_ROW_H;
        let pill_x = SIDE_INSET;
        let pill_w = PANEL_WIDTH - 2.0 * SIDE_INSET;
        let pill_y = (search_area_bottom + (SEARCH_AREA_H - SEARCH_PILL_H) / 2.0).round();
        ivars.search_bg.setFrame(NSRect::new(
            NSPoint::new(pill_x, pill_y),
            NSSize::new(pill_w, SEARCH_PILL_H),
        ));

        let icon_x = pill_x + SEARCH_PILL_PAD_X;
        let icon_y = (pill_y + (SEARCH_PILL_H - SEARCH_ICON) / 2.0).round();
        ivars.search_icon.setFrame(NSRect::new(
            NSPoint::new(icon_x, icon_y),
            NSSize::new(SEARCH_ICON, SEARCH_ICON),
        ));

        // Text field sized to its exact text height and centered in the pill, so
        // the text lands on the pill's vertical midline.
        let search_h = line_height(SEARCH_FONT_SIZE, false);
        let text_x = icon_x + SEARCH_ICON + SEARCH_ICON_GAP;
        let search_y = (pill_y + (SEARCH_PILL_H - search_h) / 2.0).round();
        let search_w = (pill_x + pill_w - SEARCH_PILL_PAD_X - text_x).max(40.0);
        ivars.search.setFrame(NSRect::new(
            NSPoint::new(text_x, search_y),
            NSSize::new(search_w, search_h),
        ));

        // Hairline separator on the boundary between the search area and results.
        let separator_frame = NSRect::new(
            NSPoint::new(SIDE_INSET, band_bottom),
            NSSize::new(PANEL_WIDTH - 2.0 * SIDE_INSET, 1.0),
        );
        ivars.separator.setFrame(separator_frame);
        ivars.separator.setHidden(!has_rows);

        // Results scroll view, inset above the bottom padding so the last row
        // is fully visible and clears the rounded corners.
        let scroll_frame = NSRect::new(
            NSPoint::new(0.0, RESULTS_BOTTOM_PAD),
            NSSize::new(PANEL_WIDTH, results_h),
        );
        ivars.scroll.setFrame(scroll_frame);
        ivars.scroll.setHidden(!has_rows);
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

/// Render an AI answer as a SINGLE rounded "answer card" item: the whole reply
/// is shown in one wrapping text view inside a rounded container, sized exactly
/// to its wrapped height (measured here on the main thread). Enter (or the
/// card's copy button) copies the full original answer.
fn answer_to_items(mtm: MainThreadMarker, answer: &str) -> Vec<Item> {
    let full = answer.trim().to_string();
    let body = clean_answer_text(&full);
    let display = if body.is_empty() {
        "(empty response)".to_string()
    } else {
        body
    };
    let text_h = measure_answer_text_height(mtm, &display);
    let card_h = (text_h + 2.0 * CARD_PAD).max(CARD_MIN_H);
    let block_h = (card_h + 2.0 * CARD_MARGIN_Y).ceil();
    let mut item = Item::new(display, "", "AI", 0, Action::CopyText(full));
    item.multiline = true;
    item.block_height = Some(block_h);
    vec![item]
}

/// Width available to the wrapping answer text inside the card (used for both
/// measuring the height and laying out the label so they match exactly).
fn answer_text_width() -> f64 {
    let card_w = PANEL_WIDTH - 2.0 * CARD_MARGIN_X;
    (card_w - ANSWER_TEXT_LEFT - CARD_PAD - ANSWER_TEXT_RIGHT_RESERVE).max(80.0)
}

/// Measure the wrapped height of the answer text at the card's text width by
/// laying out an off-screen multi-line label and reading its fitting size.
fn measure_answer_text_height(mtm: MainThreadMarker, text: &str) -> f64 {
    let text_w = answer_text_width();
    let label = make_answer_label(mtm, text);
    label.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(text_w, 10_000.0),
    ));
    unsafe {
        let _: () = msg_send![&*label, setPreferredMaxLayoutWidth: text_w];
    }
    let size: NSSize = unsafe { msg_send![&*label, fittingSize] };
    size.height.ceil().max(line_height(ANSWER_FONT_SIZE, false))
}

/// Build the wrapping, multi-line label used for an AI answer card.
fn make_answer_label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = make_label(
        mtm,
        text,
        ANSWER_FONT_SIZE,
        weight_regular(),
        &NSColor::labelColor(),
    );
    label.setUsesSingleLineMode(false);
    label.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    label.setMaximumNumberOfLines(0);
    label
}

/// Height a result row occupies. Normal rows are a fixed `ROW_H`; an AI answer
/// card uses the height precomputed (and measured) in `answer_to_items`.
fn row_height_for(item: &Item) -> f64 {
    if item.multiline {
        item.block_height.unwrap_or(ROW_H)
    } else {
        ROW_H
    }
}

/// Lightly de-markdown an answer so it reads as clean text: strip `#` headings,
/// turn `-`/`*` bullets into `•`, drop `**`/`` ` `` emphasis, and collapse runs
/// of blank lines to a single separator. Soft wrapping is left to the text view.
fn clean_answer_text(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut prev_blank = true; // suppress leading blank lines
    for raw in text.lines() {
        let line = strip_inline_markdown(raw);
        if line.trim().is_empty() {
            if !prev_blank {
                out.push(String::new());
            }
            prev_blank = true;
            continue;
        }
        prev_blank = false;
        out.push(line);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Strip the most common markdown markers so an answer renders as clean plain
/// text: `#` headings, `-`/`*` bullets (kept as `•`), and `**`/`` ` `` emphasis.
fn strip_inline_markdown(line: &str) -> String {
    let trimmed = line.trim_start();
    // Headings: drop the leading `#` run.
    let without_heading = trimmed.trim_start_matches('#').trim_start();
    let without_heading = if without_heading.len() != trimmed.len() {
        without_heading.to_string()
    } else {
        trimmed.to_string()
    };
    // List bullets -> a single bullet glyph.
    let bulleted = match without_heading
        .strip_prefix("- ")
        .or_else(|| without_heading.strip_prefix("* "))
        .or_else(|| without_heading.strip_prefix("+ "))
    {
        Some(rest) => format!("\u{2022} {rest}"),
        None => without_heading,
    };
    // Emphasis / inline-code markers.
    bulleted.replace("**", "").replace("__", "").replace('`', "")
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

/// Parse a partial `@token` the user is still typing (no whitespace yet),
/// returning the lowercase text after `@`. `@` alone yields `""` (list all);
/// once a space follows the token this returns `None` (normal query handling).
fn parse_autocomplete_token(raw: &str) -> Option<String> {
    let rest = raw.trim_start().strip_prefix('@')?;
    if rest.contains(char::is_whitespace) {
        return None;
    }
    Some(rest.to_ascii_lowercase())
}

/// The canonical `@token` (without `@`) for a category filter.
fn filter_token(filter: Filter) -> &'static str {
    match filter {
        Filter::All => "all",
        Filter::Apps => "apps",
        Filter::Files => "files",
        Filter::Clip => "clip",
        Filter::Calc => "calc",
        Filter::Web => "web",
        Filter::Cmd => "cmd",
        Filter::Emoji => "emoji",
        Filter::Ai => "ai",
    }
}

/// Build the `@`-shortcut suggestion list: category filters first, then app
/// commands (built-ins + user-defined).
fn build_autocomplete(app_commands: &[AppCommandConfig]) -> Vec<ShortcutEntry> {
    let mut out: Vec<ShortcutEntry> = Vec::new();
    for filter in Filter::CYCLE {
        if filter == Filter::All {
            continue;
        }
        out.push(ShortcutEntry {
            token: filter_token(filter).to_string(),
            desc: format!("Filter: {}", filter.label()),
        });
    }
    for cmd in app_commands {
        let desc = if !cmd.subtitle.is_empty() {
            cmd.subtitle.clone()
        } else if !cmd.name.is_empty() {
            cmd.name.clone()
        } else {
            format!("Run @{}", cmd.keyword)
        };
        out.push(ShortcutEntry {
            token: cmd.keyword.clone(),
            desc,
        });
    }
    out
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
    let delta = (line_height(SEARCH_FONT_SIZE, false) - line_height(PLACEHOLDER_FONT_SIZE, false)) / 2.0;
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

/// Build a Spotlight-style category chip button. Tagged with its index into
/// `Filter::CYCLE` so the click handler can recover which filter it selects.
fn make_chip(mtm: MainThreadMarker, idx: usize, label: &str) -> Retained<NSButton> {
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, CHIP_H)),
    );
    button.setButtonType(NSButtonType::MomentaryChange);
    button.setBezelStyle(NSBezelStyle::FlexiblePush);
    button.setBordered(true);
    button.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    button.setTitle(&NSString::from_str(label));
    button.setTag(idx as isize);
    button.setFocusRingType(NSFocusRingType::None);
    button.setWantsLayer(true);
    if let Some(layer) = button.layer() {
        unsafe {
            let _: () = msg_send![&*layer, setCornerRadius: CHIP_H / 2.0];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
        }
    }
    button
}

/// Restyle a category chip for the current active state: an accent pill when
/// active, a subtle (but clearly interactive) fill otherwise.
fn accent_selection_fill() -> Retained<NSColor> {
    NSColor::controlAccentColor().colorWithAlphaComponent(0.18)
}

fn accent_chip_active_fill() -> Retained<NSColor> {
    NSColor::controlAccentColor().colorWithAlphaComponent(0.90)
}

fn accent_chip_idle_fill() -> Retained<NSColor> {
    NSColor::labelColor().colorWithAlphaComponent(0.08)
}

/// Screen for panel placement: cursor screen, then prev-app screen, then main.
fn screen_for_panel(mtm: MainThreadMarker, prev_pid: i32) -> Option<Retained<NSScreen>> {
    let point = NSEvent::mouseLocation();
    if let Some(s) = screen_containing_point(mtm, point) {
        return Some(s);
    }
    if prev_pid > 0 {
        if let Some(point) = window::window_center_cocoa(mtm, prev_pid) {
            if let Some(s) = screen_containing_point(mtm, point) {
                return Some(s);
            }
        }
    }
    NSScreen::mainScreen(mtm)
}

fn screen_containing_point(mtm: MainThreadMarker, point: NSPoint) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    for i in 0..screens.count() {
        let s = screens.objectAtIndex(i);
        let f = s.frame();
        if point.x >= f.origin.x
            && point.x < f.origin.x + f.size.width
            && point.y >= f.origin.y
            && point.y < f.origin.y + f.size.height
        {
            return Some(s);
        }
    }
    None
}

fn style_chip(chip: &NSButton, label: &str, active: bool) {
    let (text_color, bg) = if active {
        (
            NSColor::alternateSelectedControlTextColor(),
            accent_chip_active_fill(),
        )
    } else {
        (
            NSColor::secondaryLabelColor(),
            accent_chip_idle_fill(),
        )
    };
    chip.setBezelColor(Some(&bg));
    chip.setAttributedTitle(&chip_attr_title(label, &text_color, active));
}

/// Build a chip's attributed title (12pt; medium weight when active).
fn chip_attr_title(label: &str, color: &NSColor, active: bool) -> Retained<NSAttributedString> {
    fn anyobj<T: std::convert::AsRef<AnyObject>>(x: &T) -> &AnyObject {
        x.as_ref()
    }
    let s = NSString::from_str(label);
    let weight = if active { weight_medium() } else { weight_regular() };
    let font = NSFont::systemFontOfSize_weight(12.0, weight);
    let keys: [&NSString; 2] = [unsafe { NSFontAttributeName }, unsafe {
        NSForegroundColorAttributeName
    }];
    let objs: [&AnyObject; 2] = [anyobj(&*font), anyobj(color)];
    let attrs = NSDictionary::from_slices(&keys, &objs);
    unsafe { NSAttributedString::new_with_attributes(&s, &attrs) }
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
        "Shortcut" => "at",
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

fn make_row_cell(
    mtm: MainThreadMarker,
    item: &Item,
    row: isize,
    target: &AppDelegate,
) -> Retained<NSView> {
    let width = PANEL_WIDTH;
    let height = row_height_for(item);
    let container = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
    );

    // AI answer card: one rounded container holding a leading sparkle accent, a
    // single wrapping text view, and a copy button in the top-right corner.
    if item.multiline {
        let card_w = width - 2.0 * CARD_MARGIN_X;
        let card_h = height - 2.0 * CARD_MARGIN_Y;
        let card = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(
                NSPoint::new(CARD_MARGIN_X, CARD_MARGIN_Y),
                NSSize::new(card_w, card_h),
            ),
        );
        card.setWantsLayer(true);
        if let Some(layer) = card.layer() {
            let fill = NSColor::labelColor().colorWithAlphaComponent(0.06);
            let border = NSColor::labelColor().colorWithAlphaComponent(0.10);
            unsafe {
                let fill_cg: *mut AnyObject = msg_send![&*fill, CGColor];
                let border_cg: *mut AnyObject = msg_send![&*border, CGColor];
                let _: () = msg_send![&*layer, setCornerRadius: 12.0_f64];
                let _: () = msg_send![&*layer, setMasksToBounds: true];
                let _: () = msg_send![&*layer, setBackgroundColor: fill_cg];
                let _: () = msg_send![&*layer, setBorderWidth: 1.0_f64];
                let _: () = msg_send![&*layer, setBorderColor: border_cg];
            }
        }

        // Leading sparkle accent, pinned at the top-left of the card content.
        let accent = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            NSRect::new(
                NSPoint::new(CARD_PAD, card_h - CARD_PAD - ANSWER_ACCENT),
                NSSize::new(ANSWER_ACCENT, ANSWER_ACCENT),
            ),
        );
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("sparkles"),
            None,
        ) {
            accent.setImage(Some(&image));
        }
        accent.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
        accent.setContentTintColor(Some(&NSColor::controlAccentColor()));
        card.addSubview(&accent);

        // Wrapping answer text, sized to the height measured in answer_to_items.
        let text_w = answer_text_width();
        let label = make_answer_label(mtm, &item.title);
        label.setFrame(NSRect::new(
            NSPoint::new(ANSWER_TEXT_LEFT, CARD_PAD),
            NSSize::new(text_w, card_h - 2.0 * CARD_PAD),
        ));
        card.addSubview(&label);

        // Copy button in the top-right corner.
        let copy = NSButton::initWithFrame(
            NSButton::alloc(mtm),
            NSRect::new(
                NSPoint::new(card_w - CARD_PAD - ANSWER_COPY, card_h - CARD_PAD - ANSWER_COPY),
                NSSize::new(ANSWER_COPY, ANSWER_COPY),
            ),
        );
        copy.setButtonType(NSButtonType::MomentaryChange);
        copy.setBordered(false);
        copy.setBezelStyle(NSBezelStyle::Circular);
        copy.setTitle(&NSString::from_str(""));
        copy.setFocusRingType(NSFocusRingType::None);
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("doc.on.doc"),
            None,
        ) {
            copy.setImage(Some(&image));
        }
        copy.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageOnly);
        copy.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        copy.setTag(row);
        unsafe {
            let obj: &AnyObject = target;
            copy.setTarget(Some(obj));
            copy.setAction(Some(sel!(copyAnswer:)));
        }
        card.addSubview(&copy);

        // Faint source attribution in the bottom-right corner (e.g. "AI").
        if let Some(tag_text) = source_tag(item.source) {
            let attr = make_label(mtm, tag_text, 10.0, weight_regular(), &NSColor::tertiaryLabelColor());
            attr.sizeToFit();
            let aw = attr.frame().size.width;
            let ah = line_height(10.0, false);
            attr.setFrame(NSRect::new(
                NSPoint::new(card_w - CARD_PAD - aw, CARD_PAD - ah * 0.5),
                NSSize::new(aw, ah),
            ));
            card.addSubview(&attr);
        }

        container.addSubview(&card);
        return container;
    }

    // Icon left edge shares the search field's left margin so everything lines
    // up on a common left edge.
    let icon_view = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        NSRect::new(
            NSPoint::new(ROW_CONTENT_LEFT, (ROW_H - ROW_ICON) / 2.0),
            NSSize::new(ROW_ICON, ROW_ICON),
        ),
    );
    if let Some(image) = row_icon(item) {
        icon_view.setImage(Some(&image));
    }
    icon_view.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
    container.addSubview(&icon_view);

    // Title and subtitle share this fixed left edge (icon inset + icon + gap).
    let text_x = ROW_CONTENT_LEFT + ROW_ICON + ROW_TEXT_GAP;

    // Compute the title/subtitle vertical layout up front so the right-side tag
    // can be aligned to the *title's* vertical center (cleaner on two-line rows
    // than centering it in the gap between the two lines).
    let title_h = line_height(15.0, true);
    let (title_y, subtitle_layout) = if item.subtitle.is_empty() {
        // Single line: center the title within the row.
        (((ROW_H - title_h) / 2.0).round(), None)
    } else {
        // Two lines: center the (title + gap + subtitle) block within the row.
        const GAP: f64 = 2.0;
        let sub_h = line_height(12.0, false);
        let block = title_h + GAP + sub_h;
        let bottom = ((ROW_H - block) / 2.0).round();
        (bottom + sub_h + GAP, Some((bottom, sub_h)))
    };
    let title_center = title_y + title_h / 2.0;

    // Optional right-aligned source tag (e.g. "App", "File"), vertically aligned
    // to the title center and inset symmetrically with the icon so it sits
    // comfortably INSIDE the rounded selection highlight (never flush/clipped).
    let mut right_edge = width - ROW_CONTENT_LEFT;
    if let Some(tag_text) = source_tag(item.source) {
        let tag = make_label(
            mtm,
            tag_text,
            11.0,
            weight_regular(),
            &NSColor::secondaryLabelColor().colorWithAlphaComponent(0.55),
        );
        tag.sizeToFit();
        let tag_w = tag.frame().size.width.ceil();
        let tag_h = line_height(11.0, false);
        let tag_x = (width - ROW_CONTENT_LEFT - tag_w).round();
        let tag_y = (title_center - tag_h / 2.0).round();
        tag.setFrame(NSRect::new(NSPoint::new(tag_x, tag_y), NSSize::new(tag_w, tag_h)));
        container.addSubview(&tag);
        right_edge = tag_x - ROW_TEXT_GAP;
    }
    let text_w = (right_edge - text_x).max(40.0);

    let title = make_label(mtm, &item.title, 15.0, weight_medium(), &NSColor::labelColor());
    title.setFrame(NSRect::new(
        NSPoint::new(text_x, title_y),
        NSSize::new(text_w, title_h),
    ));
    container.addSubview(&title);
    if let Some((bottom, sub_h)) = subtitle_layout {
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
    }

    container
}

struct PanelViews {
    panel: Retained<LcPanel>,
    search: Retained<NSTextField>,
    search_bg: Retained<NSView>,
    search_icon: Retained<NSImageView>,
    table: Retained<NSTableView>,
    scroll: Retained<NSScrollView>,
    separator: Retained<NSBox>,
    chips: Vec<Retained<NSButton>>,
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

    // Spotlight-style rounded pill background behind the search field. Sized and
    // positioned in `layout`; here we just establish its look (soft fill +
    // hairline border, fully rounded ends).
    let search_bg = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, SEARCH_PILL_H)),
    );
    search_bg.setWantsLayer(true);
    if let Some(layer) = search_bg.layer() {
        let fill = NSColor::labelColor().colorWithAlphaComponent(0.07);
        let border = NSColor::labelColor().colorWithAlphaComponent(0.10);
        unsafe {
            let fill_cg: *mut AnyObject = msg_send![&*fill, CGColor];
            let border_cg: *mut AnyObject = msg_send![&*border, CGColor];
            let _: () = msg_send![&*layer, setCornerRadius: SEARCH_PILL_H / 2.0];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
            let _: () = msg_send![&*layer, setBackgroundColor: fill_cg];
            let _: () = msg_send![&*layer, setBorderWidth: 1.0_f64];
            let _: () = msg_send![&*layer, setBorderColor: border_cg];
        }
    }

    // Leading magnifier glyph inside the pill.
    let search_icon = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SEARCH_ICON, SEARCH_ICON)),
    );
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str("magnifyingglass"),
        None,
    ) {
        search_icon.setImage(Some(&image));
    }
    search_icon.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
    search_icon.setContentTintColor(Some(&NSColor::secondaryLabelColor()));

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
    search.setFont(Some(&NSFont::systemFontOfSize(SEARCH_FONT_SIZE)));
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
    // Overlay scrollers (style = 1) float over content instead of insetting it,
    // so rows keep the full panel width and the source tag stays inside the row.
    unsafe {
        let _: () = msg_send![&*scroll, setScrollerStyle: 1_isize];
    }

    // Native hairline separator between the search field and results.
    let separator = NSBox::initWithFrame(
        NSBox::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(PANEL_WIDTH, 1.0)),
    );
    separator.setBoxType(NSBoxType::Separator);
    separator.setHidden(true);

    // Spotlight-style category chip row: one clickable pill per filter, in the
    // canonical cycle order. Targets/actions are wired up in `main` once the
    // delegate exists; styling/positioning happens in `layout`.
    let chips: Vec<Retained<NSButton>> = Filter::CYCLE
        .iter()
        .enumerate()
        .map(|(i, f)| make_chip(mtm, i, f.label()))
        .collect();

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

    // Pill background sits behind the field + icon; add it first so it draws
    // underneath them.
    effect.addSubview(&search_bg);
    effect.addSubview(&search_icon);
    effect.addSubview(&search);
    effect.addSubview(&scroll);
    effect.addSubview(&separator);
    for chip in &chips {
        effect.addSubview(chip);
    }
    effect.addSubview(&critter);
    effect.addSubview(&critter_label);
    panel.setContentView(Some(&effect));

    PanelViews {
        panel,
        search,
        search_bg,
        search_icon,
        table,
        scroll,
        separator,
        chips,
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
    // Developer tools, color/base/epoch converters, and date/time helpers all
    // live under the Calc category (keyword-gated, idle-cheap).
    engine.add(Box::new(DevToolsProvider), Filter::Calc);
    engine.add(Box::new(ConvertersProvider), Filter::Calc);
    engine.add(
        Box::new(DateTimeProvider::new(config.datetime.pairs())),
        Filter::Calc,
    );
    engine.add(Box::new(EmojiProvider), Filter::Emoji);
    engine.add(Box::new(ClipboardProvider::new(history)), Filter::Clip);
    // The "Commands" category groups user commands, quicklinks, snippets,
    // plugins, and system actions.
    engine.add(
        Box::new(CommandsProvider::new(config.commands.clone())),
        Filter::Cmd,
    );
    // App commands are `@keyword`-namespaced; `@` is not a category token, so
    // the raw `@keyword arg` query reaches this provider under `All` (and Cmd).
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
    // Script commands: executable scripts in the watched dir, parsed lazily and
    // cached (re-scanned only when the directory's mtime changes).
    engine.add(
        Box::new(ScriptsProvider::new(&config.scripts.dir)),
        Filter::Cmd,
    );
    engine.add(Box::new(SystemProvider::new()), Filter::Cmd);
    engine.add(Box::new(GitProvider::new(config.git.clone())), Filter::Cmd);
    engine.add(Box::new(TextTransformProvider), Filter::Cmd);
    engine.add(Box::new(ProgUtilsProvider), Filter::Cmd);
    engine.add(Box::new(PomodoroProvider::new(config.pomodoro.clone())), Filter::Cmd);
    engine.add(Box::new(NewFileProvider::new(config.newfile.clone())), Filter::Cmd);
    engine.add(Box::new(ColorProvider::new(config.color.clone())), Filter::Calc);
    if config.menu.enabled {
        engine.add(Box::new(MenuProvider::new(config.menu.clone())), Filter::Cmd);
    }
    // Window/tab switcher: keyword-gated (`windows`/`switch`/`tabs`); listing is
    // cached briefly and only runs osascript on a keyword match.
    engine.add(Box::new(SwitcherProvider::new()), Filter::Cmd);
    // Window management is opt-in (needs Accessibility); only surface its
    // commands when explicitly enabled in the config.
    if config.window.enabled {
        engine.add(Box::new(WindowProvider::new()), Filter::Cmd);
    }
    engine.add(Box::new(PluginProvider::new()), Filter::Cmd);
    // Process manager: keyword-gated (`kill`/`ps`); kills go through a confirm.
    engine.add(Box::new(ProcessProvider::new()), Filter::Cmd);
    // Keyword-gated utility providers (calendar/network/notes/dictionary/media).
    // Each returns immediately unless its keyword matches, so they stay cheap on
    // the default path; the ones that shell out only do so on a match.
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
    // File power actions + recent files/downloads (keyword-gated).
    engine.add(Box::new(FileActionsProvider), Filter::Files);
    engine.add(Box::new(BookmarksProvider::new()), Filter::Web);
    engine.add(
        Box::new(WebSearchProvider::new(config.web_search_url.clone())),
        Filter::Web,
    );
    engine
}

fn main() {
    eprintln!("[litecast] starting...");
    let mtm = MainThreadMarker::new().expect("main() must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let views = build_panel(mtm);
    let PanelViews {
        panel,
        search,
        search_bg,
        search_icon,
        table,
        scroll,
        separator,
        chips,
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
    // Toggle/screenshot combos are configurable via `[hotkey]`; fall back to the
    // built-in Option+Space / Option+Shift+Space when unset or unparseable.
    let toggle_hotkey = parse_hotkey_combo(&config.hotkey.toggle).unwrap_or_else(|| {
        if !config.hotkey.toggle.is_empty() {
            eprintln!(
                "[litecast] invalid [hotkey] toggle {:?}; using Option+Space",
                config.hotkey.toggle
            );
        }
        HotKey::new(Some(Modifiers::ALT), Code::Space)
    });
    let shot_hotkey = parse_hotkey_combo(&config.hotkey.screenshot).unwrap_or_else(|| {
        if !config.hotkey.screenshot.is_empty() {
            eprintln!(
                "[litecast] invalid [hotkey] screenshot {:?}; using Option+Shift+Space",
                config.hotkey.screenshot
            );
        }
        HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space)
    });
    if let Err(e) = manager.register(toggle_hotkey) {
        eprintln!(
            "[litecast] failed to register toggle hotkey {}: {e}",
            config.hotkey.toggle
        );
    }
    if let Err(e) = manager.register(shot_hotkey) {
        eprintln!(
            "[litecast] failed to register screenshot hotkey {}: {e}",
            config.hotkey.screenshot
        );
    }
    eprintln!(
        "[litecast] ready; press {} to toggle the panel (Cmd+1..9 selects a category)",
        config.hotkey.toggle
    );
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
                hotkey_actions.insert(parsed.id(), action);
            }
            Err(e) => eprintln!("[litecast] failed to register custom hotkey {}: {e}", hk.key),
        }
    }

    let (query_tx, query_rx) = mpsc::channel::<(u64, String, Filter)>();
    let pending: PendingResults = Arc::new(Mutex::new(None));
    let history = History::new(50, config.clipboard.max_images);
    let frecency = Frecency::load();

    let ivars = Ivars {
        panel,
        search,
        search_bg,
        search_icon,
        table,
        scroll,
        separator,
        visible: Cell::new(false),
        results: RefCell::new(Vec::new()),
        generation: Cell::new(0),
        query_tx,
        active_filter: Cell::new(Filter::All),
        chips,
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
        critters_enabled: config.ui.critters,
        critter_view: critter,
        critter_label,
        critter_images,
        critter_idx: Cell::new(0),
        frecency: frecency.clone(),
        autocomplete: build_autocomplete(&config::merged_app_commands(&config.app_commands)),
        suppress_ghost: Cell::new(false),
        pending_confirm: Cell::new(-1),
        prev_app_pid: Cell::new(-1),
        menu_enabled: config.menu.enabled,
        color_max_recent: config.color.max_recent,
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

    // Route category-chip clicks to the delegate's `chipClicked:` handler.
    for chip in &ivars.chips {
        unsafe {
            chip.setTarget(Some(obj));
            chip.setAction(Some(sel!(chipClicked:)));
        }
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
