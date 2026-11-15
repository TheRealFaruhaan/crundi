//! Ops area: Add project, What's new, the top bar's approvals button.
//! UI: ui/ops/extras.slint (global OpsExtras), ui/ops/extras-appr.slint (the
//! button, in ui/shell.slint's top bar).
//!
//! - Add project (src/webapp-html.js showAddModal / addProject): the
//!   sidebar's "+ Add project" (Shell.add-project). One name in the
//!   single-folder mode (GET /api/projects/config), alias + path + display
//!   name otherwise (a missing path asks to create the folder). The new
//!   project opens once the server's state lists it.
//! - Approvals (renderApprovals, dismissApprovals, decideApproval): the
//!   `approvals` event's items; the button shows at one or more; the panel
//!   answers what it can, opens a chat or the Secrets tab for the rest, and
//!   dismisses (declines what lives only here).
//! - What's new (crundi-whatsnew.js, scheduleWhatsNew / openWhatsNew): shown
//!   once after an install or an update (GET /api/whatsnew, 1.6 s after
//!   sign-in, never over a dialog), Skip or Done marks it seen; Settings
//!   opens it again with `open_whatsnew(true)`.

use crate::{areas::AreaEvent, kit, MainWindow, OpsApprAct, OpsApprItem, OpsExtras, OpsWnKey, OpsWnStep};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc, time::Duration};

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
    let src = kit::s(it, "source");
    let kind = kit::s(it, "kind");
    let host = kit::s(it, "host");
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
            let id = kit::s(c, "id");
            (format!("{src}:{id}"), kit::s(c, "label"), if id == "no" { "no" } else { "yes" }, String::new())
        }).collect(),
        ("chat", _) => vec![a("goto", "Open the chat", "go", "")],
        _ => vec![a("secrets", "Open secrets", "go", "")],
    };
    v.push(a("dismiss", "Dismiss", "dismiss", if src == "chat" { "Hide here; still answerable in the chat" } else { "Decline and remove" }));
    v
}

/// An item's title line: "<name> — <title>" when a person asked.
pub fn title_of(it: &Value) -> String {
    let n = kit::s(it, "name");
    let t = kit::s(it, "title");
    if n.is_empty() { t } else { format!("{n} \u{2014} {t}") }
}

/// Clear all: how many would be declined (all but chat cards).
pub fn declines(items: &[Value]) -> usize { items.iter().filter(|i| kit::s(i, "source") != "chat").count() }

/// What a decision's answer says: (toast, is an error).
pub fn decided(r: &Value, approve: bool) -> (String, bool) {
    let e = |k: &str, d: &str| { let x = r.pointer(k).and_then(|x| x.as_str()).unwrap_or(""); if x.is_empty() { d.to_string() } else { x.to_string() } };
    let okd = |k: &str| r.pointer(k).and_then(|o| o.as_bool()).unwrap_or(false);
    if !okd("/ok") { return (e("/error", "That could not be applied"), true); }
    if let Some(m) = r.get("merge").filter(|m| !m.is_null()) {
        if !okd("/merge/ok") { return (e("/merge/error", "The merge failed"), true); }
        return (format!("Merged {} into {}", m.get("merged").map(|x| x.as_str().map(String::from).unwrap_or(x.to_string())).unwrap_or_default(), kit::s(m, "into")), false);
    }
    if r.get("running").is_some_and(kit::truthy) { return ("Running. The output will go to their chat.".into(), false); }
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

/// What's new's steps, as the server sends them.
pub fn steps_of(v: &Value) -> Vec<OpsWnStep> {
    v.get("steps").and_then(|s| s.as_array()).cloned().unwrap_or_default().iter().map(|s| OpsWnStep {
        tag: kit::s(s, "tag").into(),
        version: kit::s(s, "version").into(),
        title: kit::s(s, "title").into(),
        body: kit::s(s, "body").into(),
        art_title: kit::s(s, "artTitle").into(),
        keys: ModelRc::new(VecModel::from(s.get("keys").and_then(|k| k.as_array()).cloned().unwrap_or_default().iter().map(|k| OpsWnKey {
            keys: ModelRc::new(VecModel::from(k.get("keys").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter().map(|x| x.as_str().unwrap_or("").into()).collect::<Vec<slint::SharedString>>())),
            text: kit::s(k, "text").into(),
        }).collect::<Vec<_>>())),
    }).collect()
}

// ─── State ───

#[derive(Default)]
struct St {
    single: bool,
    dir: String,
    /// A project just added: opened when the server's state lists it.
    select: Option<String>,
    items: Vec<Value>,
    model: Option<Rc<VecModel<OpsApprItem>>>,
    wn_all: bool,
    wn_tries: u32,
    wn_gen: u32,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }
fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }
fn g(f: impl FnOnce(OpsExtras)) { if let Some(w) = kit::ui() { f(w.global::<OpsExtras>()); } }
fn sync_modal() { g(|x| x.set_modal(x.get_ap_open() || x.get_wn_open())); }

