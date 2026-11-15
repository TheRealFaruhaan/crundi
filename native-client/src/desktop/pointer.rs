//! browser_mouse and browser_touch as real input (app/browser-pointer.js,
//! main.js browserTouch): CDP Input.dispatchMouseEvent / dispatchTouchEvent,
//! hit-tested like a person's pointer. The pointer remembers where it is and
//! the button held, per page; on a drawer tab the chat's cursor is drawn
//! over the page before each step.

use crate::browser;
use serde_json::{json, Value};
use std::time::Duration;

pub const ACTIONS: [&str; 6] = ["move", "click", "double_click", "down", "up", "drag"];
const MAX_MS: f64 = 10000.0;

fn bit(b: &str) -> u32 { match b { "left" => 1, "right" => 2, "middle" => 4, _ => 0 } }

fn num(v: Option<&Value>) -> Option<f64> {
    match v? { Value::Number(n) => n.as_f64(), Value::String(s) if !s.trim().is_empty() => s.trim().parse().ok(), _ => None }
}

/// A request's problem, or None (browser-pointer.js mouseError).
pub fn mouse_error(m: &Value) -> Option<String> {
    let action = m.get("action").and_then(|a| a.as_str()).unwrap_or("click");
    if !ACTIONS.contains(&action) { return Some(format!("action must be one of {}", ACTIONS.join(", "))); }
    if let Some(b) = m.get("button").filter(|b| !b.is_null()) { if bit(b.as_str().unwrap_or("")) == 0 { return Some("button must be \"left\", \"right\" or \"middle\"".into()); } }
    let (x, y) = (num(m.get("x")), num(m.get("y")));
    if action != "up" && (x.is_none() || y.is_none()) { return Some(format!("{action} needs x and y")); }
    if action == "up" && x.is_none() != y.is_none() { return Some("up takes both x and y, or neither".into()); }
    if action == "drag" && (num(m.get("toX")).is_none() || num(m.get("toY")).is_none()) { return Some("drag needs toX and toY (where to release)".into()); }
    if let Some(d) = m.get("duration").filter(|d| !d.is_null()) { if num(Some(d)).is_none_or(|d| d < 0.0) { return Some("duration must be a number of milliseconds".into()); } }
    None
}

