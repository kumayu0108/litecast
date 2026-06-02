//! Shared AppKit helpers for the Preferences window.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DeclaredClass, MainThreadOnly};
use objc2_app_kit::{
    NSBezelStyle, NSButton, NSButtonType, NSColor, NSEvent, NSEventModifierFlags, NSFont,
    NSPopUpButton, NSScrollView, NSTextField, NSView,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

pub const LABEL_W: f64 = 160.0;
pub const ROW_H: f64 = 28.0;
pub const PAD: f64 = 16.0;

pub fn label(mtm: MainThreadMarker, text: &str, x: f64, y: f64, w: f64) -> Retained<NSTextField> {
    let f = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H)),
    );
    f.setStringValue(&NSString::from_str(text));
    f.setBezeled(false);
    f.setBordered(false);
    f.setDrawsBackground(false);
    f.setEditable(false);
    f.setSelectable(false);
    f
}

/// Small, gray, in-app help caption shown under a control or section header.
/// Wraps across the given width/height so longer explanations stay readable.
pub fn caption(mtm: MainThreadMarker, text: &str, x: f64, y: f64, w: f64, h: f64) -> Retained<NSTextField> {
    let f = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)),
    );
    f.setStringValue(&NSString::from_str(text));
    f.setBezeled(false);
    f.setBordered(false);
    f.setDrawsBackground(false);
    f.setEditable(false);
    f.setSelectable(false);
    f.setTextColor(Some(&NSColor::secondaryLabelColor()));
    f.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    unsafe {
        // Wrap long captions instead of clipping.
        let cell: Option<Retained<objc2_app_kit::NSCell>> = msg_send![&f, cell];
        if let Some(cell) = cell {
            let _: () = msg_send![&cell, setWraps: true];
            let _: () = msg_send![&cell, setLineBreakMode: 0usize]; // word wrap
        }
    }
    f
}

pub fn field(
    mtm: MainThreadMarker,
    value: &str,
    x: f64,
    y: f64,
    w: f64,
) -> Retained<NSTextField> {
    let f = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H)),
    );
    f.setStringValue(&NSString::from_str(value));
    f.setBezeled(true);
    f.setEditable(true);
    f.setSelectable(true);
    f
}

pub fn checkbox(
    mtm: MainThreadMarker,
    title: &str,
    checked: bool,
    x: f64,
    y: f64,
    w: f64,
) -> Retained<NSButton> {
    let b = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H)),
    );
    b.setButtonType(NSButtonType::Switch);
    b.setTitle(&NSString::from_str(title));
    b.setState(if checked { 1 } else { 0 });
    b
}

pub fn button(
    mtm: MainThreadMarker,
    title: &str,
    x: f64,
    y: f64,
    w: f64,
    action: objc2::runtime::Sel,
    target: &AnyObject,
) -> Retained<NSButton> {
    let b = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H)),
    );
    b.setButtonType(NSButtonType::MomentaryPushIn);
    b.setBezelStyle(NSBezelStyle::Push);
    b.setTitle(&NSString::from_str(title));
    unsafe {
        b.setTarget(Some(target));
        b.setAction(Some(action));
    }
    b
}

/// Sidebar navigation row: a radio button. Radio buttons sharing a superview and
/// action form a single-selection group automatically, which is exactly the
/// behaviour we want for picking one settings section at a time.
pub fn nav_item(
    mtm: MainThreadMarker,
    title: &str,
    tag: isize,
    x: f64,
    y: f64,
    w: f64,
    action: objc2::runtime::Sel,
    target: &AnyObject,
) -> Retained<NSButton> {
    let b = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H)),
    );
    b.setButtonType(NSButtonType::Radio);
    b.setTitle(&NSString::from_str(title));
    b.setTag(tag);
    unsafe {
        b.setTarget(Some(target));
        b.setAction(Some(action));
    }
    b
}

pub fn str_field(f: &NSTextField) -> String {
    f.stringValue().to_string()
}

pub fn bool_field(b: &NSButton) -> bool {
    b.state() == 1
}

// A flipped NSView so document content lays out from the TOP-left and scrolls
// downward (the natural reading order for a settings pane / sidebar list).
define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcFlippedView"]
    struct FlippedView;

    impl FlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

/// A flipped container view (top-left origin).
pub fn flipped_view(mtm: MainThreadMarker, w: f64, h: f64) -> Retained<NSView> {
    let v: Retained<FlippedView> = unsafe {
        msg_send![
            FlippedView::alloc(mtm),
            initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h))
        ]
    };
    v.into_super()
}

/// Wrap `content` in a vertically-scrolling view whose content is anchored to
/// the TOP and that fills `w` x `h`, resizing with its container.
pub fn scroll_wrap(mtm: MainThreadMarker, content: &NSView, w: f64, h: f64) -> Retained<NSScrollView> {
    let natural = content.frame().size.height;
    let doc_h = natural.max(h);
    let doc = flipped_view(mtm, w, doc_h);
    content.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(w - 4.0, natural),
    ));
    doc.addSubview(content);

    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setDocumentView(Some(&doc));
    scroll
}

pub fn popup(
    mtm: MainThreadMarker,
    items: &[&str],
    selected: &str,
    x: f64,
    y: f64,
    w: f64,
) -> Retained<NSPopUpButton> {
    let pop = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H)),
        false,
    );
    for item in items {
        pop.addItemWithTitle(&NSString::from_str(item));
    }
    pop.selectItemWithTitle(&NSString::from_str(selected));
    pop
}

