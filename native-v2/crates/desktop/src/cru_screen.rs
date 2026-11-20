//! Cru's live screen (app/vendor/crundi-voice.js 2105-2300, capability
//! 'voice-screen'): what Cru put up. GET /api/voice/screen when Cru opens,
//! then the voice socket's events (`screen`, `game-command`, `kid`, `say`):
//! the PC holds a keyboard-mode socket (/api/voice/listen?keyboard=1, as the
//! web's keyboard mode does) while Cru is open, for these only; the
//! microphone's socket stays as it was.
//!
//! A screen is blocks (drawn natively, cru_ui.rs), a page (`doc`, in a frame
//! of its own: webview::frames, an opaque data: origin), a video
//! (youtube-nocookie only, in a frame), or a page with blocks above it.
//! `full`: Cru's whole view. The page talks to Cru through the frames'
//! message bridge: say, tell, picture (kid mode), game events; Cru sends it
//! the volume on load and game commands.

use crate::{cru, ctx as kit};
use crundi_webview::frames;
use serde_json::{json, Value};
use std::{cell::RefCell, sync::{atomic::{AtomicU64, Ordering}, Arc}, time::{Duration, Instant}};

/// The frame's key in webview::frames.
pub const FRAME: &str = "cru-screen";

#[derive(Default)]
pub struct Screen {
    pub screen: Option<Value>,
    /// The chat view's panel folded to its head (.cv-ov.scr-min).
    pub folded: bool,
    /// The frame's page as made (to keep it when only the blocks change).
    pub target: String,
    loads: u64,
    say_at: Option<Instant>,
    /// The page's last message (the test driver reads it).
    last: Value,
    /// The events socket's generation (a new one ends the old).
    sock: Option<Arc<AtomicU64>>,
}

