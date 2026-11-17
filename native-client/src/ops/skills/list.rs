//! Settings → Skills (the web's renderSkillsSection, drawSkills,
//! onSkillsClick, skUpload, skLinkInstall, skUpdate, skTransfer,
//! skOpenFile): the Claude skills on this machine, by where they live.

use super::common::{self as c, ok};
use crate::kit::{self, ApiError};
use crate::{OpsSkillFile, OpsSkillPick, OpsSkillRow, OpsSkills};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Clone, Default)]
struct SkFile { path: String, saved: String, binary: bool, too_large: bool, read_only: bool, size: f64 }

#[derive(Clone)]
enum Pending {
    Upload { file: PathBuf, scope: String, name: String, existing: Vec<String>, needs_name: bool, message: String },
    Link { url: String, scope: String, skill: String, all: bool, existing: Vec<String> },
    Choose { url: String, scope: String, skills: Vec<(String, String)>, message: String },
    Transfer { from: (String, String), scope: String, as_: String, mv: bool, existing: Vec<String> },
}

#[derive(Default)]
struct Sk {
    loaded: bool,
    data: Option<Value>,
    load_error: String,
    error: String,
    open: Option<(String, String)>,
    detail: Option<Value>,
    file: Option<SkFile>,
    form: String,
    pending: Option<Pending>,
    target: String,
    show_synced: bool,
    link_ok: Option<bool>,
    link_busy: bool,
    busy: bool,
}

thread_local! {
    static SK: RefCell<Sk> = RefCell::new(Sk { target: "global".into(), ..Default::default() });
    static ROWS: Rc<VecModel<OpsSkillRow>> = kit::model();
    static FILES: Rc<VecModel<OpsSkillFile>> = kit::model();
    static PICKS: Rc<VecModel<OpsSkillPick>> = kit::model();
}

fn sk<R>(f: impl FnOnce(&mut Sk) -> R) -> R { SK.with(|s| f(&mut s.borrow_mut())) }

fn g(f: impl FnOnce(&OpsSkills)) { if let Some(w) = kit::ui() { f(&w.global::<OpsSkills>()); } }

pub fn init(w: &crate::MainWindow) {
    let gl = w.global::<OpsSkills>();
    gl.set_sk_rows(ROWS.with(kit::rc));
    gl.set_sd_files(FILES.with(kit::rc));
    gl.set_sk_picks(PICKS.with(kit::rc));
}

pub fn signed_out() {
    sk(|s| *s = Sk { target: "global".into(), ..Default::default() });
    g(|g| { g.set_sk_state("loading".into()); g.set_ed_text("".into()); });
}

// ─── Pure rules ───

fn scopes(d: &Value) -> Vec<Value> { d.get("scopes").and_then(|x| x.as_array()).cloned().unwrap_or_default() }
fn skills(d: &Value) -> Vec<Value> { d.get("skills").and_then(|x| x.as_array()).cloned().unwrap_or_default() }

/// skScopeLabel: "your Claude account" for synced, the scope's label, or its id.
pub fn scope_label(d: Option<&Value>, id: &str) -> String {
    if id == "synced" { return "your Claude account".into(); }
    d.map(scopes).unwrap_or_default().iter().find(|x| kit::s(x, "id") == id).map(|x| kit::s(x, "label")).unwrap_or_else(|| id.to_string())
}

/// A skill's meta: "over N files" or "N files, 1.2 KB".
pub fn meta_of(sk: &Value) -> String {
    let n = kit::n(sk, "files") as i64;
    if kit::truthy(sk.get("more").unwrap_or(&Value::Null)) { return format!("over {n} files"); }
    format!("{n} {}, {}", if n == 1 { "file" } else { "files" }, c::fmt_bytes(kit::n(sk, "size")))
}

/// The replace question (skPendingHtml's conflict note).
pub fn replace_text(existing: &[String], scope_label: &str) -> String {
    let many = existing.len() > 1;
    if many {
        format!("These skills already exist in {scope_label}: {}. Replacing them removes every file in them first.", existing.join(", "))
    } else {
        format!("A skill called {} already exists in {scope_label}. Replacing it removes every file in it first.", existing.first().cloned().unwrap_or_default())
    }
}

/// Where a skill came from (skSourceHtml), after its repository's name.
pub fn source_rest(src: &Value) -> String {
    let mut t = String::new();
    let r = kit::s(src, "ref"); if !r.is_empty() { t += &format!(", {r}"); }
    let p = kit::s(src, "path"); if !p.is_empty() { t += &format!(", {p}"); }
    let cm = kit::s(src, "commit"); if !cm.is_empty() { t += &format!(" at {}", &cm[..cm.len().min(7)]); }
    if let Some(ms) = c::ms_of(src, "installedAt") { let d = c::date_short(ms); if !d.is_empty() { t += &format!(". Installed {d}"); } }
    t + ". Update installs it again from there."
}

