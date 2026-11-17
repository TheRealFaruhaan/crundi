//! What a chat asks of a drawer tab's marks (browserDrawerAnnotate,
//! browserDrawerAnnotations) and sending your marks to a chat. Ported from
//! the Slint client's src/desktop/mod.rs.

use crate::ctx as kit;
use crundi_webview::engine as browser;
use serde_json::{json, Value};

/// browserDrawerAnnotate: a chat's marks (pin, box, arrow, label), a
/// selector placed on its element (scrolled into view).
pub fn annotate_request(i: usize, key: &str, m: &Value, reply: Box<dyn FnOnce(Value)>) {
    let shapes: Vec<Value> = m.get("shapes").and_then(|s| s.as_array()).cloned().unwrap_or_default().into_iter().take(50).collect();
    if m.get("clear").and_then(|c| c.as_bool()).unwrap_or(false) {
        browser::with(|st| st.panels[i].ann.retain(|a| a.by != "claude"));
    }
    if m.get("show").and_then(|c| c.as_bool()).unwrap_or(false) {
        if let Some(id) = key.strip_prefix("drawer:") { crate::drawer::activate(id); crate::drawer::open(); }
    }
    place_shapes(i, shapes.into(), 0, reply);
}

fn place_shapes(i: usize, mut shapes: std::collections::VecDeque<Value>, drawn: usize, reply: Box<dyn FnOnce(Value)>) {
    let Some(sh) = shapes.pop_front() else {
        browser::sync(i);
        crate::drawer::render();
        let total = browser::with(|st| st.panels[i].ann.len());
        return reply(json!({ "ok": true, "drawn": drawn, "total": total }));
    };
    let num = |sh: &Value, k: &str| sh.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    let sh2 = sh.clone();
    let f = move |k: &str| num(&sh2, k);
    let kind = kit::s(&sh, "kind");
    let (k2, text, selector) = (kind.clone(), kit::s(&sh, "text"), kit::s(&sh, "selector"));
    let mk = move |x: f32, y: f32, w: f32, h: f32, to: (f32, f32)| browser::Ann { by: "claude".into(), kind: k2.clone(), x, y, w, h, to, text: text.clone(), selector: selector.clone(), ..Default::default() };
    let sel = kit::s(&sh, "selector");
    if sel.is_empty() {
        let a = mk(f("x").unwrap_or(0.0), f("y").unwrap_or(0.0), f("width").unwrap_or(0.0), f("height").unwrap_or(0.0), (f("toX").unwrap_or(0.0), f("toY").unwrap_or(0.0)));
        browser::with(|st| st.panels[i].ann.push(a));
        return place_shapes(i, shapes, drawn + 1, reply);
    }
    let code = format!("(function(s){{ var e; try {{ e=document.querySelector(s); }} catch(_) {{ return {{ error: 'bad selector' }}; }} if(!e) return null; var r=e.getBoundingClientRect(); if (r.bottom<0||r.top>innerHeight||r.right<0||r.left>innerWidth) {{ e.scrollIntoView({{block:'center',inline:'center'}}); r=e.getBoundingClientRect(); }} return {{ x: r.x, y: r.y, width: r.width, height: r.height }}; }})({})", serde_json::to_string(&sel).unwrap_or_default());
    browser::eval(i, &code, Box::new(move |r| {
        let rect = match r { Ok(v) if v.is_object() && v.get("error").is_none() => v, Ok(v) if v.get("error").is_some() => return reply(json!({ "ok": false, "error": format!("bad selector: {sel}") })), Ok(_) => return reply(json!({ "ok": false, "error": format!("No element matches {sel}") })), Err(e) => return reply(json!({ "ok": false, "error": e })) };
        let g = |k: &str| rect.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let (rx, ry, rw, rh) = (g("x"), g("y"), g("width"), g("height"));
        let (cx, cy) = (rx + rw / 2.0, ry + rh / 2.0);
        let a = match kind.as_str() {
            "box" => mk(rx, ry, rw, rh, (0.0, 0.0)),
            "arrow" => mk(f("x").unwrap_or(rx - 60.0), f("y").unwrap_or(ry - 60.0).max(0.0), 0.0, 0.0, (cx, cy)),
            _ => mk(cx, cy, 0.0, 0.0, (0.0, 0.0)),
        };
        browser::with(|st| st.panels[i].ann.push(a));
        place_shapes(i, shapes, drawn + 1, reply);
    }));
}

/// browserDrawerAnnotations: the marks (each with what is under it), and
/// the picture with them drawn in when asked.
pub fn annotations_request(i: usize, _key: &str, m: &Value, reply: Box<dyn FnOnce(Value)>) {
    let anns = browser::with(|st| st.panels[i].ann.clone());
    let pts: Vec<(f32, f32)> = anns.iter().map(browser::anchor_of).collect();
    let image = m.get("image").and_then(|v| v.as_bool()).unwrap_or(false);
    browser::describe(i, &pts, Box::new(move |els| {
        let list: Vec<Value> = anns.iter().enumerate().map(|(n, a)| {
            let mut o = browser::ann_json(n + 1, a);
            o["element"] = if a.kind == "element" && !a.element.is_null() { a.element.clone() } else { els.get(n).cloned().unwrap_or(Value::Null) };
            o
        }).collect();
        let (vw, vh) = browser::with(|st| { let (w, h, ..) = st.panels[i].view(); (w, h) });
        let (title, url) = browser::with(|st| (st.panels[i].title.clone(), st.panels[i].url.clone()));
        let base = json!({ "ok": true, "url": url, "title": title, "viewport": { "width": vw, "height": vh }, "annotations": list });
        if !image { let mut b = base; b["image"] = Value::Null; return reply(b); }
        crundi_webview::shot::composite(i, false, Box::new(move |r| {
            use base64::Engine;
            let mut b = base;
            b["image"] = r.ok().map(|p| json!(base64::engine::general_purpose::STANDARD.encode(p))).unwrap_or(Value::Null);
            reply(b)
        }));
    }));
}


/// Put the marks' text (and the picture, uploaded) into a chat's box, not
/// sent (WH brdSendToChat). Until the chats offer a way to put text into
/// another chat's composer (shared/requests.md), the words go to the
/// clipboard, with the picture's path, and a toast says so.
pub fn send_to_chat(chat: &str, text: &str, png: Option<Vec<u8>>) {
    let to_chat = |text: String| {
        kit::on_app(move |_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)));
        kit::toast_ok("Copied for the chat: paste it into its box and press Send");
    };
    match png {
        None => to_chat(text.to_string()),
        Some(png) => {
            use base64::Engine;
            let c = kit::get();
            let project = c.terminals.iter().find(|t| kit::s(t, "id") == chat).map(|t| kit::s(t, "project")).unwrap_or(c.project);
            let name = format!("browser-{}.png", browser::now_ms());
            let body = json!({ "project": project, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(&png) });
            let text = text.to_string();
            kit::call(move |api| api.post("/api/attachments/upload", &body), move |r| match r {
                Ok(v) if kit::truthy(v.get("ok").unwrap_or(&Value::Null)) && !kit::s(&v, "path").is_empty() => to_chat(format!("{text} {}", kit::s(&v, "path"))),
                Ok(v) => kit::toast_error(&format!("Could not attach: {}", Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "upload failed".into()))),
                Err(e) => kit::toast_error(&format!("Could not attach: {e}")),
            });
        }
    }
}
