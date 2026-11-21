//! A Kanban card at rest as one element (F38): laid out once per card and
//! width into a `paint::Draw` (the card's box, its title, description,
//! subtasks, progress line and its controls as they look at rest), then only
//! painted while its column scrolls. The places are the live card's (view.rs
//! `card_el`): the same tokens, laid out as taffy lays out those divs.
//! The card under the pointer, or in use, is the live one (view.rs).

use super::board::Card;
use crate::paint::{r, Draw, Op, Txt, PHI};
use crundi_theme::{IconName, T};
use gpui::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub use crundi_widgets::paint::Look;

/// What a card at rest is drawn from (a change of any of it lays it out again).
#[derive(Clone, PartialEq)]
pub struct Key { pub card: Card, pub dragged: bool, pub clip: bool, pub width: Pixels }

/// The laid out cards of a column (by card id): kept while only the scroll moves.
pub type Cache = Rc<RefCell<HashMap<String, (Key, Rc<Draw>)>>>;

fn b(x: Pixels, y: Pixels, w: Pixels, h: Pixels) -> Bounds<Pixels> { Bounds { origin: point(x, y), size: size(w, h) } }

/// A control at rest (`w::paint::button_at`): its width; its place is a hit.
#[allow(clippy::too_many_arguments)]
fn button(d: &mut Draw, x: Pixels, y: Pixels, label: Option<&str>, icon: IconName, look: Look, hit: &str, window: &mut Window, cx: &App) -> Pixels {
    let w = crundi_widgets::paint::button_at(d, x, y, label, icon, look, window, cx);
    d.hits.push((b(x, y, w, T::BTN_H.max(T::ICON_BTN)), hit.to_string()));
    w
}

fn button_w(label: Option<&str>, look: Look, window: &mut Window, cx: &App) -> Pixels { crundi_widgets::paint::button_w(label, look, window, cx) }

/// A ghost button with its own colours (clip_look / menu_look, the same
/// colours view.rs gives the live paperclip and menu): `w::paint::button_tinted_at`,
/// its place a hit. Returns its width.
#[allow(clippy::too_many_arguments)]
fn button_tint(d: &mut Draw, x: Pixels, y: Pixels, label: Option<&str>, icon: IconName, look: (Option<Hsla>, Hsla, Hsla), hit: &str, window: &mut Window, cx: &App) -> Pixels {
    let w = crundi_widgets::paint::button_tinted_at(d, x, y, label, icon, look, window, cx);
    d.hits.push((b(x, y, w, T::BTN_H.max(T::ICON_BTN)), hit.to_string()));
    w
}

/// `button_tint`'s width.
fn tint_w(label: Option<&str>, window: &mut Window, cx: &App) -> Pixels { crundi_widgets::paint::button_tinted_w(label, window, cx) }

/// The paperclip's colours (view.rs: .card-clip, .has, .open).
pub(crate) fn clip_look(media: bool, open: bool) -> (Option<Hsla>, Hsla, Hsla) {
    if open { (Some(T::bg_tertiary()), T::text(), T::border()) } else { (None, if media { T::text_secondary() } else { T::text_muted() }, transparent_black()) }
}

/// The menu button's colours (view.rs: .kc-menu, in the footer or a compact
/// card's title; .menu-open tinted).
pub(crate) fn menu_look(foot: bool, open: bool) -> (Option<Hsla>, Hsla, Hsla) {
    if open { (Some(T::accent_dim()), T::accent_hover(), T::accent()) }
    else if foot { (None, T::text_secondary(), T::border()) } else { (None, T::text_muted(), transparent_black()) }
}

