//! The Mind map's ideas at rest as drawings (F38): each idea and each "add
//! idea" box is laid out once per content and zoom into a `paint::Draw` (the
//! card, its words cut at two lines, its chips, its fold button), and one
//! element (`MapEl`) paints all of those in view at their places. A pan
//! only paints them. The places are the divs' (draw.rs `card` and `ghost`):
//! the same tokens, laid out as taffy lays out those divs.
//!
//! An idea the pointer is on, one with a glow (a drag's target, a flash)
//! or one whose chip would be cut is drawn as its div instead (draw.rs).

use super::Row;
use crate::paint::{r, Draw, Marks, Txt, PHI};
use crundi_theme::{IconName, T};
use gpui::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

fn b(x: Pixels, y: Pixels, w: Pixels, h: Pixels) -> Bounds<Pixels> { Bounds { origin: point(x, y), size: size(w, h) } }

/// What an idea is drawn from: its row without its place, and the zoom.
#[derive(Clone, PartialEq)]
pub struct Key { pub row: Row, pub z: u32 }

/// A drawing and the words on it where they are (its marks, F202).
pub type Laid = (Rc<Draw>, Rc<Marks>);

pub type Cache = Rc<RefCell<HashMap<String, (Key, Option<Laid>)>>>;

/// The idea's row as drawn (its place left out: a move does not lay it out again).
pub fn key(r_: &Row, z: f32) -> Key { Key { row: Row { x: 0., y: 0., h: 0., ..r_.clone() }, z: z.to_bits() } }

/// A meta chip (.mm-chip) at (x, y): its width and height; None when its
/// words would be cut (that idea is drawn as its div).
#[allow(clippy::too_many_arguments)]
fn chip(d: &mut Draw, mk: &mut Marks, x: Pixels, y: Pixels, kind: &str, text: &str, todo: i32, z: f32, window: &mut Window, cx: &App) -> Option<(Pixels, Pixels)> {
    let fg = match kind { "link" => T::green(), "scope" => T::accent_hover(), "note" => T::text_secondary(), _ => T::text_muted() };
    let bgc = match kind { "link" => T::green_dim(), "scope" => T::accent_dim(), _ => T::bg_tertiary() };
    let icon = match kind { "link" | "missing" => Some(IconName::Link), "scope" => Some(IconName::Folder), "note" => Some(IconName::StickyNote), _ => None };
    let bw = if kind == "inherited" { px(1.) } else { px(0.) };
    let (padx, pady, gap, ic) = (r(T::SP_6 * z), r(px(1.) * z), r(T::SP_2 * z), r(T::ICON_SM * z));
    let t = Rc::new(Txt::shape(text, T::TEXT_11 * z, FontWeight::NORMAL, d.fade(fg), PHI, None, None, window, cx));
    let icons: Vec<IconName> = icon.into_iter().chain((todo > 0).then_some(if todo == 2 { IconName::SquareCheck } else { IconName::Square })).collect();
    let max_w = r(T::MINDMAP_NODE * z - T::SP_24 * z);
    let w = bw * 2. + padx * 2. + (ic + gap) * icons.len() as f32 + t.w;
    if w > max_w { return None; }
    let inner = ic.max(t.h);
    let h = bw * 2. + pady * 2. + inner;
    d.bg(b(x, y, w, h), r(T::R_FIELD * z), bgc);
    let mut cx_ = x + bw + padx;
    let cy = y + bw + pady;
    for i in icons { d.icon(i, b(cx_, cy + r((inner - ic) / 2.), ic, ic), fg); cx_ += ic + gap; }
    let th = t.h;
    let tw = t.w;
    d.text(t, point(cx_, cy + r((inner - th) / 2.)));
    mk.push((b(cx_, cy + r((inner - th) / 2.), tw, th), SharedString::from(text.to_string()), false));
    if bw > px(0.) { d.dashed(b(x, y, w, h), r(T::R_FIELD * z), bw, T::text_muted()); }
    Some((w, h))
}

