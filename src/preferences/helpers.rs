//! Shared AppKit helpers for the Preferences window.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, MainThreadOnly};
use objc2_app_kit::{
    NSBezelStyle, NSButton, NSButtonType, NSPopUpButton, NSScrollView, NSTextField, NSView,
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
    b.setBezelStyle(NSBezelStyle::Rounded);
    b.setTitle(&NSString::from_str(title));
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

pub fn container(mtm: MainThreadMarker, w: f64, h: f64) -> Retained<NSView> {
    NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    )
}

pub fn scroll_wrap(mtm: MainThreadMarker, content: &NSView, w: f64, h: f64) -> Retained<NSScrollView> {
    let h_content = content.frame().size.height;
    content.setFrameSize(NSSize::new(w - 4.0, h_content.max(h)));
    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDocumentView(Some(content));
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