/// The list's rows, by group (drawSkills' group()).
fn rows_of(d: &Value, open: &Option<(String, String)>, show_synced: bool) -> Vec<OpsSkillRow> {
    let sc = scopes(d);
    let all = skills(d);
    let in_scope = |id: &str| all.iter().filter(|x| kit::s(x, "scope") == id).cloned().collect::<Vec<_>>();
    let mut out = Vec::new();
    let head = |title: String, path: String, toggle: &str| OpsSkillRow { kind: "group".into(), title: title.into(), path: path.into(), toggle: toggle.into(), ..Default::default() };
    let push_items = |out: &mut Vec<OpsSkillRow>, items: &[Value]| {
        if items.is_empty() { out.push(OpsSkillRow { kind: "empty".into(), ..Default::default() }); return; }
        let n = items.len();
        for (i, x) in items.iter().enumerate() {
            let (scope, name) = (kit::s(x, "scope"), kit::s(x, "name"));
            let origin = kit::s(x, "origin");
            let desc = [kit::s(x, "description"), kit::s(x, "problem")].into_iter().find(|v| !v.is_empty()).unwrap_or_else(|| "No description".into());
            out.push(OpsSkillRow {
                kind: "skill".into(), title: name.clone().into(),
                open: open.as_ref().is_some_and(|(s, n)| *s == scope && *n == name),
                scope: scope.into(), name: name.into(),
                meta: meta_of(x).into(), desc: desc.into(),
                bundled: origin == "bundled", synced: origin == "synced", linked: origin == "linked",
                github: x.get("source").is_some_and(kit::truthy), problem: !kit::s(x, "problem").is_empty(),
                first: i == 0, last: i + 1 == n,
                ..Default::default()
            });
        }
    };
    out.push(head("All projects".into(), sc.first().map(|x| kit::s(x, "path")).unwrap_or_default(), ""));
    push_items(&mut out, &in_scope("global"));
    for s in sc.iter().skip(1) {
        let it = in_scope(&kit::s(s, "id"));
        if !it.is_empty() { out.push(head(kit::s(s, "label"), kit::s(s, "path"), "")); push_items(&mut out, &it); }
    }
    let synced = in_scope("synced");
    if !synced.is_empty() {
        out.push(head(format!("Synced from your Claude account ({})", synced.len()), String::new(), if show_synced { "Hide" } else { "Show" }));
        if show_synced { push_items(&mut out, &synced); }
    }
    out
}

// ─── Loading ───

/// Load the list (skLoad); `keep` is an error to show again after it.
pub fn load(keep: Option<String>) {
    sk(|s| if !s.loaded { s.loaded = true; });
    kit::call(|api| api.get("/api/skills"), move |r| {
        match r {
            Ok(d) if ok(&d) => sk(|s| { s.data = Some(d); s.error.clear(); s.load_error.clear(); }),
            Ok(d) => sk(|s| { let e = c::err_of(&d, "Could not load the skills"); s.error = e.clone(); s.load_error = e; }),
            Err(e) => sk(|s| { s.error = e.clone(); s.load_error = e; }),
        }
        let after = move || {
            sk(|s| {
                if let Some(d) = &s.data {
                    if !scopes(d).iter().any(|x| kit::s(x, "id") == s.target) { s.target = "global".into(); }
                    if let Some((sc, n)) = &s.open {
                        if !skills(d).iter().any(|x| kit::s(x, "scope") == *sc && kit::s(x, "name") == *n) { s.open = None; s.detail = None; s.file = None; s.form.clear(); }
                    }
                }
                if let Some(k) = &keep { s.error = k.clone(); }
            });
            if sk(|s| s.open.is_some()) { load_detail(draw); } else { draw(); }
        };
        if sk(|s| s.link_ok.is_none()) {
            // Install from a link is newer than the rest: ask the server.
            kit::call(|api| api.get("/api/handshake"), move |h| {
                if let Ok(h) = h {
                    let caps = h.pointer("/server/capabilities").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    sk(|s| s.link_ok = Some(caps.iter().any(|x| x.as_str() == Some("skills-install-link"))));
                }
                after();
            });
        } else { after(); }
    });
}

fn load_detail(then: fn()) {
    let Some((scope, name)) = sk(|s| s.open.clone()) else { then(); return };
    let url = format!("/api/skills/detail?{}", c::query(&[("scope", &scope), ("name", &name)]));
    kit::call(move |api| api.get(&url), move |r| {
        let d = match r { Ok(d) if ok(&d) => d, Ok(d) => json!({ "error": c::err_of(&d, "Could not open this skill") }), Err(e) => json!({ "error": e }) };
        sk(|s| {
            if let Some(f) = &s.file {
                let files = d.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                if d.get("files").is_some() && !files.iter().any(|x| kit::s(x, "path") == f.path) { s.file = None; }
            }
            s.detail = Some(d);
        });
        then();
    });
}

fn open_skill(s: &Sk) -> Option<Value> {
    let (sc, n) = s.open.as_ref()?;
    skills(s.data.as_ref()?).into_iter().find(|x| kit::s(x, "scope") == *sc && kit::s(x, "name") == *n)
}

// ─── Drawing ───

