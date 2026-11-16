//! The Browsers tab (src/webapp-html.js loadBrowsers, renderBrowsers; the
//! Slint client's src/desktop/browsers.rs): the browsers chats opened with
//! browser_open (GET /api/browsers, not the drawer's tabs), opened, driven
//! and closed from here. Loaded when the tab comes on show, again 500 ms
//! after an open or a close; no live updates (as the web).

use crate::ctx as kit;
use crundi_theme::{IconName, T};
use crundi_webview::later;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, sync::Arc, time::Duration};

/// The web's own device list for this tab (DEVICE_PROFILES): name, w, h, group.
pub const PROFILES: &[(&str, i32, i32, &str)] = &[
    ("Desktop 1280\u{d7}720", 1280, 720, "Desktop"), ("Full HD 1920\u{d7}1080", 1920, 1080, "Desktop"),
    ("iPhone SE", 375, 667, "Apple"), ("iPhone X", 375, 812, "Apple"), ("iPhone 12 Pro", 390, 844, "Apple"), ("iPhone 14 Pro", 393, 852, "Apple"),
    ("iPhone 15 Pro", 393, 852, "Apple"), ("iPhone 16 Pro", 393, 852, "Apple"), ("iPhone 16 Pro Max", 430, 932, "Apple"), ("iPad Air", 820, 1180, "Apple"), ("iPad Pro", 1024, 1366, "Apple"),
    ("Pixel 7", 412, 915, "Google"), ("Pixel 9 Pro", 430, 932, "Google"),
    ("Galaxy S24 Ultra", 412, 915, "Samsung"), ("Galaxy Z Fold 5", 344, 882, "Samsung"),
    ("Surface Pro 9", 1200, 1800, "Microsoft"),
];
const PLACES: &[&str] = &["Run on: default", "Run on this server", "Run on the desktop app"];

