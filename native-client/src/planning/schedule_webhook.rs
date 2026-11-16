//! Webhooks (4.6): a schedule's "When called" mode, in its dialog (the
//! web's renderSchedModal webhook branch, the rotate-hook handler and
//! saveSchedule's "a new webhook schedule stays open"). The server makes
//! the key and the secret (src/schedule-store.js, rotateHookKey); the URL is
//! `<server>/api/schedule-hook/<id>` with `?key=` unless signed.

use super::{capture as capture_form, render_dialog, rules, ss, with, merge};
use crate::{kit, PlanningSchedule};
use serde_json::{json, Value};

/// The dialog's webhook fields into the draft (captureSchedForm).
pub fn capture(g: &PlanningSchedule, d: &mut Value) {
    if !d["webhook"].is_object() { d["webhook"] = rules::default_schedule("")["webhook"].clone(); }
    d["webhook"]["signatureHeader"] = json!(g.get_sig_header().as_str());
    d["webhook"]["timestampHeader"] = json!(g.get_ts_header().as_str());
    d["webhook"]["idempotency"] = json!(g.get_idem().as_str());
}

/// The draft's webhook onto the dialog: Security, the headers, the
/// idempotency key, and once saved its URL and secret.
pub fn render(g: &PlanningSchedule, d: &Value) {
    let wh = &d["webhook"];
    let signed = ss(wh, "auth") == "signature";
    let id = ss(d, "id");
    let key = ss(wh, "key");
    let url = if !id.is_empty() && !key.is_empty() { rules::hook_url(&kit::base(), &id, &key, signed) } else { String::new() };
    g.set_hook_auth(if signed { "signature".into() } else { "key".into() });
    g.set_sig_header({ let h = ss(wh, "signatureHeader"); if h.is_empty() { "x-crundi-signature".into() } else { h.into() } });
    g.set_ts_header(ss(wh, "timestampHeader").into());
    g.set_idem(ss(wh, "idempotency").into());
    g.set_hook_url(url.into());
    g.set_hook_secret(ss(wh, "secret").into());
}

/// After a new webhook schedule's first save: it has an id, a key and a secret now.
pub fn adopt_saved(d: &mut Value, saved: &Value) {
    d["id"] = saved["id"].clone();
    d["webhook"] = merge(rules::default_schedule("")["webhook"].clone(), saved.get("webhook"));
}

/// "New key" / "New secret": asks, then the server makes a new one.
pub fn rotate() {
    let Some(d) = with(|s| s.draft.clone()) else { return };
    let id = ss(&d, "id");
    if id.is_empty() { return; }
    let which = if ss(&d["webhook"], "auth") == "signature" { "secret" } else { "key" };
    let msg = if which == "secret" { "Calls signed with the old secret stop working. Make a new one?" } else { "The old URL stops working. Make a new key?" };
    kit::confirm(kit::Confirm { title: msg, message: "", ok: "OK", danger: false }, move |ok| {
        if !ok { return; }
        let body = json!({ "action": "rotateHook", "id": id, "which": which });
        let pid = super::super::park(move |res: Value| {
            if !kit::truthy(&res["ok"]) { let e = kit::s(&res, "error"); kit::toast_error(if e.is_empty() { "Failed" } else { &e }); return; }
            capture_form();
            with(|s| if let Some(d) = s.draft.as_mut() {
                d["webhook"]["key"] = res["schedule"]["webhook"]["key"].clone();
                d["webhook"]["secret"] = res["schedule"]["webhook"]["secret"].clone();
            });
            render_dialog();
        });
        kit::call(move |api| api.post("/api/schedules", &body), move |r| {
            let v = match r { Ok(v) => v, Err(e) => json!({ "ok": false, "error": e }) };
            super::super::unpark(pid, v);
        });
    });
}

