//! Add project, the approvals button and panel, What's new (Slint:
//! src/ops/extras.rs, ui/ops/extras*.slint, specs/ops/extras.md).
//!
//! - Add project (src/webapp-html.js showAddModal / addProject): the
//!   sidebar's "+ Add project" calls `open_add_project`. One name in the
//!   single-folder mode (GET /api/projects/config), alias + path + display
//!   name otherwise (a missing path asks to create the folder). The new
//!   project opens once the server's state lists it.
//! - Approvals (renderApprovals): the `approvals` event's items; the top
//!   bar's button (`approvals_button`, shown at one or more) toggles the
//!   panel under the bar (`approvals_panel`), which answers what it can,
//!   opens a chat or the Secrets tab for the rest, and dismisses.
//! - What's new (crundi-whatsnew.js): shown once after an install or an
//!   update (GET /api/whatsnew, 1.6 s after sign-in, never over a dialog);
//!   Skip or Done marks it seen; Settings opens it again with
//!   `open_whatsnew(true, cx)`.

use crate::panels::{s, toast, toast_error, truthy, with_window};
use crundi_shell::AreaEvent;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, time::Duration};

// ─── Pure rules ───

/// The single mode's alias from the name typed: lower case, anything but
/// a-z 0-9 _ - becomes "-", runs of "-" one, none at the ends.
pub fn alias_single(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().to_lowercase().chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' { c } else { '-' };
        if c == '-' && out.ends_with('-') { continue; }
        out.push(c);
    }
    out.trim_matches('-').to_string()
}

/// The multi mode's alias: lower case, anything but a-z 0-9 _ - dropped.
pub fn alias_multi(raw: &str) -> String {
    raw.trim().to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-').collect()
}

/// The single mode's hint: where the folder goes.
pub fn folder_hint(dir: &str) -> String {
    let sep = if dir.contains('/') { "/" } else { "\\" };
    format!("Folder: {dir}{sep}...")
}

/// An approvals item's buttons: (id, label, kind, title), Dismiss last.
pub fn acts_of(it: &Value) -> Vec<(String, String, &'static str, String)> {
    let src = s(it, "source");
    let kind = s(it, "kind");
    let host = s(it, "host");
    let a = |id: &str, label: &str, k: &'static str, tip: &str| (id.to_string(), label.to_string(), k, tip.to_string());
    let mut v = match (src.as_str(), kind.as_str()) {
        ("collab", _) if !host.is_empty() => vec![
            a("yes", "Allow once", "yes", "Only this request"),
            a("yes-session", "This session", "yes", "Until this chat closes"),
            a("yes-always", "Always", "yes", &format!("Add {host} to their allowed sites")),
            a("no", "Decline", "no", ""),
        ],
        ("collab", "command") => vec![a("yes", "Run", "yes", "Runs outside the sandbox as the server user; the output goes to their chat"), a("no", "Decline", "no", "")],
        ("collab", "tool") => vec![
            a("yes", "Allow once", "yes", "Only this request"),
            a("yes-session", "This session", "yes", "Allow this kind of request until the chat closes"),
            a("no", "Decline", "no", ""),
        ],
        ("collab", _) => vec![a("yes", "Approve", "yes", ""), a("no", "Decline", "no", "")],
        ("kid", _) | ("cru", _) => it.get("choices").and_then(|c| c.as_array()).cloned().unwrap_or_default().iter().map(|c| {
            let id = s(c, "id");
            (format!("{src}:{id}"), s(c, "label"), if id == "no" { "no" } else { "yes" }, String::new())
        }).collect(),
        ("chat", _) => vec![a("goto", "Open the chat", "go", "")],
        _ => vec![a("secrets", "Open secrets", "go", "")],
    };
    v.push(a("dismiss", "Dismiss", "dismiss", if src == "chat" { "Hide here; still answerable in the chat" } else { "Decline and remove" }));
    v
}

/// An item's title line: "<name> — <title>" when a person asked.
pub fn title_of(it: &Value) -> String {
    let nm = s(it, "name");
    let t = s(it, "title");
    if nm.is_empty() { t } else { format!("{nm} \u{2014} {t}") }
}

/// Clear all: how many would be declined (all but chat cards).
pub fn declines(items: &[Value]) -> usize { items.iter().filter(|i| s(i, "source") != "chat").count() }

/// What a decision's answer says: (toast, is an error).
pub fn decided(r: &Value, approve: bool) -> (String, bool) {
    let e = |k: &str, d: &str| { let x = r.pointer(k).and_then(|x| x.as_str()).unwrap_or(""); if x.is_empty() { d.to_string() } else { x.to_string() } };
    let okd = |k: &str| r.pointer(k).and_then(|o| o.as_bool()).unwrap_or(false);
    if !okd("/ok") { return (e("/error", "That could not be applied"), true); }
    if let Some(m) = r.get("merge").filter(|m| !m.is_null()) {
        if !okd("/merge/ok") { return (e("/merge/error", "The merge failed"), true); }
        return (format!("Merged {} into {}", s(m, "merged"), s(m, "into")), false);
    }
    if r.get("running").is_some_and(truthy) { return ("Running. The output will go to their chat.".into(), false); }
    if r.get("service").is_some_and(|x| !x.is_null()) {
        if !okd("/service/ok") { return (format!("Approved, but it could not be registered: {}", e("/service/error", "unknown error")), true); }
        return ("Service registered".into(), false);
    }
    if r.get("forward").is_some_and(|x| !x.is_null()) {
        if !okd("/forward/ok") { return (format!("Approved, but the forward could not be created: {}", e("/forward/error", "unknown error")), true); }
        return (format!("Published {}", e("/forward/forward/url", "")), false);
    }
    if r.get("tunnel").is_some_and(|x| !x.is_null()) {
        if !okd("/tunnel/ok") { return (format!("Approved, but the tunnel could not be turned on: {}", e("/tunnel/error", "unknown error")), true); }
        return ((if okd("/tunnel/running") { "Tunnel starting" } else { "Tunnel on: it starts when the service runs" }).into(), false);
    }
    ((if approve { "Approved" } else { "Declined" }).into(), false)
}

