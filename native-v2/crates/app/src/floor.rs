//! The frame floor (perf/README.md, "The floor"): what GPUI costs per frame
//! on this machine, apart from the client's own screens. One window, no
//! server, no sign-in; the same test driver (its `mouse` wheel, `state`
//! with the renders, `resize`, `snapshot`) as the client, so tools/measure-floor.mjs
//! sweeps it exactly as tools/measure-pc.mjs sweeps the client.
//!
//!   crundi-v2-floor <mode>      (CRUNDI_TEST_PORT=<port>, ZED_MEASUREMENTS=1)
//!
//! Modes, from the bare floor up to the client's own chat panes:
//!   empty     one view of static text, re-rendered on every wheel event:
//!             GPUI's frame with (almost) nothing in it
//!   list      gpui::list of 2000 fixed one-line text rows, the whole window
//!   uniform   the same rows in gpui::uniform_list
//!   workbench the real Workbench (crates/workbench) with five panes, each
//!             body the plain `list` of fixed rows: an empty Workbench
//!             around the floor's list
//!   chatlog   the real Workbench with one chat pane (crates/chat's LogView
//!             and Composer), its history seeded here, no server
//!   chatlog5  five chat panes, seeded alike (two 300-item lists, three
//!             long mixed-markdown replies): the PC measurement's scene
<<<<<<< ours
=======
//!   stream5   chatlog5 while the first chat streams a reply: a ui-event
//!             "entry" with the reply's text grown every 33 ms (as the
//!             server sends them), a new reply when one is done
//!
//! The driver is the client's (driver::handle: mouse, key, state with the
//! chat's own, the chat's `ui` actions such as log-texts, snapshot).
>>>>>>> theirs
//!
//! A perf tool: it never opens a session, never reads the owner's config
//! (it refuses to run without a fresh CRUNDI_CONFIG_DIR, where the
//! Workbench saves its layout) and never redraws the whole window.

#[path = "driver.rs"]
#[allow(dead_code)]
mod driver;

use crundi_theme::T;
use futures::StreamExt as _;
use gpui::*;
use gpui_component::Root;
use serde_json::json;
use std::ops::Range;

const ROWS: usize = 2000;

/// The plain rows: fixed text, one line each, the chat's size and spacing.
struct Rows { text: Vec<SharedString>, list: ListState, scroll: UniformListScrollHandle, uniform: bool }

impl Rows {
    fn new(uniform: bool) -> Self {
        let text = (0..ROWS).map(|i| SharedString::from(format!("Row {i}: the quick brown fox jumps over the lazy dog, {} times.", i * 7 % 300))).collect();
        Rows { text, list: ListState::new(ROWS, ListAlignment::Top, T::LOG_OVERDRAW), scroll: UniformListScrollHandle::new(), uniform }
    }
}

fn row(t: &SharedString) -> AnyElement {
    div().w_full().px(T::SP_16).py(px(2.)).text_size(T::TEXT_13).line_height(relative(T::LH_CHAT)).text_color(T::text()).child(t.clone()).into_any_element()
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("rows");
        let body = if self.uniform {
            uniform_list("rows", ROWS, cx.processor(|this, r: Range<usize>, _, _| r.map(|i| row(&this.text[i])).collect::<Vec<_>>()))
                .track_scroll(&self.scroll).size_full().into_any_element()
        } else {
            list(self.list.clone(), cx.processor(|this, i: usize, _, _| row(&this.text[i]))).size_full().into_any_element()
        };
        div().size_full().bg(T::bg()).child(body)
    }
}

/// The emptiest frame: static text, re-rendered on every wheel event.
struct Empty;
impl Render for Empty {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("empty");
        div().id("empty").size_full().bg(T::bg()).text_color(T::text()).p(T::SP_16).child("The frame floor")
            .on_scroll(cx.listener(|_, _: &ScrollEvent, _, cx| cx.notify()))
    }
}

// ─── The seeded chats ───

