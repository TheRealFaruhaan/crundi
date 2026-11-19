//! A segmented nav of sections (the web's .dk-seg: Docker's and Homepool's
//! sub-tabs): a tinted strip of buttons, the current one raised; each with
//! an optional icon, its label and a count (muted, or an accent pill when
//! it needs the person: `alert`). (Made by operations; Lead to review.)

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use std::rc::Rc;

/// One section of a `seg_tabs`.
#[derive(Clone, Default)]
pub struct SegTab {
    pub icon: Option<IconName>,
    pub label: SharedString,
    /// "" draws none.
    pub count: SharedString,
    pub alert: bool,
}

pub fn seg_tabs(id: &'static str, tabs: Vec<SegTab>, on: usize, pick: impl Fn(usize, &mut Window, &mut App) + 'static) -> Div {
    let pick = Rc::new(pick);
    let mut d = div().flex().flex_none().items_center().p(T::SP_2).gap(T::SP_2).rounded(T::R_FIELD).bg(T::bg_tertiary()).border_1().border_color(T::border());
    for (i, t) in tabs.into_iter().enumerate() {
        let pick = pick.clone();
        let sel = i == on;
        d = d.child(div().id((id, i)).flex().items_center().gap(T::SP_6).h(T::CONTROL_SM).px(T::SP_10).rounded(T::R_CONTROL).cursor_pointer()
            .text_size(T::TEXT_12)
            .text_color(if sel { T::text() } else { T::text_secondary() })
            .when(sel, |d| d.bg(T::bg_hover()).border_1().border_color(T::border()))
            .hover(|d| d.text_color(T::text()))
            .children(t.icon.map(|ic| crate::icon(ic, T::ICON_MD)))
            .child(t.label)
            .when(!t.count.is_empty() && !t.alert, |d| d.child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(t.count.clone())))
            .when(!t.count.is_empty() && t.alert, |d| d.child(div().px(T::SP_6).rounded(T::R_PILL).bg(T::accent()).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::white()).child(t.count.clone())))
            .on_click(move |_, w, cx| pick(i, w, cx)));
    }
    d
}
