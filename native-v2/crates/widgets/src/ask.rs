//! Ask the person (made by Operations, AREAS.md "A new control"; the Slint
//! client's kit::confirm, kit::prompt and kit::menu): a confirm (OK /
//! Cancel), a prompt (a line of text) and a context menu at a window point.
//! One of each at a time for the whole window (a second replaces the first,
//! which hears "cancelled").
//!
//! What is open is readable by the test driver (`ask_props`: the Slint
//! client's "dialog" names; `menu_props`: its "menu") and answerable
//! (`ask_answer`: "dialog-answer"; `menu_pick`: "menu-pick").
//!
//! Drawn by `ask_layer(cx)`, a view a host renders once (anywhere: it draws
//! over the whole window). The operations tab renders it until the shell does.

use crate::{button, danger, input, primary, InputState};
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::rc::Rc;

/// What a confirm or a prompt says.
#[derive(Clone, Default)]
pub struct Ask {
    pub title: SharedString,
    pub message: SharedString,
    /// The OK button's label ("" reads "OK").
    pub ok: SharedString,
    pub danger: bool,
    /// A prompt: its text box's starting value (None: a confirm).
    pub prompt: Option<String>,
    pub placeholder: SharedString,
}

/// One item of a context menu.
#[derive(Clone, Default)]
pub struct MenuEntry {
    pub id: String,
    pub label: SharedString,
    pub icon: Option<IconName>,
    pub danger: bool,
    /// A line before this item.
    pub sep: bool,
    pub disabled: bool,
}

impl MenuEntry {
    pub fn new(id: &str, label: impl Into<SharedString>) -> Self { MenuEntry { id: id.into(), label: label.into(), ..Default::default() } }
    pub fn icon(mut self, i: IconName) -> Self { self.icon = Some(i); self }
    pub fn danger(mut self) -> Self { self.danger = true; self }
    pub fn sep(mut self) -> Self { self.sep = true; self }
    pub fn disabled(mut self, off: bool) -> Self { self.disabled = off; self }
}

type Then = Rc<dyn Fn(Option<String>, &mut Window, &mut App)>;
type Picked = Rc<dyn Fn(String, &mut Window, &mut App)>;

struct Pending { a: Ask, input: Option<Entity<InputState>>, then: Then }
struct OpenMenu { at: Point<Pixels>, items: Vec<MenuEntry>, then: Picked }

pub struct AskLayer {
    pending: Option<Pending>,
    menu: Option<OpenMenu>,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

struct Handle(Entity<AskLayer>);
impl Global for Handle {}

fn layer(cx: &mut App) -> Entity<AskLayer> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(|cx| AskLayer { pending: None, menu: None, focus: cx.focus_handle(), scroll: ScrollHandle::new() });
    cx.set_global(Handle(e.clone()));
    e
}

/// The view that draws what is open (render it once, in any host view).
pub fn ask_layer(cx: &mut App) -> AnyView { layer(cx).into() }

/// Ask: `then(None)` on Cancel or Esc; `then(Some(""))` on OK for a confirm,
/// `then(Some(text))` for a prompt (OK or Enter).
pub fn ask(window: &mut Window, cx: &mut App, a: Ask, then: impl Fn(Option<String>, &mut Window, &mut App) + 'static) {
    let l = layer(cx);
    let old = l.update(cx, |l, _| l.pending.take());
    if let Some(o) = old { (o.then)(None, window, cx); }
    let input = a.prompt.clone().map(|v| {
        let ph = a.placeholder.clone();
        cx.new(|cx| { let mut s = InputState::new(window, cx).placeholder(ph); s.set_value(v, window, cx); s })
    });
    let fh = l.read(cx).focus.clone();
    if let Some(i) = &input { i.update(cx, |s, cx| { s.focus(window, cx); s.select_all(window, cx); }); } else { window.focus(&fh, cx); }
    l.update(cx, |l, cx| { l.pending = Some(Pending { a, input, then: Rc::new(then) }); cx.notify(); });
}

/// A confirm: `then(true)` on OK, `then(false)` otherwise.
pub fn ask_confirm(window: &mut Window, cx: &mut App, a: Ask, then: impl Fn(bool, &mut Window, &mut App) + 'static) {
    ask(window, cx, Ask { prompt: None, ..a }, move |r, w, cx| then(r.is_some(), w, cx));
}

/// The open dialog's answer (its buttons, keys, the driver's "dialog-answer").
/// False when nothing is open.
pub fn ask_answer(ok: bool, value: Option<String>, window: &mut Window, cx: &mut App) -> bool {
    let l = layer(cx);
    let Some(p) = l.update(cx, |l, cx| { cx.notify(); l.pending.take() }) else { return false };
    let r = if !ok { None } else if let Some(i) = &p.input {
        // The driver's value as given (an empty one too, as the Slint client's
        // dialog_answer); a press on OK or Enter: what the box holds.
        Some(value.unwrap_or_else(|| i.read(cx).value().to_string()))
    } else { Some(String::new()) };
    (p.then)(r, window, cx);
    true
}

/// For the driver: the open dialog (the Slint client's names), or Null.
pub fn ask_props(cx: &App) -> Value {
    let Some(p) = cx.try_global::<Handle>().and_then(|h| h.0.read(cx).pending.as_ref().map(|p| (p.a.clone(), p.input.clone()))) else { return Value::Null };
    let (a, input) = p;
    json!({ "title": a.title.to_string(), "message": a.message.to_string(), "ok": if a.ok.is_empty() { "OK".to_string() } else { a.ok.to_string() },
        "danger": a.danger, "prompt": a.prompt.is_some(), "value": input.map(|i| i.read(cx).value().to_string()).unwrap_or_default() })
}

