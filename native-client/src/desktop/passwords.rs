//! Saved passwords in the drawer, with the PIN (app/drawer-passwords.js;
//! spec specs/desktop/passwords-homepool-cru.md A). Kept on this computer
//! only. Until the store lands, the drawer says they are not available.

use crate::{kit, DesktopDrawer};
use slint::ComponentHandle;

/// Why passwords cannot be kept here ("" when they can).
pub fn unavailable() -> String {
    "Saved passwords are not available in this app.".into()
}

/// How many logins are saved for a page's site (the lock button).
pub fn count_for(_url: &str) -> usize { 0 }

/// The lock button: fill the one login, or offer a choice.
pub fn site_menu() { kit::toast("No saved login for this site"); }

pub fn show() {
    if let Some(w) = kit::ui() {
        let g = w.global::<DesktopDrawer>();
        g.set_pw_filter_site(unavailable().into());
        g.set_pw_pin_open(false);
    }
}

pub fn panel_closed() {}

pub fn clear_all() {}

/// The save / update / fill bar for the shown tab.
pub fn render_bar(g: &DesktopDrawer, _tab: Option<&super::drawer::Tab>) {
    if g.get_pw_bar() != "" { g.set_pw_bar("".into()); }
}

/// A drawer tab's answer to a chat, without the tab's passwords.
pub fn redact(_key: &str, v: serde_json::Value) -> serde_json::Value { v }