pub fn popup_selection(pop: &NSPopUpButton) -> String {
    pop.titleOfSelectedItem()
        .map(|s| s.to_string())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Hotkey recorder
// ---------------------------------------------------------------------------

pub struct RecorderIvars {
    /// Canonical combo string in the config format, e.g. "Cmd+Space".
    combo: RefCell<String>,
    recording: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    #[name = "LcHotkeyRecorder"]
    #[ivars = RecorderIvars]
    pub struct HotkeyRecorder;

    impl HotkeyRecorder {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            self.ivars().recording.set(true);
            self.setTitle(&NSString::from_str("Type a shortcut… (Esc cancels, ⌫ clears)"));
            if let Some(window) = self.window() {
                unsafe {
                    let _: bool = msg_send![&*window, makeFirstResponder: &*self];
                }
            }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !self.ivars().recording.get() {
                return;
            }
            let kc = event.keyCode();
            // Escape cancels; restore the previous value.
            if kc == 53 {
                self.end_recording();
                self.refresh_title();
                return;
            }
            // Delete/Backspace clears the binding.
            if kc == 51 {
                *self.ivars().combo.borrow_mut() = String::new();
                self.end_recording();
                self.refresh_title();
                return;
            }
            let mods = event.modifierFlags();
            let mut parts: Vec<&str> = Vec::new();
            if mods.contains(NSEventModifierFlags::Command) {
                parts.push("Cmd");
            }
            if mods.contains(NSEventModifierFlags::Control) {
                parts.push("Ctrl");
            }
            if mods.contains(NSEventModifierFlags::Option) {
                parts.push("Alt");
            }
            if mods.contains(NSEventModifierFlags::Shift) {
                parts.push("Shift");
            }
            let Some(key) = key_token(event) else {
                return; // dead/unsupported key: keep recording
            };
            if parts.is_empty() {
                // A bare key cannot be a global hotkey; ask for a modifier.
                self.setTitle(&NSString::from_str("Add a modifier: ⌘ ⌥ ⌃ ⇧ …"));
                return;
            }
            let mut combo = parts.join("+");
            combo.push('+');
            combo.push_str(&key);
            *self.ivars().combo.borrow_mut() = combo;
            self.end_recording();
            self.refresh_title();
        }

        #[unsafe(method(resignFirstResponder))]
        fn resign_first_responder(&self) -> bool {
            if self.ivars().recording.get() {
                self.end_recording();
                self.refresh_title();
            }
            true
        }
    }
);

impl HotkeyRecorder {
    fn end_recording(&self) {
        self.ivars().recording.set(false);
        if let Some(window) = self.window() {
            unsafe {
                let _: bool = msg_send![&*window, makeFirstResponder: std::ptr::null::<AnyObject>()];
            }
        }
    }

    fn refresh_title(&self) {
        let combo = self.ivars().combo.borrow().clone();
        let title = if combo.is_empty() {
            "Click to record…".to_string()
        } else {
            pretty_combo(&combo)
        };
        self.setTitle(&NSString::from_str(&title));
    }
}

/// Create a hotkey recorder pre-loaded with `combo` (canonical config format).
pub fn hotkey_recorder(
    mtm: MainThreadMarker,
    combo: &str,
    x: f64,
    y: f64,
    w: f64,
) -> Retained<HotkeyRecorder> {
    let this = HotkeyRecorder::alloc(mtm).set_ivars(RecorderIvars {
        combo: RefCell::new(combo.to_string()),
        recording: Cell::new(false),
    });
    let r: Retained<HotkeyRecorder> = unsafe {
        msg_send![
            super(this),
            initWithFrame: NSRect::new(NSPoint::new(x, y), NSSize::new(w, ROW_H))
        ]
    };
    r.setButtonType(NSButtonType::MomentaryPushIn);
    r.setBezelStyle(NSBezelStyle::Push);
    let title = if combo.is_empty() {
        "Click to record…".to_string()
    } else {
        pretty_combo(combo)
    };
    r.setTitle(&NSString::from_str(&title));
    r
}

/// Read the canonical combo string currently held by a recorder.
pub fn recorder_combo(r: &HotkeyRecorder) -> String {
    r.ivars().combo.borrow().clone()
}

/// Map an NSEvent to a config key token (e.g. "Space", "K", "F1").
fn key_token(event: &NSEvent) -> Option<String> {
    let token = match event.keyCode() {
        49 => "Space",
        36 => "Enter",
        48 => "Tab",
        123 => "Left",
        124 => "Right",
        125 => "Down",
        126 => "Up",
        122 => "F1",
        120 => "F2",
        99 => "F3",
        118 => "F4",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        100 => "F8",
        101 => "F9",
        109 => "F10",
        103 => "F11",
        111 => "F12",
        _ => {
            let chars = event.charactersIgnoringModifiers()?;
            let s = chars.to_string();
            let s = s.trim();
            let mut it = s.chars();
            let c = it.next()?;
            if it.next().is_some() {
                return None;
            }
            if c.is_ascii_alphanumeric() {
                return Some(c.to_ascii_uppercase().to_string());
            }
            if "-=,./\\;'`[]".contains(c) {
                return Some(c.to_string());
            }
            return None;
        }
    };
    Some(token.to_string())
}

/// Human-readable form of a canonical combo, e.g. "Cmd+Shift+Space" -> "⌘⇧Space".
fn pretty_combo(combo: &str) -> String {
    let mut out = String::new();
    let mut key = String::new();
    for (i, part) in combo.split('+').enumerate() {
        let p = part.trim();
        match p.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" | "win" | "meta" => out.push('\u{2318}'),
            "ctrl" | "control" => out.push('\u{2303}'),
            "alt" | "option" | "opt" => out.push('\u{2325}'),
            "shift" => out.push('\u{21e7}'),
            _ => {
                if i == 0 {
                    out.push_str(p);
                } else {
                    key = p.to_string();
                }
            }
        }
    }
    out.push_str(&key);
    out
}
