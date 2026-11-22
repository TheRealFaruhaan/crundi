//! The composer's voice line (the web's .cc-vline): a dot and one line of
//! words, cut with an ellipsis. Its tones: a note (a yellow dot, secondary
//! text), soft (speaking or paused: a sky dot, muted italic words), an
//! error (red words, no dot). The caller adds its press.

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

#[derive(Clone, Copy, PartialEq)]
pub enum LineTone { Note, Soft, Error }

pub fn status_line(id: impl Into<ElementId>, text: impl Into<SharedString>, tone: LineTone) -> Stateful<Div> {
    let soft = tone == LineTone::Soft;
    div().id(id).relative().flex().items_center().gap(T::SP_6).px(T::SP_2).cursor_pointer()
        .when(tone != LineTone::Error, |d| d.child(div().flex_none().size(T::SP_8).rounded_full().bg(if soft { T::sky() } else { T::yellow() })))
        .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().text_size(T::TEXT_12)
            .when(soft, |d| d.italic())
            .text_color(match tone { LineTone::Error => T::red(), LineTone::Soft => T::text_muted(), LineTone::Note => T::text_secondary() })
            .child(text.into()))
}
