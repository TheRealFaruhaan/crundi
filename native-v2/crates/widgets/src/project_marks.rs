//! The small marks of the sidebar's project rows and the project bar's tabs
//! (src/webapp-html.js renderProjects, the .tab-btn badges):
//! - `ecg_mark(live)`: the services heartbeat (.svc-heart): the ECG trace,
//!   green while a service runs, muted when all are stopped. (The web sweeps
//!   the trace while live, an endless animation; here it is still: an idle
//!   window draws no frames, ARCHITECTURE.md rule 6.)
//! - `sched_mark(on)`: the clock of a project with scheduled tasks
//!   (.sched-mark, 15; accent on the open project).
//! - `factory_mark(needs)`: a factory running there (.fx-mark, 14; green,
//!   amber when it needs you).
//! - `remove_mark(id)`: the row's trash button (.remove-btn), red on hover;
//!   the row shows it on hover (its group).
//! - `drop_line()`: where a dragged project lands (.proj-drop-line).
//! - `tab_count(n)`: a tab's red count (.secret-badge: Secrets, Factories).
//! - `update_dot()`: the Settings tab's green dot (.update-badge).

use crate::icon;
use crundi_theme::{IconName, T};
use gpui::*;
use std::{cell::RefCell, sync::Arc};

const ECG: &str = "M1 6 H7 L9 2.5 L12 10 L14.5 4 L16 6 H25";
const OVERSAMPLE: f32 = 3.0;

thread_local! { static ECGS: RefCell<Option<(Arc<Image>, Arc<Image>)>> = const { RefCell::new(None) }; }

fn ecg_svg(color: Hsla) -> String {
    let c = color.to_rgb();
    let hex = format!("#{:02x}{:02x}{:02x}", (c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8);
    let (w, h) = (f32::from(T::SP_20) * OVERSAMPLE, (f32::from(T::SP_10) + 1.0) * OVERSAMPLE);
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 26 12\" width=\"{w}\" height=\"{h}\" fill=\"none\" stroke=\"{hex}\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{ECG}\"/></svg>")
}

/// The heartbeat (20 by 11).
pub fn ecg_mark(live: bool) -> Img {
    let (on, off) = ECGS.with(|e| e.borrow_mut().get_or_insert_with(|| (
        Arc::new(Image::from_bytes(ImageFormat::Svg, ecg_svg(T::green()).into_bytes())),
        Arc::new(Image::from_bytes(ImageFormat::Svg, ecg_svg(T::text_muted()).into_bytes())),
    )).clone());
    img(if live { on } else { off }).flex_none().w(T::SP_20).h(T::SP_10 + px(1.))
}

pub fn sched_mark(on: bool) -> Div {
    div().flex_none().flex().items_center().text_color(if on { T::accent_hover() } else { T::text_muted() }).child(icon(IconName::Clock, T::ICON_LG - px(1.)))
}

pub fn factory_mark(needs: bool) -> Div {
    div().flex_none().flex().items_center().ml(T::SP_2).text_color(if needs { T::yellow() } else { T::green() }).child(icon(IconName::Factory, T::ICON_MD))
}

pub fn remove_mark(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).flex_none().flex().items_center().px(T::SP_4).cursor_pointer().text_color(T::text_muted())
        .hover(|d| d.text_color(T::red())).child(icon(IconName::Trash, T::ICON_MD))
}

pub fn drop_line() -> Div {
    div().flex_none().h(T::SP_2 + px(1.)).my(px(1.)).mx(T::SP_6).rounded(T::SP_2).bg(T::accent())
}

pub fn tab_count(text: impl Into<SharedString>) -> Div {
    div().flex().flex_none().items_center().justify_center().min_w(T::SP_16).h(T::SP_16).px(T::SP_4).ml(T::SP_2).rounded(T::SP_8)
        .bg(T::red()).text_color(T::white()).text_size(T::TEXT_10).font_weight(T::W_BOLD).child(text.into())
}

pub fn update_dot() -> Div {
    div().flex_none().size(T::SP_8).rounded(T::R_PILL).bg(T::green())
}
