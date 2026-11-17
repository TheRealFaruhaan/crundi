//! 2.16: the rest of the log (specs/log-rest.md): Claude's text in blocks
//! (markdown, links), code blocks with Copy, the user's bubble, results,
//! errors and notices, at the pane's text size. As the Slint client's
//! EntryRow (ui/chat.slint) and open_link / copy_text (src/app.rs).
use crate::LogView;
use crundi_core::{chat::Shown, head};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct State {
    /// The code block whose Copy was pressed (its tick, until another).
    pub copied: String,
    /// The reader scrolled away from the bottom (the web's !stick).
    pub away: bool,
    /// Something arrived below while reading up (the jump button's dot).
    pub jump_new: bool,
}

/// What the rest of the log did, for the driver (no chat in particular).
#[derive(Default)]
struct Done { last_opened: String, toast: String, watch: String }
impl Global for Done {}

/// The pane's text size as a scale of the base (the web's --cc-fs).
pub fn fs(v: &LogView, cx: &App) -> f32 {
    (head::FONT_BASE + v.model.read(cx).zoom) as f32 / head::FONT_BASE as f32
}

/// A link: http(s) to the system browser (under the test driver only
/// recorded); a path (path:…) opens in the Files panel, phase 3: nothing yet.
pub fn open_link(url: &str, cx: &mut App) {
    if !(url.starts_with("http://") || url.starts_with("https://")) { return; }
    cx.default_global::<Done>().last_opened = url.to_string();
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    cx.open_url(url);
}

/// Text to the clipboard: the toast "Copied" (the web's plain toast).
pub fn copy(text: &str, window: &mut Window, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    toast("Copied", w::Toast::Info, window, cx);
}

fn toast(text: &str, kind: w::Toast, window: &mut Window, cx: &mut App) {
    cx.default_global::<Done>().toast = text.to_string();
    w::toast(window, cx, text.to_string(), kind);
}

/// The last user bubble drawn: its box's and its text's heights (F122).
#[derive(Default)]
pub struct Bubble { box_h: f32, drawn_h: f32 }
impl Global for Bubble {}

/// The driver's props for no chat in particular: lastOpened, clipboard,
/// and toast (the last one the log showed: the window has no toast prop yet).
pub fn global_props(cx: &mut App) -> Value {
    let d = cx.default_global::<Done>();
    let (lo, t) = (d.last_opened.clone(), d.toast.clone());
    let b = cx.default_global::<Bubble>();
    // (Measured inside the border: the text plus the padding, as the Slint client's.)
    let bubble = (b.box_h > 0.).then(|| json!({ "box": b.box_h, "drawn": b.drawn_h, "copy": b.drawn_h }));
    json!({ "lastOpened": lo, "toast": t, "clipboard": cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default(), "bubble": bubble })
}

// ─── Following the bottom, and the jump button (log-rest.md §7, F87) ───

/// Within this of the bottom, the log follows it (the web's 60).
pub fn near() -> f32 { f32::from(T::SP_48 + T::SP_12) }

/// How far the log is from its bottom, and past it (pixels).
pub fn dist(v: &LogView) -> (f32, f32) {
    let max = f32::from(v.scroll.max_offset().y);
    let off = -f32::from(v.scroll.offset().y);
    ((max - off).max(0.), (off - max).max(0.))
}

/// The jump button shows beyond max(240, a third of the log's height).
pub fn jump_shown(dist: f32, view_h: f32) -> bool { dist > (view_h / 3.).max(240.) }

/// Content changed (a new entry when `entry`): following, back to the end;
/// reading up, an arrival lights the jump button's dot.
pub fn arrived(v: &mut LogView, entry: bool) {
    // (As the Slint client: off, nothing moves the log, even within near().)
    if !v.st.prose.away { v.stick = v.stick.max(2); v.st.prose.jump_new = false; }
    else if entry { v.st.prose.jump_new = true; }
}

