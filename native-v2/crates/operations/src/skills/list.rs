//! Settings → Skills (the web's renderSkillsSection, drawSkills,
//! onSkillsClick, skUpload, skLinkInstall, skUpdate, skTransfer,
//! skOpenFile): the Claude skills on this machine, by where they live.

use super::common::{self as c, ok};
use super::{get, upd, Sk};
use crundi_core::api::ApiError;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub(crate) struct SkFile { path: String, saved: String, binary: bool, too_large: bool, read_only: bool, size: f64 }

#[derive(Clone)]
pub(crate) enum Pending {
    Upload { file: PathBuf, scope: String, name: String, existing: Vec<String>, needs_name: bool, message: String },
    Link { url: String, scope: String, skill: String, all: bool, existing: Vec<String> },
    Choose { url: String, scope: String, skills: Vec<(String, String)>, message: String },
    Transfer { from: (String, String), scope: String, as_: String, mv: bool, existing: Vec<String> },
}

#[derive(Default)]
pub(crate) struct List {
    loaded: bool,
    data: Option<Value>,
    load_error: String,
    error: String,
    open: Option<(String, String)>,
    detail: Option<Value>,
    file: Option<SkFile>,
    form: String,
    pending: Option<Pending>,
    pub(crate) target: String,
    show_synced: bool,
    link_ok: Option<bool>,
    link_busy: bool,
    busy: bool,
    /// The transfer form's "where to" (an index into the scopes).
    pub(crate) to: usize,
}

// ─── Pure rules ───

fn scopes(d: &Value) -> Vec<Value> { d.get("scopes").and_then(|x| x.as_array()).cloned().unwrap_or_default() }
fn skills(d: &Value) -> Vec<Value> { d.get("skills").and_then(|x| x.as_array()).cloned().unwrap_or_default() }
fn strs(d: &Value, k: &str) -> Vec<String> { d.get(k).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default() }

/// skScopeLabel: "your Claude account" for synced, the scope's label, or its id.
pub fn scope_label(d: Option<&Value>, id: &str) -> String {
    if id == "synced" { return "your Claude account".into(); }
    d.map(scopes).unwrap_or_default().iter().find(|x| c::s(x, "id") == id).map(|x| c::s(x, "label")).unwrap_or_else(|| id.to_string())
}

/// A skill's meta: "over N files" or "N files, 1.2 KB".
pub fn meta_of(sk: &Value) -> String {
    let n = c::n(sk, "files") as i64;
    if c::t(sk, "more") { return format!("over {n} files"); }
    format!("{n} {}, {}", if n == 1 { "file" } else { "files" }, c::fmt_bytes(c::n(sk, "size")))
}

/// The replace question (skPendingHtml's conflict note).
pub fn replace_text(existing: &[String], scope_label: &str) -> String {
    if existing.len() > 1 {
        format!("These skills already exist in {scope_label}: {}. Replacing them removes every file in them first.", existing.join(", "))
    } else {
        format!("A skill called {} already exists in {scope_label}. Replacing it removes every file in it first.", existing.first().cloned().unwrap_or_default())
    }
}

/// Where a skill came from (skSourceHtml), after its repository's name.
pub fn source_rest(src: &Value) -> String {
    let mut t = String::new();
    let r = c::s(src, "ref"); if !r.is_empty() { t += &format!(", {r}"); }
    let p = c::s(src, "path"); if !p.is_empty() { t += &format!(", {p}"); }
    let cm = c::s(src, "commit"); if !cm.is_empty() { t += &format!(" at {}", &cm[..cm.len().min(7)]); }
    if let Some(ms) = c::ms_of(src, "installedAt") { let d = c::date_short(ms); if !d.is_empty() { t += &format!(". Installed {d}"); } }
    t + ". Update installs it again from there."
}

/// "Installed x. New chats will have it." (one, or "N skills … them").
pub fn installed_text(n: &[String]) -> String {
    if n.len() == 1 { format!("Installed {}. New chats will have it.", n[0]) } else { format!("Installed {} skills. New chats will have them.", n.len()) }
}

/// One row of the list: a group's head, "None yet." or a skill.
#[derive(Clone, Default, Debug)]
pub struct Row {
    pub kind: &'static str, pub title: String, pub path: String, pub toggle: String,
    pub scope: String, pub name: String, pub open: bool, pub meta: String, pub desc: String,
    pub bundled: bool, pub synced: bool, pub linked: bool, pub github: bool, pub problem: bool,
    pub first: bool, #[allow(dead_code)] pub last: bool,
}