pub fn draw() {
    let Some(w) = kit::ui() else { return };
    let gl = w.global::<OpsSkills>();
    sk(|s| {
        let Some(d) = s.data.clone() else {
            gl.set_sk_state(if s.loaded && !s.load_error.is_empty() { "error".into() } else { "loading".into() });
            gl.set_sk_load_error(if s.load_error.is_empty() { "Could not load the skills".into() } else { s.load_error.clone().into() });
            return;
        };
        gl.set_sk_state("data".into());
        let sc = scopes(&d);
        let labels: Vec<slint::SharedString> = sc.iter().map(|x| kit::s(x, "label").into()).collect();
        gl.set_sk_scopes(ModelRc::new(VecModel::from(labels)));
        gl.set_sk_target(sc.iter().position(|x| kit::s(x, "id") == s.target).unwrap_or(0) as i32);
        let mb = (d.pointer("/limits/upload").and_then(|x| x.as_f64()).unwrap_or(52428800.0) / 1048576.0).round() as i64;
        gl.set_sk_upload_hint(format!("Upload a .zip or .skill archive holding one or more skills, or a single SKILL.md. Up to {mb} MB.").into());
        gl.set_sk_link_off(s.link_ok == Some(false));
        gl.set_sk_link_busy(s.link_busy);
        gl.set_sk_error(s.error.clone().into());
        gl.set_sk_busy(s.busy);
        gl.set_sk_form(s.form.clone().into());
        gl.set_sk_new_label(format!("A new, empty skill in {}. The description is how Claude decides when to use it.", scope_label(Some(&d), &s.target)).into());
        // What is waiting on an answer.
        let (kind, text, picks) = match &s.pending {
            None => ("", String::new(), vec![]),
            Some(Pending::Choose { message, skills, .. }) => ("choose", message.clone(), skills.iter().map(|(n, ds)| OpsSkillPick { name: n.into(), description: ds.into() }).collect()),
            Some(Pending::Upload { needs_name: true, message, .. }) => ("name", message.clone(), vec![]),
            Some(Pending::Upload { existing, scope, .. }) | Some(Pending::Link { existing, scope, .. }) | Some(Pending::Transfer { existing, scope, .. }) => ("replace", replace_text(existing, &scope_label(Some(&d), scope)), vec![]),
        };
        gl.set_sk_pending(kind.into());
        gl.set_sk_pending_text(text.into());
        PICKS.with(|m| kit::sync_rows(m, picks));
        ROWS.with(|m| kit::sync_rows(m, rows_of(&d, &s.open, s.show_synced)));
        // The open skill.
        let sk_v = open_skill(s);
        match (&sk_v, &s.detail) {
            (_, None) => gl.set_sd_state("loading".into()),
            (_, Some(dt)) if dt.get("error").is_some() => { gl.set_sd_state("error".into()); gl.set_sd_error(kit::s(dt, "error").into()); }
            (Some(skv), Some(dt)) => {
                gl.set_sd_state("data".into());
                let ro = skv.get("readOnly").is_some_and(kit::truthy);
                let origin = kit::s(skv, "origin");
                let src = skv.get("source").filter(|x| kit::truthy(x)).cloned();
                gl.set_sd_ro(ro);
                gl.set_sd_ro_reason(kit::s(dt, "readOnlyReason").into());
                let pr = kit::s(skv, "problem");
                gl.set_sd_problem(if pr.is_empty() { "".into() } else { format!("{pr}.").into() });
                gl.set_sd_has_source(src.is_some());
                if let Some(src) = &src {
                    let repo = kit::s(src, "repo");
                    let url = kit::s(src, "url");
                    gl.set_sd_src_repo(repo.clone().into());
                    gl.set_sd_src_url(if url.is_empty() { format!("https://github.com/{repo}").into() } else { url.into() });
                    gl.set_sd_src_rest(source_rest(src).into());
                }
                gl.set_sd_can_update(!ro && src.is_some() && s.link_ok != Some(false));
                gl.set_sd_can_delete(origin == "user" || origin == "linked");
                let files = dt.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let on = s.file.as_ref().map(|f| f.path.clone()).unwrap_or_default();
                FILES.with(|m| kit::sync_rows(m, files.iter().map(|f| {
                    let p = kit::s(f, "path");
                    OpsSkillFile { deletable: !ro && p != "SKILL.md", on: p == on, size: c::fmt_bytes(kit::n(f, "size")).into(), path: p.into() }
                }).collect()));
                gl.set_sd_more(if dt.get("more").is_some_and(kit::truthy) { format!("Only the first {} files are listed. This folder holds more.", files.len()).into() } else { "".into() });
                let sl = kit::n(dt, "skippedLinks") as i64;
                gl.set_sd_links(if sl > 0 { format!("{sl}{}", if sl == 1 { " link in this folder is not shown." } else { " links in this folder are not shown." }).into() } else { "".into() });
            }
            (None, Some(_)) => gl.set_sd_state("loading".into()),
        }
        match &s.file {
            Some(f) => {
                gl.set_ed_open(true);
                gl.set_ed_path(f.path.clone().into());
                gl.set_ed_size(c::fmt_bytes(f.size).into());
                gl.set_ed_binary(f.binary);
                gl.set_ed_ro(f.read_only);
                gl.set_ed_note(format!("{} Download it to look at it.", if f.too_large { "This file is too large to open here." } else { "This is not a text file, so it cannot be edited here." }).into());
            }
            None => gl.set_ed_open(false),
        }
    });
}

