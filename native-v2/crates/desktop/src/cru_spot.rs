//! Cru pointing at something in the window (show_in_crundi): the web's
//! #cru-spot ring with its #cru-spot-note beside it (cruSpotlight,
//! src/webapp-html.js:6846-6875; its look at 3024-3027), and the chat's
//! flash (flashEl's .jump-flash, 19142-19147 and 3029-3033).
//!
//! The ring follows its target (a control found by its words, a card, an
//! idea, a chat pane) while it is on screen, pulses (1.6 s), and goes after
//! 12 s, on the next press anywhere (from 300 ms on), or when the target
//! goes. One view over the window (an overlay, cached): it draws nothing
//! and asks for no frame while nothing is shown.

use crate::ctx as kit;
use crundi_theme::T;
use crundi_webview::later;
use gpui::*;
use serde_json::Value;
use std::time::{Duration, Instant};

/// What the ring is around: where it is now, looked up again while shown.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A control found by its words (`w::find`): the words again.
    Words(String),
    /// A Kanban card by its id.
    Card(String),
    /// A mind map idea by its id.
    Idea(String),
    /// A chat's pane by its pane key (live:<id>).
    Chat(String),
}

#[derive(Clone)]
struct Ring { target: Target, note: String, at: Bounds<Pixels>, until: Instant, armed_at: Instant, seq: u64 }

#[derive(Clone)]
struct Flash { at: Bounds<Pixels>, seq: u64 }

pub struct CruSpot { ring: Option<Ring>, flash: Option<Flash>, seq: u64 }

struct Handle(Entity<CruSpot>);
impl Global for Handle {}

/// Made at init, as an overlay over the whole window (over Cru's screen too).
pub fn init(cx: &mut App) {
    let v = cx.new(|_| CruSpot { ring: None, flash: None, seq: 0 });
    crundi_shell::register_overlay(cx, v.clone().into());
    cx.set_global(Handle(v));
}

fn view(cx: &App) -> Option<Entity<CruSpot>> { cx.try_global::<Handle>().map(|h| h.0.clone()) }

/// Where `t` is in the window now, if it is on screen.
pub fn locate(t: &Target, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
    let b = match t {
        Target::Words(w) => crundi_widgets::find::find(w, window),
        Target::Card(id) => { let s = crundi_planning::kanban::state(&crundi_shell::stores(cx), window, cx); pick_box(s["boxes"]["cards"].as_array().and_then(|l| l.iter().find(|c| c["id"].as_str() == Some(id)))) }
        Target::Idea(id) => { let s = crundi_planning::mindmap::state(&crundi_shell::stores(cx), window, cx); pick_box(s["nodes"].as_array().and_then(|l| l.iter().find(|n| n["id"].as_str() == Some(id))).map(|n| &n["box"])) }
        Target::Chat(key) => {
            let stores = crundi_shell::stores(cx);
            let st = crundi_shell::registry::list(cx).into_iter().find(|a| a.name == "workbench").and_then(|a| a.state.clone())?;
            let s = st(&stores, window, cx);
            pick_box(s["leaves"].as_array().and_then(|l| l.iter().find(|n| n["key"].as_str() == Some(key))))
        }
    }?;
    let win = Bounds::new(Point::default(), window.viewport_size());
    let seen = b.intersect(&win);
    (f32::from(seen.size.width) >= 2. && f32::from(seen.size.height) >= 2.).then_some(b)
}

/// A box ({x, y, w, h}, window coordinates) as the areas report what they
/// lay out to the driver (the Kanban's cards, the map's ideas, the
/// Workbench's leaves).
fn pick_box(b: Option<&Value>) -> Option<Bounds<Pixels>> {
    let b = b?;
    let n = |k: &str| b[k].as_f64().map(|x| x as f32);
    Some(Bounds::new(point(px(n("x")?), px(n("y")?)), size(px(n("w")?), px(n("h")?))))
}

