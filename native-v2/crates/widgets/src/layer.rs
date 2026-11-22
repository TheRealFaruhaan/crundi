//! One paint layer around an element (ARCHITECTURE.md, "Paint layers"):
//! its primitives take the layer's draw order instead of one bounds-tree
//! insert each, every frame, also when a clean view is replayed; and they
//! batch together, so the frame has fewer draw calls. No layout of its own:
//! its child's.
//!
//! Inside a layer GPUI keeps the paint order among quads and among glyphs
//! and icons, but draws every quad (and path, and underline) under every
//! glyph and icon. So only what draws backgrounds under its text goes in
//! one: never a text box (its caret is a quad over the text), and anything
//! that must cover an icon or text drawn before it (a status dot on an
//! icon) goes in a layer of its own inside, which is drawn over the outer one.

use gpui::*;

pub struct Layer(AnyElement);

/// `el` painted in one layer.
pub fn layer(el: impl IntoElement) -> Layer { Layer(el.into_any_element()) }

impl IntoElement for Layer {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for Layer {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.0.request_layout(window, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.0.prepaint(window, cx);
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        window.paint_layer(bounds, |window| self.0.paint(window, cx));
    }
}

/// A text box painted in a layer while the focus is not in it (then it
/// draws no caret, so nothing in it goes over its text); as it is while
/// focused. The view that draws it re-renders on its focus and blur
/// (`watch_focus`), so the layer comes and goes with the caret.
pub fn text_box(focus: &FocusHandle, el: impl IntoElement, window: &Window, cx: &App) -> AnyElement {
    if focus.contains_focused(window, cx) { el.into_any_element() } else { layer(el).into_any_element() }
}

/// Re-render the view when `focus` (or what is in it) gains or loses the focus.
pub fn watch_focus<V: 'static>(focus: &FocusHandle, window: &mut Window, cx: &mut Context<V>) -> [Subscription; 2] {
    [cx.on_focus_in(focus, window, |_, _, cx| cx.notify()), cx.on_focus_out(focus, window, |_, _, _, cx| cx.notify())]
}