/// What's new: the step shown at `at` of `n` (the last one is the recap).
pub fn wn_step(steps: &[Value], at: usize) -> Option<&Value> { steps.get(at) }

// ─── State ───

struct Wn { version: String, steps: Vec<Value>, at: usize, all: bool }

#[derive(Default)]
struct St {
    single: bool,
    dir: String,
    /// A project just added: opened when the server's state lists it.
    select: Option<String>,
    items: Vec<Value>,
    appr_open: bool,
    add: Option<Entity<AddProject>>,
    wn: Option<Wn>,
    wn_tries: u32,
    wn_gen: u32,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }
fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|x| f(&mut x.borrow_mut())) }

// ─── Add project ───

pub struct AddProject {
    single: bool,
    hint: String,
    name: Entity<w::InputState>,
    alias: Entity<w::InputState>,
    path: Entity<w::InputState>,
    dname: Entity<w::InputState>,
    busy: bool,
    _subs: Vec<Subscription>,
}

impl AddProject {
    fn new(single: bool, hint: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mk = |ph: &'static str, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| w::InputState::new(window, cx).placeholder(ph));
        let (name, alias, path, dname) = (mk("my-project", window, cx), mk("my-project", window, cx), mk("C:\\Projects\\my-project", window, cx), mk("My Project", window, cx));
        let subs = [&name, &alias, &path, &dname].into_iter().map(|i| cx.subscribe_in(i, window, |_, _, ev: &w::InputEvent, window, cx| {
            if let w::InputEvent::PressEnter { .. } = ev { add(window, cx); }
        })).collect();
        let first = if single { name.clone() } else { alias.clone() };
        first.update(cx, |s, cx| s.focus(window, cx));
        AddProject { single, hint, name, alias, path, dname, busy: false, _subs: subs }
    }
}

impl Render for AddProject {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = |t: &'static str| w::field_label(t).pt(T::SP_8);
        div().flex().flex_col().gap(T::SP_4).p(T::SP_16)
            .child(div().pb(T::SP_12).text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Add Project"))
            .when(self.single, |d| d
                .child(label("PROJECT NAME")).child(w::input(&self.name))
                .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(self.hint.clone())))
            .when(!self.single, |d| d
                .child(label("ALIAS")).child(w::input(&self.alias))
                .child(label("PATH")).child(w::input(&self.path))
                .child(label("DISPLAY NAME (OPTIONAL)")).child(w::input(&self.dname)))
            .child(div().flex().justify_end().gap(T::SP_10).pt(T::SP_16)
                .child(w::button("ap-cancel", "Cancel").on_click(cx.listener(|_, _, window, cx| close_add(window, cx))))
                .child(w::disabled(w::primary("ap-add", "Add"), self.busy).on_click(cx.listener(|_, _, window, cx| add(window, cx)))))
            // The ask layer (the missing folder's confirm) over the dialog,
            // unless an operations tab draws it already.
    }
}

/// An operations tab is on show (it draws the ask layer).
fn crate_tab(cx: &App) -> bool {
    matches!(crundi_shell::current_tab(cx).as_str(), "panels" | "services" | "docker" | "homepool" | "info" | "secrets" | "settings")
}

fn load_config(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/projects/config"), |r, _| {
        if let Ok(v) = r { with(|st| { st.single = s(&v, "mode") == "single"; st.dir = s(&v, "projectsDir"); }); }
    });
}

/// The sidebar's "+ Add project": the dialog.
pub fn open_add_project(window: &mut Window, cx: &mut App) {
    let (single, dir) = with(|st| (st.single, st.dir.clone()));
    let view = cx.new(|cx| AddProject::new(single, folder_hint(&dir), window, cx));
    with(|st| st.add = Some(view.clone()));
    w::modal(window, cx, T::EXTRAS_ADD_W, view.into(), |_, cx| { with(|st| st.add = None); cx.refresh_windows(); });
}

fn close_add(window: &mut Window, cx: &mut App) {
    if with(|st| st.add.take()).is_some() && w::modal_open(cx) { w::close_dialog(window, cx); }
    cx.refresh_windows();
}

fn val(i: &Entity<w::InputState>, cx: &App) -> String { i.read(cx).value().to_string() }

fn set_busy(on: bool, cx: &mut App) {
    if let Some(v) = with(|st| st.add.clone()) { v.update(cx, |v, cx| { v.busy = on; cx.notify(); }); }
}