fn words(n: usize) -> String {
    const ONES: [&str; 20] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
    const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    let mut out = String::new();
    let (h, r) = (n / 100, n % 100);
    if h > 0 { out.push_str(ONES[h]); out.push_str(" hundred"); if r > 0 { out.push_str(" and "); } }
    if r >= 20 { out.push_str(TENS[r / 10]); if r % 10 > 0 { out.push('-'); out.push_str(ONES[r % 10]); } } else if r > 0 || h == 0 { out.push_str(ONES[r]); }
    out
}

fn number_list() -> String { (1..=300).map(|i| format!("- {}", words(i))).collect::<Vec<_>>().join("\n") }

fn article(seed: usize) -> String {
    let mut s = String::new();
    for k in 0..8 {
        s.push_str(&format!("## Section {} of the reply {seed}\n\n", k + 1));
        s.push_str("A hash map stores **values** by *keys*: it hashes the key, picks a bucket from the hash and keeps the entry there, so a lookup costs `O(1)` on average. When two keys land in one bucket, the map probes for the next free slot or keeps a short chain. See [the docs](https://doc.rust-lang.org/std/collections/struct.HashMap.html) for details.\n\n");
        s.push_str("- the hash function spreads keys evenly\n- the load factor decides when to grow\n- growing rehashes every entry into a larger table\n- removal leaves a tombstone in open addressing\n\n");
        s.push_str("1. hash the key\n2. find the bucket\n3. compare the stored keys\n\n");
        s.push_str("| Operation | Average | Worst |\n|---|---|---|\n| insert | O(1) | O(n) |\n| lookup | O(1) | O(n) |\n| remove | O(1) | O(n) |\n\n");
        s.push_str("```rust\nuse std::collections::HashMap;\nlet mut m = HashMap::new();\nm.insert(\"key\", 42);\nassert_eq!(m.get(\"key\"), Some(&42));\n```\n\n");
        s.push_str("> Premature optimisation is the root of all evil, but a good hash function is not premature.\n\n---\n\n");
    }
    s
}

fn seed_chat(cx: &mut App, n: usize) -> String {
    let id = format!("floor-{n}");
    let text = if n % 2 == 0 { number_list() } else { article(n) };
    let history = json!({ "type": "ui-history", "id": id, "session": { "id": id, "state": "idle", "messages": [
        { "id": format!("u{n}"), "kind": "user", "text": "Write the numbers from one to three hundred as words, one per line, as a markdown list." },
        { "id": format!("a{n}"), "kind": "assistant-text", "text": text },
    ] } });
    let s = crundi_chat::store(cx, &id);
    s.update(cx, |s, cx| s.apply_batch(&[history], cx));
    id
}

// ─── The window ───

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn,zbus=off")).init();
    let mode = std::env::args().nth(1).unwrap_or_else(|| "list".into());