// ─── Registry ───

pub fn init(w: &MainWindow) {
    let m = kit::model::<OpsApprItem>();
    let x = w.global::<OpsExtras>();
    x.set_appr_items(kit::rc(&m));
    with(|s| s.model = Some(m));
    w.global::<crate::Shell>().on_add_project(open_add);
    x.on_ap_cancel(close_add);
    x.on_ap_add(add);
    x.on_appr_toggle(|| { g(|x| x.set_appr_open(!x.get_appr_open() && x.get_appr_items().row_count() > 0)); });
    x.on_appr_act(|id, act| appr_act(&id, &act));
    x.on_appr_clear(appr_clear);
    x.on_wn_go(wn_go);
    x.on_wn_close(|how| wn_close(&how));
}

pub fn signed_in() {
    load_config();
    load_approvals();
    // What's new, 1.6 s after the app comes up. Not under the test driver
    // (it would cover every area's tests): the driver opens it on purpose.
    let driven = std::env::var("CRUNDI_TEST_PORT").is_ok_and(|p| !p.is_empty()) && std::env::var("CRUNDI_WHATSNEW").is_err();
    if !driven { schedule_whatsnew(); }
}

pub fn signed_out() {
    with(|s| { s.items.clear(); s.select = None; s.wn_gen += 1; });
    g(|x| { x.set_ap_open(false); x.set_wn_open(false); x.set_appr_open(false); });
    render_approvals();
    sync_modal();
}

pub fn project(_alias: &str) {}
pub fn tab_shown(_tab: &str) {}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("approvals", v) => {
            with(|s| s.items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default());
            render_approvals();
        }
        AreaEvent::Connected(true) => load_approvals(),
        AreaEvent::State(v) => {
            let want = with(|s| s.select.clone());
            if let Some(alias) = want {
                let listed = v.get("projects").and_then(|p| p.as_array()).is_some_and(|p| p.iter().any(|x| kit::s(x, "alias") == alias));
                // After app.rs has taken the new list (it hears the event after the areas).
                if listed { with(|s| s.select = None); slint::Timer::single_shot(Duration::ZERO, move || crate::app::select_project(&alias)); }
            }
        }
        _ => {}
    }
}

// ─── Add project ───

fn load_config() {
    kit::call(|api| api.get("/api/projects/config"), |r| {
        if let Ok(v) = r { with(|s| { s.single = kit::s(&v, "mode") == "single"; s.dir = kit::s(&v, "projectsDir"); }); }
    });
}

/// The sidebar's "+ Add project".
pub fn open_add() {
    let (single, dir) = with(|s| (s.single, s.dir.clone()));
    g(|x| {
        x.set_ap_single(single);
        x.set_ap_hint(folder_hint(&dir).into());
        x.set_ap_name("".into()); x.set_ap_alias("".into()); x.set_ap_path("".into()); x.set_ap_dname("".into());
        x.set_ap_busy(false);
        x.set_ap_open(true);
    });
    sync_modal();
}