/// A context menu at a window point; `then(id)` when an item is picked
/// (nothing when it is dismissed).
pub fn menu_at(cx: &mut App, at: Point<Pixels>, items: Vec<MenuEntry>, then: impl Fn(String, &mut Window, &mut App) + 'static) {
    layer(cx).update(cx, |l, cx| { l.menu = Some(OpenMenu { at, items, then: Rc::new(then) }); cx.notify(); });
}

/// The driver's pick of a menu item by id (false: no menu, or no such enabled item).
pub fn menu_pick(id: &str, window: &mut Window, cx: &mut App) -> bool {
    let l = layer(cx);
    let Some(m) = l.update(cx, |l, cx| { cx.notify(); l.menu.take() }) else { return false };
    if !m.items.iter().any(|i| i.id == id && !i.disabled) { return false; }
    (m.then)(id.to_string(), window, cx);
    true
}

/// For the driver: the open menu, or Null.
pub fn menu_props(cx: &App) -> Value {
    let Some(m) = cx.try_global::<Handle>().and_then(|h| h.0.read(cx).menu.as_ref().map(|m| (m.at, m.items.clone()))) else { return Value::Null };
    json!({ "x": f32::from(m.0.x), "y": f32::from(m.0.y), "items": m.1.iter().map(|i| json!({ "id": i.id, "label": i.label.to_string(), "danger": i.danger, "enabled": !i.disabled })).collect::<Vec<_>>() })
}

impl Render for AskLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The toast's own view (its changes redraw it alone).
        let mut root = div().child(crate::toast_layer(cx));
        if let Some(m) = &self.menu {
            let vp = window.viewport_size();
            let rows: Vec<AnyElement> = m.items.iter().enumerate().flat_map(|(i, it)| {
                let mut v: Vec<AnyElement> = vec![];
                if it.sep && i > 0 { v.push(crate::rule().into_any_element()); }
                let id = it.id.clone();
                let off = it.disabled;
                v.push(div().id(("ask-menu", i)).flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL)
                    .text_size(T::TEXT_13).text_color(if it.danger { T::red() } else { T::text() })
                    .when(off, |d| d.opacity(0.45))
                    .when(!off, |d| d.cursor_pointer().hover(|d| d.bg(T::bg_hover())))
                    .children(it.icon.map(|ic| crate::icon(ic, T::ICON_MD)))
                    .child(it.label.clone())
                    .when(!off, |d| d.on_click(cx.listener(move |_, _, w, cx| { let id = id.clone(); menu_pick(&id, w, cx); })))
                    .into_any_element());
                v
            }).collect();
            // A press outside closes it and goes on to what it pressed (the web's capturing
            // pointerdown, src/webapp-html.js:12185; F221): no backdrop, so a tab switches at once.
            root = root.child(deferred(anchored().position(point(px(0.), px(0.))).child(
                div().id("ask-menu-back").w(vp.width).h(vp.height)
                    // Kept inside the window, 6 px in, as the web's openCtxMenu.
                    .child(anchored().position(m.at).snap_to_window_with_margin(px(6.)).child(div().id("ask-menu-card").occlude().min_w(T::MENU_MIN_W).p(T::SP_4).flex().flex_col()
                        .bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_FIELD)
                        .on_mouse_down_out(cx.listener(|l, _, _, cx| if l.menu.take().is_some() { cx.notify(); }))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(rows))),
            )).with_priority(2));
        }
        if let Some(p) = &self.pending {
            let me = cx.entity().downgrade();
            let me2 = me.clone();
            let ok_label = if p.a.ok.is_empty() { SharedString::from("OK") } else { p.a.ok.clone() };
            let ok_btn = if p.a.danger { danger("ask-ok", ok_label) } else { primary("ask-ok", ok_label) };
            let body: Vec<AnyElement> = vec![
                div().px(T::SP_10).pt(T::SP_10).text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(p.a.title.clone()).into_any_element(),
                div().px(T::SP_10).pt(T::SP_8).text_size(T::TEXT_13).text_color(T::text_secondary()).when(p.a.message.is_empty(), |d| d.hidden()).child(p.a.message.clone()).into_any_element(),
                div().px(T::SP_10).pt(T::SP_10).children(p.input.as_ref().map(|i| input(i))).into_any_element(),
                div().flex().justify_end().gap(T::SP_8).p(T::SP_10)
                    .child(button("ask-cancel", "Cancel").on_click(|_, w, cx| { ask_answer(false, None, w, cx); }))
                    .child(ok_btn.on_click(|_, w, cx| { ask_answer(true, None, w, cx); }))
                    .into_any_element(),
            ];
            let m = crate::Modal {
                id: "ask-dialog", width: T::DIALOG_W, focus: self.focus.clone(), scroll: self.scroll.clone(),
                on_close: Rc::new(move |w, cx| { let _ = me.upgrade(); ask_answer(false, None, w, cx); }),
                on_key: Rc::new(move |e, w, cx| match e.keystroke.key.as_str() {
                    "escape" => { ask_answer(false, None, w, cx); }
                    "enter" => { let _ = me2.upgrade(); ask_answer(true, None, w, cx); }
                    _ => {}
                }),
            };
            root = root.child(crate::overlay_modal(m, window, body));
        }
        root
    }
}
