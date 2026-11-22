//! Crundi's mark, as the web draws it (src/brand.js headSvg and wordSvg):
//! the head (a rounded head in the brand gradient, two eyes cut out, the
//! amber antenna) and the name drawn in strokes (six letters from one circle
//! and one stem, the dot of the i amber). Full colour, so images (gpui's
//! svg() is one tint), drawn at OVERSAMPLE pixels per point so they stay
//! sharp on HiDPI screens. The owner's 18 Nov report: the GPUI client showed
//! a stock robot icon and plain text in the logo's place.
//!
//! Keep the paths in step with src/brand.js.

use crundi_theme::T;
use gpui::*;
use std::{cell::RefCell, sync::Arc};

const OVERSAMPLE: f32 = 3.0;
/// The brand's colours (src/brand.js BRAND).
const AMBER: &str = "#fbbf24";
const G0: &str = "#818cf8";
const G1: &str = "#4f46e5";
/// The name's strokes take the text colour (the web's currentColor).
const TEXT: &str = "#e8e8f0";

fn head_svg(px: f32) -> String {
    format!(concat!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 96 96\" width=\"{s}\" height=\"{s}\">",
        "<defs><linearGradient id=\"g\" gradientUnits=\"userSpaceOnUse\" x1=\"14\" y1=\"14\" x2=\"82\" y2=\"82\">",
        "<stop offset=\"0\" stop-color=\"{g0}\"/><stop offset=\"1\" stop-color=\"{g1}\"/></linearGradient>",
        "<mask id=\"e\"><rect width=\"96\" height=\"96\" fill=\"#fff\"/>",
        "<rect x=\"32\" y=\"43\" width=\"10.5\" height=\"18\" rx=\"5.2\" fill=\"#000\"/>",
        "<rect x=\"53.5\" y=\"43\" width=\"10.5\" height=\"18\" rx=\"5.2\" fill=\"#000\"/></mask></defs>",
        "<path d=\"M48 27V17\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke=\"{a}\" stroke-width=\"5.5\"/>",
        "<circle cx=\"48\" cy=\"13\" r=\"6.5\" fill=\"{a}\"/>",
        "<rect x=\"11\" y=\"26\" width=\"74\" height=\"57\" rx=\"23\" fill=\"url(#g)\" mask=\"url(#e)\"/></svg>"),
        s = px.round(), g0 = G0, g1 = G1, a = AMBER)
}

/// The name's box: viewBox -2 -2 279 70.
const WORD_RATIO: f32 = 279.0 / 70.0;

fn word_svg(h: f32) -> String {
    format!(concat!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"-2 -2 279 70\" width=\"{w}\" height=\"{h}\">",
        "<g fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke=\"{t}\" stroke-width=\"10\">",
        "<path d=\"M35.23 54.73A18 18 0 1 1 35.23 29.27\"/>",
        "<path d=\"M54 60V24M54 42A18 18 0 0 1 78.16 25.09\"/>",
        "<path d=\"M95 24V42A18 18 0 0 0 131 42M131 24V60\"/>",
        "<path d=\"M153 60V24M153 42A18 18 0 0 1 189 42V60\"/>",
        "<path d=\"M247 42A18 18 0 1 1 211 42A18 18 0 1 1 247 42M247 6V60\"/>",
        "<path d=\"M269 24V60\"/></g>",
        "<circle cx=\"269\" cy=\"8.5\" r=\"6\" fill=\"{a}\"/></svg>"),
        w = (h * WORD_RATIO).round(), h = h.round(), t = TEXT, a = AMBER)
}

thread_local! {
    /// The pictures, made once per size (head size in points).
    static PICS: RefCell<Vec<(u32, Arc<Image>, Arc<Image>)>> = const { RefCell::new(Vec::new()) };
}

fn pics(head: Pixels) -> (Arc<Image>, Arc<Image>) {
    let key = (f32::from(head) * 10.0) as u32;
    PICS.with(|p| {
        let mut p = p.borrow_mut();
        if let Some((_, a, b)) = p.iter().find(|x| x.0 == key) { return (a.clone(), b.clone()); }
        let h = f32::from(head) * OVERSAMPLE;
        let a = Arc::new(Image::from_bytes(ImageFormat::Svg, head_svg(h).into_bytes()));
        let b = Arc::new(Image::from_bytes(ImageFormat::Svg, word_svg(h * T::BRAND_WORD_OF_HEAD).into_bytes()));
        p.push((key, a.clone(), b.clone()));
        (a, b)
    })
}

/// The head alone, `size` square.
pub fn brand_head(size: Pixels) -> Img {
    img(pics(size).0).size(size).flex_none()
}

/// The head and the name side by side, as the web's top bar `.logo`: the
/// head `size` square, the name in proportion (15 to the head's 24).
pub fn brand_mark(size: Pixels) -> Div { brand_mark_with(size, true) }