fn close_add() {
    g(|x| { x.set_ap_open(false); x.set_ap_busy(false); });
    sync_modal();
}

fn add() {
    let Some(w) = kit::ui() else { return };
    let x = w.global::<OpsExtras>();
    if !x.get_ap_open() || x.get_ap_busy() { return; }
    if x.get_ap_single() {
        let raw = x.get_ap_name().trim().to_string();
        if raw.is_empty() { kit::toast_error("Project name is required"); return; }
        let alias = alias_single(&raw);
        if alias.is_empty() { kit::toast_error("Invalid project name"); return; }
        register(json!({ "alias": alias, "name": raw, "create": true }), "Project created");
        return;
    }
    let alias = alias_multi(&x.get_ap_alias());
    let path = x.get_ap_path().trim().to_string();
    let dn = x.get_ap_dname().trim().to_string();
    let name = if dn.is_empty() { alias.clone() } else { dn };
    if alias.is_empty() || path.is_empty() { kit::toast_error("Alias and path are required"); return; }
    x.set_ap_busy(true);
    let p = path.clone();
    kit::call(move |api| api.post("/api/projects/check-path", &json!({ "path": p })), move |r| {
        g(|x| x.set_ap_busy(false));
        // Unknown: assume it exists (the web's).
        let exists = r.ok().and_then(|v| v.get("exists").and_then(|e| e.as_bool())).unwrap_or(true);
        let body = move |create: bool| json!({ "alias": alias, "path": path, "name": name, "create": create });
        if exists { register(body(false), "Project added"); return; }
        kit::confirm(kit::Confirm { title: "Add project", message: "Path does not exist. Create empty folder?", ok: "Create", danger: false }, move |yes| {
            if yes { register(body(true), "Project added"); }
        });
    });
}

fn register(body: Value, done: &'static str) {
    let alias = kit::s(&body, "alias");
    g(|x| x.set_ap_busy(true));
    kit::call(move |api| api.post("/api/projects", &body), move |r| {
        g(|x| x.set_ap_busy(false));
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                close_add();
                kit::toast_ok(done);
                // The server's state event brings the new list; the project
                // opens then (and a moment later at the latest).
                with(|s| s.select = Some(alias.clone()));
                slint::Timer::single_shot(Duration::from_millis(2500), move || {
                    let still = with(|s| if s.select.as_deref() == Some(alias.as_str()) { s.select.take() } else { None });
                    if let Some(a) = still { crate::app::select_project(&a); }
                });
            }
            Ok(v) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { "Failed".into() } else { e } }),
            Err(e) => kit::toast_error(&format!("Error: {e}")),
        }
    });
}

// ─── Approvals ───

fn load_approvals() {
    if kit::api().is_none() { return; }
    kit::call(|api| api.get("/api/approvals"), |r| {
        if let Ok(v) = r { with(|s| s.items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default()); render_approvals(); }
    });
}

fn render_approvals() {
    let (rows, m) = with(|s| (s.items.iter().map(|it| OpsApprItem {
        id: kit::s(it, "id").into(),
        source: kit::s(it, "source").into(),
        title: title_of(it).into(),
        project: kit::s(it, "project").into(),
        detail: kit::s(it, "detail").chars().take(1200).collect::<String>().into(),
        acts: ModelRc::new(VecModel::from(acts_of(it).into_iter().map(|(id, label, kind, tip)| OpsApprAct { id: id.into(), label: label.into(), kind: kind.into(), tip: tip.into() }).collect::<Vec<_>>())),
    }).collect::<Vec<_>>(), s.model.clone()));
    let n = rows.len();
    if let Some(m) = m {
        // Rows hold their buttons' model (compared by identity): rewrite
        // only when an item came, went or changed in what is shown.
        let same = m.row_count() == n && m.iter().zip(rows.iter()).all(|(a, b)| a.id == b.id && a.title == b.title && a.detail == b.detail && a.project == b.project && a.acts.row_count() == b.acts.row_count());
        if !same { m.set_vec(rows); }
    }
    if n == 0 { g(|x| x.set_appr_open(false)); }
}

