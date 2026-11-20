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
    let url = crate::rules::browse_url(url);
    if url.is_empty() { return; }
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

impl PartialEq for Shot {
    fn eq(&self, o: &Self) -> bool {
        self.state == o.state && self.error == o.error && match (&self.image, &o.image) { (Some(a), Some(b)) => Arc::ptr_eq(a, b), (None, None) => true, _ => false }
    }
}

/// One browser's card as drawn (a copy).
#[derive(Clone, PartialEq)]
struct Card { key: String, name: String, place: &'static str, url: String, shot: Option<Shot> }

/// What the page draws (a copy, taken when the logic changed).
#[derive(Clone, PartialEq, Default)]
struct Copy { cards: Vec<Card>, loaded: bool, device: usize, place: usize, places: bool }

impl Copy {
    fn take() -> Copy {
        S.with(|s| {
            let s = s.borrow();
            let cards = s.list.iter().map(|b| {
                let key = kit::s(b, "key");
                Card {
                    name: Some(kit::s(b, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| if key.is_empty() { "browser".into() } else { key.clone() }),
                    place: match kit::s(b, "on").as_str() { "server" => "on the server", "desktop" => "on the desktop app", _ => "open" },
                    url: kit::s(b, "url"),
                    shot: s.shots.get(&key).cloned(),
                    key,
                }
            }).collect();
            Copy { cards, loaded: s.loaded, device: s.device, place: s.place, places: s.places }
        })
    }
}

/// The Browsers tab's root (layout: the shell places a tab's view uncached):
/// its one leaf, the page. It loads the list each time the tab comes on show.
pub struct BrowsersPage {
    page: Entity<BrowsersList>,
    on_show: bool,
    _sub: Subscription,
}

impl BrowsersPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let page = cx.new(|cx| BrowsersList::new(window, cx));
        let nav = crundi_shell::stores(cx).nav.clone();
        let sub = cx.observe(&nav, |this: &mut Self, nav, cx| {
            let on = nav.read(cx).tab == "browsers";
            if on && !this.on_show { load(); }
            this.on_show = on;
        });
        BrowsersPage { page, on_show: false, _sub: sub }
    }

    /// The logic changed (lib.rs `pump`): the page takes its copy.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page.update(cx, |p, cx| p.refresh(window, cx));
    }
}

impl Render for BrowsersPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.page.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// The page itself (a cached leaf): the device and place pickers, the URL
/// to open, and the browsers' cards, virtualised (only those on screen are
/// drawn; a card re-measured only when its copy changed).
struct BrowsersList {
    c: Copy,
    url: Entity<w::InputState>,
    /// Each card's address field, by the browser's key.
    urls: HashMap<String, Entity<w::InputState>>,
    list: ListState,
    _subs: Vec<Subscription>,
}

