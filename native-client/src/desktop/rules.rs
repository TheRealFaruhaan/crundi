//! The drawer's pure rules (app/vendor/crundi-browser-drawer.js,
//! app/browser-drawer.js), unit-tested: what the address field opens, a
//! tab's id, the history's ranking, a site for "Only this site", the text a
//! chat gets for the marks, the network labels.

use serde_json::Value;

/// The address field's Enter (VD go()): a URL as typed, a host made a URL
/// (http for this machine, https else), or a Google search.
pub fn url_of(typed: &str) -> String {
    let t = typed.trim();
    if t.is_empty() { return String::new(); }
    let lower = t.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower == "about:blank" { return t.to_string(); }
    let hostish = !t.contains(' ') && (t[1..].contains('.') || lower.starts_with("localhost") || is_ip_port(t));
    if hostish {
        let local = lower.starts_with("localhost") || lower.starts_with("127.") || lower.starts_with("[::1]");
        return format!("{}://{}", if local { "http" } else { "https" }, t);
    }
    format!("https://www.google.com/search?q={}", crate::kit::enc(t))
}

fn is_ip_port(t: &str) -> bool {
    let host = t.split('/').next().unwrap_or("");
    let (h, port) = match host.rsplit_once(':') { Some((h, p)) => (h, p), None => return false };
    !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && !h.is_empty() && h.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Only http(s) and about:blank open in the drawer (BD okUrl).
pub fn ok_url(u: &str) -> bool {
    let l = u.to_lowercase();
    l.starts_with("http://") || l.starts_with("https://") || l == "about:blank"
}

/// A new tab's id: time in base 36 and six random base-36 characters (VD uid).
pub fn tab_id(now_ms: u64, rand: u64) -> String {
    let mut r = rand;
    let mut tail = String::new();
    for _ in 0..6 { tail.push(std::char::from_digit((r % 36) as u32, 36).unwrap_or('0')); r /= 36; }
    format!("{}{}", base36(now_ms), tail)
}

fn base36(mut n: u64) -> String {
    if n == 0 { return "0".into(); }
    let mut v = Vec::new();
    while n > 0 { v.push(std::char::from_digit((n % 36) as u32, 36).unwrap_or('0')); n /= 36; }
    v.iter().rev().collect()
}

/// A tab id a request may name (BD TAB_ID).
pub fn valid_tab_id(id: &str) -> bool {
    (4..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A history entry (BD recordVisit): newest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Visit { pub url: String, pub title: String, pub at: u64, pub visits: u64 }

pub const HISTORY_MAX: usize = 5000;

/// A visit: only http(s), one entry per address, moved to the front.
pub fn record(history: &mut Vec<Visit>, url: &str, title: &str, now: u64) -> bool {
    let l = url.to_lowercase();
    if !(l.starts_with("http://") || l.starts_with("https://")) { return false; }
    let old = history.iter().position(|h| h.url == url).map(|i| history.remove(i));
    let title = if title.is_empty() { old.as_ref().map(|o| o.title.clone()).unwrap_or_default() } else { title.to_string() };
    history.insert(0, Visit { url: url.to_string(), title, at: now, visits: old.map(|o| o.visits).unwrap_or(0) + 1 });
    history.truncate(HISTORY_MAX);
    true
}

/// The history matching every word of [q] in its address or title (BD query).
pub fn query(history: &[Visit], q: &str, limit: usize) -> Vec<Visit> {
    let words: Vec<String> = q.to_lowercase().split_whitespace().map(str::to_string).collect();
    history.iter().filter(|h| {
        let hay = format!("{} {}", h.url, h.title).to_lowercase();
        words.iter().all(|w| hay.contains(w))
    }).take(limit.clamp(1, 200)).cloned().collect()
}

/// How long ago, as the History panel writes it: now, N min, N h, a date.
pub fn ago(now_ms: u64, at_ms: u64) -> String {
    let s = now_ms.saturating_sub(at_ms) / 1000;
    if s < 60 { return "now".into(); }
    if s < 3600 { return format!("{} min", s / 60); }
    if s < 86400 { return format!("{} h", s / 3600); }
    let d = chrono::DateTime::from_timestamp_millis(at_ms as i64).map(|d| d.with_timezone(&chrono::Local).format("%-d %b %Y").to_string());
    d.unwrap_or_default()
}

/// "Only this site": lower case, no scheme, path, port or leading dots; a
/// host with a dot, or localhost (VD normSite). None when it is not a site.
pub fn norm_site(s: &str) -> Option<String> {
    let mut t = s.trim().to_lowercase();
    if let Some(i) = t.find("://") { t = t[i + 3..].to_string(); }
    let t = t.split(['/', '?', '#']).next().unwrap_or("");
    let t = t.rsplit_once(':').map(|(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { t }).unwrap_or(t);
    let t = t.trim_start_matches('.');
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') { return None; }
    if !t.contains('.') && t != "localhost" { return None; }
    Some(t.to_string())
}

/// A page's host ("" when none).
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or("");
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    h.rsplit_once('@').map(|(_, h)| h).unwrap_or(h).to_string()
}

/// A page's origin (scheme://host[:port]), http(s) only.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let s = scheme.to_lowercase();
    if s != "http" && s != "https" { return None; }
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    if h.is_empty() { return None; }
    Some(format!("{s}://{}", h.to_lowercase()))
}

/// The count line of the annotation bar (VD renderAnn).
pub fn count_text(mine: usize, theirs: usize) -> String {
    if mine + theirs == 0 { return "Select an element, or draw".into(); }
    format!("{mine} yours{}", if theirs > 0 { format!(", {theirs} from Claude") } else { String::new() })
}

/// The text a chat gets for the marks (VD sendToChat): [marks] are the
/// person's, as ann_json makes them (renumbered here from 1).
pub fn marks_text(url: &str, title: &str, vw: i32, vh: i32, server_net: bool, tab_id: &str, with_shot: bool, marks: &[Value]) -> String {
    let mut lines = vec![format!(
        "Browser: {url}{}, viewport {vw}x{vh}{} [drawer tab key: drawer:{tab_id}]{}",
        if title.is_empty() { String::new() } else { format!(" ({title})") },
        if server_net { ", server network" } else { "" },
        if with_shot { ", screenshot attached with the marks numbered" } else { "" },
    )];
    let f = |a: &Value, k: &str| a.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0).round() as i64;
    for (i, a) in marks.iter().enumerate() {
        let n = i + 1;
        let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let text = a.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let note = if text.is_empty() { String::new() } else { format!(": {text}") };
        if let Some(e) = a.get("picked") {
            lines.push(format!("{n}. {}{note}", if kind == "element" { "element" } else { kind }));
            lines.push(format!("   selector: {}", e.get("selector").and_then(|v| v.as_str()).unwrap_or("")));
            let tag = e.get("tag").and_then(|v| v.as_str()).unwrap_or("");
            if !tag.is_empty() {
                let t = e.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let t: String = t.chars().take(100).collect();
                lines.push(format!("   element: <{tag}>{}", if t.is_empty() { String::new() } else { format!(" \"{t}\"") }));
            }
            if let Some(h) = e.get("html").and_then(|v| v.as_str()).filter(|h| !h.is_empty()) {
                lines.push(format!("   html: {}", one_line_html(h)));
            }
        } else {
            let anchor = a.get("anchor").and_then(|v| v.as_array()).map(|v| (v.first().and_then(|x| x.as_f64()).unwrap_or(0.0).round() as i64, v.get(1).and_then(|x| x.as_f64()).unwrap_or(0.0).round() as i64)).unwrap_or((0, 0));
            let wher = match kind {
                "box" => format!("box at {},{} size {}x{}", f(a, "x"), f(a, "y"), f(a, "width"), f(a, "height")),
                "pen" => format!("drawing around {},{}", anchor.0, anchor.1),
                k => format!("{k} at {},{}", anchor.0, anchor.1),
            };
            lines.push(format!("{n}. {wher} (see the screenshot){note}"));
        }
    }
    if marks.is_empty() { lines.push("(no marks: the page as it is)".into()); }
    lines.join("\n")
}

/// An element's opening tag on one line, cut at a tag's end near 200
/// characters (marked " …").
pub fn one_line_html(h: &str) -> String {
    let h = h.split_whitespace().collect::<Vec<_>>().join(" ");
    if h.chars().count() <= 200 { return h; }
    let head: String = h.chars().take(200).collect();
    let cut = head.rfind('>').filter(|&c| c > 80).map(|c| c + 1).unwrap_or(head.len());
    format!("{} \u{2026}", &head[..cut])
}

/// The network's label (BD networkLabel): mode "local"/"remote", status
/// "direct"/"tunnel"/"offline"/"unsupported".
pub fn network_label(network: &str, mode: &str, status: &str, hp_name: Option<&str>) -> String {
    if let Some(n) = network.strip_prefix("hp:") { return format!("Homepool: {}", hp_name.unwrap_or(n)); }
    if network != "server" { return "This PC".into(); }
    if mode == "local" { return "Server (this PC)".into(); }
    match status {
        "tunnel" => "Server".into(),
        "unsupported" => "Server (needs a newer server)".into(),
        _ => "Server (not connected)".into(),
    }
}

/// A network a request may name: "client", "server" or "homepool:<net>" /
/// "hp:<net>" (BD normNetwork). None: refused.
pub fn norm_network(n: &str) -> Option<String> {
    match n {
        "" | "client" => Some("client".into()),
        "server" => Some("server".into()),
        _ => {
            let id = n.strip_prefix("homepool:").or_else(|| n.strip_prefix("hp:"))?;
            let ok = !id.is_empty() && id.len() <= 40 && id.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
            ok.then(|| format!("hp:{id}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn urls() {
        assert_eq!(url_of("https://a.com/x"), "https://a.com/x");
        assert_eq!(url_of("example.com"), "https://example.com");
        assert_eq!(url_of("localhost:3000"), "http://localhost:3000");
        assert_eq!(url_of("127.0.0.1:8080/x"), "http://127.0.0.1:8080/x");
        assert_eq!(url_of("10.0.0.2:80"), "https://10.0.0.2:80");
        assert_eq!(url_of("cats and dogs"), "https://www.google.com/search?q=cats%20and%20dogs");
        assert_eq!(url_of("about:blank"), "about:blank");
        assert!(ok_url("HTTP://x") && !ok_url("file:///etc/passwd") && !ok_url("javascript:1"));
    }

    #[test]
    fn ids() {
        let id = tab_id(1_700_000_000_000, 123456789);
        assert!(valid_tab_id(&id), "{id}");
        assert!(!valid_tab_id("ab") && !valid_tab_id("a b c d"));
    }

    #[test]
    fn history() {
        let mut h = vec![];
        assert!(!record(&mut h, "about:blank", "", 1));
        record(&mut h, "https://a.com/", "A", 1);
        record(&mut h, "https://b.com/", "Bee", 2);
        record(&mut h, "https://a.com/", "", 3);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].url, "https://a.com/");
        assert_eq!(h[0].title, "A");
        assert_eq!(h[0].visits, 2);
        assert_eq!(query(&h, "bee", 8).len(), 1);
        assert_eq!(query(&h, "com", 8).len(), 2);
        assert_eq!(query(&h, "a.com zzz", 8).len(), 0);
        assert_eq!(ago(10_000, 9_000), "now");
        assert_eq!(ago(200_000, 0), "3 min");
        assert_eq!(ago(3_600_000 * 5, 0), "5 h");
    }

    #[test]
    fn sites() {
        assert_eq!(norm_site("https://Sub.Example.com:8080/x?y").as_deref(), Some("sub.example.com"));
        assert_eq!(norm_site("..example.com").as_deref(), Some("example.com"));
        assert_eq!(norm_site("localhost").as_deref(), Some("localhost"));
        assert_eq!(norm_site("nodot"), None);
        assert_eq!(norm_site("bad site.com"), None);
        assert_eq!(host_of("https://u:p@a.b.com:9/x"), "a.b.com:9");
        assert_eq!(origin_of("https://A.com/x").as_deref(), Some("https://a.com"));
        assert_eq!(origin_of("ftp://a.com"), None);
    }

    #[test]
    fn marks() {
        assert_eq!(count_text(0, 0), "Select an element, or draw");
        assert_eq!(count_text(2, 1), "2 yours, 1 from Claude");
        let m = vec![
            json!({ "n": 1, "kind": "element", "text": "this one", "picked": { "selector": "#b", "tag": "button", "text": "Click me", "html": "<button\n   id=\"b\">" } }),
            json!({ "n": 2, "kind": "box", "text": "", "x": 10.4, "y": 20, "width": 30, "height": 40, "anchor": [25, 40] }),
            json!({ "n": 3, "kind": "pen", "text": "here", "anchor": [5, 6] }),
            json!({ "n": 4, "kind": "pin", "text": "", "anchor": [7, 8] }),
        ];
        let t = marks_text("http://x/", "X", 800, 600, true, "abc123", true, &m);
        assert_eq!(t, "Browser: http://x/ (X), viewport 800x600, server network [drawer tab key: drawer:abc123], screenshot attached with the marks numbered\n1. element: this one\n   selector: #b\n   element: <button> \"Click me\"\n   html: <button id=\"b\">\n2. box at 10,20 size 30x40 (see the screenshot)\n3. drawing around 5,6 (see the screenshot): here\n4. pin at 7,8 (see the screenshot)");
        assert!(marks_text("u", "", 1, 1, false, "t", false, &[]).ends_with("(no marks: the page as it is)"));
        let long = format!("<div {}>", "a=\"b\" ".repeat(60));
        assert!(one_line_html(&long).ends_with(" \u{2026}"));
    }

    #[test]
    fn networks() {
        assert_eq!(network_label("client", "remote", "tunnel", None), "This PC");
        assert_eq!(network_label("server", "local", "direct", None), "Server (this PC)");
        assert_eq!(network_label("server", "remote", "tunnel", None), "Server");
        assert_eq!(network_label("server", "remote", "unsupported", None), "Server (needs a newer server)");
        assert_eq!(network_label("server", "remote", "offline", None), "Server (not connected)");
        assert_eq!(network_label("hp:home", "", "", Some("Home")), "Homepool: Home");
        assert_eq!(norm_network("homepool:home").as_deref(), Some("hp:home"));
        assert_eq!(norm_network("bogus"), None);
        assert_eq!(norm_network("hp:Bad"), None);
    }
}