fn add(_window: &mut Window, cx: &mut App) {
    let Some(v) = with(|st| st.add.clone()) else { return };
    let (single, busy, name, alias, path, dname) = { let x = v.read(cx); (x.single, x.busy, val(&x.name, cx), val(&x.alias, cx), val(&x.path, cx), val(&x.dname, cx)) };
    if busy { return; }
    if single {
        let raw = name.trim().to_string();
        if raw.is_empty() { return toast_error(cx, "Project name is required"); }
        let a = alias_single(&raw);
        if a.is_empty() { return toast_error(cx, "Invalid project name"); }
        return register(json!({ "alias": a, "name": raw, "create": true }), "Project created", cx);
    }
    let a = alias_multi(&alias);
    let path = path.trim().to_string();
    let dn = dname.trim().to_string();
    let name = if dn.is_empty() { a.clone() } else { dn };
    if a.is_empty() || path.is_empty() { return toast_error(cx, "Alias and path are required"); }
    set_busy(true, cx);
    let p = path.clone();
    crundi_shell::call(cx, move |api| api.post("/api/projects/check-path", &json!({ "path": p })), move |r, cx| {
        set_busy(false, cx);
        // Unknown: assume it exists (the web's).
        let exists = r.ok().and_then(|v| v.get("exists").and_then(|e| e.as_bool())).unwrap_or(true);
        let body = move |create: bool| json!({ "alias": a, "path": path, "name": name, "create": create });
        if exists { return register(body(false), "Project added", cx); }
        let b = body(true);
        with_window(cx, move |window, cx| {
            w::ask_confirm(window, cx, w::Ask { title: "Add project".into(), message: "Path does not exist. Create empty folder?".into(), ok: "Create".into(), ..Default::default() }, move |yes, _, cx| {
                if yes { register(b.clone(), "Project added", cx); }
            });
        });
    });
}

fn register(body: Value, done: &'static str, cx: &mut App) {
    let alias = s(&body, "alias");
    set_busy(true, cx);
    crundi_shell::call(cx, move |api| api.post("/api/projects", &body), move |r, cx| {
        set_busy(false, cx);
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                with_window(cx, close_add);
                toast(cx, done, w::Toast::Success);
                // The server's state event brings the new list; the project
                // opens then (and a moment later at the latest).
                with(|st| st.select = Some(alias.clone()));
                cx.spawn(async move |cx: &mut AsyncApp| {
                    cx.background_executor().timer(Duration::from_millis(2500)).await;
                    let _ = cx.update(|cx| {
                        let still = with(|st| if st.select.as_deref() == Some(alias.as_str()) { st.select.take() } else { None });
                        if let Some(a) = still { crundi_shell::session::select_project(cx, &a); }
                    });
                }).detach();
            }
            Ok(v) => toast_error(cx, { let e = s(&v, "error"); if e.is_empty() { "Failed".into() } else { e } }),
            Err(e) => toast_error(cx, format!("Error: {e}")),
        }
    });
}

// ─── Approvals ───

fn load_approvals(cx: &mut App) {
    if crundi_shell::api(cx).is_none() { return; }
    crundi_shell::call(cx, |api| api.get("/api/approvals"), |r, cx| {
        if let Ok(v) = r { set_items(v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(), cx); }
    });
}

fn set_items(items: Vec<Value>, cx: &mut App) {
    with(|st| { st.items = items; if st.items.is_empty() { st.appr_open = false; } });
    cx.refresh_windows();
}

fn item(id: &str) -> Option<Value> { with(|st| st.items.iter().find(|i| s(i, "id") == id).cloned()) }

fn appr_toggle(cx: &mut App) { with(|st| st.appr_open = !st.appr_open && !st.items.is_empty()); cx.refresh_windows(); }

fn show_tab(tab: &str, cx: &mut App) {
    if let Some(sh) = crundi_shell::shell(cx) { let t = tab.to_string(); sh.update(cx, |sh, cx| sh.show_tab(&t, cx)); }
}

fn appr_act(id: &str, act: &str, cx: &mut App) {
    let Some(it) = item(id) else { return };
    match act {
        "dismiss" => dismiss(vec![json!({ "id": id, "source": s(&it, "source") })], cx),
        "yes" => decide(id, true, "once", cx),
        "yes-session" => decide(id, true, "session", cx),
        "yes-always" => decide(id, true, "always", cx),
        "no" => decide(id, false, "once", cx),
        "goto" => {
            with(|st| st.appr_open = false);
            // The chat is usually in another project: open that first. The
            // live chat's own project wins over the one on the card.
            let sid = s(&it, "sessionId");
            let live = crundi_shell::session::get(cx).terminals.iter().find(|t| s(t, "id") == sid).map(|t| s(t, "project")).unwrap_or_default();
            let proj = if live.is_empty() { s(&it, "project") } else { live };
            if !proj.is_empty() && proj.to_lowercase() != crundi_shell::project(cx).to_lowercase() { crundi_shell::session::select_project(cx, &proj); }
            show_tab("workbench", cx);
        }
        "secrets" => { with(|st| st.appr_open = false); show_tab("secrets", cx); }
        a if a.starts_with("kid:") || a.starts_with("cru:") => {
            let path = if a.starts_with("cru:") { "/api/voice/ask" } else { "/api/voice/kid/ask" };
            let choice = a[4..].to_string();
            let body = json!({ "id": id, "choice": if choice.is_empty() { "no".to_string() } else { choice } });
            crundi_shell::call(cx, move |api| api.post(path, &body), |r, cx| match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {}
                Ok(v) => toast_error(cx, { let e = s(&v, "error"); if e.is_empty() { "That could not be applied".into() } else { e } }),
                Err(_) => toast_error(cx, "That could not be applied"),
            });
        }
        _ => {}
    }
    cx.refresh_windows();
}

