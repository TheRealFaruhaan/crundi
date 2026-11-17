//! Media Claude shows (2.16m, specs/log-rest.md section 4): the items of a
//! show_image / show_video / show_audio / show_file / show_embed call's
//! result, their boxes' sizes (known before anything loads), and the words
//! the web writes (sizes, durations, captions). Pure.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Item {
    /// image | video | audio | file | embed
    pub kind: String,
    /// The stored name (pictures, video, audio, stored HTML), or "".
    pub name: String,
    /// A video's poster frame's stored name.
    pub poster: String,
    /// A file's id.
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub caption: String,
    pub w: f64,
    pub h: f64,
    pub duration: f64,
    /// An https frame: its page, the page to open, the site.
    pub src: String,
    pub url: String,
    pub provider: String,
    pub title: String,
    /// A frame's asked size (without w/h).
    pub width: f64,
    pub height: f64,
}

fn st(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn num(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }

fn hex16(s: &str) -> bool { s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) }

/// A stored media name the client may ask for (16 hex, ".poster" maybe,
/// a known extension).
pub fn good_name(n: &str) -> bool {
    let Some((stem, ext)) = n.split_once('.') else { return false };
    let (id, rest) = match stem.len() { 16 => (stem, ext.to_string()), _ => return false };
    if !hex16(id) { return false; }
    let ext = rest.strip_prefix("poster.").unwrap_or(&rest);
    ["png", "jpg", "gif", "webp", "mp4", "webm", "mov", "mp3", "wav", "ogg", "m4a", "flac", "weba"].contains(&ext)
}

/// Is this tool's name one of the show_* tools (any MCP prefix)?
pub fn is_show(name: &str) -> bool {
    ["show_image", "show_video", "show_audio", "show_file", "show_embed"].iter().any(|t| name == *t || name.ends_with(&format!("__{t}")))
}

/// A call's result (JSON text) → its items and the call's caption. Names
/// that do not fit the rules are dropped; the 1.19.19 `images` is read too.
pub fn items(result: &str) -> (Vec<Item>, String) {
    let Ok(v) = serde_json::from_str::<Value>(result) else { return (vec![], String::new()) };
    let caption = st(&v, "caption");
    let raw: Vec<Value> = v.get("items").and_then(|x| x.as_array()).cloned()
        .or_else(|| v.get("images").and_then(|x| x.as_array()).map(|a| a.iter().map(|i| { let mut o = i.clone(); o["kind"] = "image".into(); o }).collect()))
        .unwrap_or_default();
    let out = raw.iter().filter_map(|i| {
        let kind = st(i, "kind");
        let it = Item {
            kind: kind.clone(), name: st(i, "name"), poster: st(i, "poster"), id: st(i, "id"), filename: st(i, "filename"),
            size: num(i, "size").max(0.0) as u64, caption: st(i, "caption"), w: num(i, "w"), h: num(i, "h"), duration: num(i, "duration"),
            src: st(i, "src"), url: st(i, "url"), provider: st(i, "provider"), title: st(i, "title"), width: num(i, "width"), height: num(i, "height"),
        };
        let ok = match kind.as_str() {
            "image" | "video" | "audio" => good_name(&it.name),
            "file" => hex16(&it.id),
            "embed" => (it.name.len() == 22 && it.name.ends_with(".embed") && hex16(&it.name[..16])) || it.src.starts_with("https://") || loopback(&it.src),
            _ => false,
        };
        let it = if it.kind == "video" && !good_name(&it.poster) { Item { poster: String::new(), ..it } } else { it };
        ok.then_some(it)
    }).collect();
    (out, caption)
}

/// A picture's or video's box: one item (min(w, 420·w/h, 640), never taller
/// than 420), or one of several (220 tall); unknown size: 16:9.
pub fn box_size(w: f64, h: f64, several: bool) -> (f64, f64) {
    if w <= 0.0 || h <= 0.0 {
        let bw = if several { 391.0 } else { 420.0 };
        return (bw, (bw * 9.0 / 16.0).round());
    }
    if several { return ((220.0 * w / h).round(), 220.0); }
    let bw = w.min(420.0 * w / h).min(640.0);
    (bw.round(), (bw * h / w).round())
}

/// A frame's box: a known player's w:h (640 wide when w ≥ h, else 560·w/h
/// tall-sized); else width (640) × height (420).
pub fn frame_size(it: &Item) -> (f64, f64) {
    if it.w > 0.0 && it.h > 0.0 {
        let bw = if it.w >= it.h { 640.0 } else { (560.0 * it.w / it.h).round() };
        return (bw, (bw * it.h / it.w).round());
    }
    (if it.width > 0.0 { it.width } else { 640.0 }, if it.height > 0.0 { it.height } else { 420.0 })
}

/// "12 B", "26 KB", "1.5 MB", "12 MB" (1024 steps; one decimal under 10).
pub fn fmt_bytes(n: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 { v /= 1024.0; u += 1; }
    if u == 0 { format!("{n} B") } else if v < 10.0 { format!("{:.1} {}", v, units[u]).replace(".0 ", " ") } else { format!("{} {}", v.round() as u64, units[u]) }
}

/// The download row's sizes: "12 B", "26 KB" (KB rounded), "1.5 MB" (MB one decimal).
pub fn fmt_dl_size(n: u64) -> String {
    if n < 1024 { format!("{n} B") } else if n < 1024 * 1024 { format!("{} KB", (n as f64 / 1024.0).round() as u64) } else { format!("{:.1} MB", n as f64 / 1048576.0) }
}

