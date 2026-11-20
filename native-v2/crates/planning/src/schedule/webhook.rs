//! Webhooks (4.6): a schedule's "When called" mode, in its dialog (the
//! web's renderSchedModal webhook branch, the rotate-hook handler and
//! saveSchedule's "a new webhook schedule stays open"). The server makes
//! the key and the secret (src/schedule-store.js, rotateHookKey); the URL is
//! `<server>/api/schedule-hook/<id>` with `?key=` unless signed. Ported from
//! the Slint client's src/planning/schedule_webhook.rs and ui/planning/schedule-webhook.slint.

use super::{label, merge, note, rules, ss, truthy, Dm, Fields, Sched};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

/// The dialog's webhook fields into the draft (captureSchedForm).
pub(super) fn capture(f: &Fields, d: &mut Value, cx: &App) {
    if !d["webhook"].is_object() { d["webhook"] = rules::default_schedule("")["webhook"].clone(); }
    d["webhook"]["signatureHeader"] = json!(f.sig.read(cx).value().to_string());
    d["webhook"]["timestampHeader"] = json!(f.ts.read(cx).value().to_string());
    d["webhook"]["idempotency"] = json!(f.idem.read(cx).value().to_string());
}

/// The draft's webhook onto its text boxes.
pub(super) fn fill(f: &Fields, d: &Value, window: &mut Window, cx: &mut App) {
    let wh = &d["webhook"];
    Sched::set(&f.sig, &{ let h = ss(wh, "signatureHeader"); if h.is_empty() { "x-crundi-signature".to_string() } else { h } }, window, cx);
    Sched::set(&f.ts, &ss(wh, "timestampHeader"), window, cx);
    Sched::set(&f.idem, &ss(wh, "idempotency"), window, cx);
}

/// Once saved: its URL and its secret ("" before the first save).
pub(super) fn url_secret(d: &Value, base: &str) -> (String, String) {
    let wh = &d["webhook"];
    let signed = ss(wh, "auth") == "signature";
    let (id, key) = (ss(d, "id"), ss(wh, "key"));
    let url = if !id.is_empty() && !key.is_empty() { rules::hook_url(base, &id, &key, signed) } else { String::new() };
    (url, ss(wh, "secret"))
}

/// After a new webhook schedule's first save: it has an id, a key and a secret now.
pub(super) fn adopt_saved(d: &mut Value, saved: &Value) {
    d["id"] = saved["id"].clone();
    d["webhook"] = merge(rules::default_schedule("")["webhook"].clone(), saved.get("webhook"));
}

/// The section under When (schedule-webhook.slint): the note, Security, the
/// headers, the idempotency key, and once saved the URL, the secret and New key / New secret.
pub(super) fn section(s: &mut Sched, d: &Value, m: &Dm, window: &mut Window, cx: &mut Context<Sched>) -> Div {
    let signed = ss(&d["webhook"], "auth") == "signature";
    // The URL and secret are read-only boxes (selectable), kept to the draft's.
    let (u, sc) = (m.url.clone(), m.secret.clone());
    s.f.url.update(cx, |r, cx| r.set(u, window, cx));
    s.f.secret.update(cx, |r, cx| r.set(sc, window, cx));
    let ts_empty = s.f.ts.read(cx).value().is_empty();
    let f = &s.f;
    let mut d = div().flex().flex_col()
        .child(note("Runs a background chat (or a service action) when its URL is called. Calls that arrive while it is still working go into the same chat, marked Mid-run, so it can tell whether it is the same issue. With One ongoing chat, every run comes back to the same conversation."))
        .child(label("SECURITY", "", false))
        .child(s.seg_ctl("sched-hookauth", "hookauth", &["Key in the URL", "Signed with a secret"], &["key", "signature"], if signed { 1 } else { 0 }, cx));
    let f2 = &s.f;
    if signed {
        d = d.child(div().flex().gap(T::SP_10)
            .child(div().flex().flex_col().flex_1().min_w_0().child(label("SIGNATURE HEADER", "", false)).child(w::input(&f2.sig)))
            .child(div().flex().flex_col().flex_1().min_w_0().child(label("TIMESTAMP HEADER", "(optional)", false)).child(w::input(&f2.ts))));
    }
    let _ = f;
    d = d.child(label("IDEMPOTENCY KEY", "(a call with a key already seen is not processed again)", false)).child(w::input(&s.f.idem));
    if m.url.is_empty() {
        return d.child(note("Save the schedule to get its URL."));
    }
    let hint = if ts_empty { "(HMAC-SHA256 of the raw body; hex or sha256=hex)" } else { "(HMAC-SHA256 of the raw body, or of time:body with the timestamp header, as Grafana does; hex or sha256=hex)" };
    d.child(label("URL", "(POST, JSON body)", false)).child(w::readonly_box(&s.f.url))
        .when(signed, |d| d.child(label("SECRET", hint, false)).child(w::readonly_box(&s.f.secret)))
        .child(div().flex().pt(T::SP_6)
            .child(w::button("sched-rotate", if signed { "New secret" } else { "New key" }).on_click(cx.listener(|s, _, window, cx| rotate(s, window, cx)))))
}

/// "New key" / "New secret": asks, then the server makes a new one.
pub(super) fn rotate(s: &mut Sched, window: &mut Window, cx: &mut Context<Sched>) {
    let Some(d) = s.draft.clone() else { return };
    let id = ss(&d, "id");
    if id.is_empty() { return; }
    let which = if ss(&d["webhook"], "auth") == "signature" { "secret" } else { "key" };
    let msg = if which == "secret" { "Calls signed with the old secret stop working. Make a new one?" } else { "The old URL stops working. Make a new key?" };
    let me = cx.entity().downgrade();
    w::ask_confirm(window, cx, w::Ask { title: msg.into(), ok: "OK".into(), ..Default::default() }, move |ok, _, cx| {
        if !ok { return; }
        let body = json!({ "action": "rotateHook", "id": id, "which": which });
        let _ = me.update(cx, |s, cx| s.post(body, cx, |res, s, window, cx| {
            if !truthy(&res["ok"]) { return; }
            s.capture(cx);
            if let Some(d) = s.draft.as_mut() {
                d["webhook"]["key"] = res["schedule"]["webhook"]["key"].clone();
                d["webhook"]["secret"] = res["schedule"]["webhook"]["secret"].clone();
            }
            let _ = window;
            cx.notify();
        }));
    });
}
