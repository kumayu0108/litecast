//! Menu-bar item discovery via Accessibility (opt-in, main thread only).

use accessibility_sys::{
    kAXChildrenAttribute, kAXErrorSuccess, kAXMenuBarAttribute, kAXTitleAttribute,
    AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementPerformAction,
    AXUIElementRef,
};
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::string::CFString;

/// A menu path like `["File", "Save"]` for display and activation.
#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub path: Vec<String>,
}

/// List menu-bar items for `pid` (recursive, capped).
pub fn list_menu_items(pid: i32, max: usize) -> Vec<MenuEntry> {
    if pid <= 0 {
        return Vec::new();
    }
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return Vec::new();
    }
    let _guard = unsafe { CFType::wrap_under_create_rule(app as CFTypeRef) };
    let Some(menubar) = copy_attr_element(app, kAXMenuBarAttribute) else {
        return Vec::new();
    };
    let _mb = unsafe { CFType::wrap_under_create_rule(menubar as CFTypeRef) };
    let mut out = Vec::new();
    walk_menu(menubar, Vec::new(), &mut out, max);
    out
}

/// Press a menu item by path in the target app.
pub fn press_menu_path(pid: i32, path: &[String]) -> Result<(), String> {
    if pid <= 0 || path.is_empty() {
        return Err("invalid menu path".to_string());
    }
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return Err("cannot access application".to_string());
    }
    let _guard = unsafe { CFType::wrap_under_create_rule(app as CFTypeRef) };
    let mut el = copy_attr_element(app, kAXMenuBarAttribute)
        .ok_or_else(|| "no menu bar".to_string())?;
    let _mb = unsafe { CFType::wrap_under_create_rule(el as CFTypeRef) };

    for (i, title) in path.iter().enumerate() {
        el = find_child_by_title(el, title)
            .ok_or_else(|| format!("menu item not found: {title}"))?;
        let _g = unsafe { CFType::wrap_under_create_rule(el as CFTypeRef) };
        if i < path.len() - 1 {
            let _ = unsafe {
                AXUIElementPerformAction(el, CFString::new("AXPress").as_concrete_TypeRef())
            };
        }
    }
    let err = unsafe { AXUIElementPerformAction(el, CFString::new("AXPress").as_concrete_TypeRef()) };
    if err != kAXErrorSuccess {
        return Err(format!("could not press menu item (AX error {err})"));
    }
    Ok(())
}

fn walk_menu(el: AXUIElementRef, path: Vec<String>, out: &mut Vec<MenuEntry>, max: usize) {
    if out.len() >= max {
        return;
    }
    let title = element_title(el);
    let mut path = path;
    if let Some(t) = title {
        if !t.is_empty() {
            path.push(t);
        }
    }
    if path.len() >= 2 {
        out.push(MenuEntry { path: path.clone() });
    }
    for child in copy_children(el) {
        // `child` is a retained CFType that lives until the end of this
        // iteration, so the AX ref stays valid for the whole recursive call.
        walk_menu(child.as_CFTypeRef() as AXUIElementRef, path.clone(), out, max);
        if out.len() >= max {
            break;
        }
    }
}

fn find_child_by_title(parent: AXUIElementRef, title: &str) -> Option<AXUIElementRef> {
    for child in copy_children(parent) {
        let child_ref = child.as_CFTypeRef() as AXUIElementRef;
        if element_title(child_ref).as_deref() == Some(title) {
            // Transfer ownership of the retain held by `child` to the caller,
            // which balances it with `wrap_under_create_rule`. Without the
            // forget, dropping `child` here would release the element and hand
            // back a dangling pointer (use-after-free).
            std::mem::forget(child);
            return Some(child_ref);
        }
    }
    None
}

/// Copy the child `AXUIElement`s of `el` as owned, retained `CFType` handles.
///
/// The underlying `CFArray` owns its elements and releases them when dropped;
/// returning raw element pointers would therefore be a use-after-free. We
/// instead retain each child into an owned `CFType` (via `wrap_under_get_rule`)
/// so it stays alive for as long as the caller holds the handle.
fn copy_children(el: AXUIElementRef) -> Vec<CFType> {
    let mut out = Vec::new();
    let Some(value) = copy_attr_value(el, kAXChildrenAttribute) else {
        return out;
    };
    let arr_ref = value as CFArrayRef;
    let arr = unsafe { CFArray::<*const std::ffi::c_void>::wrap_under_create_rule(arr_ref) };
    for i in 0..arr.len() {
        if let Some(item) = arr.get(i) {
            let ptr = *item;
            if !ptr.is_null() {
                out.push(unsafe { CFType::wrap_under_get_rule(ptr as CFTypeRef) });
            }
        }
    }
    out
}

fn copy_attr_element(parent: AXUIElementRef, attr: &str) -> Option<AXUIElementRef> {
    let value = copy_attr_value(parent, attr)?;
    Some(value as AXUIElementRef)
}

fn copy_attr_value(parent: AXUIElementRef, attr: &str) -> Option<CFTypeRef> {
    let name = CFString::new(attr);
    let mut value: CFTypeRef = std::ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(parent, name.as_concrete_TypeRef(), &mut value)
    };
    if err != kAXErrorSuccess || value.is_null() {
        None
    } else {
        Some(value)
    }
}

fn element_title(el: AXUIElementRef) -> Option<String> {
    let value = copy_attr_value(el, kAXTitleAttribute)?;
    let s = unsafe { CFString::wrap_under_get_rule(value as *const _) };
    unsafe { CFType::wrap_under_create_rule(value as CFTypeRef) };
    Some(s.to_string())
}