/// A screenshot's state: "loading", "ok" (with the picture), "error" (with why).
#[derive(Clone)]
struct Shot { state: &'static str, image: Option<Arc<RenderImage>>, error: String }

struct St { list: Vec<Value>, shots: HashMap<String, Shot>, loaded: bool, device: usize, place: usize, places: bool }
thread_local! {
    static S: RefCell<St> = RefCell::new(St { list: vec![], shots: HashMap::new(), loaded: false, device: PROFILES.iter().position(|p| p.0 == "iPhone 16 Pro").unwrap_or(0), place: 0, places: false });
}

/// "Apple · iPhone 16 Pro (393×852)", as the web's optgroups.
fn labels() -> &'static [&'static str] {
    thread_local! { static L: &'static [&'static str] = Box::leak(PROFILES.iter().map(|(n, w, h, g)| &*Box::leak(format!("{g} · {n} ({w}\u{d7}{h})").into_boxed_str())).collect::<Vec<&'static str>>().into_boxed_slice()); }
    L.with(|l| *l)
}

pub fn load() {
    let alias = kit::project();
    kit::call(move |api| api.get(&format!("/api/browsers?alias={}", kit::enc(&alias))), |r| match r {
        Ok(v) => {
            let places = v.get("places").cloned().unwrap_or(Value::Null);
            S.with(|s| {
                let mut s = s.borrow_mut();
                s.list = v.get("browsers").and_then(|b| b.as_array()).cloned().unwrap_or_default();
                s.loaded = true;
                s.places = kit::truthy(&places["server"]) && kit::truthy(&places["desktop"]);
            });
            kit::dirty();
        }
        Err(e) => kit::toast_error(&format!("Failed to load browsers: {e}")),
    });
}

/// After an open or a close, a hidden browser opened or closed: load again.
pub fn reload_soon() { if kit::get().tab == "browsers" { later(Duration::from_millis(500), load); } }

pub fn open(url: &str) {
    let url = url.trim().to_string();
    if url.is_empty() { return kit::toast_error("Enter a URL first"); }
    let (dev, place) = S.with(|s| { let s = s.borrow(); (s.device, s.place) });
    let (name, pw, ph, _) = PROFILES[dev.min(PROFILES.len() - 1)];
    let mut body = json!({ "alias": kit::project(), "name": format!("browser-{}", crundi_webview::engine::now_ms()), "url": url, "width": pw, "height": ph });
    match place { 1 => body["on"] = json!("server"), 2 => body["on"] = json!("desktop"), _ => {} }
    kit::call(move |api| api.post("/api/browsers/open", &body), move |r| {
        match r {
            Ok(v) if kit::truthy(&v["ok"]) => kit::toast_ok(&format!("Browser opened ({name})")),
            Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Failed to open browser".into())),
            Err(e) => kit::toast_error(&format!("Error: {e}")),
        }
        later(Duration::from_millis(500), load);
    });
}

pub fn go(key: &str, url: &str) {
    let mut url = url.trim().to_string();
    if url.is_empty() { return; }
    let l = url.to_lowercase();
    if !l.starts_with("http://") && !l.starts_with("https://") && !l.starts_with("about:") {
        url = format!("{}{url}", if l.starts_with("localhost") { "http://" } else { "https://" });
    }
    let path = format!("/api/browsers/{}/navigate", kit::enc(key));
    kit::call(move |api| api.post(&path, &json!({ "url": url })), |r| match r {
        Ok(v) if !kit::truthy(&v["ok"]) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Navigate failed".into())),
        Ok(_) => {}
        Err(e) => kit::toast_error(&format!("Error: {e}")),
    });
}

pub fn shot(key: &str) {
    let k = key.to_string();
    S.with(|s| { s.borrow_mut().shots.insert(k.clone(), Shot { state: "loading", image: None, error: String::new() }); });
    kit::dirty();
    let path = format!("/api/browsers/{}/screenshot", kit::enc(key));
    kit::call(move |api| api.get(&path), move |r| {
        let fail = |e: String| Shot { state: "error", image: None, error: e };
        let st = match r {
            Ok(v) if kit::truthy(&v["ok"]) && !kit::s(&v, "data").is_empty() => {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(kit::s(&v, "data")).ok().and_then(|b| image::load_from_memory(&b).ok()) {
                    Some(i) => {
                        // GPUI's images are BGRA.
                        let mut b = i.to_rgba8();
                        for px in b.chunks_exact_mut(4) { px.swap(0, 2); }
                        Shot { state: "ok", image: Some(Arc::new(RenderImage::new([image::Frame::new(b)]))), error: String::new() }
                    }
                    None => fail("Screenshot failed".into()),
                }
            }
            Ok(v) => fail(Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Screenshot failed".into())),
            Err(e) => fail(format!("Error: {e}")),
        };
        S.with(|s| { s.borrow_mut().shots.insert(k, st); });
        kit::dirty();
    });
}

pub fn close(key: &str) {
    let path = format!("/api/browsers/{}/close", kit::enc(key));
    kit::call(move |api| api.post(&path, &json!({})), |r| {
        match r {
            Ok(v) if kit::truthy(&v["ok"]) => kit::toast_ok("Browser closed"),
            Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Failed".into())),
            Err(e) => kit::toast_error(&format!("Error: {e}")),
        }
        later(Duration::from_millis(500), load);
    });
}

pub fn props() -> Value {
    S.with(|s| { let s = s.borrow(); json!({ "loaded": s.loaded, "list": s.list.iter().map(|b| json!({ "key": b["key"], "url": b["url"], "on": b["on"] })).collect::<Vec<_>>(), "shots": s.shots.iter().map(|(k, v)| (k.clone(), json!(v.state))).collect::<serde_json::Map<_, _>>() }) })
}

// ─── The page ───

pub struct BrowsersPage {
    url: Entity<w::InputState>,
    /// Each card's address field, by the browser's key.
    urls: HashMap<String, Entity<w::InputState>>,
    _sub: Subscription,
}

impl BrowsersPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| w::InputState::new(window, cx).placeholder("URL to open"));
        let sub = cx.subscribe_in(&url, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev { open(&st.read(cx).value()); });
        BrowsersPage { url, urls: HashMap::new(), _sub: sub }
    }

    fn card_url(&mut self, key: &str, url: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<w::InputState> {
        if let Some(e) = self.urls.get(key) { return e.clone(); }
        let e = cx.new(|cx| { let mut s = w::InputState::new(window, cx).placeholder("URL"); s.set_value(url.to_string(), window, cx); s });
        let k = key.to_string();
        cx.subscribe_in(&e, window, move |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev { go(&k, &st.read(cx).value()); }).detach();
        self.urls.insert(key.to_string(), e.clone());
        e
    }
}

