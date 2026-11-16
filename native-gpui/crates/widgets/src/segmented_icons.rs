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