/// The list's rows, by group (drawSkills' group()).
pub fn rows_of(d: &Value, open: &Option<(String, String)>, show_synced: bool) -> Vec<Row> {
    let sc = scopes(d);
    let all = skills(d);
    let in_scope = |id: &str| all.iter().filter(|x| c::s(x, "scope") == id).cloned().collect::<Vec<_>>();
    let mut out = Vec::new();
    let head = |title: String, path: String, toggle: &str| Row { kind: "group", title, path, toggle: toggle.into(), ..Default::default() };
    let push_items = |out: &mut Vec<Row>, items: &[Value]| {
        if items.is_empty() { out.push(Row { kind: "empty", ..Default::default() }); return; }
        let n = items.len();
        for (i, x) in items.iter().enumerate() {
            let (scope, name) = (c::s(x, "scope"), c::s(x, "name"));
            let origin = c::s(x, "origin");
            let desc = [c::s(x, "description"), c::s(x, "problem")].into_iter().find(|v| !v.is_empty()).unwrap_or_else(|| "No description".into());
            out.push(Row {
                kind: "skill", title: name.clone(),
                open: open.as_ref().is_some_and(|(s, nm)| *s == scope && *nm == name),
                scope, name, meta: meta_of(x), desc,
                bundled: origin == "bundled", synced: origin == "synced", linked: origin == "linked",
                github: x.get("source").is_some_and(c::truthy), problem: !c::s(x, "problem").is_empty(),
                first: i == 0, last: i + 1 == n, ..Default::default()
            });
        }
    };
    out.push(head("All projects".into(), sc.first().map(|x| c::s(x, "path")).unwrap_or_default(), ""));
    push_items(&mut out, &in_scope("global"));
    for s in sc.iter().skip(1) {
        let it = in_scope(&c::s(s, "id"));
        if !it.is_empty() { out.push(head(c::s(s, "label"), c::s(s, "path"), "")); push_items(&mut out, &it); }
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
pub fn load(cx: &mut App, keep: Option<String>) { load_then(cx, keep, |_| {}); }

/// skLoad, then something more (open a file).
fn load_then(cx: &mut App, keep: Option<String>, then: impl FnOnce(&mut App) + 'static) {
    upd(cx, |s| s.l.loaded = true);
    crundi_shell::call(cx, |api| api.get("/api/skills"), move |r, cx| {
        upd(cx, |s| match r {
            Ok(d) if ok(&d) => { s.l.data = Some(d); s.l.error.clear(); s.l.load_error.clear(); }
            Ok(d) => { let e = c::err_of(&d, "Could not load the skills"); s.l.error = e.clone(); s.l.load_error = e; }
            Err(e) => { s.l.error = e.clone(); s.l.load_error = e; }
        });
        let after = move |cx: &mut App| {
            upd(cx, |s| {
                let l = &mut s.l;
                if let Some(d) = &l.data {
                    if !scopes(d).iter().any(|x| c::s(x, "id") == l.target) { l.target = "global".into(); }
                    if let Some((sc, n)) = &l.open {
                        if !skills(d).iter().any(|x| c::s(x, "scope") == *sc && c::s(x, "name") == *n) { l.open = None; l.detail = None; l.file = None; l.form.clear(); }
                    }
                }
                if let Some(k) = &keep { l.error = k.clone(); }
            });
            if get(cx, |s| s.l.open.is_some()) { load_detail(cx, then); } else { then(cx); }
        };
        if get(cx, |s| s.l.link_ok.is_none()) {
            // Install from a link is newer than the rest: ask the server.
            crundi_shell::call(cx, |api| api.get("/api/handshake"), move |h, cx| {
                if let Ok(h) = h {
                    let caps = h.pointer("/server/capabilities").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    upd(cx, |s| s.l.link_ok = Some(caps.iter().any(|x| x.as_str() == Some("skills-install-link"))));
                }
                after(cx);
            });
        } else { after(cx); }
    });
}

fn load_detail(cx: &mut App, then: impl FnOnce(&mut App) + 'static) {
    let Some((scope, name)) = get(cx, |s| s.l.open.clone()) else { then(cx); return };
    let url = format!("/api/skills/detail?{}", c::query(&[("scope", &scope), ("name", &name)]));
    crundi_shell::call(cx, move |api| api.get(&url), move |r, cx| {
        let d = match r { Ok(d) if ok(&d) => d, Ok(d) => json!({ "error": c::err_of(&d, "Could not open this skill") }), Err(e) => json!({ "error": e }) };
        upd(cx, |s| {
            // Late: another skill is open by now.
            if s.l.open.as_ref() != Some(&(scope, name)) { return; }
            if let Some(f) = &s.l.file {
                let files = d.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                if d.get("files").is_some() && !files.iter().any(|x| c::s(x, "path") == f.path) { s.l.file = None; }
            }
            s.l.detail = Some(d);
        });
        then(cx);
    });
}

fn open_skill(l: &List) -> Option<Value> {
    let (sc, n) = l.open.as_ref()?;
    skills(l.data.as_ref()?).into_iter().find(|x| c::s(x, "scope") == *sc && c::s(x, "name") == *n)
}

// ─── Leaving a file with changes ───

fn dirty_of(s: &Sk) -> bool {
    let text = s.f.get("editor");
    s.l.file.as_ref().is_some_and(|f| !f.binary && !f.read_only && text != f.saved)
}

/// skLeaveFile: go on when nothing is unsaved, or once the person agrees.
fn leave_file(cx: &mut App, then: impl FnOnce(&mut App) + 'static) {
    if !get(cx, dirty_of) { then(cx); return; }
    c::confirm(cx, "Discard changes?", "Discard the changes you have not saved?", "Discard", true, then);
}

fn busy(cx: &mut App, on: bool) { upd(cx, |s| s.l.busy = on); }
fn field(cx: &mut App, k: &str) -> String { get(cx, |s| s.f.trim(k)) }

// ─── Actions (the buttons) ───

pub fn act(cx: &mut App, action: &str, arg: &str) {
    let arg = arg.to_string();
    match action {
        "target" => upd(cx, |s| {
            let id = s.l.data.as_ref().map(scopes).unwrap_or_default().get(arg.parse::<usize>().unwrap_or(0)).map(|x| c::s(x, "id")).unwrap_or_else(|| "global".into());
            s.l.target = id;
        }),
        "upload" => pick_upload(cx),
        "asset" => pick_asset(cx),
        "synced-toggle" => upd(cx, |s| s.l.show_synced = !s.l.show_synced),
        "form-close" => upd(cx, |s| s.l.form.clear()),
        "pending-cancel" => upd(cx, |s| s.l.pending = None),
        "new" => upd(cx, |s| {
            s.l.form = if s.l.form == "new" { String::new() } else { "new".into() };
            s.l.pending = None;
            s.f.set("new-name", "");
            s.f.set("new-desc", "");
        }),
        "open" => {
            let Some((scope, name)) = arg.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string())) else { return };
            leave_file(cx, move |cx| {
                let same = upd(cx, |s| { s.l.file = None; s.l.form.clear(); s.l.detail = None; s.l.open.as_ref().is_some_and(|(a, b)| *a == scope && *b == name) });
                if same { upd(cx, |s| s.l.open = None); return; }
                upd(cx, |s| s.l.open = Some((scope, name)));
                load_detail(cx, |cx| { upd(cx, |_| {}); });
            });
        }
        "rename" | "transfer" | "newfile" => upd(cx, |s| {
            s.l.form = if s.l.form == action { String::new() } else { action.to_string() };
            if s.l.form.is_empty() { return; }
            let (sc, n) = s.l.open.clone().unwrap_or_default();
            let list = s.l.data.as_ref().map(scopes).unwrap_or_default();
            // The other place: "every project" from a project, the first project from "every project".
            let to_id = if sc == "global" || sc == "synced" { list.iter().map(|x| c::s(x, "id")).find(|x| *x != sc).unwrap_or_else(|| "global".into()) } else { "global".into() };
            s.l.to = list.iter().position(|x| c::s(x, "id") == to_id).unwrap_or(0);
            s.f.set("rename", n.clone());
            s.f.set("as", n);
            s.f.set("newfile", "");
        }),
        "open-url" => c::open_url(&arg),
        _ => act_busy(cx, action, arg),
    }
}