// ─── Leaving a file with changes ───

fn dirty() -> bool {
    let text = kit::ui().map(|w| w.global::<OpsSkills>().get_ed_text().to_string()).unwrap_or_default();
    sk(|s| s.file.as_ref().is_some_and(|f| !f.binary && !f.read_only && text != f.saved))
}

/// skLeaveFile: go on when nothing is unsaved, or once the person agrees.
fn leave_file(then: impl FnOnce() + 'static) {
    if !dirty() { then(); return; }
    kit::confirm(kit::Confirm { title: "Discard changes?", message: "Discard the changes you have not saved?", ok: "Discard", danger: true }, move |yes| if yes { then(); });
}

fn busy(on: bool) { sk(|s| s.busy = on); g(|g| g.set_sk_busy(on)); }

fn field(get: impl FnOnce(&OpsSkills) -> slint::SharedString) -> String {
    kit::ui().map(|w| get(&w.global::<OpsSkills>()).trim().to_string()).unwrap_or_default()
}

// ─── Actions (the buttons: OpsSkills.act) ───

pub fn act(action: &str, arg: &str) {
    let arg = arg.to_string();
    match action {
        "target" => {
            let id = sk(|s| s.data.as_ref().map(scopes).unwrap_or_default().get(arg.parse::<usize>().unwrap_or(0)).map(|x| kit::s(x, "id"))).unwrap_or_else(|| "global".into());
            let redraw = sk(|s| { s.target = id; s.form == "new" });
            if redraw { draw(); }
        }
        "upload" => pick_upload(),
        "asset" => pick_asset(),
        "synced-toggle" => { sk(|s| s.show_synced = !s.show_synced); draw(); }
        "form-close" => { sk(|s| s.form.clear()); draw(); }
        "pending-cancel" => { sk(|s| s.pending = None); draw(); }
        "new" => {
            sk(|s| { s.form = if s.form == "new" { String::new() } else { "new".into() }; s.pending = None; });
            g(|g| { g.set_sk_new_name("".into()); g.set_sk_new_desc("".into()); });
            draw();
        }
        "open" => {
            let Some((scope, name)) = arg.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string())) else { return };
            leave_file(move || {
                let same = sk(|s| { s.file = None; s.form.clear(); s.detail = None; s.open.as_ref().is_some_and(|(a, b)| *a == scope && *b == name) });
                if same { sk(|s| s.open = None); draw(); return; }
                sk(|s| s.open = Some((scope, name)));
                draw();
                load_detail(draw);
            });
        }
        "rename" | "transfer" | "newfile" => {
            let on = sk(|s| { s.form = if s.form == action { String::new() } else { action.to_string() }; !s.form.is_empty() });
            if on {
                let (name, to) = sk(|s| {
                    let (sc, n) = s.open.clone().unwrap_or_default();
                    let list = s.data.as_ref().map(scopes).unwrap_or_default();
                    // The other place: "every project" from a project, the first project from "every project".
                    let to_id = if sc == "global" || sc == "synced" { list.iter().map(|x| kit::s(x, "id")).find(|x| *x != sc).unwrap_or_else(|| "global".into()) } else { "global".into() };
                    (n, list.iter().position(|x| kit::s(x, "id") == to_id).unwrap_or(0) as i32)
                });
                g(|g| { g.set_sd_rename(name.clone().into()); g.set_sd_as(name.into()); g.set_sd_to(to); g.set_sd_newfile("".into()); });
            }
            draw();
        }
        "open-url" => c::open_url(&arg),
        _ => act_busy(action, arg),
    }
}

/// The actions that talk to the server.
fn act_busy(action: &str, arg: String) {
    match action {
        "new-go" => {
            let (scope, name, desc) = (sk(|s| s.target.clone()), field(|g| g.get_sk_new_name()), field(|g| g.get_sk_new_desc()));
            busy(true);
            c::post("/api/skills/create", json!({ "scope": scope, "name": name, "description": desc }), |d| {
                busy(false);
                if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not create it")); return; }
                let sk_v = d.get("skill").cloned().unwrap_or_default();
                sk(|s| { s.form.clear(); s.open = Some((kit::s(&sk_v, "scope"), kit::s(&sk_v, "name"))); s.detail = None; s.file = None; });
                load_then(None, || open_file("SKILL.md"));
            });
        }
        "pending-replace" => {
            let Some(p) = sk(|s| s.pending.clone()) else { return };
            match p {
                Pending::Upload { file, scope, name, .. } => upload(file, scope, name, true),
                Pending::Link { url, scope, skill, all, .. } => link_install(url, scope, skill, all, true),
                Pending::Transfer { from, scope, as_, mv, .. } => transfer(from, scope, as_, mv, true),
                Pending::Choose { .. } => {}
            }
        }
        "pending-name" => {
            let Some(Pending::Upload { file, scope, .. }) = sk(|s| s.pending.clone()) else { return };
            upload(file, scope, field(|g| g.get_sk_pending_name()), false);
        }
        "link-go" => {
            let url = field(|g| g.get_sk_link_text());
            if url.is_empty() { return; }
            let scope = sk(|s| { s.form.clear(); s.error.clear(); s.pending = None; s.target.clone() });
            link_install(url, scope, String::new(), false, false);
        }
        "link-pick" | "link-all" => {
            let Some(Pending::Choose { url, scope, .. }) = sk(|s| s.pending.clone()) else { return };
            link_install(url, scope, if action == "link-pick" { arg } else { String::new() }, action == "link-all", false);
        }
        _ => act_open(action, arg),
    }
}

