//! The desktop area (AREAS.md): the browser drawer and the Browsers tab, the native host's full browser protocol, and phase 6: notifications and the tray, the Inbox, the chat bubble, saved passwords, the homepool node, Cru.
//!
//! The registry (src/areas.rs) calls the functions below; keep their
//! signatures. Server calls go through crate::kit (kit::call, never on the
//! UI thread), the shared dialogs through kit::confirm / kit::prompt /
//! kit::menu. Pure rules go in their own files here, unit-tested.

pub mod browsers;
pub mod cru;
pub mod drawer;
pub mod homepool;
pub mod inbox;
pub mod net;
pub mod notify;
pub mod passwords;
pub mod pointer;
pub mod rules;
pub mod shot;

use crate::{areas::AreaEvent, kit, MainWindow};
use slint::ComponentHandle;
use serde_json::{json, Value};

pub fn init(w: &MainWindow) {
    drawer::init(w);
    inbox::init(w);
    browsers::init(w);
    cru::init(w);
    homepool::init();
}

pub fn signed_in() {
    let base = kit::base();
    let local = base.contains("://localhost") || base.contains("://127.0.0.1") || base.contains("://[::1]");
    drawer::with(|d| { d.mode = if local { "local".into() } else { "remote".into() }; d.status = if local { "direct".into() } else { "offline".into() }; });
    drawer::render();
}

pub fn signed_out() {
    drawer::close();
    inbox::signed_out();
}

pub fn project(alias: &str) {
    drawer::set_project(alias);
}

pub fn tab_shown(tab: &str) {
    match tab {
        "browsers" => browsers::load(),
        "inbox" => inbox::load(),
        _ => {}
    }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Connected(true) => { inbox::load(); cru::load_unread(); }
        AreaEvent::Ws("voice-report", v) => cru::report(v),
        AreaEvent::Server("inbox", v) => inbox::event(v),
        _ => {}
    }
}

// ─── The native host (src/server.rs) ───

/// Registered: the server's features are known (the drawer's network labels).
pub fn host_up() {
    let tunnel = crate::server::features().iter().any(|f| f == "net-tunnel");
    drawer::with(|d| if d.mode == "remote" { d.status = if tunnel { "tunnel".into() } else { "unsupported".into() }; });
    drawer::render();
}

pub fn host_down() {
    net::dropped();
    drawer::with(|d| if d.mode == "remote" { d.status = "offline".into(); });
    drawer::render();
}

/// A hidden browser opened or closed (the Browsers tab, when shown).
pub fn hidden_changed() {
    if kit::ui().is_some_and(|w| w.get_current_tab() == "browsers") { slint::Timer::single_shot(std::time::Duration::from_millis(500), browsers::load); }
}

/// A page's load failed: a drawer tab shows why.
pub fn load_failed(i: usize) {
    let e = crate::browser::with(|st| st.panels[i].load_error.clone()).unwrap_or_default();
    if let Some(t) = drawer::tab_of_panel(i) {
        drawer::with(|d| if let Some(x) = d.projects.values_mut().flat_map(|p| p.tabs.iter_mut()).find(|x| x.id == t.id) { x.diag = e; });
        drawer::render();
    }
}

/// browserDrawerAnnotate: a chat's marks (pin, box, arrow, label), a
/// selector placed on its element (scrolled into view).
pub fn annotate_request(i: usize, key: &str, m: &Value, reply: Box<dyn FnOnce(Value)>) {
    let shapes: Vec<Value> = m.get("shapes").and_then(|s| s.as_array()).cloned().unwrap_or_default().into_iter().take(50).collect();
    if m.get("clear").and_then(|c| c.as_bool()).unwrap_or(false) {
        crate::browser::with(|st| st.panels[i].ann.retain(|a| a.by != "claude"));
    }
    if m.get("show").and_then(|c| c.as_bool()).unwrap_or(false) {
        if let Some(id) = key.strip_prefix("drawer:") { drawer::activate(id); drawer::open(); }
    }
    place_shapes(i, shapes.into(), 0, reply);
}

