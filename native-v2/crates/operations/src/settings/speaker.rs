//! Settings › Cru › "Your voice" (T18): the web's vsLoadSpeaker and
//! vsSpkRender (src/webapp-html.js), capability 'voice-speaker'. Whose voice
//! Cru acts on: the enrolment's status, "Only act on my voice", enrolling one
//! phrase at a time (this PC's microphone, cleaned as the web's, a 16 kHz WAV
//! to /api/voice/speaker/enrol), the short replies (/words), finishing, and
//! forgetting (DELETE). An older server (no /api/voice/speaker): left out.

use crate::skills::common as c;
use crundi_core::capture::{wav16, Recorder};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::cell::RefCell;

/// A recording stops by itself at this length (SPK_MAX_S).
const MAX_S: f64 = 15.0;

#[derive(Clone, Default)]
struct Res { n: usize, accepted: bool, seconds: f64, consistency: Option<f64>, sayings: i64, problem: String }

#[derive(Default)]
struct Spk { st: Option<Value>, open: bool, active: bool, words: bool, pos: usize, results: Vec<Res>, rec: Option<Recorder>, busy: bool, msg: String, bad: bool, epoch: u64 }

thread_local! { static S: RefCell<Spk> = RefCell::new(Spk::default()); }
fn with<R>(f: impl FnOnce(&mut Spk) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }
fn redraw(cx: &mut App) { super::parts_changed(cx); }

/// The server's answer (vsLoadSpeaker): shown only when it has the capability.
pub fn load(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/voice/speaker"), |r, cx| {
        let v = r.ok().filter(|v| c::t(v, "ok"));
        with(|s| s.st = v);
        redraw(cx);
    });
}

/// Settings went away: the microphone is let go (vsSpkEnd from its watch).
pub fn left(cx: &mut App) { if with(|s| s.active) { end(cx, "", false); } }

fn start(cx: &mut App, words: bool, msg: &str) {
    let m = msg.to_string();
    with(|s| { if let Some(r) = s.rec.take() { let _ = r.stop(); } s.active = true; s.words = words; s.pos = 0; s.results.clear(); s.busy = false; s.msg = m; s.bad = false; s.epoch += 1; });
    redraw(cx);
}

fn end(cx: &mut App, msg: &str, bad: bool) {
    let m = msg.to_string();
    with(|s| { if let Some(r) = s.rec.take() { let _ = r.stop(); } s.active = false; s.busy = false; s.msg = m; s.bad = bad; s.epoch += 1; });
    redraw(cx);
}

fn record(cx: &mut App) {
    if with(|s| !s.active || s.busy || s.rec.is_some()) { return; }
    match Recorder::start() {
        Ok(r) => { let e = with(|s| { s.rec = Some(r); s.msg.clear(); s.bad = false; s.epoch += 1; s.epoch }); redraw(cx); meter(cx, e); }
        Err(e) => { with(|s| { s.msg = format!("Could not open the microphone: {e}"); s.bad = true; }); redraw(cx); }
    }
}

/// While recording: the meter moves, and at 15 s it stops.
fn meter(cx: &mut App, epoch: u64) {
    c::after(cx, 60, move |cx| {
        let Some(secs) = with(|s| (s.epoch == epoch).then(|| s.rec.as_ref().map(|r| r.seconds())).flatten()) else { return };
        // Leaving Settings lets go of the microphone (the web's watch).
        if crundi_shell::stores(cx).nav.read(cx).tab != "settings" { return end(cx, "", false); }
        if secs >= MAX_S { stop(cx); return; }
        redraw(cx);
        meter(cx, epoch);
    });
}

fn stop(cx: &mut App) {
    let Some(r) = with(|s| s.rec.take()) else { return };
    let samples = r.stop();
    let (first, words, pos) = with(|s| { s.busy = true; s.msg = "Checking\u{2026}".into(); s.bad = false; (!s.results.iter().any(|x| x.accepted), s.words, s.pos) });
    redraw(cx);
    let url = if words { format!("/api/voice/speaker/words?word={pos}{}", if first { "&restart=1" } else { "" }) } else { format!("/api/voice/speaker/enrol{}", if first { "?restart=1" } else { "" }) };
    let Some(api) = crundi_shell::api(cx) else { return };
    let body = wav16(&samples);
    crundi_shell::run(cx, move || api.post_with_progress(&url, &body, &|_, _| {}).ok().and_then(|(_, t)| serde_json::from_str::<Value>(&t).ok()), move |d, cx| {
        with(|s| s.busy = false);
        if !with(|s| s.active) { return; }
        match d.filter(|d| c::t(d, "ok")) {
            None => with(|s| { s.msg = "Could not reach the server".into(); s.bad = true; }),
            Some(d) => with(|s| {
                let accepted = c::t(&d, "accepted");
                let problem = c::s(&d, "problem");
                s.results.push(Res { n: s.pos, accepted, seconds: c::n(&d, "seconds"), consistency: d.get("consistency").and_then(|v| v.as_f64()), sayings: d.get("sayings").and_then(|v| v.as_i64()).unwrap_or(0), problem: problem.clone() });
                if accepted { s.pos += 1; s.msg.clear(); } else { s.msg = if problem.is_empty() { "Try that one again.".into() } else { problem }; s.bad = true; }
            }),
        }
        redraw(cx);
    });
}