/// The open skill's actions.
fn act_open(action: &str, arg: String) {
    let Some((scope, name)) = sk(|s| s.open.clone()) else { return };
    match action {
        "update" => leave_file(move || {
            let msg = format!("Install \"{name}\" again from where it came from? This replaces its files, including any changes made here.");
            kit::confirm(kit::Confirm { title: "Update skill", message: &msg, ok: "Update", danger: false }, move |yes| if yes { update(scope, name); });
        }),
        "download" => c::download(format!("/api/skills/download?{}", c::query(&[("scope", &scope), ("name", &name)])), format!("{name}.zip")),
        "delete" => {
            let msg = format!("Delete the skill \"{name}\" and all its files? This cannot be undone.");
            kit::confirm(kit::Confirm { title: "Delete skill", message: &msg, ok: "Delete", danger: true }, move |yes| {
                if !yes { return; }
                busy(true);
                c::post("/api/skills/delete", json!({ "scope": scope, "name": name }), move |d| {
                    busy(false);
                    if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not delete it")); return; }
                    kit::toast(&format!("Deleted {name}"));
                    sk(|s| { s.open = None; s.detail = None; s.file = None; s.form.clear(); });
                    load(None);
                });
            });
        }
        "rename-go" => leave_file(move || {
            busy(true);
            c::post("/api/skills/rename", json!({ "scope": scope, "name": name, "to": field(|g| g.get_sd_rename()) }), |d| {
                busy(false);
                if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not rename it")); return; }
                let v = d.get("skill").cloned().unwrap_or_default();
                sk(|s| { s.form.clear(); s.file = None; s.open = Some((kit::s(&v, "scope"), kit::s(&v, "name"))); });
                load(None);
            });
        }),
        "copy-go" | "move-go" => {
            let mv = action == "move-go";
            let to = sk(|s| s.data.as_ref().map(scopes).unwrap_or_default()).get(kit::ui().map(|w| w.global::<OpsSkills>().get_sd_to()).unwrap_or(0).max(0) as usize).map(|x| kit::s(x, "id")).unwrap_or_else(|| "global".into());
            let as_ = field(|g| g.get_sd_as());
            let go = move || transfer((scope, name), to, as_, mv, false);
            if mv { leave_file(go); } else { go(); }
        }
        "file" => {
            if sk(|s| s.file.as_ref().is_some_and(|f| f.path == arg)) { return; }
            leave_file(move || open_file(&arg));
        }
        "file-close" => leave_file(|| { sk(|s| s.file = None); draw(); }),
        "file-download" => {
            let Some(p) = sk(|s| s.file.as_ref().map(|f| f.path.clone())) else { return };
            let leaf = p.rsplit('/').next().unwrap_or(&p).to_string();
            c::download(format!("/api/skills/file-download?{}", c::query(&[("scope", &scope), ("name", &name), ("path", &p)])), leaf);
        }
        "save" => {
            let Some(p) = sk(|s| s.file.as_ref().map(|f| f.path.clone())) else { return };
            let text = kit::ui().map(|w| w.global::<OpsSkills>().get_ed_text().to_string()).unwrap_or_default();
            busy(true);
            c::post("/api/skills/file", json!({ "scope": scope, "name": name, "path": p, "content": text }), move |d| {
                busy(false);
                if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not save it")); return; }
                sk(|s| if let Some(f) = &mut s.file { f.saved = text.clone(); f.size = kit::n(&d, "size"); });
                kit::toast(&format!("Saved {p}"));
                load(None);
            });
        }
        "newfile-go" => {
            let path = field(|g| g.get_sd_newfile());
            if path.is_empty() { return; }
            let exists = sk(|s| s.detail.as_ref().and_then(|d| d.get("files")).and_then(|x| x.as_array()).is_some_and(|fs| fs.iter().any(|f| kit::s(f, "path") == path)));
            if exists { kit::toast_error("That file already exists"); return; }
            leave_file(move || {
                busy(true);
                c::post("/api/skills/file", json!({ "scope": scope, "name": name, "path": path, "content": "" }), |d| {
                    busy(false);
                    if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not create it")); return; }
                    sk(|s| s.form.clear());
                    let p = kit::s(&d, "path");
                    load_then(None, move || open_file(&p));
                });
            });
        }
        "file-delete" => {
            let msg = format!("Delete {arg} from this skill?");
            kit::confirm(kit::Confirm { title: "Delete file", message: &msg, ok: "Delete", danger: true }, move |yes| {
                if !yes { return; }
                busy(true);
                c::post("/api/skills/file-delete", json!({ "scope": scope, "name": name, "path": arg }), move |d| {
                    busy(false);
                    if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not delete it")); return; }
                    sk(|s| if s.file.as_ref().is_some_and(|f| f.path == arg) { s.file = None; });
                    load(None);
                });
            });
        }
        _ => {}
    }
}