/// The same, its head's place left empty while `head` is false (the web's
/// visibility: hidden on .cb-headmark while Mini, the head, is out walking).
pub fn brand_mark_with(size: Pixels, head: bool) -> Div {
    let wh = size * T::BRAND_WORD_OF_HEAD;
    let (a, b) = pics(size);
    div().flex().flex_none().items_center().gap(size * T::BRAND_GAP_OF_HEAD)
        .child(if head { img(a).size(size).flex_none().into_any_element() } else { div().size(size).flex_none().into_any_element() })
        .child(img(b).h(wh).w(wh * WORD_RATIO).flex_none())
}

// ─── Mini, the whole robot (src/brand.js miniSvg), still ───
// The sign-in page's logo and the start-up screen's (additive). The web
// moves it with CSS (.cb-intro, .cb-load); here it is the still logo, as
// the web draws it with no stylesheet.

fn mini_svg(px: f32) -> String {
    format!(concat!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 96 96\" width=\"{s}\" height=\"{s}\">",
        "<defs><linearGradient id=\"g\" gradientUnits=\"userSpaceOnUse\" x1=\"14\" y1=\"14\" x2=\"82\" y2=\"82\">",
        "<stop offset=\"0\" stop-color=\"{g0}\"/><stop offset=\"1\" stop-color=\"{g1}\"/></linearGradient>",
        "<mask id=\"e\"><rect width=\"96\" height=\"96\" fill=\"#fff\"/>",
        "<rect x=\"36.5\" y=\"27\" width=\"7.5\" height=\"13\" rx=\"3.7\" fill=\"#000\"/>",
        "<rect x=\"52\" y=\"27\" width=\"7.5\" height=\"13\" rx=\"3.7\" fill=\"#000\"/></mask>",
        "<mask id=\"l\"><rect width=\"96\" height=\"96\" fill=\"#fff\"/><circle cx=\"48\" cy=\"73\" r=\"5.5\" fill=\"#000\"/></mask></defs>",
        "<path d=\"M48 16V10\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke=\"{a}\" stroke-width=\"4\"/>",
        "<circle cx=\"48\" cy=\"7.5\" r=\"4.5\" fill=\"{a}\"/>",
        "<rect x=\"23\" y=\"15\" width=\"50\" height=\"38\" rx=\"15\" fill=\"url(#g)\" mask=\"url(#e)\"/>",
        "<rect x=\"19\" y=\"60\" width=\"7\" height=\"18\" rx=\"3.5\" fill=\"{a}\"/>",
        "<rect x=\"70\" y=\"60\" width=\"7\" height=\"18\" rx=\"3.5\" fill=\"{a}\"/>",
        "<rect x=\"31\" y=\"58\" width=\"34\" height=\"30\" rx=\"10\" fill=\"url(#g)\" mask=\"url(#l)\"/></svg>"),
        s = px.round(), g0 = G0, g1 = G1, a = AMBER)
}

thread_local! {
    /// Mini and the name alone, made once per size (key: points × 10).
    static BIG: RefCell<Vec<(u32, bool, Arc<Image>)>> = const { RefCell::new(Vec::new()) };
}

fn big(size: Pixels, robot: bool) -> Arc<Image> {
    let key = (f32::from(size) * 10.0) as u32;
    BIG.with(|p| {
        let mut p = p.borrow_mut();
        if let Some((_, _, i)) = p.iter().find(|x| x.0 == key && x.1 == robot) { return i.clone(); }
        let s = f32::from(size) * OVERSAMPLE;
        let i = Arc::new(Image::from_bytes(ImageFormat::Svg, if robot { mini_svg(s) } else { word_svg(s) }.into_bytes()));
        p.push((key, robot, i.clone()));
        i
    })
}

fn robot(size: Pixels) -> Img { img(big(size, true)).size(size).flex_none() }
fn word(h: Pixels) -> Img { img(big(h, false)).h(h).w(h * WORD_RATIO).flex_none() }

/// Start decoding the sign-in page's and the start-up screen's pictures (a
/// cached view showing them first would not hear when they are ready).
pub fn warm_marks(window: &mut Window, cx: &mut App) {
    for (s, r) in [(T::SP_48 + T::SP_48 + T::SP_16, true), (T::SP_40 + T::SP_4, false), (T::SP_48 + T::SP_48 + T::SP_8, true), (T::SP_24 + T::SP_6, false)] {
        let _ = big(s, r).get_render_image(window, cx);
    }
}

/// The sign-in page's head (#login-screen): Mini 112 (its -18 margin under
/// the page's 32 gap), the name 44 high.
pub fn sign_in_mark() -> Div {
    div().flex().flex_col().items_center().gap(T::SP_12 + T::SP_2)
        .child(robot(T::SP_48 + T::SP_48 + T::SP_16))
        .child(word(T::SP_40 + T::SP_4))
}

/// The start-up screen's mark (#boot): Mini 104, the name 30 high, 22 apart.
pub fn boot_mark() -> Div {
    div().flex().flex_col().items_center().gap(T::SP_20 + T::SP_2)
        .child(robot(T::SP_48 + T::SP_48 + T::SP_8))
        .child(word(T::SP_24 + T::SP_6))
}
