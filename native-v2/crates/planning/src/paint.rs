//! The paint helpers live in the widgets crate (`crundi_widgets::paint`).
pub use crundi_widgets::paint::*;
use gpui::*;

/// The words a drawing shows, for Cru's highlight (F202): laid out with
/// the drawing (`find::Words`), put where it is drawn with `find::words_at`.
pub type Marks = Vec<(Bounds<Pixels>, SharedString, bool)>;