/// skLoad, then something more (open a file).
fn load_then(keep: Option<String>, then: impl FnOnce() + 'static) {
    thread_local! { static NEXT: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) }; }
    NEXT.with(|n| *n.borrow_mut() = Some(Box::new(then)));
    kit::call(|api| api.get("/api/skills"), move |r| {
        if let Ok(d) = r { if ok(&d) { sk(|s| { s.data = Some(d); s.error.clear(); }); } }
        if let Some(k) = keep { sk(|s| s.error = k); }
        load_detail(|| { draw(); NEXT.with(|n| if let Some(f) = n.borrow_mut().take() { f(); }); });
    });
}

fn open_file(path: &str) {
    let Some((scope, name)) = sk(|s| s.open.clone()) else { return };
    let url = format!("/api/skills/file?{}", c::query(&[("scope", &scope), ("name", &name), ("path", path)]));
    kit::call(move |api| api.get(&url), |r| {
        let d = match r { Ok(d) if ok(&d) => d, Ok(d) => { kit::toast_error(&c::err_of(&d, "Could not open that file")); return; } Err(e) => { kit::toast_error(&e); return; } };
        let text = kit::s(&d, "text");
        sk(|s| s.file = Some(SkFile {
            path: kit::s(&d, "path"), saved: text.clone(),
            binary: d.get("binary").is_some_and(kit::truthy), too_large: d.get("tooLarge").is_some_and(kit::truthy),
            read_only: d.get("readOnly").is_some_and(kit::truthy), size: kit::n(&d, "size"),
        }));
        g(|g| g.set_ed_text(text.into()));
        draw();
    });
}

// ─── Upload, link, update, transfer ───

fn pick(filters: bool) -> Option<PathBuf> {
    crate::testctl::take_pick().or_else(|| {
        let d = rfd::FileDialog::new();
        if filters { d.add_filter("Skills", &["zip", "skill", "md"]).pick_file() } else { d.pick_file() }
    })
}

fn pick_upload() {
    let scope = sk(|s| s.target.clone());
    kit::bg(|| pick(true), move |p| {
        let Some(p) = p else { return };
        let limit = sk(|s| s.data.as_ref().and_then(|d| d.pointer("/limits/upload")).and_then(|x| x.as_f64()));
        let size = std::fs::metadata(&p).map(|m| m.len() as f64).unwrap_or(0.0);
        if let Some(l) = limit { if size > l { sk(|s| s.error = format!("That file is too large (the limit is {} MB)", (l / 1048576.0).round() as i64)); draw(); return; } }
        sk(|s| { s.form.clear(); s.error.clear(); });
        upload(p, scope, String::new(), false);
    });
}

fn upload(file: PathBuf, scope: String, name: String, overwrite: bool) {
    let fname = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let url = format!("/api/skills/upload?{}", c::query(&[("scope", &scope), ("filename", &fname), ("name", &name), ("overwrite", if overwrite { "1" } else { "" })]));
    let f2 = file.clone();
    busy(true);
    kit::call(move |api| {
        let bytes = std::fs::read(&f2).map_err(|e| ApiError::Net(e.to_string()))?;
        c::raw(api.post_with_progress(&url, &bytes, &|_, _| {}))
    }, move |r| {
        busy(false);
        let d = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
        if ok(&d) {
            let inst = d.get("installed").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            let n: Vec<String> = inst.iter().map(|x| kit::s(x, "name")).collect();
            kit::toast(&installed_text(&n));
            let dropped: Vec<String> = d.get("dropped").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let keep = (!dropped.is_empty()).then(|| format!("Left out of the upload: {}", dropped.join(", ")));
            sk(|s| {
                s.pending = None;
                s.error = keep.clone().unwrap_or_default();
                if n.len() == 1 { s.open = Some((kit::s(&inst[0], "scope"), n[0].clone())); s.detail = None; s.file = None; s.form.clear(); }
            });
            load(keep);
            return;
        }
        let existing: Vec<String> = d.get("existing").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        if d.get("conflict").is_some_and(kit::truthy) {
            sk(|s| { s.pending = Some(Pending::Upload { file, scope, name, existing, needs_name: false, message: String::new() }); s.error.clear(); });
        } else if d.get("needsName").is_some_and(kit::truthy) {
            g(|g| g.set_sk_pending_name("".into()));
            sk(|s| { s.pending = Some(Pending::Upload { file, scope, name: String::new(), existing: vec![], needs_name: true, message: "This upload does not say what the skill is called. Give it a name: lowercase letters, digits and hyphens.".into() }); s.error.clear(); });
        } else {
            sk(|s| { s.pending = None; s.error = c::err_of(&d, "Could not install that"); });
        }
        draw();
    });
}

/// "Installed x. New chats will have it." (one, or "N skills … them").
pub fn installed_text(n: &[String]) -> String {
    if n.len() == 1 { format!("Installed {}. New chats will have it.", n[0]) } else { format!("Installed {} skills. New chats will have them.", n.len()) }
}

