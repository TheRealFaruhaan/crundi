//! Cru's push-to-talk shortcut, kept per device (the web's crundi_voice_ptt
//! in localStorage; here ptt.json in the client's config folder). Ctrl+Space
//! by default. Settings › Voice assistant › On this device changes it: the
//! next key pressed with its modifiers becomes the shortcut (Esc cancels).

use gpui::{App, Global, Keystroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ptt { pub ctrl: bool, pub alt: bool, pub shift: bool, pub meta: bool, pub key: String }

impl Default for Ptt {
    fn default() -> Self { Ptt { ctrl: true, alt: false, shift: false, meta: false, key: "space".into() } }
}

fn file() -> std::path::PathBuf { crundi_core::store::config_dir().join("ptt.json") }

/// The shortcut on this device.
pub fn get() -> Ptt { std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() }

/// Keep a shortcut (None: back to Ctrl+Space).
pub fn set(p: Option<&Ptt>) {
    match p {
        Some(p) => { let _ = std::fs::create_dir_all(crundi_core::store::config_dir()); let _ = std::fs::write(file(), serde_json::to_vec(p).unwrap_or_default()); }
        None => { let _ = std::fs::remove_file(file()); }
    }
}

/// "Ctrl+Space", "Alt+Shift+F9".
pub fn label(p: &Ptt) -> String {
    let mut parts: Vec<String> = vec![];
    if p.ctrl { parts.push("Ctrl".into()); }
    if p.alt { parts.push("Alt".into()); }
    if p.shift { parts.push("Shift".into()); }
    if p.meta { parts.push(if cfg!(target_os = "macos") { "Cmd".into() } else { "Super".into() }); }
    let mut k = p.key.clone();
    if let Some(f) = k.get(..1) { k = f.to_uppercase() + &k[1..]; }
    parts.push(k);
    parts.join("+")
}

/// The key press is the shortcut (its key with exactly its modifiers).
pub fn matches(p: &Ptt, k: &Keystroke) -> bool {
    k.key == p.key && k.modifiers.control == p.ctrl && k.modifiers.alt == p.alt && k.modifiers.shift == p.shift && k.modifiers.platform == p.meta
}

/// A key let go that ends the hold: the shortcut's key, or one of its modifiers.
pub fn releases(p: &Ptt, key: &str) -> bool {
    key == p.key || (p.ctrl && key == "control") || (p.alt && key == "alt") || (p.shift && key == "shift") || (p.meta && (key == "platform" || key == "cmd" || key == "super"))
}

/// Settings is waiting for the next key to become the shortcut.
#[derive(Default)]
pub struct Capturing(pub bool);
impl Global for Capturing {}

pub fn capturing(cx: &App) -> bool { cx.try_global::<Capturing>().is_some_and(|c| c.0) }
pub fn begin_capture(cx: &mut App) { cx.set_global(Capturing(true)); cx.refresh_windows(); }

/// A key press while capturing: a key with at least one modifier (or a
/// function key) becomes the shortcut; Esc cancels; a modifier alone waits.
/// True when the press was taken.
pub fn capture(cx: &mut App, k: &Keystroke) -> bool {
    if !capturing(cx) { return false; }
    let key = k.key.as_str();
    if matches!(key, "control" | "alt" | "shift" | "platform" | "cmd" | "super" | "fn") { return true; }
    if key == "escape" { cx.set_global(Capturing(false)); cx.refresh_windows(); return true; }
    let m = k.modifiers;
    let fkey = key.len() >= 2 && key.starts_with('f') && key[1..].chars().all(|c| c.is_ascii_digit());
    if !(m.control || m.alt || m.platform || fkey) { return true; }
    set(Some(&Ptt { ctrl: m.control, alt: m.alt, shift: m.shift, meta: m.platform, key: key.to_string() }));
    cx.set_global(Capturing(false));
    cx.refresh_windows();
    true
}

#[cfg(test)]
mod tests {
    use super::{label, Ptt};
    #[test]
    fn labels() {
        assert_eq!(label(&Ptt::default()), "Ctrl+Space");
        assert_eq!(label(&Ptt { ctrl: false, alt: true, shift: true, meta: false, key: "f9".into() }), "Alt+Shift+F9");
    }
}