fn place_shapes(i: usize, mut shapes: std::collections::VecDeque<Value>, drawn: usize, reply: Box<dyn FnOnce(Value)>) {
    let Some(sh) = shapes.pop_front() else {
        crate::browser::sync(i);
        drawer::render();
        let total = crate::browser::with(|st| st.panels[i].ann.len());
        return reply(json!({ "ok": true, "drawn": drawn, "total": total }));
    };
    let num = |sh: &Value, k: &str| sh.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    let sh2 = sh.clone();
    let f = move |k: &str| num(&sh2, k);
    let kind = kit::s(&sh, "kind");
    let (k2, text, selector) = (kind.clone(), kit::s(&sh, "text"), kit::s(&sh, "selector"));
    let mk = move |x: f32, y: f32, w: f32, h: f32, to: (f32, f32)| crate::browser::Ann { by: "claude".into(), kind: k2.clone(), x, y, w, h, to, text: text.clone(), selector: selector.clone(), ..Default::default() };
    let sel = kit::s(&sh, "selector");
    if sel.is_empty() {
        let a = mk(f("x").unwrap_or(0.0), f("y").unwrap_or(0.0), f("width").unwrap_or(0.0), f("height").unwrap_or(0.0), (f("toX").unwrap_or(0.0), f("toY").unwrap_or(0.0)));
        crate::browser::with(|st| st.panels[i].ann.push(a));
        return place_shapes(i, shapes, drawn + 1, reply);
    }
    let code = format!("(function(s){{ var e; try {{ e=document.querySelector(s); }} catch(_) {{ return {{ error: 'bad selector' }}; }} if(!e) return null; var r=e.getBoundingClientRect(); if (r.bottom<0||r.top>innerHeight||r.right<0||r.left>innerWidth) {{ e.scrollIntoView({{block:'center',inline:'center'}}); r=e.getBoundingClientRect(); }} return {{ x: r.x, y: r.y, width: r.width, height: r.height }}; }})({})", serde_json::to_string(&sel).unwrap_or_default());
    crate::browser::eval(i, &code, Box::new(move |r| {
        let rect = match r { Ok(v) if v.is_object() && v.get("error").is_none() => v, Ok(v) if v.get("error").is_some() => return reply(json!({ "ok": false, "error": format!("bad selector: {sel}") })), Ok(_) => return reply(json!({ "ok": false, "error": format!("No element matches {sel}") })), Err(e) => return reply(json!({ "ok": false, "error": e })) };
        let g = |k: &str| rect.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let (rx, ry, rw, rh) = (g("x"), g("y"), g("width"), g("height"));
        let (cx, cy) = (rx + rw / 2.0, ry + rh / 2.0);
        let a = match kind.as_str() {
            "box" => mk(rx, ry, rw, rh, (0.0, 0.0)),
            "arrow" => mk(f("x").unwrap_or(rx - 60.0), f("y").unwrap_or(ry - 60.0).max(0.0), 0.0, 0.0, (cx, cy)),
            _ => mk(cx, cy, 0.0, 0.0, (0.0, 0.0)),
        };
        crate::browser::with(|st| st.panels[i].ann.push(a));
        place_shapes(i, shapes, drawn + 1, reply);
    }));
}

/// browserDrawerAnnotations: the marks (each with what is under it), and
/// the picture with them drawn in when asked.
pub fn annotations_request(i: usize, _key: &str, m: &Value, reply: Box<dyn FnOnce(Value)>) {
    let anns = crate::browser::with(|st| st.panels[i].ann.clone());
    let pts: Vec<(f32, f32)> = anns.iter().map(crate::browser::anchor_of).collect();
    let image = m.get("image").and_then(|v| v.as_bool()).unwrap_or(false);
    crate::browser::describe(i, &pts, Box::new(move |els| {
        let list: Vec<Value> = anns.iter().enumerate().map(|(n, a)| {
            let mut o = crate::browser::ann_json(n + 1, a);
            o["element"] = if a.kind == "element" && !a.element.is_null() { a.element.clone() } else { els.get(n).cloned().unwrap_or(Value::Null) };
            o
        }).collect();
        let (vw, vh) = crate::browser::with(|st| { let (w, h, ..) = st.panels[i].view(); (w, h) });
        let (title, url) = crate::browser::with(|st| (st.panels[i].title.clone(), st.panels[i].url.clone()));
        let base = json!({ "ok": true, "url": url, "title": title, "viewport": { "width": vw, "height": vh }, "annotations": list });
        if !image { let mut b = base; b["image"] = Value::Null; return reply(b); }
        shot::composite(i, false, Box::new(move |r| {
            use base64::Engine;
            let mut b = base;
            b["image"] = r.ok().map(|p| json!(base64::engine::general_purpose::STANDARD.encode(p))).unwrap_or(Value::Null);
            reply(b)
        }));
    }));
}