fn appr_clear(window: &mut Window, cx: &mut App) {
    let items = with(|st| st.items.clone());
    if items.is_empty() { return; }
    let all: Vec<Value> = items.iter().map(|i| json!({ "id": s(i, "id"), "source": s(i, "source") })).collect();
    let k = declines(&items);
    if k == 0 { return dismiss(all, cx); }
    let a = w::Ask {
        title: format!("Clear all {}?", all.len()).into(),
        message: format!("{k} of them will be declined. Chat cards are only hidden and stay answerable in their chats.").into(),
        ok: "Clear all".into(), danger: true, ..Default::default()
    };
    w::ask_confirm(window, cx, a, move |yes, _, cx| if yes { dismiss(all.clone(), cx); });
}

fn dismiss(list: Vec<Value>, cx: &mut App) {
    if list.is_empty() { return; }
    // Out at once; the server's broadcast confirms it.
    let ids: Vec<String> = list.iter().map(|x| s(x, "id")).collect();
    let left: Vec<Value> = with(|st| st.items.iter().filter(|i| !ids.contains(&s(i, "id"))).cloned().collect());
    set_items(left, cx);
    crundi_shell::call(cx, move |api| api.post("/api/approvals", &json!({ "action": "dismiss", "items": list })), |r, cx| match r {
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {}
        Ok(v) => toast_error(cx, { let e = s(&v, "error"); if e.is_empty() { "Could not dismiss that".into() } else { e } }),
        Err(_) => toast_error(cx, "Could not dismiss that"),
    });
}

fn decide(id: &str, approve: bool, scope: &str, cx: &mut App) {
    let body = json!({ "id": id, "approve": approve, "scope": scope });
    crundi_shell::call(cx, move |api| api.post("/api/approvals", &body), move |r, cx| match r {
        Ok(v) => { let (m, bad) = decided(&v, approve); if bad { toast_error(cx, m); } else { toast(cx, m, w::Toast::Success); } }
        Err(_) => toast_error(cx, "That could not be applied"),
    });
}

/// The top bar's button (#approvals-btn), beside the connection badge:
/// None at zero (a permanent grey icon teaches you to stop looking at it);
/// yellow with the count otherwise. A click toggles the panel.
pub fn approvals_button(_cx: &App) -> Option<AnyElement> {
    let k = with(|st| st.items.len());
    if k == 0 { return None; }
    let b = div().id("approvals-btn").flex().flex_none().items_center().gap(T::SP_6).h(T::SP_24).px(T::SP_8).rounded(T::R_PILL)
        .border_1().border_color(T::yellow()).bg(T::bg_secondary()).cursor_pointer()
        .text_color(T::yellow()).hover(|d| d.text_color(T::text()))
        .child(w::icon(IconName::ShieldAlert, T::ICON_MD))
        .child(w::tag(k.to_string(), "warn"))
        .on_click(|_, _, cx| appr_toggle(cx));
    Some(w::tip(b, "Things waiting on you").into_any_element())
}

/// The approvals panel (.appr-panel), under the top bar at the right, over
/// the window; None while closed. For the shell to draw over everything.
pub fn approvals_panel(window: &mut Window, _cx: &mut App) -> Option<AnyElement> {
    let (open, items) = with(|st| (st.appr_open, st.items.clone()));
    if !open { return None; }
    let vp = window.viewport_size();
    let pw = T::EXTRAS_APPR_W.min(vp.width - T::SP_16);
    let mut col = div().id("appr-list").flex().flex_col().p(T::SP_6).max_h(vp.height * T::EXTRAS_APPR_MAX_SHARE).overflow_y_scroll();
    if items.is_empty() {
        col = col.child(div().py(T::SP_12).text_center().text_size(T::TEXT_12).text_color(T::text_muted()).child("Nothing is waiting."));
    } else {
        col = col.child(div().flex().items_center().px(T::SP_8).pt(T::SP_2).pb(T::SP_6).border_b_1().border_color(T::border_subtle())
            .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_muted()).child(format!("{} waiting", items.len())))
            .child(w::button("appr-clear", "Clear all").on_click(|_, window, cx| appr_clear(window, cx))));
    }
    for (i, it) in items.iter().enumerate() {
        let src = s(it, "source");
        let tn = match src.as_str() { "chat" => "accent", "secret" | "kid" => "warn", "collab" => "sky", _ => "" };
        let project = s(it, "project");
        let detail: String = s(it, "detail").chars().take(1200).collect();
        let mut acts = div().flex().gap(T::SP_6).pt(T::SP_2);
        for (j, (aid, label, kind, tip)) in acts_of(it).into_iter().enumerate() {
            let id = s(it, "id");
            let b = if kind == "dismiss" { w::ghost(("appr-act", i * 16 + j), label) } else { w::button(("appr-act", i * 16 + j), label) };
            let b = if tip.is_empty() { b } else { b.tooltip(tip) };
            acts = acts.child(b.on_click(move |_, _, cx| appr_act(&id, &aid, cx)));
        }
        col = col.child(div().flex().flex_col().gap(T::SP_4).px(T::SP_8).py(T::SP_10).when(i > 0, |d| d.border_t_1().border_color(T::border_subtle()))
            .child(div().flex().items_center().gap(T::SP_6)
                .child(w::tag(src.clone(), tn))
                .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).text_color(T::text()).child(title_of(it))))
            .when(!project.is_empty(), |d| d.child(div().px(T::SP_8).py(T::SP_4).rounded(T::R_CONTROL).bg(T::bg()).opacity(T::PANELS_O_SOFT)
                .font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).child(project)))
            .when(!detail.is_empty(), |d| d.child(div().id(("appr-detail", i)).max_h(T::EXTRAS_APPR_DETAIL_H).overflow_y_scroll().px(T::SP_8).py(T::SP_4).rounded(T::R_CONTROL).bg(T::bg())
                .font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).child(detail)))
            .child(acts));
    }
    let panel = div().id("appr-panel").occlude().absolute().top(T::TOPBAR_H + T::SP_4).right(T::SP_8).w(pw)
        .rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_secondary()).overflow_hidden()
        .child(col);
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(div().w(vp.width).h(vp.height).child(panel))).with_priority(2).into_any_element())
}