fn item(id: &str) -> Option<Value> { with(|s| s.items.iter().find(|i| kit::s(i, "id") == id).cloned()) }

fn appr_act(id: &str, act: &str) {
    let Some(it) = item(id) else { return };
    match act {
        "dismiss" => dismiss(vec![json!({ "id": id, "source": kit::s(&it, "source") })]),
        "yes" => decide(id, true, "once"),
        "yes-session" => decide(id, true, "session"),
        "yes-always" => decide(id, true, "always"),
        "no" => decide(id, false, "once"),
        "goto" => {
            g(|x| x.set_appr_open(false));
            // The chat is usually in another project: open that first. The
            // live chat's own project wins over the one on the card.
            let sid = kit::s(&it, "sessionId");
            let live = kit::terminals().unwrap_or_default().iter().find(|t| kit::s(t, "id") == sid).map(|t| kit::s(t, "project")).unwrap_or_default();
            let proj = if live.is_empty() { kit::s(&it, "project") } else { live };
            if !proj.is_empty() && proj.to_lowercase() != kit::project().to_lowercase() { crate::app::select_project(&proj); }
            kit::show_tab("workbench");
        }
        "secrets" => { g(|x| x.set_appr_open(false)); kit::show_tab("secrets"); }
        a if a.starts_with("kid:") || a.starts_with("cru:") => {
            let path = if a.starts_with("cru:") { "/api/voice/ask" } else { "/api/voice/kid/ask" };
            let choice = a[4..].to_string();
            let body = json!({ "id": id, "choice": if choice.is_empty() { "no".to_string() } else { choice } });
            kit::call(move |api| api.post(path, &body), |r| match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {}
                Ok(v) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { "That could not be applied".into() } else { e } }),
                Err(_) => kit::toast_error("That could not be applied"),
            });
        }
        _ => {}
    }
}

fn appr_clear() {
    let items = with(|s| s.items.clone());
    if items.is_empty() { return; }
    let all: Vec<Value> = items.iter().map(|i| json!({ "id": kit::s(i, "id"), "source": kit::s(i, "source") })).collect();
    let n = declines(&items);
    if n == 0 { dismiss(all); return; }
    let title = format!("Clear all {}?", all.len());
    let msg = format!("{n} of them will be declined. Chat cards are only hidden and stay answerable in their chats.");
    kit::confirm(kit::Confirm { title: &title, message: &msg, ok: "Clear all", danger: true }, move |yes| if yes { dismiss(all); });
}

fn dismiss(list: Vec<Value>) {
    if list.is_empty() { return; }
    // Out at once; the server's broadcast confirms it.
    let ids: Vec<String> = list.iter().map(|x| kit::s(x, "id")).collect();
    with(|s| s.items.retain(|i| !ids.contains(&kit::s(i, "id"))));
    render_approvals();
    kit::call(move |api| api.post("/api/approvals", &json!({ "action": "dismiss", "items": list })), |r| match r {
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {}
        Ok(v) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { "Could not dismiss that".into() } else { e } }),
        Err(_) => kit::toast_error("Could not dismiss that"),
    });
}

fn decide(id: &str, approve: bool, scope: &str) {
    let body = json!({ "id": id, "approve": approve, "scope": scope });
    kit::call(move |api| api.post("/api/approvals", &body), move |r| match r {
        Ok(v) => { let (msg, bad) = decided(&v, approve); if bad { kit::toast_error(&msg); } else { kit::toast_ok(&msg); } }
        Err(_) => kit::toast_error("That could not be applied"),
    });
}

// ─── What's new ───

