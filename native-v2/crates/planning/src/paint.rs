//! One element per card and per idea (ARCHITECTURE.md, "What a scroll frame
//! costs": the chat log's BlockEl way). A card or an idea at rest is laid
//! out once per content and width into a `Draw`: its quads, its texts
//! (shaped as GPUI's text element shapes them) and its icons, at places
//! relative to its top left. A scroll or a pan only paints it: no element
//! tree, no taffy layout, no handlers per frame.
//!
//! The places are the ones the divs they replace get from taffy (the same
//! tokens; lengths rounded to device pixels before layout, a text's height
//! its lines times its snapped line height, a text's width its widest line
//! rounded up), so a card at rest and a live card are the same picture
//! (test/planning/kanban-look.mjs checks it pixel for pixel).

use crundi_theme::{IconName, T};
use gpui::*;
use std::rc::Rc;

/// A text shaped as GPUI's text element shapes it.
pub struct Txt { pub lines: Vec<WrappedLine>, pub lh: Pixels, pub w: Pixels, pub h: Pixels }

/// GPUI's default line height (a text style's φ), or a factor.
pub const PHI: f32 = 1.618_034;

impl Txt {
    /// `text` at `size` (`lh`: the line height as a factor of it), wrapped at
    /// `wrap`; `clamp` lines at most, the last cut with "…" (line_clamp and
    /// text_ellipsis), as gpui's text element does.
    #[allow(clippy::too_many_arguments)]
    pub fn shape(text: &str, size: Pixels, weight: FontWeight, color: Hsla, lh: f32, wrap: Option<Pixels>, clamp: Option<usize>, window: &mut Window, cx: &App) -> Txt {
        let font = Font { family: T::UI_FONT.into(), weight, ..crundi_theme::ui_font() };
        let run = TextRun { len: text.len(), font: font.clone(), color, background_color: None, underline: None, strikethrough: None };
        let line_height = window.pixel_snap(size * lh);
        let runs = vec![run];
        let text = SharedString::from(text.to_string());
        let (text, runs): (SharedString, Vec<TextRun>) = match (clamp, wrap) {
            (Some(max), Some(w)) => {
                let mut lw = cx.text_system().line_wrapper(font, size);
                let (t, r) = lw.truncate_wrapped_line(text.clone(), w, max, "…", &runs, TruncateFrom::End);
                (t, r.into_owned())
            }
            _ => (text, runs),
        };
        let lines: Vec<WrappedLine> = window.text_system().shape_text(text, size, &runs, wrap, clamp).map(|l| l.into_iter().collect()).unwrap_or_default();
        let mut w = px(0.);
        let mut h = px(0.);
        for l in &lines { let s = l.size(line_height); h += s.height; w = w.max(s.width).ceil(); }
        Txt { lines, lh: line_height, w, h }
    }

    fn paint(&self, origin: Point<Pixels>, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let mut o = origin;
        for l in &self.lines {
            let _ = l.paint(o, self.lh, TextAlign::Left, Some(bounds), window, cx);
            o.y += l.size(self.lh).height;
        }
    }
}

/// One thing to paint, placed from the element's top left.
pub enum Op {
    /// A box's background (as a div paints it: before its children).
    Bg { b: Bounds<Pixels>, r: Pixels, color: Background },
    /// A box's border (as a div paints it: after its children).
    Border { b: Bounds<Pixels>, r: Pixels, w: Pixels, color: Hsla, style: BorderStyle },
    /// A text, its box (its lines are painted from the box's top left).
    Text { t: Rc<Txt>, b: Bounds<Pixels> },
    /// An icon (its svg), tinted.
    Icon { name: IconName, b: Bounds<Pixels>, color: Hsla },
}

/// A laid out card or idea: its ops and its size.
#[derive(Default)]
pub struct Draw {
    pub ops: Vec<Op>,
    pub h: Pixels,
    /// An opacity over what is added now (a div's `opacity`: GPUI multiplies
    /// each primitive's alpha by it; texts take it through `fade`).
    pub opacity: Option<f32>,
    /// Its padding box's height (what a div's measure reported), if it reports one.
    pub inner: Pixels,
    /// Where its controls are (a click on one at rest does what the live
    /// control does): their boxes and names.
    pub hits: Vec<(Bounds<Pixels>, String)>,
}

impl Draw {
    /// A colour at the opacity in force (for a text's colour, before shaping).
    pub fn fade(&self, c: Hsla) -> Hsla { match self.opacity { Some(o) => Hsla { a: c.a * o, ..c }, None => c } }
    pub fn bg(&mut self, b: Bounds<Pixels>, r: Pixels, color: impl Into<Background>) {
        let c: Background = color.into();
        let c = match self.opacity { Some(o) => c.opacity(o), None => c };
        self.ops.push(Op::Bg { b, r, color: c });
    }
    pub fn border(&mut self, b: Bounds<Pixels>, r: Pixels, w: Pixels, color: Hsla) { let color = self.fade(color); self.ops.push(Op::Border { b, r, w, color, style: BorderStyle::Solid }); }
    pub fn dashed(&mut self, b: Bounds<Pixels>, r: Pixels, w: Pixels, color: Hsla) { let color = self.fade(color); self.ops.push(Op::Border { b, r, w, color, style: BorderStyle::Dashed }); }
    pub fn text(&mut self, t: Rc<Txt>, at: Point<Pixels>) { let b = Bounds { origin: at, size: size(t.w, t.h) }; self.ops.push(Op::Text { t, b }); }
    pub fn icon(&mut self, name: IconName, b: Bounds<Pixels>, color: Hsla) { let color = self.fade(color); self.ops.push(Op::Icon { name, b, color }); }
}

/// Paint `draw` at `origin` (snapped to device pixels, as the divs' edges are).
pub fn paint_draw(draw: &Draw, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let o = window.pixel_snap_point(origin);
    let at = |b: &Bounds<Pixels>| Bounds { origin: b.origin + o, size: b.size };
    for op in &draw.ops {
        match op {
            Op::Bg { b, r, color } => {
                let b = at(b);
                let r = Corners::all(*r).clamp_radii_for_quad_size(b.size);
                window.paint_quad(quad(b, r, *color, Edges::default(), transparent_black(), BorderStyle::Solid));
            }
            Op::Border { b, r, w, color, style } => {
                let b = at(b);
                let r = Corners::all(*r).clamp_radii_for_quad_size(b.size);
                window.paint_quad(quad(b, r, Hsla { a: 0., ..*color }, Edges::all(*w), *color, *style));
            }
            Op::Text { t, b } => { let b = at(b); t.paint(b.origin, b, window, cx); }
            Op::Icon { name, b, color } => { let _ = window.paint_svg(at(b), name.path(), None, TransformationMatrix::unit(), *color, cx); }
        }
    }
}

/// A length as taffy takes an authored one: rounded to device pixels (here
/// 1 px; GPUI rounds half toward zero).
pub fn r(v: Pixels) -> Pixels { let x = f32::from(v); px(if (x - x.trunc() - 0.5).abs() < 1e-4 { x.trunc() } else { x.round() }) }