/// Go to a chat (a notification's click, the Inbox's Open chat): its
/// project, the workbench, its composer.
pub fn go_to_chat(chat: &str) {
    let project = kit::terminals().unwrap_or_default().iter().find(|t| kit::s(t, "id") == chat).map(|t| kit::s(t, "project")).unwrap_or_default();
    if !project.is_empty() && !project.eq_ignore_ascii_case(&kit::project()) { crate::app::select_project(&project); }
    kit::show_tab("workbench");
    let key = format!("live:{chat}");
    crate::app::focus_composer(&key);
}

// ─── What src/browser.rs tells the drawer ───

pub fn panel_changed(i: usize) { drawer::panel_changed(i); }
pub fn visited(i: usize, url: &str) { drawer::visited(i, url); }
pub fn popup(i: usize, url: &str) -> bool { drawer::popup(i, url) }
pub fn annotating() -> bool { drawer::with(|d| d.annotating) }
pub fn ask_note(i: usize, n: usize) { drawer::ask_note(i, n); }

// ─── Homepool for the drawer (filled in by the homepool module) ───

pub fn hp_name(_id: &str) -> Option<String> { None }
pub fn hp_refresh() {}
pub fn hp_offered() -> Vec<(String, String, bool)> { vec![] }
pub fn hp_available() -> bool { false }
pub fn hp_has_key() -> bool { false }
pub fn hp_exit_up(_net: &str) -> bool { false }
pub fn exit_text(_net: &str) -> String { "checking…".into() }
pub fn hp_panel() {
    if let Some(w) = kit::ui() { use slint::ComponentHandle; w.global::<crate::DesktopDrawer>().set_hp_state("none".into()); }
}

// ─── Cru ───

pub fn cru_available() -> bool { cru::available() }

/// Put the marks' text (and the picture, uploaded) into a chat's box, not
/// sent (WH brdSendToChat).
pub fn send_to_chat(chat: &str, text: &str, png: Option<Vec<u8>>) {
    // Cru: its screen in typing mode with the words (the picture: a gap).
    if chat == "cru" {
        if !cru::props()["open"].as_bool().unwrap_or(false) { cru::open(false); }
        cru::set_keyboard(true);
        if let Some(w) = kit::ui() { w.global::<crate::DesktopCru>().set_draft(text.into()); }
        let _ = png;
        return kit::toast_ok("Added to Cru: press Enter there");
    }
    let key = format!("live:{chat}");
    match png {
        None => {
            crate::app::insert_text(&key, text);
            crate::app::focus_composer(&key);
            kit::toast_ok("Added to the chat: press Send there");
        }
        Some(png) => {
            use base64::Engine;
            let project = kit::terminals().unwrap_or_default().iter().find(|t| kit::s(t, "id") == chat).map(|t| kit::s(t, "project")).unwrap_or_else(kit::project);
            let name = format!("browser-{}.png", crate::browser::now_ms());
            let body = json!({ "project": project, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(&png) });
            let (key, text) = (key.clone(), text.to_string());
            kit::call(move |api| api.post("/api/attachments/upload", &body), move |r| match r {
                Ok(v) if kit::truthy(v.get("ok").unwrap_or(&Value::Null)) && !kit::s(&v, "path").is_empty() => {
                    crate::app::insert_text(&key, &format!("{text} {}", kit::s(&v, "path")));
                    crate::app::focus_composer(&key);
                    kit::toast_ok("Added to the chat: press Send there");
                }
                Ok(v) => kit::toast_error(&format!("Could not attach: {}", Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "upload failed".into()))),
                Err(e) => kit::toast_error(&format!("Could not attach: {e}")),
            });
        }
    }
}