fn schedule_whatsnew() {
    let gen = with(|s| { s.wn_tries = 0; s.wn_gen += 1; s.wn_gen });
    slint::Timer::single_shot(Duration::from_millis(1600), move || maybe_show(gen));
}

/// Not on top of a dialog: the window's modal flags, the shared dialog.
fn busy() -> bool {
    let Some(w) = kit::ui() else { return true };
    let sh = w.global::<crate::Shell>();
    sh.get_modal_workspace() || sh.get_modal_planning() || sh.get_modal_ops() || sh.get_modal_desktop() || w.global::<crate::DialogUi>().get_open()
}

fn maybe_show(gen: u32) {
    if with(|s| s.wn_gen != gen) || kit::api().is_none() { return; }
    if kit::ui().is_some_and(|w| w.global::<OpsExtras>().get_wn_open()) { return; }
    if busy() {
        let again = with(|s| { s.wn_tries += 1; s.wn_tries < 20 });
        if again { slint::Timer::single_shot(Duration::from_millis(1500), move || maybe_show(gen)); }
        return;
    }
    open_whatsnew(false);
}

/// Open What's new: `all` is Settings' button (the latest edition again,
/// never marked seen); otherwise what this install has not been shown.
pub fn open_whatsnew(all: bool) {
    let path = if all { "/api/whatsnew?all=1" } else { "/api/whatsnew" };
    kit::call(move |api| api.get(path), move |r| {
        let Ok(v) = r else { return };
        let steps = steps_of(&v);
        if v.get("ok").and_then(|o| o.as_bool()) != Some(true) || steps.is_empty() {
            if all { kit::toast("Nothing new to show for this version."); }
            return;
        }
        with(|s| s.wn_all = all);
        g(|x| {
            x.set_wn_version(kit::s(&v, "version").into());
            x.set_wn_steps(ModelRc::new(VecModel::from(steps)));
            x.set_wn_at(0);
            x.set_wn_open(true);
        });
        sync_modal();
    });
}

fn wn_go(i: i32) {
    g(|x| { let n = x.get_wn_steps().row_count() as i32 + 1; x.set_wn_at(i.clamp(0, n - 1)); });
}

fn wn_close(_how: &str) {
    let open = kit::ui().is_some_and(|w| w.global::<OpsExtras>().get_wn_open());
    if !open { return; }
    g(|x| x.set_wn_open(false));
    sync_modal();
    if !with(|s| s.wn_all) {
        kit::call(|api| api.post("/api/whatsnew/seen", &json!({})), |_| {});
    }
}

