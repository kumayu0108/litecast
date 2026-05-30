//! Window management via the Accessibility (AX) API.
//!
//! This is the only litecast feature that needs the **Accessibility** permission,
//! and it is strictly opt-in: the provider is registered only when
//! `[window] enabled = true`, and we never call `AXIsProcessTrustedWithOptions`
//! with the prompt option until the user actually activates a window command.
//!
//! All calls here run on the main thread (from `activate_selection`), never on
//! the query worker.

use std::ffi::c_void;

use accessibility_sys::{
    kAXErrorSuccess, kAXFocusedWindowAttribute, kAXPositionAttribute, kAXSizeAttribute,
    kAXValueTypeCGPoint, kAXValueTypeCGSize, AXIsProcessTrustedWithOptions,
    AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetAttributeValue, AXValueCreate, AXValueGetValue, AXValueRef,
};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_graphics_types::geometry::{CGPoint, CGSize};
use objc2_app_kit::NSScreen;
use objc2_foundation::MainThreadMarker;

use crate::model::WindowOp;

/// Is litecast trusted for Accessibility? With `prompt = true` this shows the
/// system permission prompt (used the first time the user runs a window
/// command); with `false` it only checks, never prompting.
pub fn trusted(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            return AXIsProcessTrustedWithOptions(std::ptr::null());
        }
        // Key string is "AXTrustedCheckOptionPrompt"; building it by value
        // avoids depending on the (unexported) framework constant.
        let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

/// Apply a window operation to `pid`'s focused window. Assumes AX trust has
/// already been verified by the caller.
pub fn apply(mtm: MainThreadMarker, pid: i32, op: WindowOp) -> Result<(), String> {
    if pid <= 0 {
        return Err("no target application (open the panel over a window first)".to_string());
    }
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return Err("cannot access the frontmost application".to_string());
    }
    let _app_guard = unsafe { CFType::wrap_under_create_rule(app as CFTypeRef) };

    let win = copy_element(app, kAXFocusedWindowAttribute)
        .ok_or_else(|| "the frontmost app has no focused window".to_string())?;
    let _win_guard = unsafe { CFType::wrap_under_create_rule(win as CFTypeRef) };

    let pos = copy_point(win, kAXPositionAttribute)
        .ok_or_else(|| "cannot read the window position".to_string())?;
    let size = copy_size(win, kAXSizeAttribute)
        .ok_or_else(|| "cannot read the window size".to_string())?;

    let (target_pos, target_size) = compute_target(mtm, op, pos, size)?;

    // Set position, then size, then position again: some apps clamp the move
    // until they have been resized, so a second position set lands it correctly.
    set_point(win, kAXPositionAttribute, target_pos)?;
    set_size(win, kAXSizeAttribute, target_size)?;
    set_point(win, kAXPositionAttribute, target_pos)?;
    Ok(())
}

fn copy_element(parent: AXUIElementRef, attr: &str) -> Option<AXUIElementRef> {
    let name = CFString::new(attr);
    let mut value: CFTypeRef = std::ptr::null();
    let err =
        unsafe { AXUIElementCopyAttributeValue(parent, name.as_concrete_TypeRef(), &mut value) };
    if err != kAXErrorSuccess || value.is_null() {
        return None;
    }
    Some(value as AXUIElementRef)
}

fn copy_point(el: AXUIElementRef, attr: &str) -> Option<CGPoint> {
    let value = copy_ax_value(el, attr)?;
    let mut p = CGPoint { x: 0.0, y: 0.0 };
    let ok = unsafe {
        AXValueGetValue(
            value,
            kAXValueTypeCGPoint,
            &mut p as *mut CGPoint as *mut c_void,
        )
    };
    unsafe { CFType::wrap_under_create_rule(value as CFTypeRef) };
    ok.then_some(p)
}