impl BrowsersList {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // (The web's field starts with http://localhost:3000.)
        let url = cx.new(|cx| { let mut s = w::InputState::new(window, cx).placeholder("URL to open"); s.set_value("http://localhost:3000", window, cx); s });
        let mut subs = vec![cx.subscribe_in(&url, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev { open(&st.read(cx).value()); })];
        let f = url.read(cx).focus_handle(cx);
        subs.extend(w::watch_focus(&f, window, cx));
        BrowsersList { c: Copy::default(), url, urls: HashMap::new(), list: ListState::new(0, ListAlignment::Top, T::SHOT_H * 2.), _subs: subs }
    }

    /// A new copy: the list reset when the browsers changed, else only the
    /// cards whose copy moved re-measured; a notify only when anything moved.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = Copy::take();
        if c == self.c { return; }
        let keys = |c: &Copy| c.cards.iter().map(|x| x.key.clone()).collect::<Vec<_>>();
        if keys(&c) != keys(&self.c) {
            self.list.reset(c.cards.len());
            self.urls.retain(|k, _| c.cards.iter().any(|x| &x.key == k));
        } else {
            for (i, (a, b)) in c.cards.iter().zip(self.c.cards.iter()).enumerate() { if a != b { self.list.splice(i..i + 1, 1); } }
        }
        // Each card's address field: made once, then following the list's URL while not typed in.
        for card in &c.cards {
            let old = self.c.cards.iter().find(|x| x.key == card.key).map(|x| x.url.clone());
            match self.urls.get(&card.key) {
                Some(e) => if old.as_deref() != Some(card.url.as_str()) && !e.read(cx).focus_handle(cx).is_focused(window) { let u = card.url.clone(); e.update(cx, |s, cx| s.set_value(u, window, cx)); },
                None => {
                    let e = cx.new(|cx| { let mut s = w::InputState::new(window, cx).placeholder("URL"); s.set_value(card.url.clone(), window, cx); s });
                    let k = card.key.clone();
                    self._subs.push(cx.subscribe_in(&e, window, move |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev { go(&k, &st.read(cx).value()); }));
                    self.urls.insert(card.key.clone(), e);
                }
            }
        }
        self.c = c;
        cx.notify();
    }

    fn card(&mut self, n: usize, _: &mut Window, _: &mut Context<Self>) -> AnyElement {
        let Some(b) = self.c.cards.get(n).cloned() else { return div().into_any_element() };
        let Some(u) = self.urls.get(&b.key).cloned() else { return div().into_any_element() };
        let (k1, k2, k3, k4) = (b.key.clone(), b.key.clone(), b.key.clone(), b.key.clone());
        let u2 = u.clone();
        let shot_box = div().id(("shot", n)).flex().items_center().justify_center().min_h(T::SP_48 + T::SP_8).rounded(T::R_CHIP).border_1().border_color(T::border()).bg(T::bg()).overflow_hidden().cursor_pointer()
            .on_click(move |_, _, _| shot(&k1));
        let shot_box = match b.shot {
            Some(Shot { state: "ok", image: Some(img), .. }) => shot_box.child(gpui::img(ImageSource::Render(img)).w_full().max_h(T::SHOT_H).object_fit(ObjectFit::Contain)),
            Some(Shot { state: "loading", .. }) => shot_box.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child("Loading...")),
            Some(s) => shot_box.child(div().text_size(T::TEXT_11).text_color(T::red()).child(s.error)),
            None => shot_box.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child("Click to take screenshot")),
        };
        div().pb(T::SP_12).child(w::card().p(T::SP_12).gap(T::SP_8)
            .child(div().flex().items_center().gap(T::SP_8)
                .child(div().flex_1().min_w_0().truncate().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_14).text_color(T::text()).child(b.name))
                .child(w::tag(b.place, "running")))
            .child(div().flex().items_center().gap(T::SP_4)
                .child(div().flex_1().min_w_0().child(w::input(&u)))
                .child(w::button(("go", n), "Go").on_click(move |_, _, cx| go(&k2, &u2.read(cx).value()))))
            .child(shot_box)
            .child(div().flex().gap(T::SP_6)
                .child(w::button(("shot-btn", n), "Screenshot").icon(IconName::Image).on_click(move |_, _, _| shot(&k3)))
                .child(w::danger(("close", n), "Close").icon(IconName::X).on_click(move |_, _, _| close(&k4)))))
            .into_any_element()
    }
}

impl Render for BrowsersList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("browsers");
        let (device, place, places) = (self.c.device, self.c.place, self.c.places);
        let me = cx.entity().downgrade();
        let tools = div().flex().flex_col().gap(T::SP_6)
            .child(div().flex().items_center().gap(T::SP_6)
                .child(w::menu_button("dev", IconName::Smartphone, "Device", labels(), device, me.clone(), |_, i, _| { S.with(|s| s.borrow_mut().device = i); kit::dirty(); }))
                .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).text_color(T::text()).child(labels()[device]))
                .when(places, |d| d
                    .child(w::menu_button("place", IconName::Server, "Where it runs", PLACES, place, me.clone(), |_, i, _| { S.with(|s| s.borrow_mut().place = i); kit::dirty(); }))
                    .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child(PLACES[place]))))
            .child(div().flex().items_center().gap(T::SP_6)
                .child(div().flex_1().min_w_0().child(w::input(&self.url)))
                .child(w::primary("open", "+ Open").on_click(cx.listener(|this, _, _, cx| open(&this.url.read(cx).value())))));
        let body: AnyElement = if self.c.loaded && self.c.cards.is_empty() {
            w::empty("No browser instances open").into_any_element()
        } else {
            list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.card(ix, window, cx))).flex_1().min_h_0().into_any_element()
        };
        // In one layer unless a text box has the focus (its caret).
        let typing = std::iter::once(&self.url).chain(self.urls.values()).any(|e| e.read(cx).focus_handle(cx).is_focused(window));
        let root = div().flex().flex_col().size_full().p(T::SP_16).gap(T::SP_12).bg(T::bg()).child(tools).child(body);
        if typing { root.into_any_element() } else { w::layer(root).into_any_element() }
    }
}