/// Lay a card out at `width` (its box's width).
pub fn lay(k: &Key, window: &mut Window, cx: &App) -> Draw {
    let cd = &k.card;
    let mut d = Draw::default();
    let cw = k.width;
    let (x0, iw) = (px(1.) + T::SP_12, cw - px(2.) - T::SP_12 * 2.);
    let mut y = px(1.) + T::SP_12;
    // Dragged: dimmed (the card's opacity).
    if k.dragged { d.opacity = Some(T::KANBAN_O_DIM); }
    let card_bg = d.ops.len();
    d.bg(b(px(0.), px(0.), cw, px(0.)), T::R_CARD, T::bg_card());

    // The title row: the title, the mind map count, the paperclip, the menu (compact).
    let mut rights: Vec<Pixels> = vec![];
    // The mind map count (`w::icon_tag`, accent): measured here, painted below.
    let mm_w = (cd.mm > 0).then(|| { let mut m = Draw::default(); crundi_widgets::paint::icon_tag_at(&mut m, px(0.), px(0.), IconName::Share2, &cd.mm.to_string(), "accent", window, cx).size.width });
    if let Some(w) = mm_w { rights.push(w); }
    let media = (cd.media > 0).then(|| cd.media.to_string());
    rights.push(tint_w(media.as_deref(), window, cx));
    if cd.compact { rights.push(T::ICON_BTN); }
    let rw: Pixels = rights.iter().fold(px(0.), |a, w| a + *w) + T::SP_8 * rights.len() as f32;
    let tw = (iw - rw).max(px(0.));
    let title = Rc::new(Txt::shape(&cd.title, T::TEXT_14, T::W_SEMIBOLD, d.fade(T::text()), PHI, Some(tw), None, window, cx));
    let mut row_h = title.h.max(T::ICON_BTN);
    let th = title.h;
    d.text(title, point(x0, y));
    let mut x = x0 + tw + T::SP_8;
    if let Some(w) = mm_w {
        let cy = y + T::SP_2;
        let tb = crundi_widgets::paint::icon_tag_at(&mut d, x, cy, IconName::Share2, &cd.mm.to_string(), "accent", window, cx);
        row_h = row_h.max(T::SP_2 + tb.size.height);
        d.hits.push((tb, "mm".into()));
        x += w + T::SP_8;
    }
    let w = button_tint(&mut d, x, y, media.as_deref(), IconName::Paperclip, clip_look(cd.media > 0, k.clip), "clip", window, cx);
    x += w + T::SP_8;
    if cd.compact { button_tint(&mut d, x, y, None, IconName::Menu, menu_look(false, cd.menu_open), "menu", window, cx); }
    let _ = th;
    y += row_h;

    if !cd.desc.is_empty() {
        let t = Rc::new(Txt::shape(&cd.desc, T::TEXT_12, FontWeight::NORMAL, d.fade(T::text_secondary()), PHI, Some(iw), None, window, cx));
        let h = t.h;
        d.text(t, point(x0, y));
        y += h;
    }

    // The subtasks (.kanban-todo rows at rest).
    if cd.total > 0 {
        y += T::SP_8;
        for (i, td) in cd.todos.iter().enumerate() {
            if i > 0 { y += px(1.); }
            let (rx, rwid) = (x0 + T::SP_6, iw - T::SP_6 - T::SP_4);
            let media = (td.media > 0).then(|| td.media.to_string());
            let mw = media.as_ref().map(|m| button_w(Some(m), Look::Default, window, cx));
            let n = 3 + mw.is_some() as usize;
            let tw = (rwid - T::CHECK_BOX - mw.unwrap_or(px(0.)) - T::ICON_BTN - T::SP_8 * (n - 1) as f32).max(px(0.));
            let t = Rc::new(Txt::shape(&td.text, T::TEXT_13, FontWeight::NORMAL, d.fade(if td.done { T::text_muted() } else { T::text() }), PHI, Some(tw), None, window, cx));
            let inner = t.h.max(T::SP_2 + T::CHECK_BOX).max(T::ICON_BTN).max(if mw.is_some() { T::BTN_H } else { px(0.) });
            let ry = y + T::SP_4;
            // The box (w::check's at rest).
            let cb = crundi_widgets::paint::check_at(&mut d, point(rx, ry + T::SP_2), td.done);
            d.hits.push((cb, format!("tick:{}:{}", td.id, td.done)));
            let mut cx_ = rx + T::CHECK_BOX + T::SP_8;
            let h = t.h;
            d.text(t, point(cx_, ry));
            cx_ += tw + T::SP_8;
            if let (Some(m), Some(w)) = (media.as_deref(), mw) { button(&mut d, cx_, ry, Some(m), IconName::Paperclip, Look::Default, &format!("tmedia:{}", td.id), window, cx); cx_ += w + T::SP_8; }
            // The "…" at its faint opacity (times the card's).
            let was = d.opacity;
            d.opacity = Some(was.unwrap_or(1.) * T::KANBAN_O_CARET);
            button(&mut d, cx_, ry, None, IconName::Ellipsis, Look::Ghost, &format!("more:{}", td.id), window, cx);
            d.opacity = was;
            let _ = h;
            y += inner + T::SP_4 * 2.;
        }
        // The progress line and "n of m".
        y += T::SP_8;
        let label = Rc::new(Txt::shape(&format!("{} of {}", cd.done, cd.total), T::TEXT_11, FontWeight::NORMAL, d.fade(T::text_muted()), PHI, None, None, window, cx));
        let lh = label.h;
        let bh = T::SP_2 + px(1.);
        let rh = lh.max(bh);
        let bar_w = iw - T::SP_8 - label.w;
        let by = y + r((rh - bh) / 2.);
        d.bg(b(x0, by, bar_w, bh), T::R_PILL, T::bg_tertiary());
        let frac = cd.done as f32 / cd.total.max(1) as f32;
        // (Its right edge as taffy snaps it: half a pixel rounds toward zero.)
        let fw = r(bar_w * frac);
        if fw > px(0.) { d.bg(b(x0, by, fw, bh), T::R_PILL, if cd.done == cd.total { T::green() } else { T::accent() }); }
        d.text(label, point(x0 + bar_w + T::SP_8, y + r((rh - lh) / 2.)));
        y += rh;
    }

    // The footer: the menu, then + Subtask (not on a compact card).
    if !cd.compact {
        y += T::SP_10;
        button_tint(&mut d, x0, y, None, IconName::Menu, menu_look(true, cd.menu_open), "menu", window, cx);
        button(&mut d, x0 + T::ICON_BTN + T::SP_6, y, Some("Subtask"), IconName::Plus, Look::Default, "sub", window, cx);
        y += T::ICON_BTN.max(T::BTN_H);
    }
    let h = y + T::SP_10 + px(1.);
    if let Op::Bg { b: bb, .. } = &mut d.ops[card_bg] { bb.size.height = h; }
    d.border(b(px(0.), px(0.), cw, h), T::R_CARD, px(1.), T::border());
    d.h = h;
    d
}