/// The ring around `t` with Cru's note (none: the ring alone), for 12 s.
/// False when `t` is not on screen.
pub fn spotlight(t: Target, note: &str, window: &mut Window, cx: &mut App) -> bool {
    let Some(v) = view(cx) else { return false };
    let Some(at) = locate(&t, window, cx) else { return false };
    let now = Instant::now();
    let seq = v.update(cx, |s, cx| {
        s.seq += 1;
        s.ring = Some(Ring { target: t, note: note.to_string(), at, until: now + Duration::from_secs(12), armed_at: now + Duration::from_millis(300), seq: s.seq });
        cx.notify();
        s.seq
    });
    follow(seq);
    true
}

/// The chat pane's flash (flashEl: the accent ring that fades in 2.2 s).
pub fn flash(t: Target, window: &mut Window, cx: &mut App) -> bool {
    let Some(v) = view(cx) else { return false };
    let Some(at) = locate(&t, window, cx) else { return false };
    let seq = v.update(cx, |s, cx| { s.seq += 1; s.flash = Some(Flash { at, seq: s.seq }); cx.notify(); s.seq });
    later(Duration::from_millis(2400), move || kit::on_app(move |_, cx| if let Some(v) = view(cx) {
        v.update(cx, |s, cx| if s.flash.as_ref().is_some_and(|f| f.seq == seq) { s.flash = None; cx.notify(); });
    }));
    true
}

/// The ring goes (cruSpotOff).
pub fn off(cx: &mut App) {
    if let Some(v) = view(cx) { v.update(cx, |s, cx| if s.ring.take().is_some() { cx.notify(); }); }
}

/// What is shown, for the driver.
pub fn state(cx: &App) -> Value {
    let Some(v) = view(cx) else { return Value::Null };
    let s = v.read(cx);
    let b = |b: &Bounds<Pixels>| serde_json::json!({ "x": f32::from(b.origin.x), "y": f32::from(b.origin.y), "w": f32::from(b.size.width), "h": f32::from(b.size.height) });
    serde_json::json!({
        "ring": s.ring.as_ref().map(|r| serde_json::json!({ "target": format!("{:?}", r.target), "note": r.note, "at": b(&r.at) })),
        "flash": s.flash.as_ref().map(|f| b(&f.at)),
        "marks": crundi_widgets::find::live(),
    })
}

/// While the ring is up: where its target is now (it follows a scroll),
/// every 100 ms; gone when the time is up or the target went.
fn follow(seq: u64) {
    later(Duration::from_millis(100), move || kit::on_app(move |window, cx| {
        let Some(v) = view(cx) else { return };
        let Some(r) = v.read(cx).ring.clone().filter(|r| r.seq == seq) else { return };
        let at = if Instant::now() > r.until { None } else { locate(&r.target, window, cx) };
        v.update(cx, |s, cx| match at {
            None => { s.ring = None; cx.notify(); }
            Some(at) => if let Some(r) = s.ring.as_mut().filter(|r| r.at != at) { r.at = at; cx.notify(); },
        });
        if at.is_some() { follow(seq); }
    }));
}

