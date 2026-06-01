//! Dock-visible app shell: application menu and menu-bar status item.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{MainThreadMarker, NSString};

/// Install the main menu bar (litecast → About, Settings, Quit) and a status-item menu.
pub fn install(mtm: MainThreadMarker, target: &AnyObject) {
    install_app_menu(mtm, target);
    install_status_item(mtm, target);
}

fn install_app_menu(mtm: MainThreadMarker, target: &AnyObject) {
    let app = NSApplication::sharedApplication(mtm);
    let bar = NSMenu::new(mtm);
    let app_menu = NSMenu::new(mtm);
    let app_title = NSMenuItem::new(mtm);
    app_title.setTitle(&NSString::from_str("litecast"));
    app_menu.addItem(&app_title);

    let about = menu_item(mtm, "About litecast", Some(sel!(showAbout:)), target);
    app_menu.addItem(&about);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));

    let settings = menu_item(mtm, "Settings…", Some(sel!(openPreferences:)), target);
    unsafe {
        settings.setKeyEquivalent(&NSString::from_str(","));
    }
    app_menu.addItem(&settings);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));

    let quit = menu_item(mtm, "Quit litecast", Some(sel!(quitApp:)), target);
    unsafe {
        quit.setKeyEquivalent(&NSString::from_str("q"));
    }
    app_menu.addItem(&quit);

    app_title.setSubmenu(Some(&app_menu));
    bar.addItem(&app_title);
    app.setMainMenu(Some(&bar));
}

fn install_status_item(mtm: MainThreadMarker, target: &AnyObject) {
    let status_bar = NSStatusBar::systemStatusBar();
    let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = item.button(mtm) {
        button.setTitle(&NSString::from_str("⌘"));
        button.setToolTip(Some(&NSString::from_str("litecast")));
    }

    let menu = NSMenu::new(mtm);
    menu.addItem(&menu_item(
        mtm,
        "Open litecast",
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
    menu.addItem(&menu_item(mtm, "Quit", Some(sel!(quitApp:)), target));
    item.setMenu(Some(&menu));
    // Keep alive for process lifetime (leak is intentional; matches menu bar pattern).
    let _keep: Retained<NSStatusItem> = item;
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<objc2::runtime::Sel>,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str(title));
    if let Some(action) = action {
        unsafe {
            item.setAction(Some(action));
            item.setTarget(Some(target));
        }
    }
    item
}
