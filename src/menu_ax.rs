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
        walk_menu(child, path.clone(), out, max);
        if out.len() >= max {
            break;
        }
    }
}

fn find_child_by_title(parent: AXUIElementRef, title: &str) -> Option<AXUIElementRef> {
    for child in copy_children(parent) {
        if element_title(child).as_deref() == Some(title) {
            return Some(child);
        }
    }
    None
}

fn copy_children(el: AXUIElementRef) -> Vec<AXUIElementRef> {
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
                out.push(ptr as AXUIElementRef);
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