impl Render for BrowsersPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (list, shots, loaded, device, place, places) = S.with(|s| { let s = s.borrow(); (s.list.clone(), s.shots.clone(), s.loaded, s.device, s.place, s.places) });
        let me = cx.entity().downgrade();
        let tools = div().flex().flex_col().gap(T::SP_6)
            .child(div().flex().items_center().gap(T::SP_6)
                .child(w::menu_button("dev", IconName::Smartphone, "Device", labels(), device, me.clone(), |_, i, cx| { S.with(|s| s.borrow_mut().device = i); cx.notify(); }))
                .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).text_color(T::text()).child(labels()[device]))
                .when(places, |d| d
                    .child(w::menu_button("place", IconName::Server, "Where it runs", PLACES, place, me.clone(), |_, i, cx| { S.with(|s| s.borrow_mut().place = i); cx.notify(); }))
                    .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child(PLACES[place]))))
            .child(div().flex().items_center().gap(T::SP_6)
                .child(div().flex_1().min_w_0().child(w::input(&self.url)))
                .child(w::primary("open", "+ Open").on_click(cx.listener(|this, _, _, cx| open(&this.url.read(cx).value())))));
        let mut page = w::page("browsers-page").child(tools);
        if loaded && list.is_empty() { page = page.child(w::empty("No browser instances open")); }
        for (n, b) in list.iter().enumerate() {
            let key = kit::s(b, "key");
            let name = Some(kit::s(b, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| if key.is_empty() { "browser".into() } else { key.clone() });
            let place = match kit::s(b, "on").as_str() { "server" => "on the server", "desktop" => "on the desktop app", _ => "open" };
            let u = self.card_url(&key, &kit::s(b, "url"), window, cx);
            let sh = shots.get(&key).cloned();
            let (k1, k2, k3, k4) = (key.clone(), key.clone(), key.clone(), key.clone());
            let u2 = u.clone();
            let shot_box = div().id(("shot", n)).flex().items_center().justify_center().min_h(T::SP_48 + T::SP_8).rounded(T::R_CHIP).border_1().border_color(T::border()).bg(T::bg()).overflow_hidden().cursor_pointer()
                .on_click(move |_, _, _| shot(&k1));
            let shot_box = match sh {
                Some(Shot { state: "ok", image: Some(img), .. }) => shot_box.child(gpui::img(ImageSource::Render(img)).w_full().max_h(T::SHOT_H).object_fit(ObjectFit::Contain)),
                Some(Shot { state: "loading", .. }) => shot_box.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child("Loading...")),
                Some(s) => shot_box.child(div().text_size(T::TEXT_11).text_color(T::red()).child(s.error)),
                None => shot_box.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child("Click to take screenshot")),
            };
            page = page.child(w::card().p(T::SP_12).gap(T::SP_8)
                .child(div().flex().items_center().gap(T::SP_8)
                    .child(div().flex_1().min_w_0().truncate().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_14).text_color(T::text()).child(name))
                    .child(w::tag(place, "running")))
                .child(div().flex().items_center().gap(T::SP_4)
                    .child(div().flex_1().min_w_0().child(w::input(&u)))
                    .child(w::button(("go", n), "Go").on_click(move |_, _, cx| go(&k2, &u2.read(cx).value()))))
                .child(shot_box)
                .child(div().flex().gap(T::SP_6)
                    .child(w::button(("shot-btn", n), "Screenshot").icon(IconName::Image).on_click(move |_, _, _| shot(&k3)))
                    .child(w::danger(("close", n), "Close").icon(IconName::X).on_click(move |_, _, _| close(&k4)))));
        }
        page
    }
}
