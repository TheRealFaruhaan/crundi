//! The paint helpers live in the widgets crate (`crundi_widgets::paint`).
pub use crundi_widgets::paint::*;
use gpui::*;

/// The words a drawing shows, for Cru's highlight (F202): each one's box in
/// the drawing, its words, and whether they are a tip (`find::label_mark`)
/// rather than words on it (`find::mark`). Laid out with the drawing.
pub type Marks = Vec<(Bounds<Pixels>, SharedString, bool)>;

/// Put a drawing's marks where it is drawn (from an element's prepaint), as
/// `find::mark` children over its divs would be: each in its own namespace
/// under `ns` (the card's or idea's id), so equal words keep their own place.
pub fn prepaint_marks(ns: &str, marks: &Marks, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
    if marks.is_empty() { return; }
    window.with_element_namespace(ElementId::Name(SharedString::from(ns.to_string())), |window| {
        for (i, (b, t, label)) in marks.iter().enumerate() {
            let mut m = if *label { crundi_widgets::find::label_mark(t.clone()) } else { crundi_widgets::find::mark(t.clone()) };
            let at = Bounds { origin: origin + b.origin, size: b.size };
            window.with_element_namespace(i, |window| Element::prepaint(&mut m, None, None, at, &mut (), window, cx));
        }
    });
}
