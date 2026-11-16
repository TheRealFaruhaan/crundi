//! Saved passwords (phase 6.4): not built yet in the GPUI client (they need
//! the OS keychain). The Passwords panel says so, nothing is offered, and a
//! drawer tab's answers pass as they are.

use serde_json::Value;

pub const UNAVAILABLE: &str = "Saved passwords are not available in this client yet.";
pub fn site_menu() { crate::ctx::toast("No saved login for this site"); }
pub fn show() {}
pub fn panel_closed() {}
pub fn clear_all() {}
pub fn redact(_key: &str, v: Value) -> Value { v }