/// An idea at rest (draw.rs `card`, Mode::Normal, not hovered): None when
/// it must be its div.
pub fn lay_idea(r_: &Row, z: f32, window: &mut Window, cx: &App) -> Option<Laid> {
    if r_.flash || r_.target { return None; }
    let mut d = Draw::default();
    let mut mk = Marks::new();
    if r_.lifted || r_.dim { d.opacity = Some(T::MINDMAP_O_FAINT); }
    let strong = r_.hit;
    let bw = if strong { px(2.) } else { px(1.) };
    let border = if r_.pending { T::text_muted() } else if strong { T::accent() } else if r_.linked { T::green() } else if r_.root || r_.scoped { T::accent() } else { T::border() };
    let w = r(T::MINDMAP_NODE * z);
    let (padx, pady, gap) = (r(T::SP_10 * z + px(1.)), r(T::SP_8 * z), r(T::SP_6 * z));
    let tw = (w - bw * 2. - padx * 2.).max(px(0.));
    let words = Rc::new(Txt::shape(&r_.text, T::TEXT_13 * z, T::W_SEMIBOLD, d.fade(T::text()), T::LH_UI, Some(tw), Some(2), window, cx));
    let radius = r(T::R_BUBBLE * z);
    let bg_ix = d.ops.len();
    d.bg(b(px(0.), px(0.), w, px(0.)), radius, if r_.root { T::accent_dim() } else { T::bg_card() });
    let (x0, mut y) = (bw + padx, bw + pady);
    let wh = words.h;
    mk.push((b(x0, y, tw, wh), SharedString::from(r_.text.clone()), false));
    d.text(words, point(x0, y));
    y += wh;
    if !r_.chip.is_empty() || r_.notes > 0 {
        y += gap;
        let mut x = x0;
        let mut row_h = px(0.);
        if !r_.chip.is_empty() {
            let (cw, ch) = chip(&mut d, &mut mk, x, y, &r_.chip, &r_.chip_text, r_.todo, z, window, cx)?;
            x += cw + r(T::SP_4 * z);
            row_h = row_h.max(ch);
        }
        if r_.notes > 0 {
            let t = format!("{} {}", r_.notes, if r_.notes == 1 { "note" } else { "notes" });
            let (_, ch) = chip(&mut d, &mut mk, x, y, "note", &t, 0, z, window, cx)?;
            row_h = row_h.max(ch);
        }
        y += row_h;
    }
    let h = y + pady + bw;
    if let Some(crate::paint::Op::Bg { b: bb, .. }) = d.ops.get_mut(bg_ix) { bb.size.height = h; }
    // Its fold button (.mm-collapse), on its right edge, centred on it.
    if r_.kids {
        let fs = r(T::SP_16 * z + T::SP_2 * z);
        let fx = bw + r(T::MINDMAP_NODE * z - T::SP_10 * z);
        let fy = bw + r((h - bw * 2. - fs) / 2.);
        d.bg(b(fx, fy, fs, fs), fs / 2., T::bg_tertiary());
        let ic = r(T::ICON_SM * z);
        let o = r((fs - px(2.) - ic) / 2.);
        d.icon(if r_.collapsed { IconName::Plus } else { IconName::Minus }, b(fx + px(1.) + o, fy + px(1.) + o, ic, ic), T::text_secondary());
        d.border(b(fx, fy, fs, fs), fs / 2., px(1.), T::border());
    }
    if r_.pending { d.dashed(b(px(0.), px(0.), w, h), radius, bw, border); } else { d.border(b(px(0.), px(0.), w, h), radius, bw, border); }
    d.h = h;
    d.inner = h - bw * 2.;
    Some((Rc::new(d), Rc::new(mk)))
}

/// An "add idea" box at rest (draw.rs `ghost`, not hovered).
pub fn lay_ghost(z: f32, window: &mut Window, cx: &App) -> Laid {
    let mut d = Draw { opacity: Some(T::MINDMAP_O_DIM), ..Default::default() };
    let w = r(T::MINDMAP_NODE * z);
    let (pady, gap, ic) = (r(T::SP_8 * z), r(T::SP_6 * z), r(T::ICON_MD * z));
    let t = Rc::new(Txt::shape("add idea", T::TEXT_12 * z, T::W_MEDIUM, d.fade(T::text_muted()), PHI, None, None, window, cx));
    let inner = ic.max(t.h);
    let h = r(T::SP_40 * z).max(px(2.) + pady * 2. + inner);
    let content_h = h - px(2.) - pady * 2.;
    let total = ic + gap + t.w;
    let x = px(1.) + r((w - px(2.) - total) / 2.);
    let cy = px(1.) + pady;
    d.icon(IconName::Plus, b(x, cy + r((content_h - ic) / 2.), ic, ic), T::text_muted());
    let (th, tw) = (t.h, t.w);
    let mk: Marks = vec![(b(x + ic + gap, cy + r((content_h - th) / 2.), tw, th), "add idea".into(), false)];
    d.text(t, point(x + ic + gap, cy + r((content_h - th) / 2.)));
    d.dashed(b(px(0.), px(0.), w, h), r(T::R_BUBBLE * z), px(1.), T::border());
    d.h = h;
    d.inner = h - px(2.);
    (Rc::new(d), Rc::new(mk))
}

/// One thing the map paints: an idea or a ghost at its place (unscaled
/// left, scaled top), and its id (an idea reports its height).
pub struct Item { pub id: String, pub left: Pixels, pub top: Pixels, pub draw: Rc<Draw>, pub marks: Rc<Marks> }

/// Every idea and ghost at rest in view, one element over the map's content.
pub struct MapEl { pub items: Vec<Item>, pub z: f32 }

impl IntoElement for MapEl {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for MapEl {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut st = Style::default();
        st.position = Position::Absolute;
        st.inset.top = px(0.).into();
        st.inset.left = px(0.).into();
        st.size.width = relative(1.).into();
        st.size.height = relative(1.).into();
        (window.request_layout(st, None, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        // Each idea's height as drawn, unscaled (the layout's measure, as the
        // div's padding box reported it): a change lays the map out again.
        for it in &self.items { super::measured(&it.id, f32::from(it.draw.inner) / self.z.max(0.01), cx); }
        // The words on them, for Cru's highlight (F202).
        for it in &self.items { crate::paint::prepaint_marks(&it.id, &it.marks, bounds.origin + point(it.left, it.top), window, cx); }
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        for it in &self.items { crate::paint::paint_draw(&it.draw, bounds.origin + point(it.left, it.top), window, cx); }
    }
}
