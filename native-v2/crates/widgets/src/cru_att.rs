//! Cru's attachments and the kid screen's big button (crundi-voice.js):
//!
//! - `att_chip`: a file waiting to go with the next message (.cv-chip,
//!   VJ:450, renderAtt 1454-1462): a picture's 28 px thumbnail, the name
//!   (or "name: why" when it failed, with a red edge), and its ×, the only
//!   way to remove it ("Remove"); faded while it uploads.
//! - `att_tag`: an attachment line in a message of mine (.cv-m .att, VJ:451).
//! - `kid_pill`: the kid screen's call to action over a video, "Talk to
//!   Cru" (.cv-talk, VJ:542): a big amber pill with a flat drop.
//!
//! (Made by Desktop, F192; the Architect to review.)

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use std::sync::Arc;

pub struct AttChip {
    pub name: SharedString,
    /// Why it failed (empty: it didn't).
    pub bad: SharedString,
    /// Still uploading.
    pub up: bool,
    pub thumb: Option<Arc<Image>>,
}

pub fn att_chip(id: impl Into<ElementId>, x_id: impl Into<ElementId>, a: AttChip, remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    let bad = !a.bad.is_empty();
    let label: SharedString = if bad { format!("{}: {}", a.name, a.bad).into() } else { a.name.clone() };
    let tip = if bad { a.bad.clone() } else { a.name.clone() };
    let x = crate::tip(div().id(x_id).flex_none().px(T::SP_2).cursor_pointer().text_size(T::TEXT_14).line_height(relative(1.)).text_color(T::text_muted())
        .hover(|d| d.text_color(T::text())).child("×")
        .on_click(move |e, w, cx| { cx.stop_propagation(); remove(e, w, cx); }), "Remove");
    crate::tip(div().id(id).flex().items_center().gap(T::SP_6).max_w(T::CRU_ATT_MAX_W).pl(T::SP_8).pr(T::SP_6).py(T::SP_4)
        .rounded(T::CRU_ATT_R).bg(T::bg_tertiary()).border_1().border_color(if bad { T::cru_att_bad() } else { T::border() })
        .text_size(T::TEXT_12).text_color(T::text()).when(a.up, |d| d.opacity(T::CRU_ATT_UP_O))
        .when_some(a.thumb, |d, t| d.child(img(t).flex_none().size(T::CRU_ATT_THUMB).rounded(T::CRU_ATT_THUMB_R).object_fit(ObjectFit::Cover)))
        .child(div().min_w_0().truncate().child(label))
        .child(x), tip)
}

pub fn att_tag(text: impl Into<SharedString>) -> Div {
    div().flex().items_center().gap(T::SP_6).my(T::SP_2).px(T::SP_8).py(T::CRU_ATT_TAG_PAD_Y).rounded(T::CRU_ATT_TAG_R)
        .bg(T::cru_att_tag()).text_size(T::TEXT_12).child(text.into())
}

pub fn kid_pill(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    let drop = BoxShadow { color: T::kid_talk_drop(), offset: point(px(0.), T::KID_TALK_DROP), blur_radius: px(0.), spread_radius: px(0.), inset: false };
    div().id(id).px(T::KID_TALK_PAD_X).py(T::KID_TALK_PAD_Y).rounded(T::R_PILL).bg(T::kid_talk()).text_color(T::kid_talk_text())
        .text_size(T::KID_TALK_TEXT).font_weight(T::W_BOLD).shadow(vec![drop]).cursor_pointer()
        .child({ let l: SharedString = label.into(); div().relative().child(l.clone()).child(crate::find::mark(l)) })
}
