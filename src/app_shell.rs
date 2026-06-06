//! Menu-bar agent app shell: application menu and status item (no Dock icon).

use std::sync::Once;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::sel;
use objc2::AnyThread;
use objc2_app_kit::{
    NSApplication, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSSquareStatusItemLength, NSVariableStatusItemLength,
};
use objc2_foundation::{MainThreadMarker, NSBundle, NSSize, NSString};

use crate::debug_log;

static INSTALLED: Once = Once::new();

/// Install the main menu bar (litecast → About, Settings, Quit) and a status-item menu.
/// Idempotent: safe to call from `main` after `finishLaunching` and from
/// `applicationDidFinishLaunching`.
pub fn install(mtm: MainThreadMarker, target: &AnyObject) {
    INSTALLED.call_once(|| {
        // DEBUG-TEMP
        debug_log::log("app_shell::install", "begin app menu", "{}");
        install_app_menu(mtm, target);
        // DEBUG-TEMP
        debug_log::log("app_shell::install", "begin status item", "{}");
        install_status_item(mtm, target);
        // DEBUG-TEMP
        debug_log::log("app_shell::install", "done", "{}");
    });
}

fn install_app_menu(mtm: MainThreadMarker, target: &AnyObject) {
    debug_log::log("install_app_menu", "sharedApplication", "{}"); // DEBUG-TEMP
    let app = NSApplication::sharedApplication(mtm);
    debug_log::log("install_app_menu", "NSMenu::new bar", "{}"); // DEBUG-TEMP
    let bar = NSMenu::new(mtm);
    debug_log::log("install_app_menu", "NSMenu::new app_menu", "{}"); // DEBUG-TEMP
    let app_menu = NSMenu::new(mtm);
    debug_log::log("install_app_menu", "NSMenuItem::new app_title", "{}"); // DEBUG-TEMP
    let app_title = NSMenuItem::new(mtm);
    debug_log::log("install_app_menu", "setTitle app_title", "{}"); // DEBUG-TEMP
    app_title.setTitle(&NSString::from_str("litecast"));
    // NOTE: `app_title` is the menu-BAR item; it must NOT be added into `app_menu`.
    // Adding it here AND then calling `app_title.setSubmenu(app_menu)` below created
    // a cycle (app_menu contains app_title whose submenu is app_menu), which made
    // AppKit's setSubmenu hang forever on the main thread, freezing the run loop.

    debug_log::log("install_app_menu", "menu_item About", "{}"); // DEBUG-TEMP
    let about = menu_item(mtm, "About litecast", Some(sel!(showAbout:)), target);
    app_menu.addItem(&about);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));

    let settings = menu_item(mtm, "Settings…", Some(sel!(openPreferences:)), target);
    settings.setKeyEquivalent(&NSString::from_str(","));
    app_menu.addItem(&settings);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));

    let quit = menu_item(mtm, "Quit litecast", Some(sel!(quitApp:)), target);
    debug_log::log("install_app_menu", "quit setKeyEquivalent", "{}"); // DEBUG-TEMP
    quit.setKeyEquivalent(&NSString::from_str("q"));
    debug_log::log("install_app_menu", "quit addItem", "{}"); // DEBUG-TEMP
    app_menu.addItem(&quit);

    debug_log::log("install_app_menu", "setSubmenu", "{}"); // DEBUG-TEMP
    app_title.setSubmenu(Some(&app_menu));
    debug_log::log("install_app_menu", "bar addItem app_title", "{}"); // DEBUG-TEMP
    bar.addItem(&app_title);

    // Standard Edit menu. Without it, macOS never routes ⌘A/⌘C/⌘V/⌘X/⌘Z to the
    // first responder / field editor, so those shortcuts silently do nothing in
    // every NSTextField (Settings fields AND the launcher search field). Each
    // item uses the standard editing selector with target = nil, so AppKit
    // dispatches it down the responder chain to whichever text field is focused.
    let edit_item = NSMenuItem::new(mtm);
    edit_item.setTitle(&NSString::from_str("Edit"));
    let edit_menu = NSMenu::new(mtm);
    edit_menu.setTitle(&NSString::from_str("Edit"));

    edit_menu.addItem(&edit_menu_item(mtm, "Undo", sel!(undo:), "z"));
    edit_menu.addItem(&edit_menu_item_shift(mtm, "Redo", sel!(redo:), "z"));
    edit_menu.addItem(&NSMenuItem::separatorItem(mtm));
    edit_menu.addItem(&edit_menu_item(mtm, "Cut", sel!(cut:), "x"));
    edit_menu.addItem(&edit_menu_item(mtm, "Copy", sel!(copy:), "c"));
    edit_menu.addItem(&edit_menu_item(mtm, "Paste", sel!(paste:), "v"));
    edit_menu.addItem(&edit_menu_item(mtm, "Select All", sel!(selectAll:), "a"));

    edit_item.setSubmenu(Some(&edit_menu));
    bar.addItem(&edit_item);

    debug_log::log("install_app_menu", "setMainMenu", "{}"); // DEBUG-TEMP
    app.setMainMenu(Some(&bar));
    debug_log::log("install_app_menu", "setMainMenu done", "{}"); // DEBUG-TEMP
}