/// The actions that talk to the server.
fn act_busy(cx: &mut App, action: &str, arg: String) {
    match action {
        "new-go" => {
            let (scope, name, desc) = (get(cx, |s| s.l.target.clone()), field(cx, "new-name"), field(cx, "new-desc"));
            busy(cx, true);
            c::post(cx, "/api/skills/create", json!({ "scope": scope, "name": name, "description": desc }), |d, cx| {
                busy(cx, false);
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not create it")); return; }
                let v = d.get("skill").cloned().unwrap_or_default();
                upd(cx, |s| { s.l.form.clear(); s.l.open = Some((c::s(&v, "scope"), c::s(&v, "name"))); s.l.detail = None; s.l.file = None; });
                load_then(cx, None, |cx| open_file(cx, "SKILL.md"));
            });
        }
        "pending-replace" => {
            let Some(p) = get(cx, |s| s.l.pending.clone()) else { return };
            match p {
                Pending::Upload { file, scope, name, .. } => upload(cx, file, scope, name, true),
                Pending::Link { url, scope, skill, all, .. } => link_install(cx, url, scope, skill, all, true),
                Pending::Transfer { from, scope, as_, mv, .. } => transfer(cx, from, scope, as_, mv, true),
                Pending::Choose { .. } => {}
            }
        }
        "pending-name" => {
            let Some(Pending::Upload { file, scope, .. }) = get(cx, |s| s.l.pending.clone()) else { return };
            let n = field(cx, "pending-name");
            upload(cx, file, scope, n, false);
        }
        "link-go" => {
            let url = field(cx, "link");
            if url.is_empty() { return; }
            let scope = upd(cx, |s| { s.l.form.clear(); s.l.error.clear(); s.l.pending = None; s.l.target.clone() });
            link_install(cx, url, scope, String::new(), false, false);
        }
        "link-pick" | "link-all" => {
            let Some(Pending::Choose { url, scope, .. }) = get(cx, |s| s.l.pending.clone()) else { return };
            link_install(cx, url, scope, if action == "link-pick" { arg } else { String::new() }, action == "link-all", false);
        }
        _ => act_open(cx, action, arg),
    }
}

/// The open skill's actions.
fn act_open(cx: &mut App, action: &str, arg: String) {
    let Some((scope, name)) = get(cx, |s| s.l.open.clone()) else { return };
    match action {
        "update" => leave_file(cx, move |cx| {
            let msg = format!("Install \"{name}\" again from where it came from? This replaces its files, including any changes made here.");
            c::confirm(cx, "Update skill", &msg, "Update", false, move |cx| update(cx, scope, name));
        }),
        "download" => c::download(cx, format!("/api/skills/download?{}", c::query(&[("scope", &scope), ("name", &name)])), format!("{name}.zip")),
        "delete" => {
            let msg = format!("Delete the skill \"{name}\" and all its files? This cannot be undone.");
            c::confirm(cx, "Delete skill", &msg, "Delete", true, move |cx| {
                busy(cx, true);
                c::post(cx, "/api/skills/delete", json!({ "scope": scope, "name": name }), move |d, cx| {
                    busy(cx, false);
                    if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not delete it")); return; }
                    c::toast(cx, &format!("Deleted {name}"));
                    upd(cx, |s| { s.l.open = None; s.l.detail = None; s.l.file = None; s.l.form.clear(); });
                    load(cx, None);
                });
            });
        }
        "rename-go" => leave_file(cx, move |cx| {
            let to = field(cx, "rename");
            busy(cx, true);
            c::post(cx, "/api/skills/rename", json!({ "scope": scope, "name": name, "to": to }), |d, cx| {
                busy(cx, false);
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not rename it")); return; }
                let v = d.get("skill").cloned().unwrap_or_default();
                upd(cx, |s| { s.l.form.clear(); s.l.file = None; s.l.detail = None; s.l.open = Some((c::s(&v, "scope"), c::s(&v, "name"))); });
                load(cx, None);
            });
        }),
        "copy-go" | "move-go" => {
            let mv = action == "move-go";
            let to = get(cx, |s| s.l.data.as_ref().map(scopes).unwrap_or_default().get(s.l.to).map(|x| c::s(x, "id"))).unwrap_or_else(|| "global".into());
            let as_ = field(cx, "as");
            let go = move |cx: &mut App| transfer(cx, (scope, name), to, as_, mv, false);
            if mv { leave_file(cx, go); } else { go(cx); }
        }
        "file" => {
            if get(cx, |s| s.l.file.as_ref().is_some_and(|f| f.path == arg)) { return; }
            leave_file(cx, move |cx| open_file(cx, &arg));
        }
        "file-close" => leave_file(cx, |cx| upd(cx, |s| s.l.file = None)),
        "file-download" => {
            let Some(p) = get(cx, |s| s.l.file.as_ref().map(|f| f.path.clone())) else { return };
            let leaf = p.rsplit('/').next().unwrap_or(&p).to_string();
            c::download(cx, format!("/api/skills/file-download?{}", c::query(&[("scope", &scope), ("name", &name), ("path", &p)])), leaf);
        }
        "save" => {
            let Some(p) = get(cx, |s| s.l.file.as_ref().map(|f| f.path.clone())) else { return };
            let text = get(cx, |s| s.f.get("editor"));
            busy(cx, true);
            c::post(cx, "/api/skills/file", json!({ "scope": scope, "name": name, "path": p, "content": text }), move |d, cx| {
                busy(cx, false);
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not save it")); return; }
                upd(cx, |s| if let Some(f) = &mut s.l.file { f.saved = text.clone(); f.size = c::n(&d, "size"); });
                c::toast(cx, &format!("Saved {p}"));
                load(cx, None);
            });
        }
        "newfile-go" => {
            let path = field(cx, "newfile");
            if path.is_empty() { return; }
            let exists = get(cx, |s| s.l.detail.as_ref().and_then(|d| d.get("files")).and_then(|x| x.as_array()).is_some_and(|fs| fs.iter().any(|f| c::s(f, "path") == path)));
            if exists { c::toast_error(cx, "That file already exists"); return; }
            leave_file(cx, move |cx| {
                busy(cx, true);
                c::post(cx, "/api/skills/file", json!({ "scope": scope, "name": name, "path": path, "content": "" }), |d, cx| {
                    busy(cx, false);
                    if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not create it")); return; }
                    upd(cx, |s| s.l.form.clear());
                    let p = c::s(&d, "path");
                    load_then(cx, None, move |cx| open_file(cx, &p));
                });
            });
        }
        "file-delete" => {
            let msg = format!("Delete {arg} from this skill?");
            c::confirm(cx, "Delete file", &msg, "Delete", true, move |cx| {
                busy(cx, true);
                c::post(cx, "/api/skills/file-delete", json!({ "scope": scope, "name": name, "path": arg }), move |d, cx| {
                    busy(cx, false);
                    if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not delete it")); return; }
                    upd(cx, |s| if s.l.file.as_ref().is_some_and(|f| f.path == arg) { s.l.file = None; });
                    load(cx, None);
                });
            });
        }
        _ => {}
    }
}