thread_local! { static S: RefCell<Screen> = RefCell::new(Screen::default()); }
pub fn with<R>(f: impl FnOnce(&mut Screen) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

/// The chat view's "Fold the panel": its body hidden, or shown again.
pub fn fold() { with(|s| s.folded = !s.folded); crate::cru::render(); }

pub fn screen() -> Option<Value> { with(|s| s.screen.clone()) }

/// Is it Cru's full view (and not kid mode, which has its own)?
pub fn full() -> bool { with(|s| s.screen.as_ref().is_some_and(|x| kit::truthy(x.get("full").unwrap_or(&Value::Null)))) }

/// A video's embed address, when it is youtube-nocookie (else none).
pub fn video_url(scr: &Value) -> Option<String> {
    let src = scr.pointer("/video/embed").and_then(|v| v.as_str())?;
    if !src.starts_with("https://www.youtube-nocookie.com/embed/") { return None; }
    Some(format!("{src}{}enablejsapi=1", if src.contains('?') { '&' } else { '?' }))
}

/// The frame's page for a screen: the video's address, or the page itself
/// (with the bridge first) as a data: URL; None: blocks only.
pub fn frame_target(scr: &Value) -> Option<String> {
    if scr.get("video").is_some() { return video_url(scr); }
    let doc = scr.get("doc").and_then(|d| d.as_str())?;
    use base64::Engine as _;
    let page = format!("{}{doc}", frames::bridge_js(&["say", "tell", "picture", "event"], &["game"]));
    Some(format!("data:text/html;charset=utf-8;base64,{}", base64::engine::general_purpose::STANDARD.encode(page)))
}

/// Cru put something up (None: nothing).
pub fn show(scr: Option<Value>) {
    let target = scr.as_ref().and_then(frame_target).unwrap_or_default();
    let was = with(|s| std::mem::replace(&mut s.target, target.clone()));
    if was != target { frames::close(FRAME); with(|s| s.loads = 0); }
    with(|s| s.screen = scr.filter(|v| !v.is_null()));
    cru::render();
}

/// When Cru opens: what is up now, and the events socket.
pub fn opened() {
    kit::call(|api| api.get("/api/voice/screen"), |r| if let Ok(v) = r { if v.get("screen").is_some() && v.get("ok").and_then(|o| o.as_bool()) != Some(false) { show(v.get("screen").cloned()); } });
    start_events();
}

/// When Cru closes: the frame and the socket go (the screen stays on the server).
pub fn closed() {
    frames::close(FRAME);
    with(|s| { s.target.clear(); s.loads = 0; s.screen = None; if let Some(g) = s.sock.take() { g.fetch_add(1, Ordering::Relaxed); } });
}

fn start_events() {
    let c = kit::get();
    let Some(api) = c.api else { return };
    let base = c.base.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
    let stop_gen = Arc::new(AtomicU64::new(0));
    with(|s| { if let Some(g) = s.sock.replace(stop_gen.clone()) { g.fetch_add(1, Ordering::Relaxed); } });
    std::thread::spawn(move || {
        use tungstenite::{stream::MaybeTlsStream, Message};
        let Ok(token) = api.fresh_access() else { return };
        let url = format!("{base}/api/voice/listen?keyboard=1&token={token}");
        let Ok((mut ws, _)) = tungstenite::connect(url.as_str()) else { return };
        if let MaybeTlsStream::Plain(s) = ws.get_mut() { let _ = s.set_read_timeout(Some(Duration::from_millis(200))); }
        loop {
            if stop_gen.load(Ordering::Relaxed) != 0 { let _ = ws.send(Message::text(r#"{"type":"stop"}"#)); let _ = ws.close(None); return; }
            match ws.read() {
                Ok(Message::Text(t)) => {
                    let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                    if matches!(v.get("type").and_then(|x| x.as_str()), Some("screen" | "game-command" | "kid" | "say")) { kit::main_thread(move || event(&v)); }
                }
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => return,
            }
        }
    });
}

/// A voice socket event (main thread).
pub fn event(v: &Value) {
    match kit::s(v, "type").as_str() {
        "screen" => show(v.get("screen").cloned()),
        "game-command" => if let Some(a) = v.pointer("/command/action").and_then(|x| x.as_str()) { frames::post(FRAME, &json!({ "type": "command", "action": a })); },
        "kid" => crate::cru_kid::state(v.get("kid").unwrap_or(&Value::Null)),
        // An instant line (kid mode): said if Cru is free.
        "say" => if crate::cru_kid::on() && cru::with(|c| c.phase == "listen" && c.heard.is_empty()) { let t = kit::s(v, "text"); if !t.is_empty() { cru::with(|c| c.reply = t.clone()); cru::say(&t, false); } },
        _ => {}
    }
}

/// The voice settings' volume (0-100) as 0..1.
fn volume() -> f64 { cru::with(|c| c.settings.get("volume").and_then(|v| v.as_f64()).unwrap_or(100.0)) / 100.0 }

/// Each pump: the page's messages, and the volume on each load.
pub fn pump() {
    if with(|s| s.target.is_empty()) { return; }
    let loads = frames::loads(FRAME);
    if with(|s| { let n = s.loads != loads; s.loads = loads; n }) && loads > 0 {
        frames::post(FRAME, &json!({ "cru": "volume", "value": volume(), "muted": false }));
    }
    for m in frames::take_messages(FRAME) { message(&m); }
}

fn busy() -> bool { cru::with(|c| c.phase == "speak" || c.phase == "think") }

/// A message from the page (onFrameMessage).
pub fn message(m: &Value) {
    with(|s| s.last = m.clone());
    let text = |k: &str| kit::s(m, k).chars().take(300).collect::<String>();
    match (kit::s(m, "cru").as_str(), kit::s(m, "type").as_str()) {
        // A game says a word: now, unless Cru is talking.
        ("say", _) => {
            let t = text("text");
            if t.is_empty() || busy() || with(|s| s.say_at.is_some_and(|a| a.elapsed() < Duration::from_millis(600))) { return; }
            with(|s| s.say_at = Some(Instant::now()));
            cru::say(&t, false);
        }
        ("tell", _) => tell(&text("text")),
        ("picture", _) => picture(&kit::s(m, "data"), &kit::s(m, "label")),
        (_, "game") => game(json!({ "event": m["event"], "item": m["item"], "detail": m["detail"], "streak": m["streak"] })),
        ("event", _) => {
            let kind = kit::s(m, "kind");
            game(if kind == "result" { json!({ "event": "result", "skill": m["skill"], "item": m["item"], "ok": m["ok"], "ms": m["ms"] }) }
                else { json!({ "event": match kind.as_str() { "wrong" => "miss", "level" => "level-up", k => k }, "item": m["detail"], "detail": m["detail"] }) });
        }
        _ => {}
    }
}

/// Something tapped that Cru should hear: answered like something said.
pub fn tell(t: &str) {
    if t.is_empty() || cru::with(|c| c.phase == "think") { return; }
    cru::with(|c| c.phase = "think".into());
    cru::render();
    let body = json!({ "text": t });
    kit::call(move |api| api.post("/api/voice/screen/event", &body), |r| {
        cru::with(|c| if c.phase == "think" { c.phase = "listen".into(); });
        if let Ok(v) = r {
            let (speak, show) = (kit::s(&v, "speak"), kit::s(&v, "show"));
            if kit::truthy(&v["ok"]) && (!speak.is_empty() || !show.is_empty()) {
                cru::with(|c| { c.reply = speak.clone(); c.detail = show; });
                if !speak.is_empty() { cru::say(&speak, false); }
            }
        }
        cru::render();
    });
}

/// The child shows Cru a picture (kid mode): kept on the server, then Cru looks.
fn picture(data: &str, label: &str) {
    let ok_kind = ["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/webp;base64,"].iter().any(|p| data.starts_with(p));
    if !crate::cru_kid::on() || !ok_kind || data.len() > 2_500_000 { return; }
    let label = label.to_string();
    let body = json!({ "data": data, "label": label });
    kit::call(move |api| api.post("/api/voice/kid/picture", &body), move |r| {
        if !matches!(&r, Ok(v) if kit::truthy(&v["ok"])) { return; }
        let what = if label.is_empty() { "their drawing".to_string() } else { label };
        tell(&format!("[The child tapped \"Show Cru\" to show you {what}: call look_at_picture, then tell them what you see.]"));
    });
}

/// A game event: the play log and levels, and the instant line to say.
fn game(mut ev: Value) {
    if ev.get("event").is_none_or(|e| e.is_null()) { return; }
    ev["speaking"] = json!(busy());
    kit::call(move |api| api.post("/api/voice/kid/play", &ev), |r| {
        let Ok(v) = r else { return };
        let t = v.pointer("/reflex/text").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if t.is_empty() || busy() { return; }
        crundi_webview::later(Duration::from_millis(250), move || if !busy() { cru::with(|c| c.reply = t.clone()); cru::say(&t, false); cru::render(); });
    });
}

/// The panel's ×: gone here and on every screen.
pub fn close_screen() {
    if crate::cru_kid::on() { return; }
    show(None);
    kit::call(|api| api.post("/api/voice/screen/close", &json!({})), |_| {});
}

/// Out of Cru's full view (the content stays as the panel).
pub fn normal_view() {
    let Some(mut s) = screen() else { return };
    if !full() { return; }
    if let Some(o) = s.as_object_mut() { o.remove("full"); }
    show(Some(s));
    kit::call(|api| api.post("/api/voice/screen/normal", &json!({})), |_| {});
}

/// Back to Cru's full view (not for a video, not in kid mode).
pub fn full_view() {
    let Some(mut s) = screen() else { return };
    if full() || crate::cru_kid::on() || s.get("video").is_some() { return; }
    s["full"] = json!(true);
    show(Some(s));
    kit::call(|api| api.post("/api/voice/screen/full", &json!({})), |_| {});
}

pub fn props() -> Value {
    with(|s| json!({
        "screen": s.screen, "frame": !s.target.is_empty(), "full": s.screen.as_ref().is_some_and(|x| kit::truthy(x.get("full").unwrap_or(&Value::Null))),
        "painted": frames::painted(FRAME), "loads": frames::loads(FRAME), "socket": s.sock.is_some(),
        "title": frames::title(FRAME), "lastMessage": s.last,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets() {
        assert_eq!(frame_target(&json!({ "blocks": [] })), None);
        assert!(frame_target(&json!({ "doc": "<p>hi</p>" })).unwrap().starts_with("data:text/html;charset=utf-8;base64,"));
        assert_eq!(video_url(&json!({ "video": { "embed": "https://www.youtube.com/embed/x" } })), None);
        assert_eq!(video_url(&json!({ "video": { "embed": "https://www.youtube-nocookie.com/embed/x" } })).as_deref(), Some("https://www.youtube-nocookie.com/embed/x?enablejsapi=1"));
    }
}