/// "0:06", "1:15" (rounded seconds, minutes not padded).
pub fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The caption under an item: an image's caption; a video's or audio's
/// caption (or file name) · duration.
pub fn item_caption(it: &Item) -> String {
    match it.kind.as_str() {
        "video" | "audio" => {
            let base = if it.caption.is_empty() { it.filename.clone() } else { it.caption.clone() };
            if it.duration > 0.0 { if base.is_empty() { fmt_duration(it.duration) } else { format!("{base} · {}", fmt_duration(it.duration)) } } else { base }
        }
        _ => it.caption.clone(),
    }
}

/// A frame's caption and its Open link: "title · caption" (or either), else
/// the provider, else "Embedded HTML"; the link "Open on <provider>" when
/// the caption is not just the provider, else "Open" (none without a url).
pub fn frame_caption(it: &Item) -> (String, String) {
    let parts: Vec<&str> = [it.title.as_str(), it.caption.as_str()].into_iter().filter(|x| !x.is_empty()).collect();
    let cap = if !parts.is_empty() { parts.join(" · ") } else if !it.provider.is_empty() { it.provider.clone() } else { "Embedded HTML".into() };
    let link = if it.url.is_empty() { String::new() } else if !it.provider.is_empty() && cap != it.provider { format!("Open on {}", it.provider) } else { "Open".into() };
    (cap, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_items() {
        assert!(good_name("0123456789abcdef.png") && good_name("0123456789abcdef.poster.jpg") && !good_name("0123456789ABCDEF.png") && !good_name("../x.png") && !good_name("0123456789abcdef.exe"));
        assert!(is_show("mcp__crundi__show_image") && is_show("show_file") && !is_show("showcase"));
        let (it, cap) = items(r#"{"ok":true,"items":[{"kind":"image","name":"0123456789abcdef.png","w":1200,"h":700,"caption":"A"},{"kind":"image","name":"bad.png"},{"kind":"file","id":"0123456789abcdef","filename":"export.csv","size":12}],"caption":"Both"}"#);
        assert_eq!(it.len(), 2);
        assert_eq!(cap, "Both");
        assert_eq!(it[1].filename, "export.csv");
        let (legacy, _) = items(r#"{"images":[{"name":"0123456789abcdef.webp","caption":"x"}]}"#);
        assert_eq!(legacy[0].kind, "image");
        assert!(items("not json").0.is_empty());
    }

    #[test]
    fn sizes() {
        assert_eq!(box_size(1200.0, 700.0, false), (640.0, 373.0));
        assert_eq!(box_size(600.0, 900.0, false), (280.0, 420.0));
        assert_eq!(box_size(500.0, 500.0, true), (220.0, 220.0));
        assert_eq!(box_size(0.0, 0.0, false), (420.0, 236.0));
        assert_eq!(box_size(0.0, 0.0, true), (391.0, 220.0));
        let yt = Item { w: 16.0, h: 9.0, ..Default::default() };
        assert_eq!(frame_size(&yt), (640.0, 360.0));
        let tall = Item { w: 9.0, h: 16.0, ..Default::default() };
        assert_eq!(frame_size(&tall), (315.0, 560.0));
        assert_eq!(frame_size(&Item { height: 160.0, ..Default::default() }), (640.0, 160.0));
    }

    #[test]
    fn words() {
        assert_eq!(fmt_bytes(12), "12 B");
        assert_eq!(fmt_bytes(26 * 1024), "26 KB");
        assert_eq!(fmt_bytes(1536 * 1024), "1.5 MB");
        assert_eq!(fmt_bytes(12 * 1024 * 1024), "12 MB");
        assert_eq!(fmt_dl_size(12), "12 B");
        assert_eq!(fmt_dl_size(26 * 1024 + 300), "26 KB");
        assert_eq!(fmt_dl_size(1536 * 1024), "1.5 MB");
        assert_eq!(fmt_duration(6.2), "0:06");
        assert_eq!(fmt_duration(75.0), "1:15");
        let v = Item { kind: "video".into(), caption: "The test pattern".into(), duration: 6.0, ..Default::default() };
        assert_eq!(item_caption(&v), "The test pattern · 0:06");
        let a = Item { kind: "audio".into(), filename: "tone.mp3".into(), duration: 8.0, ..Default::default() };
        assert_eq!(item_caption(&a), "tone.mp3 · 0:08");
        let yt = Item { title: "The talk".into(), caption: "from 2:10".into(), provider: "YouTube".into(), url: "https://y".into(), ..Default::default() };
        assert_eq!(frame_caption(&yt), ("The talk · from 2:10".into(), "Open on YouTube".into()));
        let pg = Item { provider: "example.com".into(), url: "https://e".into(), title: "A plain page".into(), ..Default::default() };
        assert_eq!(frame_caption(&pg).1, "Open on example.com");
        let only = Item { provider: "example.com".into(), url: "https://e".into(), ..Default::default() };
        assert_eq!(frame_caption(&only), ("example.com".into(), "Open".into()));
        assert_eq!(frame_caption(&Item { title: "Counter".into(), ..Default::default() }), ("Counter".into(), String::new()));
    }
}

/// A page on this machine (the frame's src): localhost or 127.0.0.1.
pub fn loopback(u: &str) -> bool {
    ["http://localhost:", "http://localhost/", "http://127.0.0.1:", "http://127.0.0.1/"].iter().any(|p| u.starts_with(p))
}
