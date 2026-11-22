//! The log's part in the window (called once from the chat area's init):
//! the resume picker as the launcher's chooser (F76), and the log's driver
//! actions and state (an area of its own, "chat-log", so the chat area's
//! keys stay as they are). Actions go to the focused chat's log.

use super::LogView;
use crundi_shell::Area;
use gpui::*;
use serde_json::{json, Value};

pub fn init(cx: &mut App) {
    super::resume::init(cx);
    crundi_shell::register(cx, Area::new("chat-log")
        .test_ui(|_, name, args, window, cx| act(name, args, window, cx))
        .state(|_, _, cx| json!({ "log": state(cx) })));
}

fn focused_log(cx: &mut App) -> Option<Entity<LogView>> {
    let id = crate::focused_chat(cx);
    crate::log_of(cx, &id)
}

fn act(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    // For no chat in particular: the resume picker.
    if let Some(r) = super::resume::act(name, args, window, cx) { return Some(r); }
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    // A server event played into the focused chat (as the live link would bring it).
    if name == "chat-event" {
        let ev: Value = serde_json::from_str(&a(0)).ok()?;
        let id = crate::focused_chat(cx);
        let s = crate::find(cx, &id)?;
        super::shared::local(&s, ev, cx);
        return Some(json!({ "ok": true }));
    }
    let log = focused_log(cx)?;
    let chat = log.read(cx).chat(cx);
    if let Some(r) = super::media::act(name, args, &chat, cx) { return Some(r); }
    log.update(cx, |v, cx| -> Option<Value> {
        let ok = Some(json!({ "ok": true }));
        match name {
            "tool-toggle" => { v.toggle(&a(0), cx); return ok; }
            "think-toggle" => { super::shared::think_toggle(&v.store.clone(), &a(0), cx); return ok; }
            "log-jump" => { super::prose::go_newest(v, cx); return ok; }
            "copy-press" => return Some(json!({ "ok": super::copy::press_now(v, window, cx) })),
            "log-context" => {
                let p = point(px(a(0).parse().unwrap_or(0.)), px(a(1).parse().unwrap_or(0.)));
                v.context(&MouseDownEvent { button: MouseButton::Right, position: p, modifiers: Modifiers::default(), click_count: 1, first_mouse: false }, cx);
                return ok;
            }
            "sug-send" => { super::suggest::send(v, cx); return ok; }
            "sug-dismiss" => { super::suggest::dismiss(&v.store.clone(), cx); return ok; }
            "recap-dismiss" => { super::suggest::recap_dismiss(&v.store.clone(), cx); return ok; }
            _ => {}
        }
        super::questions::act(name, args, v, window, cx)
            .or_else(|| super::agents::act(name, args, v, cx))
            .or_else(|| super::goal::act(name, v, cx))
            .or_else(|| super::aside::act(name, args, v, cx))
            .or_else(|| super::parked::act(name, args, v, window, cx))
            .or_else(|| super::cards::act(name, args, v, window, cx))
    })
}

fn state(cx: &mut App) -> Value {
    let picker = super::resume::state(cx);
    let Some(log) = focused_log(cx) else { return json!({ "resume": picker }) };
    let v = log.read(cx);
    let st = v.store.read(cx);
    let chat = st.id.clone();
    let rows: Vec<Value> = st.rows.iter().enumerate().filter(|(_, r)| r.block.as_ref().is_none_or(|b| b.ix == 0)).map(|(i, r)| {
        let e = &r.shown;
        let mut j = json!({ "row": i, "id": e.id, "kind": e.kind, "status": e.status });
        if e.kind == "tool" {
            j["tool"] = super::tools::card_json(e, v.open.contains(&e.id));
            j["agentRow"] = json!(st.log.agent_rows.contains(&e.id));
            if !e.group.is_empty() { j["groupOpen"] = json!(v.open.contains(&format!("g:{}", e.group))); j["groupCount"] = json!(st.log.groups.get(&e.group).map_or(0, |m| m.len())); }
            let m = super::media::media_json(&chat, e, cx);
            if !m.is_null() { j["media"] = m; }
        }
        if e.kind == "thinking" { j["open"] = json!(st.log.think.open(&e.id)); }
        if let Some(h) = v.list.bounds_for_item(i) { j["h"] = json!(f32::from(h.size.height)); j["y"] = json!(f32::from(h.origin.y)); }
        j
    }).collect();
    let d = super::prose::dist(v);
    let vh = f32::from(v.list.viewport_bounds().size.height);
    json!({
        "chat": chat, "rows": rows, "following": v.list.is_following_tail(),
        "jump": { "shown": super::prose::shown(d, vh), "new": v.ui.jump.new, "dist": d },
        "copy": v.ui.copy.on.as_ref().map(|t| json!({ "what": t.what, "text": t.text, "x": f32::from(t.at.x), "y": f32::from(t.at.y), "done": v.ui.copy.done })),
        "hot": v.hot.as_ref().map(|h| json!([h.0, h.1, h.2.start, h.2.end])),
        "blink": v.ui.blink.on,
        "tail": super::tail::state(v, cx),
        // The activity row (activity.rs): its words, and whether its own view draws it (at the foot).
        "activity": v.activity(cx).map(|own| json!({ "label": super::activity::label(&st.conv.entries), "own": own })),
        "question": super::questions::state(v, cx),
        "agentDock": super::agents::state(v, cx),
        "goal": super::goal::state(v, cx),
        "suggestion": if super::suggest::shown(st) { json!(st.conv.suggestion) } else { Value::Null },
        "recap": st.conv.recap.as_ref().map(|r| json!({ "text": r.0, "why": r.1 })),
        "asides": super::aside::state(v, cx),
        "parked": super::parked::state(v, cx),
        "fr": super::cards::state(v, cx),
        "media": super::media::state(&chat, cx),
        "frames": super::frames::live(cx),
        "resume": picker,
        "menu": crundi_widgets::menu_props(cx),
        "toast": crundi_widgets::toast_text(cx).to_string(),
    })
}