fn open_file(cx: &mut App, path: &str) {
    let Some((scope, name)) = get(cx, |s| s.l.open.clone()) else { return };
    let url = format!("/api/skills/file?{}", c::query(&[("scope", &scope), ("name", &name), ("path", path)]));
    crundi_shell::call(cx, move |api| api.get(&url), |r, cx| {
        let d = match r { Ok(d) if ok(&d) => d, Ok(d) => { c::toast_error(cx, &c::err_of(&d, "Could not open that file")); return; } Err(e) => { c::toast_error(cx, &e); return; } };
        let text = c::s(&d, "text");
        upd(cx, |s| {
            s.l.file = Some(SkFile {
                path: c::s(&d, "path"), saved: text.clone(), binary: c::t(&d, "binary"), too_large: c::t(&d, "tooLarge"),
                read_only: c::t(&d, "readOnly"), size: c::n(&d, "size"),
            });
            s.f.set("editor", text);
        });
    });
}

// ─── Upload, link, update, transfer ───

fn pick_upload(cx: &mut App) {
    let scope = get(cx, |s| s.l.target.clone());
    crundi_shell::run(cx, || c::pick_file(true), move |p, cx| {
        let Some(p) = p else { return };
        let limit = get(cx, |s| s.l.data.as_ref().and_then(|d| d.pointer("/limits/upload")).and_then(|x| x.as_f64()));
        let size = std::fs::metadata(&p).map(|m| m.len() as f64).unwrap_or(0.0);
        if let Some(l) = limit { if size > l { upd(cx, |s| s.l.error = format!("That file is too large (the limit is {} MB)", (l / 1048576.0).round() as i64)); return; } }
        upd(cx, |s| { s.l.form.clear(); s.l.error.clear(); });
        upload(cx, p, scope, String::new(), false);
    });
}

fn upload(cx: &mut App, file: PathBuf, scope: String, name: String, overwrite: bool) {
    let fname = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let url = format!("/api/skills/upload?{}", c::query(&[("scope", &scope), ("filename", &fname), ("name", &name), ("overwrite", if overwrite { "1" } else { "" })]));
    let f2 = file.clone();
    busy(cx, true);
    crundi_shell::call(cx, move |api| {
        let bytes = std::fs::read(&f2).map_err(|e| ApiError::Net(e.to_string()))?;
        c::raw(api.post_with_progress(&url, &bytes, &|_, _| {}))
    }, move |r, cx| {
        busy(cx, false);
        let d = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
        if ok(&d) {
            let inst = d.get("installed").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            let n: Vec<String> = inst.iter().map(|x| c::s(x, "name")).collect();
            c::toast(cx, &installed_text(&n));
            let dropped = strs(&d, "dropped");
            let keep = (!dropped.is_empty()).then(|| format!("Left out of the upload: {}", dropped.join(", ")));
            upd(cx, |s| {
                s.l.pending = None;
                s.l.error = keep.clone().unwrap_or_default();
                if n.len() == 1 { s.l.open = Some((c::s(&inst[0], "scope"), n[0].clone())); s.l.detail = None; s.l.file = None; s.l.form.clear(); }
            });
            load(cx, keep);
            return;
        }
        let existing = strs(&d, "existing");
        upd(cx, |s| {
            if c::t(&d, "conflict") {
                s.l.pending = Some(Pending::Upload { file, scope, name, existing, needs_name: false, message: String::new() });
                s.l.error.clear();
            } else if c::t(&d, "needsName") {
                s.f.set("pending-name", "");
                s.l.pending = Some(Pending::Upload { file, scope, name: String::new(), existing: vec![], needs_name: true, message: "This upload does not say what the skill is called. Give it a name: lowercase letters, digits and hyphens.".into() });
                s.l.error.clear();
            } else {
                s.l.pending = None;
                s.l.error = c::err_of(&d, "Could not install that");
            }
        });
    });
}

fn not_supported(cx: &mut App, e: String) -> Value {
    if e == "Not supported by this server" {
        upd(cx, |s| s.l.link_ok = Some(false));
        json!({ "ok": false, "error": "This server does not support this yet. Update the server to use it." })
    } else { json!({ "ok": false, "error": e }) }
}

fn link_install(cx: &mut App, url: String, scope: String, skill: String, all: bool, overwrite: bool) {
    upd(cx, |s| s.l.link_busy = true);
    let body = json!({ "url": url, "scope": scope, "skill": skill, "all": all, "overwrite": overwrite });
    crundi_shell::call(cx, move |api| c::post_raw(api, "/api/skills/install-from-link", &body), move |r, cx| {
        upd(cx, |s| s.l.link_busy = false);
        let d = match r { Ok(d) => d, Err(e) => not_supported(cx, e) };
        if ok(&d) {
            let inst = d.get("installed").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            let n: Vec<String> = inst.iter().map(|x| c::s(x, "name")).collect();
            c::toast(cx, &installed_text(&n));
            upd(cx, |s| {
                s.f.set("link", "");
                s.l.pending = None; s.l.error.clear();
                if n.len() == 1 { s.l.open = Some((c::s(&inst[0], "scope"), n[0].clone())); s.l.detail = None; s.l.file = None; s.l.form.clear(); }
            });
            load(cx, None);
            return;
        }
        upd(cx, |s| {
            if c::t(&d, "choose") {
                let skills = d.get("skills").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter().map(|x| (c::s(x, "name"), c::s(x, "description"))).collect();
                s.l.pending = Some(Pending::Choose { url, scope, skills, message: c::s(&d, "error") });
                s.l.error.clear();
            } else if c::t(&d, "conflict") {
                s.l.pending = Some(Pending::Link { url, scope, skill, all, existing: strs(&d, "existing") });
                s.l.error.clear();
            } else {
                s.l.pending = None;
                s.l.error = c::err_of(&d, "Could not install that");
            }
        });
    });
}

fn update(cx: &mut App, scope: String, name: String) {
    busy(cx, true);
    let body = json!({ "scope": scope, "name": name });
    crundi_shell::call(cx, move |api| c::post_raw(api, "/api/skills/update-from-source", &body), move |r, cx| {
        busy(cx, false);
        let d = match r { Ok(d) => d, Err(e) => not_supported(cx, e) };
        if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not update it")); return; }
        if c::t(&d, "upToDate") { c::toast(cx, &format!("{name} is up to date")); return; }
        let commit = d.pointer("/source/commit").and_then(|x| x.as_str()).unwrap_or("");
        let to = if commit.is_empty() { String::new() } else { format!(" to {}", &commit[..commit.len().min(7)]) };
        c::toast(cx, &format!("Updated {name}{to}. New chats will have it."));
        upd(cx, |s| s.l.file = None);
        load(cx, None);
    });
}