/// The reader's wheel (as the Slint client's, F87/F91): a step up stops
/// following at once; down, back within near() of the end follows again.
pub fn wheel(v: &mut LogView, e: &ScrollEvent, window: &Window) {
    let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
    let max = f32::from(v.scroll.max_offset().y);
    let to = (dist(v).0 + dy).clamp(0., max.max(0.));
    if let Some(away) = after_wheel(v.st.prose.away, dy, to) { v.st.prose.away = away; }
    if v.st.prose.away { v.stick = 0; } else { v.st.prose.jump_new = false; }
}

/// Following after a wheel step of `dy` (up > 0) that leaves the log `to`
/// from its end: Some(away) when it changes.
pub fn after_wheel(away: bool, dy: f32, to: f32) -> Option<bool> {
    if dy > 0. { Some(true) } else if dy < 0. && away && to < near() { Some(false) } else { None }
}

/// Each render while following: a log that grew or shrank (a fold, a
/// resize) goes back to its end.
pub fn keep(v: &mut LogView) {
    if !v.st.prose.away && v.stick == 0 && dist(v).0 > 1. { v.stick = 1; }
}

/// The jump button (.cc-jump), over the log's bottom centre: "Go to the
/// newest message"; its dot when something arrived below.
pub fn jump(v: &mut LogView, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let b = v.scroll.bounds();
    if !jump_shown(dist(v).0, f32::from(b.size.height)) { return None; }
    let new = v.st.prose.jump_new;
    let btn = w::icon_button("log-jump", IconName::ArrowDown, "Go to the newest message")
        .on_click(cx.listener(|this, _, _, cx| { this.st.prose.away = false; this.st.prose.jump_new = false; this.stick = 3; cx.notify(); }));
    Some(div().absolute().bottom(T::SP_10).left_0().right_0().flex().justify_center()
        .child(div().relative().size(T::SP_40).rounded_full().flex().items_center().justify_center()
            .bg(T::bg_secondary()).border_1().border_color(if new { T::accent() } else { T::border() })
            .shadow_lg().child(btn)
            .when(new, |d| d.child(div().absolute().top(-T::SP_2).right(-T::SP_2).size(T::SP_10).rounded_full().bg(T::accent()).border_2().border_color(T::bg()))))
        .into_any_element())
}

/// The watched log's fields under props.log (log-watch <pane key>): dist,
/// over, stick, jump, jumpNew, jumpX / jumpY (the button's centre).
pub fn watch_props(cx: &mut App, o: &mut Map<String, Value>) {
    let key = cx.default_global::<Done>().watch.clone();
    o.insert("key".into(), json!(key));
    let Some(v) = key.strip_prefix("live:").and_then(|id| crate::view_of(cx, id)) else { return };
    let v = v.read(cx);
    let (d, over) = dist(v);
    let b = v.scroll.bounds();
    let h = f32::from(b.size.height);
    o.insert("dist".into(), json!(d));
    o.insert("over".into(), json!(over));
    o.insert("stick".into(), json!(!v.st.prose.away));
    o.insert("jump".into(), json!(jump_shown(d, h)));
    o.insert("jumpNew".into(), json!(v.st.prose.jump_new));
    o.insert("jumpX".into(), json!(f32::from(b.origin.x) + f32::from(b.size.width) / 2.));
    o.insert("jumpY".into(), json!(f32::from(b.origin.y) + h - f32::from(T::SP_10 + T::SP_40 / 2.)));
}

/// Registers the props above (called from the log's init).
pub fn init(cx: &mut App) {
    crundi_shell::register(cx, crundi_shell::Area::new("log-rest").props(|_, cx| global_props(cx)));
}

/// This feature's fields in the chat's props (its rows are in "tools", tools.rs).
pub fn props(_v: &LogView, _cx: &App, _o: &mut Map<String, Value>) {}