// ─── What's new ───

pub struct WhatsNew { focus: FocusHandle }

fn schedule_whatsnew(cx: &mut App) {
    let g = with(|st| { st.wn_tries = 0; st.wn_gen += 1; st.wn_gen });
    later(cx, 1600, move |cx| maybe_show(g, cx));
}

fn later(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        let _ = cx.update(f);
    }).detach();
}

/// Not on top of a dialog: a modal, the shared dialog, an ask.
fn maybe_show(g: u32, cx: &mut App) {
    if with(|st| st.wn_gen != g || st.wn.is_some()) || crundi_shell::api(cx).is_none() { return; }
    with_window(cx, move |window, cx| {
        let busy = w::dialog_open(window, cx) || !w::ask_props(cx).is_null() || with(|st| st.add.is_some());
        if busy {
            if with(|st| { st.wn_tries += 1; st.wn_tries < 20 }) { later(cx, 1500, move |cx| maybe_show(g, cx)); }
            return;
        }
        open_whatsnew(false, cx);
    });
}

/// Open What's new: `all` is Settings' button (the latest edition again,
/// never marked seen); otherwise what this install has not been shown.
pub fn open_whatsnew(all: bool, cx: &mut App) {
    let path = if all { "/api/whatsnew?all=1" } else { "/api/whatsnew" };
    crundi_shell::call(cx, move |api| api.get(path), move |r, cx| {
        let Ok(v) = r else { return };
        let steps = v.get("steps").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        if v.get("ok").and_then(|o| o.as_bool()) != Some(true) || steps.is_empty() {
            if all { toast(cx, "Nothing new to show for this version.", w::Toast::Info); }
            return;
        }
        with_window(cx, move |window, cx| {
            let view = cx.new(|cx| WhatsNew { focus: cx.focus_handle() });
            let fh = view.read(cx).focus.clone();
            with(|st| st.wn = Some(Wn { version: s(&v, "version"), steps, at: 0, all }));
            // A full-window view: drawn by the modal host (its own card is
            // hidden under it), so it covers every tab and takes the keys.
            w::modal(window, cx, T::DIALOG_W, view.into(), |_, cx| { wn_finish(cx); });
            window.focus(&fh, cx);
        });
    });
}

fn wn_go(i: i64, cx: &mut App) {
    with(|st| if let Some(wn) = &mut st.wn { let k = wn.steps.len() as i64 + 1; wn.at = i.clamp(0, k - 1) as usize; });
    cx.refresh_windows();
}

/// Closed (Skip, Esc, Done): marked seen unless opened from Settings. True when it was open.
fn wn_finish(cx: &mut App) -> bool {
    let Some(wn) = with(|st| st.wn.take()) else { return false };
    if !wn.all { crundi_shell::call(cx, |api| api.post("/api/whatsnew/seen", &json!({})), |_, _| {}); }
    cx.refresh_windows();
    true
}

fn wn_close(window: &mut Window, cx: &mut App) {
    if wn_finish(cx) && w::modal_open(cx) { w::close_dialog(window, cx); }
}

fn wn_key(e: &KeyDownEvent, window: &mut Window, cx: &mut App) {
    let (at, last) = with(|st| st.wn.as_ref().map(|wn| (wn.at as i64, wn.at == wn.steps.len())).unwrap_or((0, false)));
    match e.keystroke.key.as_str() {
        // Esc: the modal host closes it (wn_finish).
        "escape" => return,
        "right" | "enter" | "pagedown" => { if last { wn_close(window, cx); } else { wn_go(at + 1, cx); } }
        "left" | "pageup" => wn_go(at - 1, cx),
        _ => {}
    }
    cx.stop_propagation();
}