impl Render for CruSpot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("cru-spot");
        let root = div().id("cru-spot-layer").absolute().top_0().left_0().size_full();
        let mut root = root;
        if let Some(f) = &self.flash {
            // .jump-flash: 0 0 0 3px accent, 0 0 18px 4px accent-dim, held to 60%, then gone.
            root = root.child(div().absolute().left(f.at.origin.x).top(f.at.origin.y).w(f.at.size.width).h(f.at.size.height).rounded(T::R_CARD)
                .with_animation(("cru-flash", f.seq as usize), Animation::new(Duration::from_millis(2200)).with_max_fps(T::ANIM_FPS), |d, t| {
                    let k = if t <= 0.6 { 1.0 } else { 1.0 - (t - 0.6) / 0.4 };
                    d.shadow(vec![shadow(T::accent().opacity(k), 0., px(3.) * k), shadow(T::accent_dim().opacity(k), 18., px(4.) * k)])
                }));
        }
        if let Some(r) = &self.ring {
            let pad = px(6.);
            let a = r.at;
            // #cru-spot: 2px accent-hover edge, 10px corners, a glow that breathes (cruSpot, 1.6 s).
            root = root.child(div().absolute().left(a.origin.x - pad).top(a.origin.y - pad).w(a.size.width + pad * 2.).h(a.size.height + pad * 2.)
                .border_2().border_color(T::accent_hover()).rounded(T::R_CARD)
                .with_animation(("cru-spot", r.seq as usize), Animation::new(Duration::from_millis(1600)).repeat().with_max_fps(T::ANIM_FPS), |d, t| {
                    // 0% and 100%: 3px and 18px 4px; 50%: 7px and 34px 10px (ease-in-out).
                    let k = 0.5 - 0.5 * (t * std::f32::consts::TAU).cos();
                    d.shadow(vec![shadow(T::accent_dim(), 0., px(3. + 4. * k)), shadow(T::accent_dim(), 18. + 16. * k, px(4. + 6. * k))])
                }));
            if !r.note.is_empty() {
                // #cru-spot-note: under the target (12 px), above it if it would leave the window; its left
                // edge with the target's, kept 8 px inside the window.
                let vp = window.viewport_size();
                let (tw, th) = note_size(&r.note, window);
                let mut top = a.origin.y + a.size.height + px(12.);
                if top + th > vp.height - px(8.) { top = (a.origin.y - th - px(12.)).max(px(8.)); }
                let left = a.origin.x.min(vp.width - tw - px(8.)).max(px(8.));
                root = root.child(div().absolute().left(left).top(top).max_w(px(320.))
                    .flex().gap(T::SP_8).items_baseline().px(T::SP_12).py(T::SP_8).rounded(T::R_BUBBLE)
                    .bg(T::bg_card()).border_1().border_color(T::accent()).text_color(T::text()).text_size(T::TEXT_13).line_height(relative(1.4))
                    .shadow(vec![BoxShadow { color: hsla(0., 0., 0., 0.45), offset: point(px(0.), px(10.)), blur_radius: px(30.), spread_radius: px(0.), inset: false }])
                    .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(T::accent_hover()).child("Cru"))
                    .child(div().min_w_0().child(r.note.clone())));
            }
            // Gone on the next press anywhere (from 300 ms on); the press still does what it does.
            let armed_at = r.armed_at;
            let me = cx.entity().downgrade();
            root = root.child(canvas(|_, _, _| {}, move |_, _, window, _| {
                let me = me.clone();
                window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || Instant::now() < armed_at { return; }
                    if let Some(me) = me.upgrade() { me.update(cx, |s, cx| if s.ring.take().is_some() { cx.notify(); }); }
                });
            }).absolute().size_0());
        }
        root
    }
}

fn shadow(color: Hsla, blur: f32, spread: Pixels) -> BoxShadow {
    BoxShadow { color, offset: point(px(0.), px(0.)), blur_radius: px(blur), spread_radius: spread, inset: false }
}

/// The note bubble's size: "Cru", 8 px, the note at 13 px wrapped in 320 px
/// less its padding; 1.4 lines.
fn note_size(note: &str, window: &mut Window) -> (Pixels, Pixels) {
    let run = |s: &str, bold: bool| {
        let mut f = window.text_style().font();
        if bold { f.weight = FontWeight::SEMIBOLD; }
        TextRun { len: s.len(), font: f, color: T::text(), background_color: None, underline: None, strikethrough: None }
    };
    let w = |s: &str, bold: bool| window.text_system().shape_line(s.to_string().into(), T::TEXT_13, &[run(s, bold)], None).width;
    let head = w("Cru", true) + T::SP_8 + T::SP_12 * 2. + px(2.);
    let text_w = w(note, false);
    let room = px(320.) - head;
    let lines = (f32::from(text_w) / f32::from(room).max(40.)).ceil().max(1.);
    let tw = (head + text_w).min(px(320.));
    (tw, px(13. * 1.4 * lines) + T::SP_8 * 2. + px(2.))
}
