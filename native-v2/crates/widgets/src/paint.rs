//! Paint helpers (moved here from Planning, F83): one element per card and
//! per idea (ARCHITECTURE.md, "What a scroll frame
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
//!
//! The controls' looks at rest are here too, painted from the same tokens
//! and theme colours as the live controls (`button_at`, `check_at`,
//! `icon_tag_at`, `count_badge_at`, and `lite_button` for a div at rest), so
//! an area never copies a control's look (AREAS.md, the shared controls).

use crundi_theme::{IconName, T};
use gpui::*;
use gpui_component::ActiveTheme as _;
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
        Self::shape_in(T::UI_FONT, text, size, weight, color, lh, wrap, clamp, window, cx)
    }

    /// The same in another family (`T::MONO`).
    #[allow(clippy::too_many_arguments)]
    pub fn shape_in(family: &'static str, text: &str, size: Pixels, weight: FontWeight, color: Hsla, lh: f32, wrap: Option<Pixels>, clamp: Option<usize>, window: &mut Window, cx: &App) -> Txt {
        let font = Font { family: family.into(), weight, ..crundi_theme::ui_font() };
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

// ─── The controls at rest ───

fn b(x: Pixels, y: Pixels, w: Pixels, h: Pixels) -> Bounds<Pixels> { Bounds { origin: point(x, y), size: size(w, h) } }

/// A button's look at rest (as `w::button`, `w::icon_button`, `w::icon_toggle`
/// and `w::toggle` tint them: the default bordered one, the ghost, the accent).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look { Default, Ghost, Primary }

/// (background, text, border) of a look, from the theme's button colours.
fn colors(look: Look, cx: &App) -> (Option<Background>, Hsla, Option<Hsla>) {
    let th = cx.theme();
    match look {
        Look::Default => (Some(th.tokens.button.background), th.button_foreground, Some(th.input)),
        Look::Ghost => (None, th.secondary_foreground, None),
        Look::Primary => (Some(th.tokens.button_primary.background), th.button_primary_foreground, None),
    }
}

/// A button's label shaped as the button's (its size, weight, line height 1.25).
fn btn_label(t: &str, color: Hsla, window: &mut Window, cx: &App) -> Rc<Txt> {
    Rc::new(Txt::shape(t, T::BTN_TEXT, T::BTN_WEIGHT, color, 1.25, None, None, window, cx))
}

/// A button at rest painted at (x, y): its icon, its label (none: the
/// square icon button). Returns its width; its height is `BTN_H` (labelled)
/// or `ICON_BTN`.
#[allow(clippy::too_many_arguments)]
pub fn button_at(d: &mut Draw, x: Pixels, y: Pixels, label: Option<&str>, icon: IconName, look: Look, window: &mut Window, cx: &App) -> Pixels {
    let (bg, fg, border) = colors(look, cx);
    let bw = if border.is_some() { px(1.) } else { px(0.) };
    match label {
        None => {
            let s = T::ICON_BTN;
            if let Some(c) = bg { d.bg(b(x, y, s, s), T::BTN_RADIUS, c); }
            let o = r((s - bw * 2. - T::ICON_BTN_ICON) / 2.);
            d.icon(icon, b(x + bw + o, y + bw + o, T::ICON_BTN_ICON, T::ICON_BTN_ICON), fg);
            if let Some(c) = border { d.border(b(x, y, s, s), T::BTN_RADIUS, bw, c); }
            s
        }
        Some(l) => {
            let t = btn_label(l, d.fade(fg), window, cx);
            let gap = r(T::BTN_TEXT * 0.5);
            let w = bw * 2. + T::BTN_PAD_X * 2. + T::BTN_ICON + gap + t.w;
            let h = T::BTN_H;
            if let Some(c) = bg { d.bg(b(x, y, w, h), T::BTN_RADIUS, c); }
            let inner = h - bw * 2.;
            let cx0 = x + bw + T::BTN_PAD_X;
            d.icon(icon, b(cx0, y + bw + r((inner - T::BTN_ICON) / 2.), T::BTN_ICON, T::BTN_ICON), fg);
            let th_ = t.h;
            d.text(t, point(cx0 + T::BTN_ICON + gap, y + bw + r((inner - th_) / 2.)));
            if let Some(c) = border { d.border(b(x, y, w, h), T::BTN_RADIUS, bw, c); }
            w
        }
    }
}

/// A button's width at rest (laid out without painting).
pub fn button_w(label: Option<&str>, look: Look, window: &mut Window, cx: &App) -> Pixels {
    match label {
        None => T::ICON_BTN,
        Some(l) => {
            let bw = if matches!(look, Look::Default) { px(1.) } else { px(0.) };
            bw * 2. + T::BTN_PAD_X * 2. + T::BTN_ICON + r(T::BTN_TEXT * 0.5) + btn_label(l, white(), window, cx).w
        }
    }
}

/// `w::check`'s box at rest at `at` (CHECK_BOX square), ticked when `on`.
pub fn check_at(d: &mut Draw, at: Point<Pixels>, on: bool) -> Bounds<Pixels> {
    let cb = b(at.x, at.y, T::CHECK_BOX, T::CHECK_BOX);
    let (edge, fill) = if on { (T::accent(), T::accent()) } else { (T::text_muted(), T::bg()) };
    d.bg(cb, T::R_CHIP, fill);
    if on {
        let s = T::CHECK_BOX - T::SP_2 * 2.;
        let o = r((T::CHECK_BOX - px(2.) - s) / 2.);
        d.icon(IconName::Check, b(at.x + px(1.) + o, at.y + px(1.) + o, s, s), T::white());
    }
    d.border(cb, T::R_CHIP, px(1.), edge);
    cb
}

/// `w::count_badge` at rest at (x, y): returns its box.
pub fn count_badge_at(d: &mut Draw, x: Pixels, y: Pixels, text: &str, window: &mut Window, cx: &App) -> Bounds<Pixels> {
    let t = Rc::new(Txt::shape(text, T::TEXT_10, T::W_BOLD, d.fade(T::white()), PHI, None, None, window, cx));
    let w = (t.w + T::SP_4 * 2.).max(T::SP_16);
    let bb = b(x, y, w, T::SP_16);
    d.bg(bb, T::R_PILL, T::accent());
    let (tw, th) = (t.w, t.h);
    d.text(t, point(x + r((w - tw) / 2.), y + r((T::SP_16 - th) / 2.)));
    bb
}

/// `w::icon_tag` at rest at (x, y) (its tone's colours): returns its box.
#[allow(clippy::too_many_arguments)]
pub fn icon_tag_at(d: &mut Draw, x: Pixels, y: Pixels, icon: IconName, text: &str, tone: &str, window: &mut Window, cx: &App) -> Bounds<Pixels> {
    let (fg, bg) = crate::icon_tag::tone(tone);
    let t = Rc::new(Txt::shape_in(T::MONO, text, T::TEXT_10, FontWeight::NORMAL, d.fade(fg), PHI, None, None, window, cx));
    let h = t.h.max(T::ICON_SM);
    let w = T::SP_8 * 2. + T::ICON_SM + T::SP_2 + t.w;
    let bb = b(x, y, w, h);
    d.bg(bb, T::R_PILL, bg);
    d.icon(icon, b(x + T::SP_8, y + r((h - T::ICON_SM) / 2.), T::ICON_SM, T::ICON_SM), fg);
    let th = t.h;
    d.text(t, point(x + T::SP_8 + T::ICON_SM + T::SP_2, y + r((h - th) / 2.)));
    bb
}

/// A button at rest as a div (no focus, hover, tooltip or handlers: a row a
/// scroll frame rebuilds, its live controls only under the pointer). The
/// same box, colours, icon and label as the live control.
pub fn lite_button(label: Option<SharedString>, icon: Option<IconName>, look: Look, cx: &App) -> AnyElement {
    use gpui::prelude::FluentBuilder as _;
    let (bg, fg, border) = colors(look, cx);
    let d = div().flex().flex_none().flex_shrink_0().items_center().justify_center().rounded(T::BTN_RADIUS).line_height(relative(1.25))
        .bg(bg.unwrap_or(transparent_black().into())).text_color(fg).when_some(border, |d, b| d.border_1().border_color(b));
    let content = div().flex().flex_row().size_full().min_w_0().overflow_hidden().whitespace_nowrap().items_center().justify_center().text_base().gap_2();
    match label {
        None => d.w(T::ICON_BTN).h(T::ICON_BTN).child(content.children(icon.map(|i| crate::icon_of(i, T::ICON_BTN_ICON)))).into_any_element(),
        Some(l) => d.h(T::BTN_H).px(T::BTN_PAD_X).font_weight(T::BTN_WEIGHT)
            .child(content.children(icon.map(|i| crate::icon_of(i, T::BTN_ICON))).child(div().min_w_0().whitespace_nowrap().text_ellipsis().child(l)))
            .into_any_element(),
    }
}
