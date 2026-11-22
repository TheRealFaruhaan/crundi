//! 2.8: saved drafts (the web's saveDraft / readDraft / pruneDrafts;
//! crundi_core::drafts has the rules). Every change of a box's text is its
//! chat's draft, and the project's copy by the claim rules; they live in the
//! client's settings per server (Saved.drafts), written at most every
//! 350 ms and at quit. A composer made takes its draft back (its own, else
//! the project's unclaimed copy, with any held lines an earlier run left in
//! front); a composer that goes lets its claim go, so a new chat in the
//! project adopts it. Drafts older than 30 days are pruned when loaded.
//!
//! The drafts are an entity (its handle in a Global); no view draws them.

use super::Composer;
use crundi_core::{chat as cchat, drafts::Drafts, store};
use gpui::*;
use serde_json::{json, Value};

/// The drafts of the server signed in to.
#[derive(Default)]
pub struct Saved {
    server: String,
    d: Drafts,
    timer: bool,
    dirty: bool,
}

struct Handle(Entity<Saved>);
impl Global for Handle {}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn server(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default() }

fn entity(cx: &mut App) -> Entity<Saved> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(|_| Saved::default());
    cx.set_global(Handle(e.clone()));
    cx.on_app_quit(|cx| { flush(cx); async {} }).detach();
    e
}

/// Work on the drafts, loaded (and pruned) for the server signed in to.
fn with<R>(cx: &mut App, f: impl FnOnce(&mut Saved) -> R) -> R {
    let srv = server(cx);
    entity(cx).update(cx, |s, _| {
        if s.server != srv {
            s.server = srv.clone();
            s.d = Drafts::from_saved(store::load().drafts.get(&srv).unwrap_or(&Value::Null));
            s.d.prune(now_ms());
        }
        f(s)
    })
}

/// A chat's project (the event stream's list, else the open project).
fn alias_of(cx: &App, chat: &str) -> String {
    let s = crundi_shell::stores(cx);
    let p = s.projects.read(cx);
    p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat))
        .and_then(|t| t.get("project").and_then(|p| p.as_str()).map(String::from))
        .unwrap_or_else(|| p.open.clone())
}

/// The box's text changed (typed, sent, a choice, a give-back).
pub fn changed(c: &Composer, text: &str, cx: &mut App) {
    if crundi_shell::api(cx).is_none() { return; }
    let alias = alias_of(cx, &c.id);
    let chat = c.id.clone();
    with(cx, |s| { s.d.save(&chat, &alias, text, now_ms()); s.dirty = true; });
    write_soon(cx);
}

/// A chat's held lines (2.9), kept with the drafts so they outlive the app.
pub fn set_held(cx: &mut App, chat: &str, text: &str) {
    if crundi_shell::api(cx).is_none() { return; }
    with(cx, |s| { s.d.set_held(chat, text, now_ms()); s.dirty = true; });
    write_soon(cx);
}

/// A composer made: it takes its draft back (and any held lines before it).
pub fn restore(c: &mut Composer, window: &mut Window, cx: &mut Context<Composer>) {
    if crundi_shell::api(cx).is_none() { return; }
    let chat = c.id.clone();
    let alias = alias_of(cx, &chat);
    let now = now_ms();
    let found = with(cx, |s| {
        let mut found = s.d.restore(&chat, &alias, now);
        if let Some(h) = s.d.take_held(&chat) {
            let t = cchat::given_back(&h, found.as_deref().unwrap_or(""));
            s.d.save(&chat, &alias, &t, now);
            found = Some(t);
        }
        s.dirty = true;
        found
    });
    write_soon(cx);
    if let Some(t) = found.filter(|t| !t.is_empty()) {
        let n = t.len();
        c.input.update(cx, |st, cx| { st.set_value(t.clone(), window, cx); st.set_selected_range(n..n, cx); });
        c.last_text_set(t);
    }
}

/// A composer gone (its pane closed, the project switched): its claim on
/// the project's copy goes.
pub fn release(cx: &mut App, chat: &str) {
    if !cx.has_global::<Handle>() { return; }
    let alias = alias_of(cx, chat);
    let chat = chat.to_string();
    with(cx, |s| s.d.release(&chat, &alias));
}

fn write_soon(cx: &mut App) {
    if with(cx, |s| std::mem::replace(&mut s.timer, true)) { return; }
    super::after(cx, 350, |cx| { with(cx, |s| s.timer = false); flush(cx); });
}

/// The drafts to disk now (also at quit).
pub fn flush(cx: &mut App) {
    let Some(e) = cx.try_global::<Handle>().map(|h| h.0.clone()) else { return };
    let out = e.update(cx, |s, _| {
        if !s.dirty || s.server.is_empty() { return None; }
        s.dirty = false;
        Some((s.server.clone(), s.d.to_saved()))
    });
    if let Some((srv, v)) = out { store::update(|x| { x.drafts.insert(srv, v); }); }
}

/// savedDrafts: this server's saved drafts, as written.
pub fn state(_c: &Composer, cx: &App) -> Value {
    let v = cx.try_global::<Handle>().map(|h| h.0.read(cx).d.to_saved()).unwrap_or_else(|| json!({}));
    json!({ "savedDrafts": v })
}