<<<<<<< ours
    if !["empty", "list", "uniform", "workbench", "chatlog", "chatlog5"].contains(&mode.as_str()) {
        eprintln!("usage: crundi-v2-floor empty|list|uniform|workbench|chatlog|chatlog5");
=======
    if !["empty", "list", "uniform", "workbench", "chatlog", "chatlog5", "stream5"].contains(&mode.as_str()) {
        eprintln!("usage: crundi-v2-floor empty|list|uniform|workbench|chatlog|chatlog5|stream5");
>>>>>>> theirs
        std::process::exit(2);
    }
    // The Workbench saves its layout in the config folder: a fresh one only.
    match std::env::var("CRUNDI_CONFIG_DIR") {
        Ok(d) if std::fs::read_dir(&d).map(|mut r| r.next().is_none()).unwrap_or(true) => {}
        _ => { eprintln!("Set CRUNDI_CONFIG_DIR to a fresh, empty folder (mktemp -d)."); std::process::exit(2); }
    }
    gpui_platform::application().with_assets(crundi_theme::AllAssets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        crundi_core::syntax::warm();
        crundi_theme::init(cx);
        let stores = crundi_shell::init(cx);
        crundi_workbench::init(cx);
        crundi_chat::init(cx);
        if mode == "workbench" {
            crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
                prefix: "floor:",
                chat: false,
                make: std::rc::Rc::new(|key, _, _, cx| {
                    let v = cx.new(|_| Rows::new(false));
                    let title: SharedString = key.to_string().into();
                    crundi_workbench::PaneParts {
                        body: std::rc::Rc::new(move |_| v.clone().cached(StyleRefinement::default().size_full()).into_any_element()),
                        info: std::rc::Rc::new(move |_| crundi_workbench::HeadInfo { title: title.to_string(), long: String::new(), state: String::new() }),
                        watch: Box::new(|_| vec![]),
                    }
                }),
                close: None,
                zoom: None,
            });
        }
        let wsize = size(px(1920.), px(1040.));
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.), px(0.)), size: wsize })),
            titlebar: None,
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some("crundi-floor".into()),
            ..Default::default()
        };
        let m = mode.clone();
        let s = stores.clone();
        let handle = cx.open_window(opts, move |window, cx| {
            let view: AnyView = match m.as_str() {
                "empty" => cx.new(|_| Empty).into(),
                "list" => cx.new(|_| Rows::new(false)).into(),
                "uniform" => cx.new(|_| Rows::new(true)).into(),
                _ => {
                    let wb = cx.new(|cx| crundi_workbench::Workbench::new(s.clone(), cx));
                    let keys: Vec<String> = match m.as_str() {
                        "workbench" => (1..=5).map(|i| format!("floor:{i}")).collect(),
                        "chatlog" => vec![format!("live:{}", seed_chat(cx, 1))],
                        _ => (1..=5).map(|i| format!("live:{}", seed_chat(cx, i))).collect(),
                    };
                    wb.update(cx, |w, cx| for k in &keys { w.place(k, cx); });
                    let _ = window;
                    wb.into()
                }
            };
            cx.new(|cx| Root::new(view, window, cx))
        }).expect("window");

<<<<<<< ours
=======
        if mode == "stream5" {
            // The first chat streams: its reply grows by about 40 bytes
            // every 33 ms (a fast reply), one store update per event batch.
            cx.spawn(async move |cx: &mut AsyncApp| {
                let full = article(9);
                let mut n = 0usize;
                loop {
                    n += 1;
                    let mut at = 0;
                    while at < full.len() {
                        cx.background_executor().timer(std::time::Duration::from_millis(33)).await;
                        at = (at + 40).min(full.len());
                        while !full.is_char_boundary(at) { at += 1; }
                        let done = at == full.len();
                        let ev = json!({ "type": "ui-event", "id": "floor-1", "event": { "type": "entry", "entry": {
                            "id": format!("s{n}"), "kind": "assistant-text", "text": &full[..at], "status": if done { "done" } else { "streaming" } } } });
                        cx.update(|cx| { if let Some(st) = crundi_chat::find(cx, "floor-1") { st.update(cx, |st, cx| st.apply_batch(&[ev], cx)); } });
                    }
                }
            }).detach();
        }
>>>>>>> theirs
        if let Some(port) = crundi_core::driver::port() {
            let (tx, mut rx) = futures::channel::mpsc::unbounded::<crundi_core::driver::Request>();
            crundi_core::driver::start(port, move |r| { let _ = tx.unbounded_send(r); });
            let any: AnyWindowHandle = handle.into();
<<<<<<< ours
            cx.spawn(async move |cx: &mut AsyncApp| {
                while let Some(r) = rx.next().await {
                    if cx.update_window(any, move |_, window, cx| {
                        let v = match r.name() {
                            "mouse" => { driver::mouse(&r, window, cx); json!({ "ok": true }) }
                            "snapshot" => driver::snapshot(window, &r.str("path")),
                            "resize" => { window.resize(size(px(r.num("w") as f32), px(r.num("h") as f32))); json!({ "ok": true }) }
                            "state" => json!({ "ok": true, "state": { "renders": crundi_core::driver::renders() } }),
                            other => json!({ "ok": false, "error": format!("unknown cmd {other}") }),
                        };
                        r.answer(v);
                    }).is_err() { break; }
=======
            let s = stores.clone();
            cx.spawn(async move |cx: &mut AsyncApp| {
                while let Some(r) = rx.next().await {
                    let s = s.clone();
                    if cx.update_window(any, move |_, window, cx| driver::handle(r, &s, window, cx)).is_err() { break; }
>>>>>>> theirs
                }
            }).detach();
        }
    });
}