fn link_install(url: String, scope: String, skill: String, all: bool, overwrite: bool) {
    sk(|s| s.link_busy = true);
    draw();
    let body = json!({ "url": url, "scope": scope, "skill": skill, "all": all, "overwrite": overwrite });
    kit::call(move |api| c::post_raw(api, "/api/skills/install-from-link", &body), move |r| {
        sk(|s| s.link_busy = false);
        let d = match r {
            Ok(d) => d,
            Err(e) if e == "Not supported by this server" => { sk(|s| s.link_ok = Some(false)); json!({ "ok": false, "error": "This server does not support this yet. Update the server to use it." }) }
            Err(e) => json!({ "ok": false, "error": e }),
        };
        if ok(&d) {
            let inst = d.get("installed").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            let n: Vec<String> = inst.iter().map(|x| kit::s(x, "name")).collect();
            kit::toast(&installed_text(&n));
            g(|g| g.set_sk_link_text("".into()));
            sk(|s| {
                s.pending = None; s.error.clear();
                if n.len() == 1 { s.open = Some((kit::s(&inst[0], "scope"), n[0].clone())); s.detail = None; s.file = None; s.form.clear(); }
            });
            load(None);
            return;
        }
        if d.get("choose").is_some_and(kit::truthy) {
            let skills = d.get("skills").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter().map(|x| (kit::s(x, "name"), kit::s(x, "description"))).collect();
            sk(|s| { s.pending = Some(Pending::Choose { url, scope, skills, message: kit::s(&d, "error") }); s.error.clear(); });
        } else if d.get("conflict").is_some_and(kit::truthy) {
            let existing = d.get("existing").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            sk(|s| { s.pending = Some(Pending::Link { url, scope, skill, all, existing }); s.error.clear(); });
        } else {
            sk(|s| { s.pending = None; s.error = c::err_of(&d, "Could not install that"); });
        }
        draw();
    });
}

fn update(scope: String, name: String) {
    busy(true);
    let body = json!({ "scope": scope, "name": name });
    kit::call(move |api| c::post_raw(api, "/api/skills/update-from-source", &body), move |r| {
        busy(false);
        let d = match r {
            Ok(d) => d,
            Err(e) if e == "Not supported by this server" => { sk(|s| s.link_ok = Some(false)); json!({ "ok": false, "error": "This server does not support this yet. Update the server to use it." }) }
            Err(e) => json!({ "ok": false, "error": e }),
        };
        if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not update it")); draw(); return; }
        if d.get("upToDate").is_some_and(kit::truthy) { kit::toast(&format!("{name} is up to date")); return; }
        let commit = d.pointer("/source/commit").and_then(|x| x.as_str()).unwrap_or("");
        let to = if commit.is_empty() { String::new() } else { format!(" to {}", &commit[..commit.len().min(7)]) };
        kit::toast(&format!("Updated {name}{to}. New chats will have it."));
        sk(|s| s.file = None);
        load(None);
    });
}

fn transfer(from: (String, String), scope: String, as_: String, mv: bool, overwrite: bool) {
    busy(true);
    let body = json!({ "scope": from.0, "name": from.1, "to": scope, "as": as_, "move": mv, "overwrite": overwrite });
    c::post("/api/skills/transfer", body, move |d| {
        busy(false);
        if ok(&d) {
            let v = d.get("skill").cloned().unwrap_or_default();
            let label = sk(|s| scope_label(s.data.as_ref(), &kit::s(&v, "scope")));
            kit::toast(&format!("{}{label}", if mv { "Moved to " } else { "Copied to " }));
            sk(|s| { s.pending = None; s.form.clear(); s.file = None; s.open = Some((kit::s(&v, "scope"), kit::s(&v, "name"))); s.detail = None; });
            load(None);
            return;
        }
        if d.get("conflict").is_some_and(kit::truthy) {
            let existing = d.get("existing").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            sk(|s| s.pending = Some(Pending::Transfer { from, scope, as_, mv, existing }));
            draw();
            return;
        }
        sk(|s| s.pending = None);
        kit::toast_error(&c::err_of(&d, "Could not do that"));
        draw();
    });
}

/// "Upload file" in a skill: one file into it (asks before replacing).
fn pick_asset() {
    kit::bg(|| pick(false), |p| {
        let Some(p) = p else { return };
        let Some((scope, name)) = sk(|s| s.open.clone()) else { return };
        let fname = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let go = {
            let fname = fname.clone();
            move || {
                let url = format!("/api/skills/file-upload?{}", c::query(&[("scope", &scope), ("name", &name), ("path", &fname)]));
                busy(true);
                kit::call(move |api| {
                    let bytes = std::fs::read(&p).map_err(|e| ApiError::Net(e.to_string()))?;
                    c::raw(api.post_with_progress(&url, &bytes, &|_, _| {}))
                }, move |r| {
                    busy(false);
                    let d = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
                    if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not add that file")); return; }
                    kit::toast(&format!("Added {fname}"));
                    load(None);
                });
            }
        };
        let exists = sk(|s| s.detail.as_ref().and_then(|d| d.get("files")).and_then(|x| x.as_array()).is_some_and(|fs| fs.iter().any(|f| kit::s(f, "path") == fname)));
        if exists {
            let msg = format!("Replace {fname} in this skill?");
            kit::confirm(kit::Confirm { title: "Replace file", message: &msg, ok: "Replace", danger: false }, move |yes| if yes { go(); });
        } else { go(); }
    });
}

