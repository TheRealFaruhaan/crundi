//! A scrollbar for a view that scrolls itself (a canvas with its own pan,
//! such as the mind map): the web's `::-webkit-scrollbar` (6 px, a thumb
//! the colour of a border, the muted text's on hover, rounded 3 px, the
//! track clear). Drag the thumb to scroll; a press on the track moves a
//! page towards the press, as the browser's does. Shown only when the
//! content is longer than the view.
//!
//! The caller places it in a relatively placed box over its content (it
//! sits on the right edge, or the bottom one) and gives it the view's
//! length, the content's, the offset, and `set` (the new offset, clamped by
//! the caller). Its drag is kept in GPUI's keyed state (`id`), so it
//! survives the frames the scroll redraws.

use crundi_theme::T;
use gpui::*;
use std::rc::Rc;

/// The thumb taken: (the pointer's coordinate along the bar, the offset
/// then); and where the track starts on the window, as last painted.
#[derive(Default)]
pub struct ScrollbarDrag(Option<(f32, f32)>, Rc<std::cell::Cell<f32>>);

/// `vertical`: on the right edge (else the bottom). `view`, `content`,
/// `offset` in px along the bar's axis. `inset`: room left at the far end
/// (the other bar's corner).
#[allow(clippy::too_many_arguments)]
pub fn scrollbar(id: impl Into<SharedString>, vertical: bool, view: f32, content: f32, offset: f32, inset: Pixels, window: &mut Window, cx: &mut App,
                 set: impl Fn(f32, &mut Window, &mut App) + 'static) -> Option<AnyElement> {
    if content <= view + 1. || view <= 0. { return None; }
    let id: SharedString = id.into();
    let state = window.use_keyed_state(ElementId::Name(id.clone()), cx, |_, _| ScrollbarDrag::default());
    let track = (view - f32::from(inset)).max(1.);
    let thumb = (track * view / content).max(f32::from(T::SP_24)).min(track);
    let range = (content - view).max(1.);
    let pos = (offset / range).clamp(0., 1.) * (track - thumb);
    let set: Rc<dyn Fn(f32, &mut Window, &mut App)> = Rc::new(set);
    let along = move |p: Point<Pixels>| f32::from(if vertical { p.y } else { p.x });
    let per_px = range / (track - thumb).max(1.);
    let dragging = state.read(cx).0.is_some();

    let s1 = state.clone();
    let thumb_el = div().id(ElementId::Name(format!("{id}-thumb").into())).absolute().rounded(T::SP_2 + px(1.))
        .bg(if dragging { T::text_muted() } else { T::border() }).hover(|d| d.bg(T::text_muted()))
        .map(|d| if vertical { d.left_0().right_0().top(px(pos)).h(px(thumb)) } else { d.top_0().bottom_0().left(px(pos)).w(px(thumb)) })
        .on_mouse_down(MouseButton::Left, move |e, _, cx| {
            cx.stop_propagation();
            s1.update(cx, |s, cx| { s.0 = Some((along(e.position), offset)); cx.notify(); });
        });
    // While the thumb is held: the pointer anywhere moves it, its release lets go.
    let (s2, set2) = (state.clone(), set.clone());
    let held = canvas(|_, _, _| {}, move |_, _, window, _| {
        if !dragging { return; }
        let (s3, s4, set3) = (s2.clone(), s2.clone(), set2.clone());
        window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble { return; }
            let Some((p0, o0)) = s3.read(cx).0 else { return };
            set3(o0 + (along(e.position) - p0) * per_px, window, cx);
        });
        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble && e.button == MouseButton::Left { s4.update(cx, |s, cx| { s.0 = None; cx.notify(); }); }
        });
    }).absolute().size_0();
    // A press on the track: a page towards it (before the thumb: back).
    let (set5, start) = (set.clone(), state.read(cx).1.clone());
    let start2 = start.clone();
    let measure = canvas(move |b, _, _| start2.set(f32::from(if vertical { b.origin.y } else { b.origin.x })), |_, _, _, _| {}).absolute().size_full();
    let bar = div().id(ElementId::Name(format!("{id}-track").into())).absolute()
        .map(|d| if vertical { d.top_0().right_0().w(T::SP_6).h(px(track)) } else { d.left_0().bottom_0().h(T::SP_6).w(px(track)) })
        .on_mouse_down(MouseButton::Left, move |e, window, cx| {
            cx.stop_propagation();
            let at = along(e.position) - start.get();
            set5(offset + view * if at < pos { -1. } else { 1. }, window, cx);
        })
        .child(measure)
        .child(thumb_el)
        .child(held);
    Some(bar.into_any_element())
}

use gpui::prelude::FluentBuilder as _;