/// How long the cursor takes between two points.
pub fn glide_ms(from: Option<(f64, f64)>, to: (f64, f64)) -> f64 {
    let Some(f) = from else { return 0.0 };
    let d = ((to.0 - f.0).powi(2) + (to.1 - f.1).powi(2)).sqrt();
    if d < 1.0 { return 0.0; }
    (140.0 + d * 0.45).clamp(140.0, 480.0).round()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The chat's cursor at (x, y), then wait ms.
    Cursor { x: f64, y: f64, pressed: bool, ms: f64, end: bool },
    /// A mouse event: type, x, y, button, buttons, clickCount.
    Ev(&'static str, f64, f64, String, u32, u32),
    Sleep(f64),
}

/// The pointer's state per page: where, and the button held.
#[derive(Debug, Clone, Default)]
pub struct Ptr { pub x: Option<f64>, pub y: Option<f64>, pub down: Option<String> }

/// The steps of one request (runMouse), and the pointer after them.
pub fn plan(m: &Value, st: &mut Ptr) -> Result<Vec<Step>, String> {
    if let Some(e) = mouse_error(m) { return Err(e); }
    let action = m.get("action").and_then(|a| a.as_str()).unwrap_or("click");
    let button = m.get("button").and_then(|b| b.as_str()).unwrap_or("left").to_string();
    let (x, y) = (num(m.get("x")), num(m.get("y")));
    let dur = num(m.get("duration"));
    let mut v = vec![];
    let held = |st: &Ptr| st.down.as_ref().map(|b| (b.clone(), bit(b))).unwrap_or(("none".into(), 0));
    let travel = |v: &mut Vec<Step>, st: &mut Ptr, tx: f64, ty: f64, ms: Option<f64>| {
        let from = st.x.zip(st.y);
        let ms = ms.unwrap_or_else(|| glide_ms(from, (tx, ty))).clamp(0.0, MAX_MS).round();
        if st.down.is_none() {
            v.push(Step::Cursor { x: tx, y: ty, pressed: false, ms, end: false });
            let (b, bs) = held(st);
            v.push(Step::Ev("mouseMoved", tx, ty, b, bs, 0));
        } else {
            let f = from.unwrap_or((tx, ty));
            let n = (ms / 16.0).round().clamp(2.0, 120.0) as usize;
            v.push(Step::Cursor { x: tx, y: ty, pressed: true, ms: 0.0, end: false });
            for i in 1..=n {
                let (b, bs) = held(st);
                v.push(Step::Ev("mouseMoved", f.0 + (tx - f.0) * i as f64 / n as f64, f.1 + (ty - f.1) * i as f64 / n as f64, b, bs, 0));
                v.push(Step::Sleep(ms / n as f64));
            }
        }
        st.x = Some(tx); st.y = Some(ty);
    };
    let press = |v: &mut Vec<Step>, st: &mut Ptr, count: u32| {
        st.down = Some(button.clone());
        let (px, py) = (st.x.unwrap_or(0.0), st.y.unwrap_or(0.0));
        v.push(Step::Cursor { x: px, y: py, pressed: true, ms: 0.0, end: false });
        v.push(Step::Ev("mousePressed", px, py, button.clone(), bit(&button), count));
    };
    let release = |v: &mut Vec<Step>, st: &mut Ptr, count: u32| {
        let b = st.down.take().unwrap_or_else(|| button.clone());
        let (px, py) = (st.x.unwrap_or(0.0), st.y.unwrap_or(0.0));
        v.push(Step::Ev("mouseReleased", px, py, b, 0, count));
        v.push(Step::Cursor { x: px, y: py, pressed: false, ms: 0.0, end: false });
    };
    match action {
        "move" => travel(&mut v, st, x.unwrap(), y.unwrap(), dur),
        "click" | "double_click" => {
            if st.down.is_some() { release(&mut v, st, 1); }
            travel(&mut v, st, x.unwrap(), y.unwrap(), dur);
            press(&mut v, st, 1); release(&mut v, st, 1);
            if action == "double_click" { v.push(Step::Sleep(60.0)); press(&mut v, st, 2); release(&mut v, st, 2); }
        }
        "down" => {
            if st.down.is_some() { release(&mut v, st, 1); }
            travel(&mut v, st, x.unwrap(), y.unwrap(), dur);
            press(&mut v, st, 1);
        }
        "up" => {
            if let (Some(x), Some(y)) = (x, y) { travel(&mut v, st, x, y, dur); }
            if st.x.is_none() { return Err("up needs x and y: the pointer has not been anywhere yet".into()); }
            if st.down.is_some() { release(&mut v, st, 1); }
        }
        _ => {
            if st.down.is_some() { release(&mut v, st, 1); }
            travel(&mut v, st, x.unwrap(), y.unwrap(), None);
            press(&mut v, st, 1);
            travel(&mut v, st, num(m.get("toX")).unwrap(), num(m.get("toY")).unwrap(), Some(dur.unwrap_or(500.0)));
            release(&mut v, st, 1);
        }
    }
    if let (Some(x), Some(y)) = (st.x, st.y) { v.push(Step::Cursor { x, y, pressed: st.down.is_some(), ms: 0.0, end: true }); }
    Ok(v)
}

/// Run the steps in order; then done.
fn run(i: usize, drawer: bool, mut steps: std::collections::VecDeque<Step>, done: Box<dyn FnOnce()>) {
    let Some(step) = steps.pop_front() else { return done() };
    match step {
        Step::Ev(t, x, y, b, bs, c) => {
            let mut p = json!({ "type": t, "x": x, "y": y, "button": b, "buttons": bs });
            if c > 0 { p["clickCount"] = json!(c); }
            browser::cdp(i, "Input.dispatchMouseEvent", p, Box::new(move |_, _| run(i, drawer, steps, done)));
        }
        Step::Sleep(ms) => slint::Timer::single_shot(Duration::from_millis(ms.max(0.0) as u64), move || run(i, drawer, steps, done)),
        Step::Cursor { x, y, ms, end, .. } => {
            if drawer {
                browser::with(|st| st.panels[i].cursor_at = Some((x as f32, y as f32, std::time::Instant::now())));
                browser::sync(i);
                if end { cursor_fade(i); }
            }
            if drawer && ms > 0.0 { slint::Timer::single_shot(Duration::from_millis(ms as u64), move || run(i, drawer, steps, done)); }
            else { run(i, drawer, steps, done) }
        }
    }
}

/// The chat's cursor goes 1.8 s after it last moved (VD idle fade).
fn cursor_fade(i: usize) {
    slint::Timer::single_shot(Duration::from_millis(1800), move || {
        let gone = browser::with(|st| st.panels.get_mut(i).is_some_and(|p| {
            let old = p.cursor_at.is_some_and(|(_, _, t)| t.elapsed() >= Duration::from_millis(1700)) && p.mouse_down.is_none();
            if old { p.cursor_at = None; }
            old
        }));
        if gone { browser::sync(i); }
    });
}

/// browserMouse.
pub fn mouse(i: usize, key: &str, m: &Value, reply: Box<dyn FnOnce(Value)>) {
    let mut st = browser::with(|s| { let p = &s.panels[i]; Ptr { x: p.mouse_at.map(|m| m.0 as f64), y: p.mouse_at.map(|m| m.1 as f64), down: p.mouse_down.clone() } });
    let steps = match plan(m, &mut st) { Ok(s) => s, Err(e) => return reply(json!({ "ok": false, "error": e })) };
    browser::with(|s| { let p = &mut s.panels[i]; p.mouse_at = st.x.zip(st.y).map(|(x, y)| (x as f32, y as f32)); p.mouse_down = st.down.clone(); });
    let drawer = key.starts_with("drawer:");
    run(i, drawer, steps.into(), Box::new(move || {
        let url = browser::with(|s| s.panels[i].url.clone());
        reply(json!({ "ok": true, "url": url, "x": st.x, "y": st.y, "held": st.down }))
    }));
}

/// browserTouch: touch emulation on, then each step's touch event and wait.
pub fn touch(i: usize, m: &Value, reply: Box<dyn FnOnce(Value)>) {
    let steps: Vec<Value> = m.get("steps").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    let first = browser::with(|s| !std::mem::replace(&mut s.panels[i].touch_on, true));
    let go = move || touch_steps(i, steps.into(), Box::new(move || {
        let url = browser::with(|s| s.panels[i].url.clone());
        reply(json!({ "ok": true, "url": url }))
    }));
    if first {
        browser::cdp(i, "Emulation.setTouchEmulationEnabled", json!({ "enabled": true, "maxTouchPoints": 5 }), Box::new(move |_, _| go()));
    } else { go(); }
}

fn touch_steps(i: usize, mut steps: std::collections::VecDeque<Value>, done: Box<dyn FnOnce()>) {
    let Some(s) = steps.pop_front() else { return done() };
    let ty = s.get("type").and_then(|t| t.as_str()).unwrap_or("touchStart").to_string();
    let points: Vec<Value> = s.get("points").and_then(|p| p.as_array()).cloned().unwrap_or_default().iter()
        .map(|p| json!({ "x": num(p.get("x")).unwrap_or(0.0), "y": num(p.get("y")).unwrap_or(0.0), "id": num(p.get("id")).unwrap_or(0.0) })).collect();
    let wait = num(s.get("wait")).unwrap_or(0.0).clamp(0.0, MAX_MS) as u64;
    browser::cdp(i, "Input.dispatchTouchEvent", json!({ "type": ty, "touchPoints": points }), Box::new(move |_, _| {
        if wait > 0 { slint::Timer::single_shot(Duration::from_millis(wait), move || touch_steps(i, steps, done)); } else { touch_steps(i, steps, done); }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors() {
        assert_eq!(mouse_error(&json!({ "action": "fly", "x": 1, "y": 1 })).unwrap(), "action must be one of move, click, double_click, down, up, drag");
        assert_eq!(mouse_error(&json!({ "action": "click" })).unwrap(), "click needs x and y");
        assert_eq!(mouse_error(&json!({ "action": "drag", "x": 1, "y": 1 })).unwrap(), "drag needs toX and toY (where to release)");
        assert_eq!(mouse_error(&json!({ "action": "up", "x": 1 })).unwrap(), "up takes both x and y, or neither");
        assert_eq!(mouse_error(&json!({ "x": 1, "y": 2, "button": "side" })).unwrap(), "button must be \"left\", \"right\" or \"middle\"");
        assert!(mouse_error(&json!({ "x": "3", "y": 4 })).is_none());
    }

    #[test]
    fn plans() {
        let mut st = Ptr::default();
        let s = plan(&json!({ "x": 10, "y": 20 }), &mut st).unwrap();
        let evs: Vec<_> = s.iter().filter_map(|x| if let Step::Ev(t, _, _, _, _, c) = x { Some((*t, *c)) } else { None }).collect();
        assert_eq!(evs, vec![("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 1)]);
        assert_eq!((st.x, st.y, st.down.clone()), (Some(10.0), Some(20.0), None));
        // Down, then move while held: the path in steps.
        plan(&json!({ "action": "down", "x": 0, "y": 0 }), &mut st).unwrap();
        assert_eq!(st.down.as_deref(), Some("left"));
        let s = plan(&json!({ "action": "move", "x": 100, "y": 0, "duration": 160 }), &mut st).unwrap();
        assert_eq!(s.iter().filter(|x| matches!(x, Step::Ev("mouseMoved", ..))).count(), 10);
        let s = plan(&json!({ "action": "up" }), &mut st).unwrap();
        assert!(s.iter().any(|x| matches!(x, Step::Ev("mouseReleased", ..))));
        assert!(st.down.is_none());
        let s = plan(&json!({ "action": "double_click", "x": 5, "y": 5 }), &mut st).unwrap();
        assert!(s.iter().any(|x| matches!(x, Step::Ev("mousePressed", _, _, _, _, 2))));
        assert_eq!(glide_ms(Some((0.0, 0.0)), (0.0, 0.5)), 0.0);
        assert_eq!(glide_ms(Some((0.0, 0.0)), (1000.0, 0.0)), 480.0);
        let mut fresh = Ptr::default();
        assert!(plan(&json!({ "action": "up" }), &mut fresh).is_err());
    }
}