// ─── The driver ───

pub fn props() -> Value {
    let text = kit::ui().map(|w| w.global::<OpsSkills>().get_ed_text().to_string()).unwrap_or_default();
    let dirty = dirty();
    sk(|s| {
        let rows: Vec<Value> = ROWS.with(|m| (0..slint::Model::row_count(&**m)).filter_map(|i| slint::Model::row_data(&**m, i)).map(|r| json!({ "kind": r.kind.as_str(), "title": r.title.as_str(), "scope": r.scope.as_str(), "name": r.name.as_str(), "open": r.open, "meta": r.meta.as_str(), "toggle": r.toggle.as_str() })).collect());
        let files: Vec<String> = FILES.with(|m| (0..slint::Model::row_count(&**m)).filter_map(|i| slint::Model::row_data(&**m, i)).map(|f| f.path.to_string()).collect());
        json!({
            "state": if s.data.is_some() { "data" } else if s.load_error.is_empty() { "loading" } else { "error" },
            "scopes": s.data.as_ref().map(scopes).unwrap_or_default().iter().map(|x| kit::s(x, "id")).collect::<Vec<_>>(),
            "target": s.target, "rows": rows, "form": s.form, "error": s.error,
            "open": s.open.as_ref().map(|(a, b)| json!({ "scope": a, "name": b })),
            "detail": s.detail.as_ref().map(|d| if d.get("error").is_some() { "error" } else { "data" }).unwrap_or(if s.open.is_some() { "loading" } else { "" }),
            "files": files,
            "pending": match &s.pending { None => Value::Null, Some(Pending::Choose { skills, .. }) => json!({ "kind": "choose", "skills": skills.iter().map(|x| x.0.clone()).collect::<Vec<_>>() }), Some(Pending::Upload { needs_name: true, .. }) => json!({ "kind": "name" }), Some(_) => json!({ "kind": "replace" }) },
            "editor": s.file.as_ref().map(|f| json!({ "path": f.path, "binary": f.binary, "readOnly": f.read_only, "dirty": dirty, "text": text })),
            "linkOk": s.link_ok, "linkBusy": s.link_busy, "busy": s.busy,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Value {
        json!({ "ok": true,
            "scopes": [{ "id": "global", "label": "Every project", "path": "~/.claude/skills" }, { "id": "p:demo", "label": "demo", "path": "/x/.claude/skills" }],
            "skills": [
                { "scope": "global", "name": "a", "files": 1, "size": 1536, "origin": "user", "description": "A" },
                { "scope": "global", "name": "b", "files": 400, "more": true, "origin": "bundled" },
                { "scope": "synced", "name": "s", "files": 2, "size": 10, "origin": "synced", "problem": "No SKILL.md" },
            ] })
    }

    #[test]
    fn groups_and_rows() {
        let r = rows_of(&data(), &Some(("global".into(), "a".into())), false);
        let kinds: Vec<&str> = r.iter().map(|x| x.kind.as_str()).collect();
        // All projects (2 skills); the empty project scope is left out; synced folded.
        assert_eq!(kinds, ["group", "skill", "skill", "group"]);
        assert_eq!(r[0].title.as_str(), "All projects");
        assert!(r[1].open && r[1].first && !r[1].last && r[2].last);
        assert_eq!(r[1].meta.as_str(), "1 file, 1.5 KB");
        assert_eq!(r[2].meta.as_str(), "over 400 files");
        assert!(r[2].bundled && r[2].desc.as_str() == "No description");
        assert_eq!(r[3].title.as_str(), "Synced from your Claude account (1)");
        assert_eq!(r[3].toggle.as_str(), "Show");
        let r = rows_of(&data(), &None, true);
        assert_eq!(r.last().unwrap().desc.as_str(), "No SKILL.md");
        assert!(r.last().unwrap().problem);
    }

    #[test]
    fn empty_global_says_so() {
        let r = rows_of(&json!({ "scopes": [{ "id": "global", "label": "G", "path": "p" }], "skills": [] }), &None, false);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].kind.as_str(), "empty");
    }

    #[test]
    fn words() {
        assert_eq!(scope_label(Some(&data()), "p:demo"), "demo");
        assert_eq!(scope_label(Some(&data()), "synced"), "your Claude account");
        assert_eq!(replace_text(&["x".into()], "demo"), "A skill called x already exists in demo. Replacing it removes every file in it first.");
        assert_eq!(replace_text(&["x".into(), "y".into()], "demo"), "These skills already exist in demo: x, y. Replacing them removes every file in them first.");
        assert_eq!(installed_text(&["x".into()]), "Installed x. New chats will have it.");
        assert_eq!(installed_text(&["x".into(), "y".into()]), "Installed 2 skills. New chats will have them.");
        assert_eq!(source_rest(&json!({ "repo": "o/r", "ref": "main", "path": "skills/x", "commit": "abcdef123" })), ", main, skills/x at abcdef1. Update installs it again from there.");
    }
}