impl Render for WhatsNew {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some((version, steps, at)) = with(|st| st.wn.as_ref().map(|wn| (wn.version.clone(), wn.steps.clone(), wn.at))) else { return div().into_any_element() };
        let k = steps.len() + 1;
        let last = at == k - 1;
        let step = wn_step(&steps, at).cloned().unwrap_or(Value::Null);
        let vp = window.viewport_size();
        let copy_w = T::EXTRAS_WN_COPY_MIN.max(vp.width * T::EXTRAS_WN_COPY_SHARE);
        let tag = s(&step, "tag");
        // The words, on the left.
        let mut copy = div().flex().flex_col().justify_center().gap(T::SP_16).w(copy_w).h_full().pl(T::SP_48 + T::SP_16).pr(T::SP_48)
            .child(div().flex().items_center().gap(T::SP_10)
                .child(div().text_size(T::TEXT_12).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(if last { "THAT IS EVERYTHING".to_string() } else if tag.is_empty() { "NEW".into() } else { tag.to_uppercase() }))
                .when(!last && !s(&step, "version").is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(format!("new in {}", s(&step, "version"))))))
            .child(div().text_size(T::EXTRAS_WN_TITLE).font_weight(T::W_BOLD).text_color(T::text()).child(if last { "You are up to date".to_string() } else { s(&step, "title") }))
            .child(div().text_size(T::TEXT_16).text_color(T::text_secondary()).child(if last { "This shows once after each update. You can open it again any time from Settings, under What's new.".to_string() } else { s(&step, "body") }));
        if !last {
            for kr in step.get("keys").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let mut keys = div().flex().flex_none().gap(T::SP_4).w(T::EXTRAS_WN_KEY_W);
                for kk in kr.get("keys").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    keys = keys.child(div().px(T::SP_6).py(T::SP_2).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_secondary())
                        .font_family(T::MONO).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(kk.as_str().unwrap_or("").to_string()));
                }
                copy = copy.child(div().flex().items_center().gap(T::SP_12).child(keys).child(div().flex_1().min_w_0().text_size(T::TEXT_14).text_color(T::text_secondary()).child(s(&kr, "text"))));
            }
        }
        // The art (or the recap), on the right.
        let stage: AnyElement = if !last {
            div().flex().flex_col().flex_1().min_w_0().items_center().justify_center().gap(T::SP_12).bg(T::extras_wn_haze())
                .child(div().text_color(T::accent()).child(w::icon(IconName::Bot, T::SP_48 + T::SP_16)))
                .child(div().text_size(T::TEXT_20).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(tag.clone()))
                .child(div().text_size(T::TEXT_14).text_color(T::text_secondary()).child(s(&step, "title")))
                .into_any_element()
        } else {
            let mut rc = div().id("wn-recap").flex().flex_col().flex_1().min_w_0().gap(T::SP_8).px(T::SP_20).py(T::SP_16).overflow_y_scroll();
            for (i, st) in steps.iter().enumerate() {
                rc = rc.child(div().id(("wn-recap-item", i)).flex().items_center().gap(T::SP_12).px(T::SP_12).py(T::SP_10).rounded(T::R_BUBBLE).border_1().border_color(T::border()).bg(T::bg_card())
                    .cursor_pointer().hover(|d| d.border_color(T::accent()))
                    .child(div().text_color(T::green()).child(w::icon(IconName::Check, T::ICON_LG)))
                    .child(div().flex().flex_col().flex_1().min_w_0()
                        .child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(s(st, "title")))
                        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(s(st, "tag"))))
                    .on_click(move |_, _, cx| wn_go(i as i64, cx)));
            }
            rc.into_any_element()
        };
        let art_title = if last { "In this update".to_string() } else { Some(s(&step, "artTitle")).filter(|t| !t.is_empty()).unwrap_or_else(|| tag.clone()) };
        let fw = T::EXTRAS_WN_FRAME_MAX.min(vp.width - copy_w - T::SP_48 - T::SP_24);
        let fh = (fw * (T::EXTRAS_WN_STAGE_H / T::EXTRAS_WN_STAGE_W) + T::SP_32).min(vp.height - T::SP_48 * 4.);
        let art = div().flex().flex_1().min_w_0().items_center().justify_center().pr(T::SP_48)
            .child(div().flex().flex_col().w(fw).h(fh).rounded(T::R_BUBBLE).border_1().border_color(T::border()).bg(T::bg_secondary()).overflow_hidden()
                .child(div().flex().flex_none().items_center().gap(T::SP_8).h(T::SP_32).pl(T::SP_12).border_b_1().border_color(T::border())
                    .children((0..3).map(|_| w::dot(T::bg_hover())))
                    .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(art_title)))
                .child(stage));
        // The top: the brand, the version, Skip.
        let top = div().flex().flex_none().items_center().gap(T::SP_10).h(T::CONTROL_LG).mt(T::SP_16).px(T::SP_24)
            .child(div().text_color(T::accent()).text_size(T::TEXT_20).child(IconName::Bot))
            .child(div().font_weight(T::W_BOLD).text_size(T::TEXT_20).text_color(T::text()).child("crundi"))
            .when(!version.is_empty(), |d| d.child(w::tag(version.clone(), "accent")))
            .child(div().flex_1().min_w_0())
            .when(!last, |d| d.child(w::button("wn-skip", "Skip").icon(IconName::X).on_click(|_, window, cx| wn_close(window, cx))));
        // The foot: Back, the dots, the count, the hint, Next / Done.
        let mut dots = div().flex().items_center().gap(T::SP_6);
        for i in 0..k {
            let c = if i == at { T::accent() } else if i < at { T::accent_edge() } else { T::bg_hover() };
            dots = dots.child(div().id(("wn-dot", i)).h(T::SP_6).w(if i == at { T::SP_24 } else { T::SP_6 }).rounded(T::R_PILL).bg(c).cursor_pointer()
                .on_click(move |_, _, cx| wn_go(i as i64, cx)));
        }
        let at_i = at as i64;
        let foot = div().flex().flex_none().items_center().gap(T::SP_16).h(T::CONTROL_LG).mb(T::SP_24).pl(T::SP_48 + T::SP_16).pr(T::SP_48)
            .child(div().when(at == 0, |d| d.invisible()).child(w::button("wn-back", "Back").icon(IconName::ChevronLeft).on_click(move |_, _, cx| if at_i > 0 { wn_go(at_i - 1, cx) })))
            .child(dots)
            .child(div().font_family(T::MONO).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child(format!("{} / {}", at + 1, k)))
            .child(div().flex_1().min_w_0())
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Arrow keys, or swipe"))
            .child(w::primary("wn-next", if last { "Done" } else { "Next" }).icon(if last { IconName::Check } else { IconName::ChevronRight })
                .on_click(move |_, window, cx| if last { wn_close(window, cx) } else { wn_go(at_i + 1, cx) }));
        let full = div().id("whatsnew").occlude().track_focus(&self.focus).on_key_down(wn_key)
            .w(vp.width).h(vp.height).flex().flex_col().bg(T::bg()).text_color(T::text()).font(crundi_theme::ui_font())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(top)
            .child(div().flex().flex_1().min_w_0().min_h_0().items_center().child(copy).child(art))
            .child(foot);
        div().child(deferred(anchored().position(point(px(0.), px(0.))).child(full)).with_priority(2)).into_any_element()
    }
}