fn transfer(cx: &mut App, from: (String, String), scope: String, as_: String, mv: bool, overwrite: bool) {
    busy(cx, true);
    let body = json!({ "scope": from.0, "name": from.1, "to": scope, "as": as_, "move": mv, "overwrite": overwrite });
    c::post(cx, "/api/skills/transfer", body, move |d, cx| {
        busy(cx, false);
        if ok(&d) {
            let v = d.get("skill").cloned().unwrap_or_default();
            let label = get(cx, |s| scope_label(s.l.data.as_ref(), &c::s(&v, "scope")));
            c::toast(cx, &format!("{}{label}", if mv { "Moved to " } else { "Copied to " }));
            upd(cx, |s| { s.l.pending = None; s.l.form.clear(); s.l.file = None; s.l.open = Some((c::s(&v, "scope"), c::s(&v, "name"))); s.l.detail = None; });
            load(cx, None);
            return;
        }
        if c::t(&d, "conflict") {
            upd(cx, |s| s.l.pending = Some(Pending::Transfer { from, scope, as_, mv, existing: strs(&d, "existing") }));
            return;
        }
        upd(cx, |s| s.l.pending = None);
        c::toast_error(cx, &c::err_of(&d, "Could not do that"));
    });
}

/// "Upload file" in a skill: one file into it (asks before replacing).
fn pick_asset(cx: &mut App) {
    crundi_shell::run(cx, || c::pick_file(false), |p, cx| {
        let Some(p) = p else { return };
        let Some((scope, name)) = get(cx, |s| s.l.open.clone()) else { return };
        let fname = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let f2 = fname.clone();
        let go = move |cx: &mut App| {
            let url = format!("/api/skills/file-upload?{}", c::query(&[("scope", &scope), ("name", &name), ("path", &f2)]));
            busy(cx, true);
            crundi_shell::call(cx, move |api| {
                let bytes = std::fs::read(&p).map_err(|e| ApiError::Net(e.to_string()))?;
                c::raw(api.post_with_progress(&url, &bytes, &|_, _| {}))
            }, move |r, cx| {
                busy(cx, false);
                let d = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not add that file")); return; }
                c::toast(cx, &format!("Added {f2}"));
                load(cx, None);
            });
        };
        let exists = get(cx, |s| s.l.detail.as_ref().and_then(|d| d.get("files")).and_then(|x| x.as_array()).is_some_and(|fs| fs.iter().any(|f| c::s(f, "path") == fname)));
        if exists { c::confirm(cx, "Replace file", &format!("Replace {fname} in this skill?"), "Replace", false, go); } else { go(cx); }
    });
}

// ─── Drawing ───

fn hint(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::text_muted()).child(text.into()) }
fn err(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::red()).child(text.into()) }
fn head(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text.into()) }
thread_local! { static OPEN_Y: std::cell::Cell<Option<Pixels>> = const { std::cell::Cell::new(None) }; }

/// Where the open skill's row was last laid out (window y), if one is open.
pub fn open_row_y() -> Option<Pixels> { OPEN_Y.get() }

fn row() -> Div { div().flex().items_center().gap(T::SP_8) }
fn col(gap: Pixels) -> Div { div().flex().flex_col().gap(gap) }
/// A boxed note with its icon (SkNote): muted, or amber when it warns.
fn sk_note(icon: IconName, warn: bool, text: impl Into<SharedString>) -> Div {
    div().flex().gap(T::SP_8).p(T::SP_10).rounded(T::R_FIELD).border_1()
        .border_color(if warn { T::yellow() } else { T::border() }).bg(if warn { T::yellow_dim() } else { T::bg() })
        .child(div().flex_none().text_color(if warn { T::amber() } else { T::text_muted() }).child(w::icon(icon, T::ICON_MD)))
        .child(col(T::SP_8).flex_1().min_w_0().child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(text.into())))
}
/// A small form (SkForm): its label over a row of fields and buttons.
fn sk_form(label: impl Into<SharedString>, items: Vec<AnyElement>) -> Div {
    col(T::SP_6).p(T::SP_10).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg())
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(label.into()))
        .child(row().children(items))
}
fn grow(e: impl IntoElement) -> AnyElement { div().flex_1().min_w_0().child(e).into_any_element() }
fn btn(id: &'static str, label: &str, icon: Option<IconName>, primary: bool, off: bool, f: impl Fn(&mut App) + 'static) -> AnyElement {
    let b = if primary { w::primary(id, label.to_string()) } else { w::button(id, label.to_string()) };
    let b = if let Some(i) = icon { b.icon(i) } else { b };
    w::disabled(b, off).on_click(move |_, _, cx| f(cx)).into_any_element()
}

fn key(k: &'static str, arg: &str, cx: &mut App) {
    let (_, blur) = c::key_arg(arg);
    if blur { return; }
    let a = match k { "new-name" | "new-desc" => "new-go", "link" => "link-go", "pending-name" => "pending-name", "rename" => "rename-go", "as" => "copy-go", "newfile" => "newfile-go", "editor" => "save", _ => return };
    act(cx, a, "");
}