// ─── The test driver ───

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let w = kit::ui()?;
    let x = w.global::<OpsExtras>();
    match name {
        "extras-add-open" => w.global::<crate::Shell>().invoke_add_project(),
        "extras-add-set" => {
            let t = a(1);
            match a(0).as_str() { "name" => x.set_ap_name(t.into()), "alias" => x.set_ap_alias(t.into()), "path" => x.set_ap_path(t.into()), "dname" => x.set_ap_dname(t.into()), _ => return Some(json!({ "ok": false })) }
        }
        "extras-add-submit" => add(),
        "extras-add-cancel" => close_add(),
        "extras-appr-toggle" => x.invoke_appr_toggle(),
        "extras-appr-act" => {
            // By id, or by the item's title.
            let id = with(|s| s.items.iter().find(|i| kit::s(i, "id") == a(0) || kit::s(i, "title") == a(0)).map(|i| kit::s(i, "id"))).unwrap_or_default();
            appr_act(&id, &a(1));
        }
        "extras-appr-clear" => appr_clear(),
        "extras-wn-open" => open_whatsnew(a(0) == "true"),
        "extras-wn-auto" => { let gen = with(|s| { s.wn_tries = 0; s.wn_gen += 1; s.wn_gen }); maybe_show(gen); }
        "extras-wn-go" => wn_go(a(0).parse().unwrap_or(0)),
        "extras-wn-close" => wn_close(&a(0)),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// This screen's part of the area's props (under "extras").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let x = w.global::<OpsExtras>();
    let items: Vec<Value> = x.get_appr_items().iter().map(|i| json!({
        "id": i.id.as_str(), "source": i.source.as_str(), "title": i.title.as_str(), "project": i.project.as_str(), "detail": i.detail.as_str(),
        "acts": i.acts.iter().map(|a| a.label.to_string()).collect::<Vec<_>>(),
    })).collect();
    let steps = x.get_wn_steps();
    let at = x.get_wn_at();
    let n = steps.row_count() as i32 + 1;
    let step = steps.row_data(at as usize);
    json!({
        "modal": x.get_modal(),
        "add": if x.get_ap_open() { json!({ "single": x.get_ap_single(), "hint": x.get_ap_hint().as_str(), "name": x.get_ap_name().as_str(), "alias": x.get_ap_alias().as_str(), "path": x.get_ap_path().as_str(), "busy": x.get_ap_busy() }) } else { Value::Null },
        "appr": { "open": x.get_appr_open(), "count": items.len(), "items": items },
        "wn": if x.get_wn_open() { json!({ "version": x.get_wn_version().as_str(), "at": at, "n": n, "last": at == n - 1, "title": step.as_ref().map(|s| s.title.to_string()).unwrap_or_else(|| "You are up to date".into()), "tag": step.as_ref().map(|s| s.tag.to_string()).unwrap_or_default(), "all": with(|s| s.wn_all) }) } else { Value::Null },
        "pendingSelect": with(|s| s.select.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases() {
        assert_eq!(alias_single("My Project!"), "my-project");
        assert_eq!(alias_single("  --a  b--  "), "a-b");
        assert_eq!(alias_single("!!!"), "");
        assert_eq!(alias_single("web_app-2"), "web_app-2");
        assert_eq!(alias_multi(" My Proj_1 "), "myproj_1");
        assert_eq!(folder_hint("/home/x/projects"), "Folder: /home/x/projects/...");
        assert_eq!(folder_hint("C:\\Projects"), "Folder: C:\\Projects\\...");
    }

    #[test]
    fn approval_buttons() {
        let labels = |v: Value| acts_of(&v).into_iter().map(|a| a.1).collect::<Vec<_>>();
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

    #[test]
    fn decisions_read() {
        assert_eq!(decided(&json!({ "ok": true }), true), ("Approved".into(), false));
        assert_eq!(decided(&json!({ "ok": true }), false), ("Declined".into(), false));
        assert_eq!(decided(&json!({ "ok": false }), true), ("That could not be applied".into(), true));
        assert_eq!(decided(&json!({ "ok": true, "merge": { "ok": true, "merged": "sam", "into": "main" } }), true), ("Merged sam into main".into(), false));
        assert_eq!(decided(&json!({ "ok": true, "running": true }), true).0, "Running. The output will go to their chat.");
        assert_eq!(decided(&json!({ "ok": true, "service": { "ok": false, "error": "port" } }), true), ("Approved, but it could not be registered: port".into(), true));
        assert_eq!(decided(&json!({ "ok": true, "forward": { "ok": true, "forward": { "url": "https://a" } } }), true).0, "Published https://a");
        assert_eq!(decided(&json!({ "ok": true, "tunnel": { "ok": true, "running": false } }), true).0, "Tunnel on: it starts when the service runs");
    }

    #[test]
    fn whatsnew_steps() {
        let s = steps_of(&json!({ "steps": [{ "tag": "Cru", "title": "T", "body": "B", "artTitle": "Voice", "keys": [{ "keys": ["Ctrl", "K"], "text": "open" }] }] }));
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].art_title.as_str(), "Voice");
        assert_eq!(s[0].keys.row_count(), 1);
        assert_eq!(s[0].keys.row_data(0).unwrap().keys.row_count(), 2);
        assert!(steps_of(&json!({})).is_empty());
    }
}
