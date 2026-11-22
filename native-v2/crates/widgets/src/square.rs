//! The composer's square buttons (the Slint client's SquareBtn, MicBtn and
//! CancelX; the web's .cc-attach, .cc-mic, .cc-rec-x): a control-sized
//! bordered square with one icon and a tooltip, in one of a few looks. A
//! Stateful<Div>, so the caller adds a click, or a press and a release (the
//! mic tells a tap from a hold).

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SquareLook {
    /// bg-primary, a border; hover an accent edge and icon.
    Plain,
    /// No fill (the clock): muted icon; hover as Plain.
    Bare,
    /// Faint and inert (attach while a file uploads).
    Busy,
    /// Accent fill, white icon (the mic recording).
    On,
    /// An accent outline and icon (the mic paused for another chat).
    Outline,
    /// Solid red, white icon (× hovered, or a hold over it).
    Hot,
}

/// A square button: `size` is its side (T::CONTROL in the box row).
pub fn square_button(id: impl Into<ElementId>, icon: IconName, tip: impl Into<SharedString>, look: SquareLook, size: Pixels, icon_size: Pixels) -> Stateful<Div> {
    let tip: SharedString = tip.into();
    let (bg, edge, ink) = match look {
        SquareLook::Plain | SquareLook::Busy => (Some(T::bg()), T::border(), T::text_secondary()),
        SquareLook::Bare => (None, T::border(), T::text_muted()),
        SquareLook::On => (Some(T::accent()), T::accent(), T::white()),
        SquareLook::Outline => (Some(T::bg()), T::accent(), T::accent_hover()),
        SquareLook::Hot => (Some(T::red()), T::red(), T::white()),
    };
    let hover = matches!(look, SquareLook::Plain | SquareLook::Bare);
    div().id(id).flex().flex_none().items_center().justify_center().size(size)
        .rounded(T::R_CONTROL).border_1().border_color(edge).when_some(bg, |d, c| d.bg(c))
        .text_color(ink).text_size(icon_size)
        .when(look == SquareLook::Busy, |d| d.opacity(T::MEDIA_O_SOFT))
        .when(look != SquareLook::Busy, |d| d.cursor_pointer())
        .when(hover, |d| d.hover(|s| s.border_color(T::accent()).text_color(T::accent_hover())))
        .child(icon)
        .tooltip(crate::tooltip(tip.clone()))
}