fn copy_size(el: AXUIElementRef, attr: &str) -> Option<CGSize> {
    let value = copy_ax_value(el, attr)?;
    let mut s = CGSize {
        width: 0.0,
        height: 0.0,
    };
    let ok = unsafe {
        AXValueGetValue(
            value,
            kAXValueTypeCGSize,
            &mut s as *mut CGSize as *mut c_void,
        )
    };
    unsafe { CFType::wrap_under_create_rule(value as CFTypeRef) };
    ok.then_some(s)
}

/// Copy an attribute that holds an `AXValue`, returning the +1 ref (the caller
/// must release it; `copy_point`/`copy_size` do).
fn copy_ax_value(el: AXUIElementRef, attr: &str) -> Option<AXValueRef> {
    let name = CFString::new(attr);
    let mut value: CFTypeRef = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(el, name.as_concrete_TypeRef(), &mut value) };
    if err != kAXErrorSuccess || value.is_null() {
        return None;
    }
    Some(value as AXValueRef)
}

fn set_point(el: AXUIElementRef, attr: &str, p: CGPoint) -> Result<(), String> {
    let v = unsafe { AXValueCreate(kAXValueTypeCGPoint, &p as *const CGPoint as *const c_void) };
    if v.is_null() {
        return Err("could not encode the window position".to_string());
    }
    let _guard = unsafe { CFType::wrap_under_create_rule(v as CFTypeRef) };
    set_attribute(el, attr, v as CFTypeRef)
}

fn set_size(el: AXUIElementRef, attr: &str, s: CGSize) -> Result<(), String> {
    let v = unsafe { AXValueCreate(kAXValueTypeCGSize, &s as *const CGSize as *const c_void) };
    if v.is_null() {
        return Err("could not encode the window size".to_string());
    }
    let _guard = unsafe { CFType::wrap_under_create_rule(v as CFTypeRef) };
    set_attribute(el, attr, v as CFTypeRef)
}

fn set_attribute(el: AXUIElementRef, attr: &str, value: CFTypeRef) -> Result<(), String> {
    let name = CFString::new(attr);
    let err = unsafe { AXUIElementSetAttributeValue(el, name.as_concrete_TypeRef(), value) };
    if err == kAXErrorSuccess {
        Ok(())
    } else {
        Err("the app would not let its window be moved or resized".to_string())
    }
}