fn editor_save(k: &'static str, arg: &str, cx: &mut App) {
    if get(cx, |s| s.l.file.as_ref().is_some_and(|f| !f.read_only)) { key(k, arg, cx); }
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = get(cx, |s| if s.l.data.is_some() { "data" } else if s.l.loaded && !s.l.load_error.is_empty() { "error" } else { "loading" });
    if state == "loading" { return hint("Loading\u{2026}").into_any_element(); }
    if state == "error" { return err(get(cx, |s| s.l.load_error.clone())).into_any_element(); }
    let link = c::line::<Sk>(window, cx, "link", "https://github.com/owner/repo --skill name", false, Some(key));
    let (form, pending_name) = get(cx, |s| (s.l.form.clone(), matches!(s.l.pending, Some(Pending::Upload { needs_name: true, .. }))));
    let new_name = (form == "new").then(|| c::line::<Sk>(window, cx, "new-name", "skill-name", false, Some(key)));
    let new_desc = (form == "new").then(|| c::line::<Sk>(window, cx, "new-desc", "What it does and when to use it", false, Some(key)));
    let pname = pending_name.then(|| c::line::<Sk>(window, cx, "pending-name", "skill-name", false, Some(key)));
    let detail = if get(cx, |s| s.l.open.is_some()) { Some(detail(window, cx)) } else { None };
    let s = super::st(cx);
    let l = &s.l;
    let d = l.data.clone().unwrap_or_default();
    let sc = scopes(&d);
    let target = sc.iter().position(|x| c::s(x, "id") == l.target).unwrap_or(0);
    let mb = (d.pointer("/limits/upload").and_then(|x| x.as_f64()).unwrap_or(52428800.0) / 1048576.0).round() as i64;
    let link_off = l.link_ok == Some(false);
    let mut out = col(T::SP_8)
        .child(hint("A skill is a folder with a SKILL.md that tells Claude how to do one kind of task. A chat loads its skills when it starts, so changes here reach new chats. Chats that are already running keep the skills they started with."))
        .child(row().pt(T::SP_4)
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Add to"))
            .child(w::select("sk-target", sc.iter().map(|x| SharedString::from(c::s(x, "label"))).collect(), Some(target), "", false, |i, _, cx| act(cx, "target", &i.to_string())))
            .child(btn("sk-upload", "Upload", Some(IconName::Upload), true, l.busy, |cx| act(cx, "upload", "")))
            .child(btn("sk-new", "New", Some(IconName::Plus), false, false, |cx| act(cx, "new", ""))))
        .child(hint(format!("Upload a .zip or .skill archive holding one or more skills, or a single SKILL.md. Up to {mb} MB.")))
        .child(sk_form("Or install from a GitHub link, or paste an npx skills add line. Only the skill\u{2019}s files are copied; nothing from the repository is run.", vec![
            grow(div().font_family(T::MONO).child(w::input(&link).disabled(link_off || l.link_busy))),
            btn("sk-link-go", if l.link_busy { "Fetching\u{2026}" } else { "Install from link" }, Some(IconName::GitBranch), true, link_off || l.link_busy, |cx| act(cx, "link-go", "")),
        ]))
        .when(link_off, |d| d.child(hint("This server does not support this yet. Update the server to use it. Upload still works.")))
        .when(!l.error.is_empty(), |d| d.child(err(l.error.clone())));
    match &l.pending {
        Some(Pending::Choose { message, skills, .. }) => {
            let n = skills.len();
            out = out.child(sk_note(IconName::Info, true, message.clone())
                .child(col(T::SP_6).children(skills.iter().enumerate().map(|(i, (nm, ds))| {
                    let nm2 = nm.clone();
                    row().p(T::SP_6).pl(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
                        .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary()).child(if ds.is_empty() { nm.clone() } else { format!("{nm} {ds}") }))
                        .child(w::disabled(w::button(("sk-pick", i), "Install"), l.link_busy).on_click(move |_, _, cx| act(cx, "link-pick", &nm2)))
                })))
                .child(row().child(w::disabled(w::primary("sk-link-all", format!("Install all {n}")), l.link_busy).on_click(|_, _, cx| act(cx, "link-all", "")))
                    .child(btn("sk-pend-cancel", "Cancel", None, false, false, |cx| act(cx, "pending-cancel", "")))));
        }
        Some(Pending::Upload { needs_name: true, message, .. }) => {
            if let Some(p) = &pname {
                out = out.child(sk_form(message.clone(), vec![grow(w::input(p)),
                    btn("sk-pend-name", "Install", None, true, l.busy, |cx| act(cx, "pending-name", "")),
                    btn("sk-pend-cancel", "Cancel", None, false, false, |cx| act(cx, "pending-cancel", ""))]));
            }
        }
        Some(Pending::Upload { existing, scope, .. }) | Some(Pending::Link { existing, scope, .. }) | Some(Pending::Transfer { existing, scope, .. }) => {
            out = out.child(sk_note(IconName::Info, true, replace_text(existing, &scope_label(Some(&d), scope)))
                .child(row().child(w::disabled(w::danger("sk-replace", "Replace"), l.busy || l.link_busy).on_click(|_, _, cx| act(cx, "pending-replace", "")))
                    .child(btn("sk-pend-cancel", "Cancel", None, false, false, |cx| act(cx, "pending-cancel", "")))));
        }
        None => {}
    }
    if let (Some(nn), Some(nd)) = (&new_name, &new_desc) {
        out = out.child(sk_form(format!("A new, empty skill in {}. The description is how Claude decides when to use it.", scope_label(Some(&d), &l.target)), vec![
            grow(w::input(nn)), div().flex_grow(3.).flex_basis(relative(0.)).min_w_0().child(w::input(nd)).into_any_element(),
            btn("sk-new-go", "Create", None, true, l.busy, |cx| act(cx, "new-go", "")),
            btn("sk-form-close", "Cancel", None, false, false, |cx| act(cx, "form-close", "")),
        ]));
    }
    let mut detail = detail;
    // Each group's skills in one list (.bk-list: one frame, rounded; each
    // .sk-item a hairline over it but the first).
    let list = || div().flex().flex_col().border_1().border_color(T::border()).rounded(T::R_FIELD).overflow_hidden();
    let mut cur: Option<Div> = None;
    for (i, r) in rows_of(&d, &l.open, l.show_synced).into_iter().enumerate() {
        match r.kind {
            "group" => {
                if let Some(x) = cur.take() { out = out.child(x); }
                out = out.child(row().pt(T::SP_16).pb(T::SP_2)
                    .child(head(r.title.clone()))
                    .child(div().flex_1().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(r.path.clone()))
                    .when(!r.toggle.is_empty(), |d| d.child(btn("sk-synced", &r.toggle, None, false, false, |cx| act(cx, "synced-toggle", "")))));
            }
            "empty" => {
                if let Some(x) = cur.take() { out = out.child(x); }
                out = out.child(list().child(div().p(T::SP_12 + T::SP_2).text_size(T::TEXT_13).text_color(T::text_muted()).child("None yet.")));
            }
            _ => {
                let arg = format!("{}\n{}", r.scope, r.name);
                let tags = [(r.bundled, "Included with Crundi", ""), (r.synced, "Synced", ""), (r.linked, "Link", ""), (r.github, "GitHub", ""), (r.problem, "Needs attention", "warn")];
                let headr = div().id(("sk-row", i)).flex().flex_col().gap(T::SP_4).px(T::SP_12).py(T::SP_10).cursor_pointer()
                    .when(r.open, |d| d.bg(T::bg_hover())).hover(|d| d.bg(T::bg_hover()))
                    .child(row().child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).truncate().child(r.name.clone()))
                        .children(tags.into_iter().filter(|t| t.0).map(|t| w::tag(t.1, t.2)))
                        .child(div().flex_1().min_w_0())
                        .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(r.meta.clone())))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).when(!r.open, |d| d.line_clamp(2).text_ellipsis()).child(r.desc.clone()))
                    .on_click(move |_, _, cx| act(cx, "open", &arg));
                let item = div().flex().flex_col().when(!r.first, |d| d.border_t_1().border_color(T::border()))
                    // The open skill's place, for the driver's reveal (settings-reveal-skill).
                    .when(r.open, |d| d.relative().child(canvas(|b, _, _| OPEN_Y.set(Some(b.origin.y)), |_, _, _, _| {}).absolute().top_0().left_0().size_0()))
                    .child(headr)
                    .when(r.open, |d| d.children(detail.take()));
                cur = Some(cur.take().unwrap_or_else(list).child(item));
            }
        }
    }
    if let Some(x) = cur.take() { out = out.child(x); }
    out.into_any_element()
}