fn finish(cx: &mut App) {
    if with(|s| s.busy) { return; }
    let words = with(|s| { s.busy = true; s.words });
    redraw(cx);
    let path = if words { "/api/voice/speaker/words/finish" } else { "/api/voice/speaker/finish" };
    crundi_shell::call(cx, move |api| api.post(path, &json!({})), move |r, cx| {
        with(|s| s.busy = false);
        let d = match r { Ok(d) if c::t(&d, "ok") => d, Ok(d) => { let e = c::s(&d, "error"); with(|s| { s.msg = if e.is_empty() { "Not saved".into() } else { e }; s.bad = true; }); return redraw(cx); } Err(_) => { with(|s| { s.msg = "Not saved".into(); s.bad = true; }); return redraw(cx); } };
        with(|s| merge(s, &d));
        if words { return end(cx, "Cru knows your short replies now.", false); }
        let more = with(|s| s.st.as_ref().is_some_and(|st| st.get("shortWords").is_some_and(|w| !w.is_null()) && st["wordPrompts"].as_array().is_some_and(|a| !a.is_empty())));
        if more { start(cx, true, "Cru knows your voice now. Next, optionally: short replies, so a bare yes or no is recognised too."); } else { end(cx, "Cru knows your voice now.", false); }
    });
}

fn merge(s: &mut Spk, d: &Value) {
    let mut st = s.st.clone().unwrap_or_else(|| json!({}));
    if let (Some(o), Some(n)) = (st.as_object_mut(), d.as_object()) { for (k, v) in n { o.insert(k.clone(), v.clone()); } }
    s.st = Some(st);
}

fn forget(cx: &mut App) {
    c::confirm(cx, "Forget your voice?", "Forget your voice? Cru will act on any voice again, and admin mode will not check who is speaking.", "Forget", true, |cx| {
        crundi_shell::call(cx, |api| api.delete("/api/voice/speaker"), |r, cx| {
            match r {
                Ok(d) if c::t(&d, "ok") => with(|s| { merge(s, &d); s.msg = "Your voice is forgotten.".into(); s.bad = false; }),
                Ok(d) => with(|s| { let e = c::s(&d, "error"); s.msg = if e.is_empty() { "Not changed".into() } else { e }; s.bad = true; }),
                Err(_) => with(|s| { s.msg = "Not changed".into(); s.bad = true; }),
            }
            redraw(cx);
        });
    });
}

fn owner_only(cx: &mut App, on: bool) {
    crundi_shell::call(cx, move |api| api.post("/api/voice/settings", &json!({ "ownerOnly": on })), move |r, cx| {
        let good = matches!(&r, Ok(d) if c::t(d, "ok"));
        if good { with(|s| if let Some(st) = s.st.as_mut() { st["ownerOnly"] = json!(on); }); }
        let text = if good { if on { "Cru acts on your voice only".to_string() } else { "Cru acts on any voice".to_string() } } else { match &r { Ok(d) if !c::s(d, "error").is_empty() => c::s(d, "error"), _ => "Not saved".into() } };
        super::voice_status(cx, text, !good);
    });
}

// ─── Drawing ───

fn hint(t: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_11).text_color(T::text_muted()).child(t.into()) }
fn say(t: &str, bad: bool) -> Option<Div> { (!t.is_empty()).then(|| div().text_size(T::TEXT_11).text_color(if bad { T::red() } else { T::text_muted() }).child(t.to_string())) }
fn line(icon: IconName, t: String) -> Div { div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_12).text_color(T::text_secondary()).child(w::icon(icon, T::ICON_SM)).child(t) }
fn result_line(icon: IconName, t: String, good: bool) -> Div { div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_11).text_color(if good { T::text_secondary() } else { T::red() }).child(w::icon(icon, T::ICON_SM)).child(t) }
fn label(t: String) -> Div { div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(t) }
fn plural(n: i64) -> &'static str { if n == 1 { "" } else { "s" } }