/// The test driver's "desktop:<name>" actions; Some(answer) for a known one.
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = || Some(json!({ "ok": true }));
    match name {
        "ping" => Some(json!({ "ok": true, "area": "desktop" })),
        "browser" => { drawer::toggle(); ok() }
        "new-tab" => match drawer::new_tab(&kit::project(), &Some(a(0)).filter(|u| !u.is_empty()).unwrap_or_else(|| drawer::START.into()), &Some(a(1)).filter(|u| !u.is_empty()).unwrap_or_else(|| "client".into()), true, true) {
            Ok(id) => { drawer::show_active(); Some(json!({ "ok": true, "id": id })) }
            Err(e) => Some(json!({ "ok": false, "error": e })),
        },
        "close-tab" => { drawer::close_tab(&a(0)); ok() }
        "activate" => { drawer::activate(&a(0)); ok() }
        "go" => { drawer::go(&a(0)); ok() }
        "pop" => { drawer::pop_open(&a(0)); ok() }
        "pick" => { drawer::pop_pick(&a(0)); ok() }
        "annotate" => { drawer::annotate(a(0) != "false"); ok() }
        "tool" => { drawer::set_tool(&a(0)); ok() }
        "note" => { drawer::note(&a(0), a(1) != "false"); ok() }
        "send" => { drawer::send(a(0) == "shot"); ok() }
        "panel" => { drawer::open_panel(&a(0)); ok() }
        "panel-close" => { drawer::panel_close(); ok() }
        "history-query" => { drawer::history_query(&a(0)); ok() }
        "clear-run" => { drawer::clear_run(); ok() }
        "hide" => { drawer::close(); ok() }
        "close-all" => { drawer::close_all(); ok() }
        "url-typed" => { drawer::url_typed(&a(0)); ok() }
        "inbox" => { inbox::open_page(); ok() }
        // A real click on a tab's page, at page px.
        "click" => {
            let Some(i) = drawer::find(&a(0)).and_then(|(_, t)| t.panel) else { return Some(json!({ "ok": false, "error": "no page" })) };
            let r = crate::browser::mouse(i, a(1).parse().unwrap_or(0.0), a(2).parse().unwrap_or(0.0), "click");
            Some(json!({ "ok": r.is_ok() }))
        }
        "inbox-open" => { inbox::open_thread(&a(0)); ok() }
        "inbox-mark-all" => { inbox::mark_all(); ok() }
        "inbox-mark-read" => { inbox::mark_read(); ok() }
        "inbox-delete-thread" => { inbox::delete_thread(); ok() }
        "inbox-delete" => { inbox::delete_item(&a(0)); ok() }
        "inbox-open-chat" => { inbox::open_chat(); ok() }
        "browsers-open" => {
            if let Some(w) = kit::ui() { let g = w.global::<crate::DesktopBrowsers>(); if !a(0).is_empty() { g.set_url(a(0).into()); } }
            browsers::open(); ok()
        }
        "browsers-shot" => { browsers::shot(&a(0)); ok() }
        "browsers-go" => { browsers::go(&a(0), &a(1)); ok() }
        "browsers-close" => { browsers::close(&a(0)); ok() }
        "notify-click" => { notify::clicked(&a(0)); ok() }
        "cru" => { cru::toggle(); ok() }
        "cru-keyboard" => { cru::set_keyboard(a(0) != "false"); ok() }
        "cru-send" => { cru::ask(&a(0), true); ok() }
        "cru-menu" => { cru::menu_pick(&a(0)); ok() }
        "homepool-status" => Some(json!({ "ok": true, "status": homepool::status() })),
        // Run code in a tab's page; the answer comes in props.evals[token].
        "eval" => {
            let Some(i) = drawer::find(&a(0)).and_then(|(_, t)| t.panel) else { return Some(json!({ "ok": false, "error": "no page" })) };
            let token = EVALS.with(|e| { let mut e = e.borrow_mut(); e.0 += 1; e.0 });
            crate::browser::eval(i, &a(1), Box::new(move |r| {
                let v = match r { Ok(v) => json!({ "ok": true, "result": v }), Err(e) => json!({ "ok": false, "error": e }) };
                EVALS.with(|e| { e.borrow_mut().1.insert(token.to_string(), v); });
            }));
            Some(json!({ "ok": true, "token": token.to_string() }))
        }
        _ => None,
    }
}

thread_local! {
    /// The driver's evals: the last token, and the answers by token.
    static EVALS: std::cell::RefCell<(u64, serde_json::Map<String, Value>)> = std::cell::RefCell::new((0, serde_json::Map::new()));
}

/// What the test driver sees of the area ({"cmd":"area","area":"desktop"}).
pub fn props() -> Value {
    let active = kit::ui().map(|w| w.get_drawer_active()).unwrap_or(-1);
    let hidden: Vec<Value> = crate::browser::with(|st| st.panels.iter().enumerate().filter(|(_, p)| !p.closed && p.key.is_some()).map(|(i, p)| json!({ "key": p.key, "panel": i, "url": p.url })).collect());
    json!({
        "drawer": drawer::props(), "drawerActive": active, "evals": EVALS.with(|e| Value::Object(e.borrow().1.clone())),
        "inbox": inbox::props(), "browsers": browsers::props(), "notifications": notify::props(), "hidden": hidden,
        "host": { "features": crate::server::features() },
        "cru": cru::props(), "homepool": homepool::status(),
    })
}