/// The open skill (SkDetail): notes, actions, forms, files and the editor.
fn detail(window: &mut Window, cx: &mut App) -> AnyElement {
    let form = get(cx, |s| s.l.form.clone());
    let rn = (form == "rename").then(|| c::line::<Sk>(window, cx, "rename", "", false, Some(key)));
    let as_ = (form == "transfer").then(|| c::line::<Sk>(window, cx, "as", "", false, Some(key)));
    let nf = (form == "newfile").then(|| c::line::<Sk>(window, cx, "newfile", "reference/notes.md", false, Some(key)));
    let ed = get(cx, |s| s.l.file.as_ref().is_some_and(|f| !f.binary)).then(|| c::area::<Sk>(window, cx, "editor", "", 16, Some(editor_save)));
    let s = super::st(cx);
    let l = &s.l;
    let body = col(T::SP_10).px(T::SP_12).pt(T::SP_4).pb(T::SP_16);
    let Some(dt) = &l.detail else { return body.child(hint("Loading\u{2026}")).into_any_element() };
    if dt.get("error").is_some() { return body.child(err(c::s(dt, "error"))).into_any_element(); }
    let Some(skv) = open_skill(l) else { return body.child(hint("Loading\u{2026}")).into_any_element() };
    let ro = c::t(&skv, "readOnly");
    let origin = c::s(&skv, "origin");
    let src = skv.get("source").filter(|x| c::truthy(x)).cloned();
    let can_update = !ro && src.is_some() && l.link_ok != Some(false);
    let can_delete = origin == "user" || origin == "linked";
    let ro_reason = c::s(dt, "readOnlyReason");
    let problem = c::s(&skv, "problem");
    let files = dt.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let on = l.file.as_ref().map(|f| f.path.clone()).unwrap_or_default();
    let sl = c::n(dt, "skippedLinks") as i64;
    let sc = l.data.as_ref().map(scopes).unwrap_or_default();
    let mut d = body
        .when(!ro_reason.is_empty(), |d| d.child(sk_note(IconName::Lock, false, ro_reason.clone())))
        .when(!problem.is_empty(), |d| d.child(sk_note(IconName::Info, true, format!("{problem}."))));
    if let Some(src) = &src {
        let repo = c::s(src, "repo");
        let url = { let u = c::s(src, "url"); if u.is_empty() { format!("https://github.com/{repo}") } else { u } };
        d = d.child(sk_note(IconName::GitBranch, false, "").child(row().gap(T::SP_4).text_size(T::TEXT_12)
            .child(div().text_color(T::text_secondary()).child("From"))
            .child(w::link("sk-src", repo).on_click(move |_, _, _| c::open_url(&url)))
            .child(div().flex_1().min_w_0().text_color(T::text_secondary()).child(source_rest(src)))));
    }
    d = d.child(row()
        .when(can_update, |d| d.child(btn("sd-update", "Update", Some(IconName::RefreshCw), false, l.busy, |cx| act(cx, "update", ""))))
        .when(!ro, |d| d.child(btn("sd-rename", "Rename", Some(IconName::Pencil), false, false, |cx| act(cx, "rename", ""))))
        .child(btn("sd-transfer", if ro { "Copy to\u{2026}" } else { "Copy or move\u{2026}" }, Some(IconName::Copy), false, false, |cx| act(cx, "transfer", "")))
        .child(btn("sd-download", "Download", Some(IconName::Download), false, l.busy, |cx| act(cx, "download", "")))
        .when(can_delete, |d| d.child(w::disabled(w::danger("sd-delete", "Delete").icon(IconName::Trash), l.busy).on_click(|_, _, cx| act(cx, "delete", "")))));
    if let Some(rn) = &rn {
        d = d.child(sk_form("New name. Lowercase letters, digits and hyphens.", vec![grow(w::input(rn)),
            btn("sd-rename-go", "Rename", None, true, l.busy, |cx| act(cx, "rename-go", "")),
            btn("sd-form-close", "Cancel", None, false, false, |cx| act(cx, "form-close", ""))]));
    }
    if let Some(as_) = &as_ {
        d = d.child(sk_form("Where to, and under what name. A copy leaves this one where it is.", vec![
            grow(w::select("sd-to", sc.iter().map(|x| SharedString::from(c::s(x, "label"))).collect(), Some(l.to), "", false, |i, _, cx| upd(cx, |s| s.l.to = i))),
            grow(w::input(as_)),
            btn("sd-copy", "Copy", None, true, l.busy, |cx| act(cx, "copy-go", "")),
        ]).when(!ro, |d| d.child(row().child(btn("sd-move", "Move", None, false, l.busy, |cx| act(cx, "move-go", ""))).child(btn("sd-form-close", "Cancel", None, false, false, |cx| act(cx, "form-close", "")))))
          .when(ro, |d| d.child(row().child(btn("sd-form-close", "Cancel", None, false, false, |cx| act(cx, "form-close", ""))))));
    }
    let mut list = div().flex().flex_col().rounded(T::R_FIELD).border_1().border_color(T::border()).overflow_hidden();
    for (i, f) in files.iter().enumerate() {
        let p = c::s(f, "path");
        let (p1, p2) = (p.clone(), p.clone());
        let deletable = !ro && p != "SKILL.md";
        list = list.child(div().id(("sd-file", i)).flex().items_center().gap(T::SP_8).min_h(T::CONTROL).pl(T::SP_10).pr(T::SP_6).cursor_pointer()
            .when(i > 0, |d| d.border_t_1().border_color(T::border()))
            .when(p == on, |d| d.bg(T::bg_hover())).hover(|d| d.bg(T::bg_hover()))
            .child(div().flex_1().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(p.clone()))
            .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(c::fmt_bytes(c::n(f, "size"))))
            .when(deletable, |d| d.child(w::disabled(w::icon_button(("sd-file-del", i), IconName::Trash, "Delete"), l.busy).on_click(move |_, _, cx| { cx.stop_propagation(); act(cx, "file-delete", &p2); })))
            .on_click(move |_, _, cx| act(cx, "file", &p1)));
    }
    d = d.child(list)
        .when(c::t(dt, "more"), |d| d.child(hint(format!("Only the first {} files are listed. This folder holds more.", files.len()))))
        .when(sl > 0, |d| d.child(hint(format!("{sl}{}", if sl == 1 { " link in this folder is not shown." } else { " links in this folder are not shown." }))))
        .when(!ro, |d| d.child(row()
            .child(btn("sd-newfile", "New file", Some(IconName::Plus), false, false, |cx| act(cx, "newfile", "")))
            .child(btn("sd-asset", "Upload file", Some(IconName::Upload), false, l.busy, |cx| act(cx, "asset", "")))));
    if let (false, Some(nf)) = (ro, &nf) {
        d = d.child(sk_form("Path inside the skill, for example reference/notes.md", vec![grow(div().font_family(T::MONO).child(w::input(nf))),
            btn("sd-newfile-go", "Create", None, true, l.busy, |cx| act(cx, "newfile-go", "")),
            btn("sd-form-close", "Cancel", None, false, false, |cx| act(cx, "form-close", ""))]));
    }
    if let Some(f) = &l.file {
        let note = format!("{} Download it to look at it.", if f.too_large { "This file is too large to open here." } else { "This is not a text file, so it cannot be edited here." });
        d = d.child(col(T::SP_8)
            .child(row().child(div().flex_1().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(f.path.clone()))
                .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(c::fmt_bytes(f.size))))
            .when(f.binary, |d| d.child(sk_note(IconName::File, false, note)))
            .children(ed.as_ref().map(|e| div().font_family(T::MONO).child(w::textarea(e).disabled(f.read_only))))
            .child(row()
                .when(!f.binary && !f.read_only, |d| d.child(btn("ed-save", "Save", None, true, l.busy, |cx| act(cx, "save", ""))))
                .child(btn("ed-download", "Download", Some(IconName::Download), false, l.busy, |cx| act(cx, "file-download", "")))
                .child(btn("ed-close", "Close", None, false, false, |cx| act(cx, "file-close", "")))));
    }
    d.into_any_element()
}