/// The level bar beside Record (#vs-spk-meter).
fn meter_bar() -> Div {
    let lv = with(|s| s.rec.as_ref().map(|r| r.level()).unwrap_or(0.0));
    let pct = (lv.sqrt().sqrt() * 2.5).min(1.0);
    div().w(px(160.)).h(px(6.)).rounded(px(3.)).bg(T::bg_tertiary()).overflow_hidden()
        .child(div().id("vs-spk-meter").h_full().w(relative(pct)).bg(T::accent()))
}

fn rec_button() -> AnyElement {
    let (rec, busy) = with(|s| (s.rec.is_some(), s.busy));
    let b = if rec { w::primary_icon("vs-spk-rec", IconName::Square, "Stop") } else { w::primary_icon("vs-spk-rec", IconName::Mic, "Record") };
    w::disabled(b, busy).on_click(|_, _, cx| if with(|s| s.rec.is_some()) { stop(cx) } else { record(cx) }).into_any_element()
}

/// The part under Cru's settings: nothing until the server says it has the check.
pub fn part() -> Option<Div> {
    let st = with(|s| s.st.clone())?;
    let open = with(|s| s.open);
    let mut d = div().flex().flex_col().gap(T::SP_6).child(
        w::disclosure("vs-spk-box", "Your voice", open).text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text())
            .on_click(|_, _, cx| { with(|s| s.open = !s.open); redraw(cx); }));
    if !open { return Some(d); }
    let (active, words, pos, results, rec, busy, msg, bad) = with(|s| (s.active, s.words, s.pos, s.results.clone(), s.rec.is_some(), s.busy, s.msg.clone(), s.bad));
    let mut b = div().flex().flex_col().gap(T::SP_6);
    if !active {
        let available = c::t(&st, "available");
        let enrolled = c::t(&st, "enrolled");
        if !available { b = b.children(say("The voice check is not available on this server: onnxruntime-node is not installed.", false)); }
        else if enrolled {
            let when = c::s(&st, "enrolledAt");
            let day = chrono::DateTime::parse_from_rfc3339(&when).ok().map(|t| t.with_timezone(&chrono::Local).format("%-d/%-m/%Y").to_string());
            b = b.child(line(IconName::Check, format!("Enrolled{}: {} phrases, {} s of speech", day.map(|d| format!(" on {d}")).unwrap_or_default(), c::s(&st, "phrases"), c::s(&st, "seconds"))));
            if let Some(sw) = st.get("shortWords").filter(|v| v.is_object()) {
                let n = st["wordPrompts"].as_array().map(|a| a.len()).unwrap_or(0);
                b = b.child(if c::t(sw, "trained") { line(IconName::Check, format!("Short replies taught: {} of {n}, so a bare yes or no is recognised", c::s(sw, "words"))) } else { line(IconName::Info, "Short replies not taught: a bare yes or no is too short to tell".into()) });
            }
        } else { b = b.children(say("Not enrolled yet: Cru acts on any voice. Read a few short phrases so it knows yours (about a minute).", false)); }
        if available {
            let only = c::t(&st, "ownerOnly");
            let check = w::check("vs-spk-only", "Only act on my voice", only, |on, _, cx| owner_only(cx, *on));
            b = b.child(div().when(!enrolled, |d| d.opacity(T::SETTINGS_O_DIM)).child(if enrolled { check.into_any_element() } else { check.disabled(true).into_any_element() }))
                .child(hint("Anyone may still ask Cru questions; only your voice, or typing, changes anything. Admin mode always needs your voice."));
            let mut btns = div().flex().flex_wrap().gap(T::SP_8);
            btns = btns.child(if enrolled { w::button("vs-spk-start", "Enrol again") } else { w::primary("vs-spk-start", "Enrol my voice") }.on_click(|_, _, cx| start(cx, false, "")));
            if enrolled { if let Some(sw) = st.get("shortWords").filter(|v| v.is_object()) { btns = btns.child(w::button("vs-spk-words", if c::t(sw, "trained") { "Teach short replies again" } else { "Teach short replies" }).on_click(|_, _, cx| start(cx, true, ""))); } }
            if enrolled { btns = btns.child(w::button("vs-spk-forget", "Forget my voice").on_click(|_, _, cx| forget(cx))); }
            b = b.child(btns);
        }
        b = b.children(say(&msg, bad));
    } else if words {
        let prompts: Vec<String> = st["wordPrompts"].as_array().map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
        let kept = results.iter().filter(|r| r.accepted).count();
        let min = st["minWords"].as_u64().unwrap_or(5) as usize;
        let times = st["wordRepeats"].as_u64().unwrap_or(3);
        let trained = st.get("shortWords").is_some_and(|sw| c::t(sw, "trained"));
        if pos < prompts.len() {
            b = b.child(label(format!("Short reply {} of {}: click Record, say it {times} times with a short pause between, then Stop", pos + 1, prompts.len())))
                .child(div().id("vs-spk-prompt").text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(prompts[pos].clone()))
                .child(div().flex().flex_wrap().items_center().gap(T::SP_8).child(rec_button()).child(meter_bar())
                    .when(!rec, |d| d.child(w::disabled(w::button("vs-spk-skip", "Skip"), busy).on_click(|_, _, cx| { with(|s| { s.pos += 1; s.msg.clear(); s.bad = false; }); redraw(cx); })))
                    .when(kept >= min && !rec, |d| d.child(w::button("vs-spk-finish", "Finish now").on_click(|_, _, cx| finish(cx))))
                    .child(w::button("vs-spk-cancel", if trained { "Cancel" } else { "Not now" }).on_click(|_, _, cx| end(cx, "Short replies not changed.", false))));
        } else if kept >= min {
            b = b.child(label("All short replies said.".into())).child(div().flex().flex_wrap().gap(T::SP_8)
                .child(w::disabled(w::primary("vs-spk-finish", "Finish"), busy).on_click(|_, _, cx| finish(cx)))
                .child(w::button("vs-spk-cancel", "Cancel").on_click(|_, _, cx| end(cx, "Short replies not changed.", false))));
        } else {
            b = b.child(label(format!("At least {min} short replies are needed ({kept} so far)."))).child(div().flex().flex_wrap().gap(T::SP_8)
                .child(w::primary("vs-spk-words", "Start over").on_click(|_, _, cx| start(cx, true, "")))
                .child(w::button("vs-spk-cancel", "Cancel").on_click(|_, _, cx| end(cx, "Short replies not changed.", false))));
        }
        for r in &results {
            let word = prompts.get(r.n).cloned().unwrap_or_default();
            let t = format!("{word}{}{}", if r.accepted { format!(": learnt from {} saying{}", r.sayings, plural(r.sayings)) } else { String::new() }, if r.problem.is_empty() { String::new() } else { format!(": {}", r.problem) });
            b = b.child(result_line(if r.accepted { IconName::Check } else { IconName::X }, t, r.accepted));
        }
        b = b.children(if rec { say("Listening\u{2026}", false) } else { say(&msg, bad) });
    } else {
        let prompts: Vec<String> = st["prompts"].as_array().map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
        let kept = results.iter().filter(|r| r.accepted).count();
        let min = st["minPhrases"].as_u64().unwrap_or(5) as usize;
        if pos < prompts.len() {
            b = b.child(label(format!("Phrase {} of {}: click Record, read it as you would say it to Cru, then Stop", pos + 1, prompts.len())))
                .child(div().id("vs-spk-prompt").text_size(T::TEXT_14).text_color(T::text()).child(prompts[pos].clone()))
                .child(div().flex().flex_wrap().items_center().gap(T::SP_8).child(rec_button()).child(meter_bar())
                    .when(kept >= min && !rec, |d| d.child(w::button("vs-spk-finish", "Finish now").on_click(|_, _, cx| finish(cx))))
                    .child(w::button("vs-spk-cancel", "Cancel").on_click(|_, _, cx| end(cx, "Enrolment cancelled: nothing changed.", false))));
        } else {
            b = b.child(label("All phrases read.".into())).child(div().flex().flex_wrap().gap(T::SP_8)
                .child(w::disabled(w::primary("vs-spk-finish", "Finish"), busy).on_click(|_, _, cx| finish(cx)))
                .child(w::button("vs-spk-cancel", "Cancel").on_click(|_, _, cx| end(cx, "Enrolment cancelled: nothing changed.", false))));
        }
        for r in &results {
            let t = format!("Phrase {}: {} s of speech{}{}", r.n + 1, super::js_num(r.seconds), r.consistency.map(|c| format!(", {}% like the others", (c * 100.0).round() as i64)).unwrap_or_default(), if r.problem.is_empty() { String::new() } else { format!(". {}", r.problem) });
            b = b.child(result_line(if r.accepted { IconName::Check } else { IconName::X }, t, r.accepted));
        }
        b = b.children(if rec { say("Listening\u{2026}", false) } else { say(&msg, bad) });
    }
    d = d.child(b);
    Some(d)
}

/// The driver's view of it (tests).
pub fn state() -> Value {
    with(|s| json!({
        "shown": s.st.is_some(), "open": s.open, "active": s.active, "words": s.words, "pos": s.pos, "recording": s.rec.is_some(), "busy": s.busy,
        "msg": s.msg, "bad": s.bad, "results": s.results.iter().map(|r| json!({ "n": r.n, "accepted": r.accepted, "problem": r.problem })).collect::<Vec<_>>(),
        "enrolled": s.st.as_ref().is_some_and(|v| c::t(v, "enrolled")), "ownerOnly": s.st.as_ref().is_some_and(|v| c::t(v, "ownerOnly")),
    }))
}