/// This feature's driver actions: tool-copy <text> (a code block's Copy),
/// open-link <url> (a link in a reply).
pub fn act(name: &str, args: &[String], _v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let a = args.first().cloned().unwrap_or_default();
    match name {
        "tool-copy" => copy(&a, window, cx),
        "open-link" => open_link(&a, cx),
        "log-watch" => cx.default_global::<Done>().watch = a,
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// A red box (an error, a failed turn's end).
fn error_box(text: &str, fs: f32) -> Div {
    div().w_full().px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(T::red()).bg(T::red_dim())
        .text_color(T::red()).text_size(T::TEXT_12 * fs).child(text.to_string())
}

/// An entry that is not Claude's text, a tool or thinking: the user's
/// bubble, a result, an error, a notice (None: nothing to draw).
pub fn entry(v: &mut LogView, _i: usize, e: &Shown, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let fs = fs(v, cx);
    Some(match e.kind {
        // The bubble: accent tint and edge, one sharp corner (bottom right).
        "user" => div().flex().justify_end()
            .child(div().relative().max_w(relative(0.82)).px(T::SP_12).py(T::SP_8)
                .rounded_tl(T::R_BUBBLE).rounded_tr(T::R_BUBBLE).rounded_bl(T::R_BUBBLE).rounded_br(T::R_CHIP)
                .bg(T::accent_dim()).border_1().border_color(T::accent_edge())
                .text_size(T::TEXT_13 * fs).line_height(relative(T::LH_CHAT)).text_color(T::text())
                // The driver's "bubble" (F122): the box's height and its text's (the
                // text is drawn once, so the selectable copy is the drawn text).
                .child(canvas(|b, _, cx| { cx.default_global::<Bubble>().box_h = f32::from(b.size.height); }, |_, _, _, _| {}).absolute().size_full())
                .child(div().relative().child(e.text.clone())
                    .child(canvas(|b, _, cx| { cx.default_global::<Bubble>().drawn_h = f32::from(b.size.height); }, |_, _, _, _| {}).absolute().size_full())))
            .into_any_element(),
        // A failed turn: the box with its text (or "Error").
        "result" if e.error => error_box(if e.text.is_empty() { "Error" } else { &e.text }, fs).into_any_element(),
        // The end of a turn: a dashed line above, its time and cost, centred.
        "result" => div().flex().flex_col().w_full()
            // (A solid hairline: GPUI draws no dashed edge on a 1px box.)
            .child(div().w_full().h(px(1.)).bg(T::border_subtle()))
            .child(div().w_full().py(T::SP_4).text_center().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(e.detail.clone()))
            .into_any_element(),
        "error" => error_box(&e.text, fs).into_any_element(),
        "notice" => div().w_full().text_center().text_size(T::TEXT_12 * fs).italic().text_color(T::text_muted()).child(e.text.clone()).into_any_element(),
        _ => return None,
    })
}

/// A block's markdown, its links to open_link.
fn md(id: &str, text: &str) -> AnyElement {
    w::markdown(SharedString::from(format!("md-{id}")), text.to_string(), |u, _, cx| open_link(u, cx)).into_any_element()
}

/// The gap below a block (Slint's EntryRow gaps): paragraphs and quotes 8,
/// list items 2, a list's last 8, a heading 6, `##` and rules none; the
/// message's last block the entry gap.
pub fn gap(kind: &str, last: bool) -> Pixels {
    if last { return px(0.); }
    match kind { "li" => T::SP_2, "lie" => T::SP_8, "h" => T::SP_6, "h4" | "hr" => px(0.), _ => T::SP_8 }
}

/// One block of Claude's text in the log: (id, markdown, kind, list
/// marker, the message's last), at the pane's text size.
pub fn block_in(v: &mut LogView, id: &str, text: &str, kind: &str, marker: &str, last: bool, cx: &mut Context<LogView>) -> AnyElement {
    // (The log's rows are SP_10 apart: a block's own gap pulls the next one up.)
    let mb = if last { px(0.) } else { gap(kind, false) - T::SP_10 };
    draw(id, text, kind, marker, fs(v, cx), mb)
}

/// One block of text in a card of its own (a side answer, the plan): its
/// gap below it, at the base size.
pub fn block(id: &str, text: &str, kind: &str, marker: &str, last: bool, _cx: &mut Context<LogView>) -> AnyElement {
    draw(id, text, kind, marker, 1.0, gap(kind, last))
}

fn draw(id: &str, text: &str, kind: &str, marker: &str, fs: f32, mb: Pixels) -> AnyElement {
    let base = div().w_full().mb(mb).line_height(relative(T::LH_CHAT));
    match kind {
        // The browser's 2px rule: a dark line over a light one.
        "hr" => base.child(div().h(px(1.)).bg(T::border())).child(w::rule()).into_any_element(),
        "li" | "lie" => base.flex().text_size(T::TEXT_13 * fs).text_color(T::text())
            .child(div().flex_none().w(T::SP_20).text_center().child(marker.to_string()))
            .child(div().flex_1().min_w_0().child(md(id, text))).into_any_element(),
        "quote" => base.flex().text_size(T::TEXT_13 * fs).text_color(T::text_secondary())
            .child(div().flex_none().w(T::SP_2).bg(T::border()))
            .child(div().flex_1().min_w_0().pl(T::SP_8).child(md(id, text))).into_any_element(),
        "h" => base.pt(T::SP_4).text_size(T::TEXT_14 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(md(id, text)).into_any_element(),
        _ => base.text_size(T::TEXT_13 * fs).text_color(T::text()).child(md(id, text)).into_any_element(),
    }
}

/// A fenced code block: its lines kept whole, scrolling sideways; the copy
/// button in its top-right corner while the pointer is over it ("Copy
/// code"; pressed: the tick, the toast "Copied").
pub fn code(v: &mut LogView, id: &str, code: &str, last: bool, cx: &mut Context<LogView>) -> AnyElement {
    let fs = fs(v, cx);
    let copied = v.st.prose.copied == id;
    let group = SharedString::from(format!("cb-{id}"));
    let (t, k) = (code.to_string(), id.to_string());
    let btn = w::icon_button(SharedString::from(format!("cc-{id}")), if copied { IconName::Check } else { IconName::Copy }, "Copy code")
        .on_click(cx.listener(move |this, _, window, cx| { copy(&t, window, cx); this.st.prose.copied = k.clone(); cx.notify(); }));
    div().relative().w_full().group(group.clone()).when(!last, |d| d.mb(T::SP_8 - T::SP_10))
        .rounded(T::R_CONTROL).border_1().border_color(T::border_subtle()).bg(T::bg_secondary())
        .child(div().id(SharedString::from(format!("cs-{id}"))).w_full().overflow_x_scroll().px(T::SP_12).py(T::SP_10)
            .child(div().whitespace_nowrap().font_family(T::MONO).text_size(T::TEXT_12 * fs).line_height(relative(T::LH_CHAT)).text_color(T::text())
                .children(code.split('\n').map(|l| div().child(if l.is_empty() { " ".to_string() } else { crundi_core::tools::expand_tabs(l) })))))
        .child(div().absolute().top(T::SP_6).right(T::SP_6).invisible().group_hover(group, |s| s.visible())
            .rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_secondary())
            .text_color(if copied { T::green() } else { T::text_secondary() }).child(btn))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn gaps() {
        assert_eq!(gap("p", false), T::SP_8);
        assert_eq!(gap("li", false), T::SP_2);
        assert_eq!(gap("lie", false), T::SP_8);
        assert_eq!(gap("h4", false), px(0.));
        assert_eq!(gap("p", true), px(0.));
    }

    #[::core::prelude::v1::test]
    fn wheel_steps() {
        assert_eq!(after_wheel(false, 15., 15.), Some(true));
        assert_eq!(after_wheel(true, -100., 30.), Some(false));
        assert_eq!(after_wheel(true, -100., 300.), None);
        assert_eq!(after_wheel(false, -100., 0.), None);
    }

    #[::core::prelude::v1::test]
    fn the_jump_button_shows_past_a_third() {
        assert!(!jump_shown(240., 600.));
        assert!(jump_shown(241., 600.));
        assert!(!jump_shown(300., 1200.));
        assert!(jump_shown(401., 1200.));
    }
}