/// Edit-menu item with a ⌘<key> equivalent and target = nil (first responder).
fn edit_menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    key: &str,
) -> Retained<NSMenuItem> {
    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str(title));
    unsafe {
        item.setAction(Some(action));
        // target stays nil so the action dispatches to the first responder.
        item.setTarget(None);
    }
    item.setKeyEquivalent(&NSString::from_str(key));
    item
}

/// Edit-menu item with a ⇧⌘<key> equivalent (e.g. Redo) and target = nil.
fn edit_menu_item_shift(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    key: &str,
) -> Retained<NSMenuItem> {
    use objc2_app_kit::NSEventModifierFlags;
    let item = edit_menu_item(mtm, title, action, key);
    item.setKeyEquivalentModifierMask(
        NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
    );
    item
}

/// Menu-bar icon from the app bundle. The bundled `.icns` is full-color and
/// 1024×1024; without an explicit small size it renders as a blank status item.
fn status_item_icon() -> Option<Retained<NSImage>> {
    let bundle = NSBundle::mainBundle();
    let path = bundle.pathForResource_ofType(
        Some(&NSString::from_str("litecast")),
        Some(&NSString::from_str("icns")),
    )?;
    debug_log::log(
        "status_item_icon",
        "bundle resource",
        &format!(r#"{{"path":{:?}}}"#, path.to_string()),
    );
    let img = NSImage::initWithContentsOfFile(NSImage::alloc(), &path)?;
    // Menu-bar extra size (24pt; AppKit picks @2x from the icns).
    img.setSize(NSSize::new(24.0, 24.0));
    Some(img)
}

fn install_status_item(mtm: MainThreadMarker, target: &AnyObject) {
    debug_log::log("install_status_item", "systemStatusBar", "{}"); // DEBUG-TEMP
    let status_bar = NSStatusBar::systemStatusBar();
    let icon = status_item_icon();
    let length = if icon.is_some() {
        NSSquareStatusItemLength
    } else {
        NSVariableStatusItemLength
    };
    debug_log::log("install_status_item", "statusItemWithLength", "{}"); // DEBUG-TEMP
    let item = status_bar.statusItemWithLength(length);
    // Always show; do not use autosaveName — it can restore a user-hidden state.
    item.setVisible(true);
    debug_log::log(
        "install_status_item",
        "created",
        &format!(
            r#"{{"has_icon":{},"visible":{}}}"#,
            icon.is_some(),
            item.isVisible()
        ),
    );
    if let Some(button) = item.button(mtm) {
        if let Some(ref icon) = icon {
            button.setImage(Some(icon));
            button.setTitle(&NSString::from_str(""));
        } else {
            button.setTitle(&NSString::from_str("⌘"));
        }
        button.setToolTip(Some(&NSString::from_str("litecast")));
    }

    let menu = NSMenu::new(mtm);
    menu.addItem(&menu_item(
        mtm,
        "Show Launcher",
        Some(sel!(toggleFromHotkey)),
        target,
    ));
    menu.addItem(&menu_item(
        mtm,
        "Settings…",
        Some(sel!(openPreferences:)),
        target,
    ));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&menu_item(
        mtm,
        "Quit litecast",
        Some(sel!(quitApp:)),
        target,
    ));
    item.setMenu(Some(&menu));
    // Retain for process lifetime — dropping this `Retained` at return would
    // release our reference while the status bar still shows the item.
    std::mem::forget(item);
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<objc2::runtime::Sel>,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    debug_log::log("menu_item", "new", &format!(r#"{{"title":"{title}"}}"#)); // DEBUG-TEMP
    let item = NSMenuItem::new(mtm);
    debug_log::log("menu_item", "setTitle", &format!(r#"{{"title":"{title}"}}"#)); // DEBUG-TEMP
    item.setTitle(&NSString::from_str(title));
    if let Some(action) = action {
        unsafe {
            debug_log::log("menu_item", "setAction", &format!(r#"{{"title":"{title}"}}"#)); // DEBUG-TEMP
            item.setAction(Some(action));
            debug_log::log("menu_item", "setTarget", &format!(r#"{{"title":"{title}"}}"#)); // DEBUG-TEMP
            item.setTarget(Some(target));
            debug_log::log("menu_item", "set done", &format!(r#"{{"title":"{title}"}}"#)); // DEBUG-TEMP
        }
    }
    item
}
