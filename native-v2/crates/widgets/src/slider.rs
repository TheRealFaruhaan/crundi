//! A slider (the web's `<input type=range>` with `accent-color`: the
//! factory run's replay bar): a thin track, the part up to the value in the
//! accent, a round thumb. A press on the track sets the value there and a
//! drag follows the pointer (even past the track's ends) until the button is
//! let go; `step` rounds the value. Theme tokens only; an id for its hover
//! (the thumb brightens). Added by Planning (Factories, F92).

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use std::rc::Rc;

/// `value` in `min..=max`, rounded to `step` (0: any). `on_change` hears each
/// new value (only when it changes) while pressed or dragged.
pub fn slider(id: impl Into<ElementId>, min: f32, max: f32, step: f32, value: f32, disabled: bool, on_change: impl Fn(f32, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    let id: ElementId = id.into();
    let span = (max - min).max(f32::EPSILON);
    let share = ((value - min) / span).clamp(0.0, 1.0);
    let on_change: Rc<dyn Fn(f32, &mut Window, &mut App)> = Rc::new(on_change);
    let pick = move |x: Pixels, b: Bounds<Pixels>| -> f32 {
        let f = (f32::from(x - b.origin.x) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0);
        let v = min + f * span;
        if step > 0.0 { (min + ((v - min) / step).round() * step).clamp(min, max) } else { v }
    };
    let key = id.clone();
    // The pointer's part: a hitbox over the track; while pressed, the window's
    // moves and the release (so a drag past the ends keeps following).
    let input = canvas(
        move |b, window, cx| {
            let drag = window.use_keyed_state(key.clone(), cx, |_, _| false);
            (window.insert_hitbox(b, HitboxBehavior::Normal), drag)
        },
        move |b, (hit, drag), window, _cx| {
            if disabled { return; }
            let last = Rc::new(std::cell::Cell::new(value));
            { let (drag, on, last) = (drag.clone(), on_change.clone(), last.clone());
                window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || e.button != MouseButton::Left || !hit.is_hovered(window) { return; }
                    drag.update(cx, |d, _| *d = true);
                    let v = pick(e.position.x, b);
                    if v != last.get() { last.set(v); on(v, window, cx); }
                    cx.stop_propagation();
                }); }
            { let (drag, on, last) = (drag.clone(), on_change.clone(), last.clone());
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || !*drag.read(cx) { return; }
                    if e.pressed_button != Some(MouseButton::Left) { drag.update(cx, |d, _| *d = false); return; }
                    let v = pick(e.position.x, b);
                    if v != last.get() { last.set(v); on(v, window, cx); }
                }); }
            window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left && *drag.read(cx) { drag.update(cx, |d, _| *d = false); }
            });
        },
    ).absolute().top_0().left_0().size_full();
    let group: SharedString = format!("slider-{id}").into();
    div().id(id).group(group.clone()).relative().flex().flex_1().min_w_0().h(T::SP_16).items_center()
        .when(!disabled, |d| d.cursor_pointer()).when(disabled, |d| d.opacity(0.45))
        .child(div().absolute().left_0().right_0().h(T::SP_4).rounded(T::R_PILL).bg(T::bg_tertiary()))
        .child(div().absolute().left_0().h(T::SP_4).rounded(T::R_PILL).bg(T::accent()).w(relative(share)))
        .child(div().absolute().left(relative(share)).ml(-T::SP_6).size(T::SP_12).rounded(T::R_PILL).bg(T::accent())
            .group_hover(group, |d| d.bg(T::accent_hover())))
        .child(input)
}