/// The card at rest, one element: a measured leaf (its height from its
/// layout), painted from the cache; a press on it arms a drag (mod.rs).
pub struct CardEl { pub key: Key, pub cache: Cache, pub status: String, pub boxes: Rc<RefCell<super::Boxes>> }

impl IntoElement for CardEl {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl CardEl {
    fn draw(&self, width: Pixels, window: &mut Window, cx: &App) -> Rc<Draw> {
        let mut key = self.key.clone();
        key.width = width;
        if let Some((k, d)) = self.cache.borrow().get(&key.card.id) { if *k == key { return d.clone(); } }
        let d = Rc::new(lay(&key, window, cx));
        self.cache.borrow_mut().insert(key.card.id.clone(), (key, d.clone()));
        d
    }
}

impl Element for CardEl {
    type RequestLayoutState = ();
    type PrepaintState = Option<(Rc<Draw>, Hitbox)>;
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, _: &mut App) -> (LayoutId, ()) {
        let me = CardEl { key: self.key.clone(), cache: self.cache.clone(), status: self.status.clone(), boxes: self.boxes.clone() };
        let id = window.request_measured_layout(Style::default(), move |known, avail, window, cx| {
            let w = known.width.or(match avail.width { AvailableSpace::Definite(w) => Some(w), _ => None }).unwrap_or(px(0.));
            size(w, me.draw(w, window, cx).h)
        });
        (id, ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Option<(Rc<Draw>, Hitbox)> {
        let d = self.draw(bounds.size.width, window, cx);
        // Its box, as painted (the drop target, a press's grab).
        let bx = super::board::Box4 { x: f32::from(bounds.origin.x), y: f32::from(bounds.origin.y), w: f32::from(bounds.size.width), h: f32::from(bounds.size.height) };
        let mut bxs = self.boxes.borrow_mut();
        bxs.cards.push((self.status.clone(), self.key.card.id.clone(), bx));
        bxs.controls.retain(|c| c.0 != self.key.card.id);
        for (hb, n) in &d.hits {
            bxs.controls.push((self.key.card.id.clone(), n.clone(), super::board::Box4 { x: bx.x + f32::from(hb.origin.x), y: bx.y + f32::from(hb.origin.y), w: f32::from(hb.size.width), h: f32::from(hb.size.height) }));
        }
        drop(bxs);
        Some((d, window.insert_hitbox(bounds, HitboxBehavior::Normal)))
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), st: &mut Option<(Rc<Draw>, Hitbox)>, window: &mut Window, cx: &mut App) {
        let Some((d, hit)) = st.take() else { return };
        window.set_cursor_style(if self.key.dragged { CursorStyle::ClosedHand } else { CursorStyle::OpenHand }, &hit);
        let (id, title) = (self.key.card.id.clone(), self.key.card.title.clone());
        // A press on a control at rest: its click (down and up on the same
        // control) does what the live control does (a click right after a
        // scroll, before the pointer moved and made this card live).
        let pressed: Rc<RefCell<Option<String>>> = Rc::default();
        let origin = bounds.origin;
        let at = move |p: Point<Pixels>, d: &Draw| d.hits.iter().find(|(bb, _)| bb.contains(&(p - origin))).map(|(_, n)| n.clone());
        let (d1, p1, h1, b1) = (d.clone(), pressed.clone(), hit.clone(), self.boxes.clone());
        window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || e.button != MouseButton::Left || !h1.is_hovered(window) { return; }
            *p1.borrow_mut() = at(e.position, &d1);
            b1.borrow_mut().pressing = true;
            super::press(&id, &title, e.position, cx);
        });
        let (d2, id2, cw, h2, b2) = (d.clone(), self.key.card.id.clone(), bounds.size.width, hit.clone(), self.boxes.clone());
        window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || e.button != MouseButton::Left { return; }
            b2.borrow_mut().pressing = false;
            // Its own release lets go of the card: a press and release in one
            // frame come before the overlay listens for the release (F213).
            if h2.is_hovered(window) { super::pointer_up(cx); }
            let Some(was) = pressed.borrow_mut().take() else { return };
            if !h2.is_hovered(window) || at(e.position, &d2).as_deref() != Some(was.as_str()) { return; }
            let Some((hb, _)) = d2.hits.iter().find(|(_, n)| *n == was) else { return };
            let bx = super::board::Box4 { x: f32::from(origin.x + hb.origin.x), y: f32::from(origin.y + hb.origin.y), w: f32::from(hb.size.width), h: f32::from(hb.size.height) };
            super::click_control(&id2, &was, bx, cw, window, cx);
        });
        crate::paint::paint_draw(&d, bounds.origin, window, cx);
    }
}