// ─── The spine (lib.rs) ───

/// Not a tab: Add project, the approvals and What's new are dialogs.
pub fn view(_tab: &str, _window: &mut Window, _cx: &mut App) -> Option<AnyElement> { None }

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Server("approvals", v) => set_items(v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(), cx),
        AreaEvent::Connected(true) => load_approvals(cx),
        AreaEvent::State(v) => {
            if let Some(alias) = with(|st| st.select.clone()) {
                let listed = v.get("projects").and_then(|p| p.as_array()).is_some_and(|p| p.iter().any(|x| s(x, "alias") == alias));
                if listed { with(|st| st.select = None); cx.defer(move |cx| crundi_shell::session::select_project(cx, &alias)); }
            }
        }
        _ => {}
    }
}

pub fn on_project(_alias: &str, _cx: &mut App) {}

pub fn on_signed_in(cx: &mut App) {
    load_config(cx);
    load_approvals(cx);
    // What's new, 1.6 s after the app comes up. Not under the test driver
    // (it would cover every area's tests): the driver opens it on purpose.
    let driven = std::env::var("CRUNDI_TEST_PORT").is_ok_and(|p| !p.is_empty()) && std::env::var("CRUNDI_WHATSNEW").is_err();
    if !driven { schedule_whatsnew(cx); }
}

pub fn on_signed_out(cx: &mut App) {
    with(|st| { st.items.clear(); st.select = None; st.wn_gen += 1; st.appr_open = false; st.add = None; st.wn = None; });
    with_window(cx, |window, cx| if w::modal_open(cx) { w::close_dialog(window, cx); });
    cx.refresh_windows();
}

pub fn on_tab(_tab: &str, _cx: &mut App) {}

