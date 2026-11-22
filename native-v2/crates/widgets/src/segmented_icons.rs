//! A segmented control whose options are icons (the composer's Enter switch:
//! ⏎ and ⌃⏎ drawn as Lucide icons, never text glyphs). The same look as
//! `segmented`.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Icon;

/// `options`: each option's icons, in order (e.g. [[CornerDownLeft], [ChevronUp, CornerDownLeft]]).
pub fn segmented_icons(id: &'static str, options: &[&[IconName]], on: usize, pick: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Div {
    let pick = std::rc::Rc::new(pick);
    let mut d = div().flex().items_center().p(T::SP_2).gap(T::SP_2).rounded(T::R_PILL).bg(T::bg_card()).border_1().border_color(T::border());
    for (i, icons) in options.iter().enumerate() {
        let pick = pick.clone();
        let fg = if i == on { T::accent_hover() } else { T::text_secondary() };
        let mut o = div().id((id, i)).flex().items_center().px(T::SP_6).py(T::SP_2).rounded(T::R_PILL).cursor_pointer()
            .when(i == on, |d| d.bg(T::accent_dim()))
            .on_click(move |_, w, cx| pick(&i, w, cx));
        for ic in icons.iter() { o = o.child(Icon::new(*ic).size(T::ICON_SM).text_color(fg)); }
        d = d.child(o);
    }
    d
}

/// A two-way switch that is one button (the composer's Enter switch, the
/// web's .cc-toggle: the track with both options, the thumb on the one in
/// use, and its label inside, "sends"): a press anywhere on it, the label
/// and the gap included, is the caller's `on_click`. Secondary text; on
/// hover (.cc-toggle:hover) the label and the option not in use brighten
/// to the primary text. A gpui-component Icon fixes its colour when it is
/// built (group_hover never reaches it), so the hover is held in `hover`
/// (an entity the caller's view observes: only an enter or a leave redraws
/// it, and its driver state can show it) and set on each Icon.
pub fn icon_switch(id: impl Into<ElementId>, options: &[&[IconName]], on: usize, label: Option<SharedString>, hover: Entity<bool>, cx: &App) -> Stateful<Div> {
    let hovered = *hover.read(cx);
    let idle = if hovered { T::text() } else { T::text_secondary() };
    let mut track = div().flex().items_center().p(T::SP_2).gap(T::SP_2).rounded(T::R_PILL).bg(T::bg_card()).border_1().border_color(T::border());
    for (i, icons) in options.iter().enumerate() {
        let fg = if i == on { T::accent_hover() } else { idle };
        let mut o = div().flex().items_center().px(T::SP_6).py(T::SP_2).rounded(T::R_PILL).when(i == on, |d| d.bg(T::accent_dim()));
        for ic in icons.iter() { o = o.child(Icon::new(*ic).size(T::ICON_SM).text_color(fg)); }
        track = track.child(o);
    }
    div().id(id).flex().items_center().gap(T::SP_4).cursor_pointer().text_color(idle)
        .on_hover(move |over, _, cx| hover.update(cx, |h, cx| if *h != *over { *h = *over; cx.notify(); }))
        .child(track)
        .when_some(label, |d, l| d.child(l))
}