// ─── The driver ───

pub fn props(cx: &mut App) -> Value {
    let s = super::st(cx);
    let l = &s.l;
    let dirty = dirty_of(s);
    let rows: Vec<Value> = l.data.as_ref().map(|d| rows_of(d, &l.open, l.show_synced)).unwrap_or_default().iter()
        .map(|r| json!({ "kind": r.kind, "title": r.title, "scope": r.scope, "name": r.name, "open": r.open, "meta": r.meta, "toggle": r.toggle })).collect();
    let files: Vec<String> = l.detail.as_ref().and_then(|d| d.get("files")).and_then(|x| x.as_array()).map(|a| a.iter().map(|f| c::s(f, "path")).collect()).unwrap_or_default();
    json!({
        "state": if l.data.is_some() { "data" } else if l.load_error.is_empty() { "loading" } else { "error" },
        "scopes": l.data.as_ref().map(scopes).unwrap_or_default().iter().map(|x| c::s(x, "id")).collect::<Vec<_>>(),
        "target": l.target, "rows": rows, "form": l.form, "error": l.error,
        "open": l.open.as_ref().map(|(a, b)| json!({ "scope": a, "name": b })),
        "detail": l.detail.as_ref().map(|d| if d.get("error").is_some() { "error" } else { "data" }).unwrap_or(if l.open.is_some() { "loading" } else { "" }),
        "files": files,
        "pending": match &l.pending { None => Value::Null, Some(Pending::Choose { skills, .. }) => json!({ "kind": "choose", "skills": skills.iter().map(|x| x.0.clone()).collect::<Vec<_>>() }), Some(Pending::Upload { needs_name: true, .. }) => json!({ "kind": "name" }), Some(_) => json!({ "kind": "replace" }) },
        "editor": l.file.as_ref().map(|f| json!({ "path": f.path, "binary": f.binary, "readOnly": f.read_only, "dirty": dirty, "text": s.f.get("editor") })),
        "linkOk": l.link_ok, "linkBusy": l.link_busy, "busy": l.busy,
    })
}

#[cfg(test)]
mod tests {
    use super::{rows_of, scope_label, replace_text, installed_text, source_rest};
    use serde_json::{json, Value};

    fn data() -> Value {
        json!({ "ok": true,
            "scopes": [{ "id": "global", "label": "Every project", "path": "~/.claude/skills" }, { "id": "p:demo", "label": "demo", "path": "/x/.claude/skills" }],
            "skills": [
                { "scope": "global", "name": "a", "files": 1, "size": 1536, "origin": "user", "description": "A" },
                { "scope": "global", "name": "b", "files": 400, "more": true, "origin": "bundled" },
                { "scope": "synced", "name": "s", "files": 2, "size": 10, "origin": "synced", "problem": "No SKILL.md" },
            ] })
    }

    #[::core::prelude::v1::test]
    fn groups_and_rows() {
        let r = rows_of(&data(), &Some(("global".into(), "a".into())), false);
        let kinds: Vec<&str> = r.iter().map(|x| x.kind).collect();
        // All projects (2 skills); the empty project scope is left out; synced folded.
        assert_eq!(kinds, ["group", "skill", "skill", "group"]);
        assert_eq!(r[0].title, "All projects");
        assert!(r[1].open && r[1].first && !r[1].last && r[2].last);
        assert_eq!(r[1].meta, "1 file, 1.5 KB");
        assert_eq!(r[2].meta, "over 400 files");
        assert!(r[2].bundled && r[2].desc == "No description");
        assert_eq!(r[3].title, "Synced from your Claude account (1)");
        assert_eq!(r[3].toggle, "Show");
        let r = rows_of(&data(), &None, true);
        assert_eq!(r.last().unwrap().desc, "No SKILL.md");
        assert!(r.last().unwrap().problem);
        let r = rows_of(&json!({ "scopes": [{ "id": "global", "label": "G", "path": "p" }], "skills": [] }), &None, false);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].kind, "empty");
    }

    #[::core::prelude::v1::test]
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