/// A screen's full and content (menu-bar/Dock-excluded) frames, in Cocoa
/// coordinates (bottom-left origin).
struct ScreenRect {
    frame: Rect,
    visible: Rect,
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

fn screens(mtm: MainThreadMarker) -> Vec<ScreenRect> {
    let list = NSScreen::screens(mtm);
    let mut out = Vec::with_capacity(list.count());
    for i in 0..list.count() {
        let s = list.objectAtIndex(i);
        let f = s.frame();
        let v = s.visibleFrame();
        out.push(ScreenRect {
            frame: Rect {
                x: f.origin.x,
                y: f.origin.y,
                w: f.size.width,
                h: f.size.height,
            },
            visible: Rect {
                x: v.origin.x,
                y: v.origin.y,
                w: v.size.width,
                h: v.size.height,
            },
        });
    }
    out
}

/// Compute the target position+size in AX coordinates (top-left origin) for the
/// given op. AX uses a global top-left origin; Cocoa/`NSScreen` uses bottom-left,
/// so we do all geometry in Cocoa and flip once at the end.
fn compute_target(
    mtm: MainThreadMarker,
    op: WindowOp,
    pos: CGPoint,
    size: CGSize,
) -> Result<(CGPoint, CGSize), String> {
    let screens = screens(mtm);
    if screens.is_empty() {
        return Err("no screens available".to_string());
    }
    // The global coordinate origin is the bottom-left of the primary (menu-bar)
    // screen, whose frame origin is (0, 0).
    let primary_h = screens
        .iter()
        .find(|s| s.frame.x == 0.0 && s.frame.y == 0.0)
        .map(|s| s.frame.h)
        .unwrap_or(screens[0].frame.h);

    // Current window rect in Cocoa coordinates.
    let cur = Rect {
        x: pos.x,
        y: primary_h - pos.y - size.height,
        w: size.width,
        h: size.height,
    };
    let cx = cur.x + cur.w / 2.0;
    let cy = cur.y + cur.h / 2.0;
    let cur_idx = screens
        .iter()
        .position(|s| s.frame.contains(cx, cy))
        .unwrap_or(0);

    let target_idx = match op {
        WindowOp::NextDisplay => (cur_idx + 1) % screens.len(),
        WindowOp::PrevDisplay => (cur_idx + screens.len() - 1) % screens.len(),
        _ => cur_idx,
    };
    let vf = screens[target_idx].visible;

    let rect = match op {
        WindowOp::LeftHalf => Rect {
            x: vf.x,
            y: vf.y,
            w: vf.w / 2.0,
            h: vf.h,
        },
        WindowOp::RightHalf => Rect {
            x: vf.x + vf.w / 2.0,
            y: vf.y,
            w: vf.w / 2.0,
            h: vf.h,
        },
        // "Top" is visually the upper half: higher y in Cocoa's bottom-left space.
        WindowOp::TopHalf => Rect {
            x: vf.x,
            y: vf.y + vf.h / 2.0,
            w: vf.w,
            h: vf.h / 2.0,
        },
        WindowOp::BottomHalf => Rect {
            x: vf.x,
            y: vf.y,
            w: vf.w,
            h: vf.h / 2.0,
        },
        // Quarters: upper row is higher y in Cocoa's bottom-left space.
        WindowOp::TopLeft => Rect {
            x: vf.x,
            y: vf.y + vf.h / 2.0,
            w: vf.w / 2.0,
            h: vf.h / 2.0,
        },
        WindowOp::TopRight => Rect {
            x: vf.x + vf.w / 2.0,
            y: vf.y + vf.h / 2.0,
            w: vf.w / 2.0,
            h: vf.h / 2.0,
        },
        WindowOp::BottomLeft => Rect {
            x: vf.x,
            y: vf.y,
            w: vf.w / 2.0,
            h: vf.h / 2.0,
        },
        WindowOp::BottomRight => Rect {
            x: vf.x + vf.w / 2.0,
            y: vf.y,
            w: vf.w / 2.0,
            h: vf.h / 2.0,
        },
        WindowOp::LeftThird => Rect {
            x: vf.x,
            y: vf.y,
            w: vf.w / 3.0,
            h: vf.h,
        },
        WindowOp::CenterThird => Rect {
            x: vf.x + vf.w / 3.0,
            y: vf.y,
            w: vf.w / 3.0,
            h: vf.h,
        },
        WindowOp::RightThird => Rect {
            x: vf.x + 2.0 * vf.w / 3.0,
            y: vf.y,
            w: vf.w / 3.0,
            h: vf.h,
        },
        WindowOp::CenterTwoThirds => Rect {
            x: vf.x + vf.w / 6.0,
            y: vf.y,
            w: 2.0 * vf.w / 3.0,
            h: vf.h,
        },
        WindowOp::Maximize => vf,
        WindowOp::Center => {
            let w = cur.w.min(vf.w);
            let h = cur.h.min(vf.h);
            Rect {
                x: vf.x + (vf.w - w) / 2.0,
                y: vf.y + (vf.h - h) / 2.0,
                w,
                h,
            }
        }
        // Move to another display: keep the size (clamped) and center it there.
        WindowOp::NextDisplay | WindowOp::PrevDisplay => {
            let w = cur.w.min(vf.w);
            let h = cur.h.min(vf.h);
            Rect {
                x: vf.x + (vf.w - w) / 2.0,
                y: vf.y + (vf.h - h) / 2.0,
                w,
                h,
            }
        }
    };

    // Flip Cocoa (bottom-left) back to AX (top-left).
    let ax_pos = CGPoint {
        x: rect.x,
        y: primary_h - rect.y - rect.h,
    };
    let ax_size = CGSize {
        width: rect.w,
        height: rect.h,
    };
    Ok((ax_pos, ax_size))
}
