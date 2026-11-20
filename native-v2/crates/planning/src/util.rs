//! Small helpers every planning screen uses: JSON reads, the query
//! encoding, a toast or a tab from a handler that has no window, the
//! saved settings (the web's localStorage), a picture's format.

use crundi_widgets as w;
use gpui::*;
use serde_json::Value;

pub fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// JavaScript's truthiness, as the web reads the server's answers.
pub fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::Number(n) => n.as_f64() != Some(0.0), Value::String(s) => !s.is_empty(), _ => true }
}

/// A query value, percent-encoded.
pub fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Run with the window (a server call's answer comes with the App only).
pub fn in_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    if let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) { let _ = cx.update_window(h, |_, window, cx| f(window, cx)); }
}

/// Later, on the UI thread, with the window.
pub fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(std::time::Duration::from_millis(ms)).await;
        let _ = cx.update(|cx| in_window(cx, f));
    }).detach();
}

/// A toast, from anywhere (deferred to the window).
pub fn toast(cx: &mut App, text: &str, kind: w::Toast) {
    let t = text.to_string();
    cx.defer(move |cx| in_window(cx, move |window, cx| w::toast(window, cx, t, kind)));
}

/// Show a project tab (Kanban's "Open in the Mindmap", Brainstorm).
pub fn show_tab(cx: &mut App, tab: &str) {
    let t = tab.to_string();
    cx.defer(move |cx| { let s = crundi_shell::stores(cx); s.nav.update(cx, |n, cx| n.show_tab(&t, cx)); });
}

/// The tab on show now (handlers only).
pub fn current_tab(cx: &App) -> String { crundi_shell::stores(cx).nav.read(cx).tab.clone() }

/// A panel is a pane on the workbench.
pub fn pane_shown(cx: &mut App, key: &str) -> bool {
    crundi_workbench::workbench(cx).is_some_and(|wb| wb.update(cx, |w, _| w.keys().iter().any(|k| k == key)))
}

/// A picture's format from its first bytes.
pub fn image_format(b: &[u8]) -> Option<ImageFormat> {
    Some(if b.starts_with(b"\x89PNG") { ImageFormat::Png } else if b.starts_with(&[0xff, 0xd8]) { ImageFormat::Jpeg } else if b.starts_with(b"GIF8") { ImageFormat::Gif }
        else if b.len() > 12 && &b[8..12] == b"WEBP" { ImageFormat::Webp } else if b.starts_with(b"BM") { ImageFormat::Bmp } else { return None })
}

/// Small saved settings (the web's localStorage), in the config folder.
pub mod prefs {
    use serde_json::Value;
    fn path() -> std::path::PathBuf { crundi_core::store::config_dir().join("planning.json") }
    fn all() -> Value { std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Object(Default::default())) }
    pub fn get(k: &str) -> String { all().get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
    pub fn set(k: &str, v: &str) {
        let mut a = all();
        a[k] = Value::String(v.to_string());
        let _ = std::fs::write(path(), a.to_string());
    }
}

/// The ids of elements, from a prefix and a key.
pub fn eid(p: &str, k: &str) -> ElementId { ElementId::Name(format!("{p}-{k}").into()) }

/// An element painted in one paint layer (as the chat log's list,
/// ARCHITECTURE.md "One paint layer per log"): its primitives take the
/// layer's order instead of one bounds-tree insert each, also when a clean
/// view is replayed. No layout of its own: its child's.
pub struct Layer(pub AnyElement);

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