/// A driver action (Some(answer) for a name it knows).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "extras-add-open" => open_add_project(window, cx),
        "extras-add-set" => {
            let Some(v) = with(|st| st.add.clone()) else { return Some(json!({ "ok": false, "error": "not open" })) };
            let x = v.read(cx);
            let i = match a(0).as_str() { "name" => x.name.clone(), "alias" => x.alias.clone(), "path" => x.path.clone(), "dname" => x.dname.clone(), _ => return Some(json!({ "ok": false })) };
            let t = a(1);
            i.update(cx, |s, cx| s.set_value(t, window, cx));
        }
        "extras-add-submit" => add(window, cx),
        "extras-add-cancel" => close_add(window, cx),
        "extras-appr-toggle" => appr_toggle(cx),
        "extras-appr-act" => {
            // By id, or by the item's title.
            let id = with(|st| st.items.iter().find(|i| s(i, "id") == a(0) || s(i, "title") == a(0)).map(|i| s(i, "id"))).unwrap_or_default();
            appr_act(&id, &a(1), cx);
        }
        "extras-appr-clear" => appr_clear(window, cx),
        "extras-wn-open" => open_whatsnew(a(0) == "true", cx),
        "extras-wn-auto" => { let g = with(|st| { st.wn_tries = 0; st.wn_gen += 1; st.wn_gen }); maybe_show(g, cx); }
        "extras-wn-go" => wn_go(a(0).parse().unwrap_or(0), cx),
        "extras-wn-close" => wn_close(window, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// The driver's view of this screen (the Slint client's names).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let (add, items, appr_open, select) = with(|st| (st.add.clone(), st.items.clone(), st.appr_open, st.select.clone()));
    let wn = with(|st| st.wn.as_ref().map(|wn| {
        let k = wn.steps.len() + 1;
        let step = wn.steps.get(wn.at);
        json!({ "version": wn.version, "at": wn.at, "n": k, "last": wn.at == k - 1,
            "title": step.map(|x| s(x, "title")).unwrap_or_else(|| "You are up to date".into()), "tag": step.map(|x| s(x, "tag")).unwrap_or_default(), "all": wn.all })
    }));
    let add_v = add.as_ref().map(|v| { let x = v.read(cx); json!({ "single": x.single, "hint": x.hint, "name": val(&x.name, cx), "alias": val(&x.alias, cx), "path": val(&x.path, cx), "dname": val(&x.dname, cx), "busy": x.busy }) });
    json!({
        "modal": add.is_some() || wn.is_some(),
        "add": add_v.unwrap_or(Value::Null),
        "appr": { "open": appr_open, "count": items.len(), "items": items.iter().map(|i| json!({
            "id": s(i, "id"), "source": s(i, "source"), "title": title_of(i), "project": s(i, "project"), "detail": s(i, "detail").chars().take(1200).collect::<String>(),
            "acts": acts_of(i).into_iter().map(|a| a.1).collect::<Vec<_>>(),
        })).collect::<Vec<_>>() },
        "wn": wn.unwrap_or(Value::Null),
        "pendingSelect": select,
    })
}

#[cfg(test)]
mod tests {
    use super::{acts_of, alias_multi, alias_single, decided, declines, folder_hint, title_of, wn_step};
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn aliases() {
        assert_eq!(alias_single("My Project!"), "my-project");
        assert_eq!(alias_single("  --a  b--  "), "a-b");
        assert_eq!(alias_single("!!!"), "");
        assert_eq!(alias_single("web_app-2"), "web_app-2");
        assert_eq!(alias_multi(" My Proj_1 "), "myproj_1");
        assert_eq!(folder_hint("/home/x/projects"), "Folder: /home/x/projects/...");
        assert_eq!(folder_hint("C:\\Projects"), "Folder: C:\\Projects\\...");
    }

    #[::core::prelude::v1::test]
    fn approval_buttons() {
        let labels = |v: serde_json::Value| acts_of(&v).into_iter().map(|a| a.1).collect::<Vec<_>>();
        assert_eq!(labels(json!({ "source": "secret" })), ["Open secrets", "Dismiss"]);
        assert_eq!(labels(json!({ "source": "chat" })), ["Open the chat", "Dismiss"]);
        assert_eq!(labels(json!({ "source": "collab", "host": "x.com" })), ["Allow once", "This session", "Always", "Decline", "Dismiss"]);
        assert_eq!(labels(json!({ "source": "collab", "kind": "command" })), ["Run", "Decline", "Dismiss"]);
        assert_eq!(labels(json!({ "source": "collab", "kind": "tool" })), ["Allow once", "This session", "Decline", "Dismiss"]);
        assert_eq!(labels(json!({ "source": "collab", "kind": "merge" })), ["Approve", "Decline", "Dismiss"]);
        let kid = acts_of(&json!({ "source": "kid", "choices": [{ "id": "yes", "label": "Yes" }, { "id": "no", "label": "No" }] }));
        assert_eq!(kid[0].0, "kid:yes");
        assert_eq!(kid[1].2, "no");
        assert_eq!(acts_of(&json!({ "source": "chat" })).last().unwrap().3, "Hide here; still answerable in the chat");
        assert_eq!(title_of(&json!({ "name": "Sam", "title": "push" })), "Sam \u{2014} push");
        assert_eq!(declines(&[json!({ "source": "chat" }), json!({ "source": "secret" })]), 1);
    }

    #[::core::prelude::v1::test]
    fn decisions_read() {
        assert_eq!(decided(&json!({ "ok": true }), true), ("Approved".into(), false));
        assert_eq!(decided(&json!({ "ok": true }), false), ("Declined".into(), false));
        assert_eq!(decided(&json!({ "ok": false }), true), ("That could not be applied".into(), true));
        assert_eq!(decided(&json!({ "ok": true, "merge": { "ok": true, "merged": "sam", "into": "main" } }), true), ("Merged sam into main".into(), false));
        assert_eq!(decided(&json!({ "ok": true, "running": true }), true).0, "Running. The output will go to their chat.");
        assert_eq!(decided(&json!({ "ok": true, "service": { "ok": false, "error": "port" } }), true), ("Approved, but it could not be registered: port".into(), true));
        assert_eq!(decided(&json!({ "ok": true, "forward": { "ok": true, "forward": { "url": "https://a" } } }), true).0, "Published https://a");
        assert_eq!(decided(&json!({ "ok": true, "tunnel": { "ok": true, "running": false } }), true).0, "Tunnel on: it starts when the service runs");
        let steps = vec![json!({ "title": "A" })];
        assert!(wn_step(&steps, 0).is_some());
        assert!(wn_step(&steps, 1).is_none(), "the recap is past the steps");
    }
}
