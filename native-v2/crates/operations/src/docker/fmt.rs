//! The Docker tab's pure rules (app/vendor/crundi-docker.js; as the Slint
//! client's src/ops/docker/fmt.rs): numbers, times, states, ports, the
//! filter, ANSI colour as runs (with the filter's matches marked), the
//! charts' series, a forward's suggested name and the shell's screen. Unit-tested.

use serde_json::Value;

/// bytes(): "—" for nothing, "512 B", "1.5 KB", "120 MB" (three digits round).
pub fn bytes(n: Option<f64>) -> String {
    let Some(n) = n.filter(|x| x.is_finite()) else { return "—".into() };
    if n < 1024.0 { return format!("{} B", n.round() as i64); }
    let u = ["KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (n / 1024.0, 0);
    while v >= 1024.0 && i < u.len() - 1 { v /= 1024.0; i += 1; }
    if v >= 100.0 { format!("{} {}", v.round() as i64, u[i]) } else { format!("{:.1} {}", v, u[i]) }
}

/// pct(): "—", "3.4%", "120%".
pub fn pct(n: Option<f64>) -> String {
    let Some(n) = n.filter(|x| x.is_finite()) else { return "—".into() };
    if n >= 100.0 { format!("{}%", n.round() as i64) } else { format!("{:.1}%", n) }
}

/// A JSON number as Some, null or missing as None.
pub fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64())
}

/// Milliseconds since 1970 from a unix-seconds number or an ISO string.
pub fn ms_of(t: &Value) -> Option<f64> {
    match t {
        Value::Number(n) => n.as_f64().map(|s| s * 1000.0),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis() as f64),
        _ => None,
    }
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// ago(): "just now", "5 min ago", "3 h ago", "4 days ago", "2 months ago", "3 years ago".
pub fn ago_at(now: f64, t: &Value) -> String {
    let Some(ms) = ms_of(t).filter(|m| *m > 0.0) else { return "—".into() };
    let s = ((now - ms) / 1000.0).max(0.0).floor() as i64;
    if s < 60 { return "just now".into(); }
    let m = s / 60; if m < 60 { return format!("{m} min ago"); }
    let h = m / 60; if h < 48 { return format!("{h} h ago"); }
    let d = h / 24; if d < 60 { return format!("{d} days ago"); }
    let mo = d / 30; if mo < 24 { return format!("{mo} months ago"); }
    format!("{} years ago", d / 365)
}

pub fn ago(t: &Value) -> String { ago_at(now_ms(), t) }

/// when(): the local date and time, then (ago); "—" for Docker's zero time.
pub fn when(iso: &Value) -> String {
    let Some(ms) = ms_of(iso).filter(|m| *m >= 86400000.0) else { return "—".into() };
    let local = chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.with_timezone(&chrono::Local));
    let Some(local) = local else { return "—".into() };
    format!("{} ({})", local.format("%-d/%-m/%Y, %H:%M:%S"), ago(iso))
}

/// stateKey(): unhealthy when a running container fails its check, else its state.
pub fn state_key(c: &Value) -> String {
    let st = c.get("state").and_then(|x| x.as_str()).unwrap_or("");
    if c.get("health").and_then(|x| x.as_str()) == Some("unhealthy") && st == "running" { return "unhealthy".into(); }
    if st.is_empty() { "exited".into() } else { st.into() }
}

/// portText(): "8080/tcp" unpublished; "127.0.0.1:80→8080", "80→53/udp".
pub fn port_text(p: &Value) -> String {
    let private = crate::services::kit::s(p, "private");
    let ty = crate::services::kit::s(p, "type");
    let public = crate::services::kit::s(p, "public");
    if public.is_empty() || public == "0" { return format!("{private}/{ty}"); }
    let ip = crate::services::kit::s(p, "ip");
    let ip = if !ip.is_empty() && ip != "0.0.0.0" && ip != "::" { format!("{ip}:") } else { String::new() };
    format!("{ip}{public}→{private}{}", if ty != "tcp" { format!("/{ty}") } else { String::new() })
}

/// matches(): every word of the filter is somewhere in the parts (any case).
pub fn matches(filter: &str, parts: &[String]) -> bool {
    if filter.trim().is_empty() { return true; }
    let hay = parts.join(" ").to_lowercase();
    filter.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

/// slug(): a forward's suggested name from a service or container name.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
            if c == '-' { if !dash { out.push('-'); } dash = true; } else { out.push(c); dash = false; }
        } else if !dash { out.push('-'); dash = true; }
    }
    let t: String = out.trim_matches('-').chars().take(32).collect();
    let t = t.trim_end_matches('-').to_string();
    if t.is_empty() { "app".into() } else { t }
}

// ─── ANSI colour ───

/// One run of a log line: its text, its colour (30..37, 0 none) and bold.
#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub text: String, pub color: u8, pub bold: bool }

/// The web's ansiHtml(): SGR colours (30-37, 90-97 as 30-37), bold, reset;
/// every other escape is dropped.
pub fn ansi_runs(text: &str) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let (mut color, mut bold) = (0u8, false);
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |cur: &mut String, out: &mut Vec<Run>, color: u8, bold: bool| {
        if !cur.is_empty() { out.push(Run { text: std::mem::take(cur), color, bold }); }
    };
    while i < chars.len() {
        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '[' {
            let mut j = i + 2;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == ';' || chars[j] == '?') { j += 1; }
            if j < chars.len() && chars[j].is_ascii_alphabetic() {
                let params: String = chars[i + 2..j].iter().collect();
                if chars[j] == 'm' {
                    flush(&mut cur, &mut out, color, bold);
                    let params = if params.is_empty() { "0".to_string() } else { params };
                    for c in params.split(';') {
                        let c: u32 = c.parse().unwrap_or(0);
                        match c {
                            0 => { color = 0; bold = false; }
                            1 => bold = true,
                            39 => color = 0,
                            30..=37 => color = c as u8,
                            90..=97 => color = (c - 60) as u8,
                            _ => {}
                        }
                    }
                }
                i = j + 1;
                continue;
            }
        }
        cur.push(chars[i]);
        i += 1;
    }
    flush(&mut cur, &mut out, color, bold);
    out
}

/// stripAnsi().
pub fn strip_ansi(text: &str) -> String {
    ansi_runs(text).into_iter().map(|r| r.text).collect()
}

/// One piece of a log line as drawn: its text, its colour (30..37, 0
/// none), bold, and whether it is a match of the filter (the web's <mark>).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece { pub text: String, pub color: u8, pub bold: bool, pub marked: bool }

/// A log line's runs, cut where the filter matches (any case). Tabs read as
/// spaces; other control characters are dropped.
pub fn pieces(text: &str, highlight: &str) -> Vec<Piece> {
    let hl = highlight.to_lowercase();
    let mut out = Vec::new();
    for r in ansi_runs(text) {
        let t: String = r.text.chars().map(|c| if c == '\t' { ' ' } else { c }).filter(|c| !c.is_control()).collect();
        let lower = t.to_lowercase();
        // Matching on the lower-cased text is only safe while the case
        // change keeps byte offsets (ASCII); otherwise no mark.
        if hl.is_empty() || lower.len() != t.len() {
            if !t.is_empty() { out.push(Piece { text: t, color: r.color, bold: r.bold, marked: false }); }
            continue;
        }
        let mut at = 0;
        while let Some(k) = lower[at..].find(&hl) {
            let s = at + k;
            if s > at { out.push(Piece { text: t[at..s].to_string(), color: r.color, bold: r.bold, marked: false }); }
            out.push(Piece { text: t[s..s + hl.len()].to_string(), color: r.color, bold: r.bold, marked: true });
            at = s + hl.len();
        }
        if at < t.len() { out.push(Piece { text: t[at..].to_string(), color: r.color, bold: r.bold, marked: false }); }
    }
    out
}

/// The web's log record: a leading Docker timestamp split off.
pub fn split_ts(line: &str) -> (String, String) {
    let b = line.as_bytes();
    // 2026-11-15T10:20:30.123456789Z
    if b.len() > 20 && b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[..4].iter().all(|c| c.is_ascii_digit()) {
        if let Some(z) = line[11..].find('Z') {
            let end = 11 + z;
            if line[11..end].chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.') {
                let ts = line[..=end].to_string();
                let mut rest = &line[end + 1..];
                if let Some(r) = rest.strip_prefix(' ') { rest = r; }
                return (ts, rest.to_string());
            }
        }
    }
    (String::new(), line.to_string())
}

/// A timestamp as the log shows it: "2026-11-15 10:20:30.123".
pub fn ts_show(ts: &str) -> String {
    ts.replacen('T', " ", 1).chars().take(23).collect()
}

// ─── Charts ───

/// How many samples a chart holds across its width (the web's chartSvg: 90).
pub const CHART_N: usize = 90;

/// A series for the chart: its last CHART_N values, a missing one as 0.
pub fn series(values: &[Option<f64>]) -> Vec<f32> {
    values.iter().rev().take(CHART_N).rev().map(|v| v.unwrap_or(0.0) as f32).collect()
}

/// The largest value of a few series (and a hint).
pub fn chart_max(series: &[&[Option<f64>]], hint: f64) -> f64 {
    let mut m = hint;
    for s in series { for v in s.iter().flatten() { if *v > m { m = *v; } } }
    if m > 0.0 { m } else { 1.0 }
}

/// rates(): a counter's per-second rate between samples.
pub fn rates(samples: &[Value], key: &str) -> Vec<Option<f64>> {
    let mut out = Vec::new();
    for i in 1..samples.len() {
        let dt = (crate::services::kit::n(&samples[i], "at") - crate::services::kit::n(&samples[i - 1], "at")) / 1000.0;
        out.push(Some(if dt > 0.0 { ((crate::services::kit::n(&samples[i], key) - crate::services::kit::n(&samples[i - 1], key)) / dt).max(0.0) } else { 0.0 }));
    }
    out
}

// ─── The shell's screen ───

/// The closest a plain text view gets to xterm (Slint has no terminal
/// emulator): lines, a cursor column, carriage return, backspace, erase to
/// the end of the line and clear; colours and cursor moves are dropped.
#[derive(Default, Clone, Debug)]
pub struct Screen { pub lines: Vec<Vec<char>>, col: usize, esc: Option<String> }

const SCREEN_MAX: usize = 3000;

impl Screen {
    pub fn new() -> Screen { Screen { lines: vec![Vec::new()], col: 0, esc: None } }

    pub fn write(&mut self, data: &str) {
        if self.lines.is_empty() { self.lines.push(Vec::new()); }
        for c in data.chars() {
            if let Some(mut seq) = self.esc.take() {
                seq.push(c);
                if !self.seq_done(&seq) { self.esc = Some(seq); }
                continue;
            }
            match c {
                '\u{1b}' => self.esc = Some(String::new()),
                '\r' => self.col = 0,
                '\n' => { self.lines.push(Vec::new()); self.col = 0; }
                '\u{8}' => self.col = self.col.saturating_sub(1),
                '\t' => { let n = 8 - self.col % 8; for _ in 0..n { self.put(' '); } }
                c if (c as u32) < 32 || c == '\u{7f}' => {}
                c => self.put(c),
            }
        }
        if self.lines.len() > SCREEN_MAX { let cut = self.lines.len() - SCREEN_MAX; self.lines.drain(..cut); }
    }

    fn put(&mut self, c: char) {
        let line = self.lines.last_mut().unwrap();
        while line.len() < self.col { line.push(' '); }
        if self.col < line.len() { line[self.col] = c; } else { line.push(c); }
        self.col += 1;
    }

    /// Whether an escape sequence (after ESC) is complete; a finished one is applied.
    fn seq_done(&mut self, seq: &str) -> bool {
        let first = seq.chars().next().unwrap_or(' ');
        match first {
            '[' => {
                let last = seq.chars().last().unwrap_or(' ');
                if seq.len() < 2 || !(('@'..='~').contains(&last)) { return false; }
                let params = &seq[1..seq.len() - 1];
                match last {
                    'K' => {
                        let line = self.lines.last_mut().unwrap();
                        match params { "1" => { for x in line.iter_mut().take(self.col) { *x = ' '; } } "2" => line.clear(), _ => line.truncate(self.col) }
                    }
                    'J' if params == "2" || params == "3" => { self.lines = vec![Vec::new()]; self.col = 0; }
                    'D' => { let n: usize = params.parse().unwrap_or(1); self.col = self.col.saturating_sub(n.max(1)); }
                    'C' => { let n: usize = params.parse().unwrap_or(1); self.col += n.max(1); }
                    'G' => { let n: usize = params.parse().unwrap_or(1); self.col = n.max(1) - 1; }
                    'P' => { let n: usize = params.parse().unwrap_or(1); let col = self.col; let line = self.lines.last_mut().unwrap(); for _ in 0..n.max(1) { if col < line.len() { line.remove(col); } } }
                    _ => {}
                }
                true
            }
            // OSC: up to BEL or ESC \.
            ']' => seq.ends_with('\u{7}') || seq.ends_with("\u{1b}\\"),
            '(' | ')' | '#' => seq.chars().count() >= 2,
            _ => true,
        }
    }

    pub fn text(&self) -> String {
        self.lines.iter().map(|l| l.iter().collect::<String>().trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn numbers() {
        assert_eq!(bytes(None), "—");
        assert_eq!(bytes(Some(512.0)), "512 B");
        assert_eq!(bytes(Some(1536.0)), "1.5 KB");
        assert_eq!(bytes(Some(163762176.0)), "156 MB");
        assert_eq!(bytes(Some(12541480960.0)), "11.7 GB");
        assert_eq!(pct(Some(3.456)), "3.5%");
        assert_eq!(pct(Some(123.4)), "123%");
        assert_eq!(pct(None), "—");
    }

    #[::core::prelude::v1::test]
    fn times() {
        let now = 1_000_000_000_000.0;
        assert_eq!(ago_at(now, &json!(now / 1000.0 - 30.0)), "just now");
        assert_eq!(ago_at(now, &json!(now / 1000.0 - 600.0)), "10 min ago");
        assert_eq!(ago_at(now, &json!(now / 1000.0 - 5.0 * 86400.0)), "5 days ago");
        assert_eq!(ago_at(now, &json!(now / 1000.0 - 90.0 * 86400.0)), "3 months ago");
        assert_eq!(ago_at(now, &json!("not a date")), "—");
        assert_eq!(when(&json!("0001-01-01T00:00:00Z")), "—");
    }

    #[::core::prelude::v1::test]
    fn states_and_ports() {
        assert_eq!(state_key(&json!({ "state": "running", "health": "unhealthy" })), "unhealthy");
        assert_eq!(state_key(&json!({ "state": "paused" })), "paused");
        assert_eq!(state_key(&json!({})), "exited");
        assert_eq!(port_text(&json!({ "ip": "127.0.0.1", "public": 7800, "private": 8080, "type": "tcp" })), "127.0.0.1:7800→8080");
        assert_eq!(port_text(&json!({ "ip": "0.0.0.0", "public": 53, "private": 53, "type": "udp" })), "53→53/udp");
        assert_eq!(port_text(&json!({ "private": 5010, "type": "tcp" })), "5010/tcp");
    }

    #[::core::prelude::v1::test]
    fn filter_and_slug() {
        let parts = vec!["homepool-chisel".to_string(), "jpillora/chisel".to_string()];
        assert!(matches("", &parts));
        assert!(matches("CHISEL home", &parts));
        assert!(!matches("chisel redis", &parts));
        assert_eq!(slug("My_App  Web"), "my-app-web");
        assert_eq!(slug("--"), "app");
    }

    #[::core::prelude::v1::test]
    fn ansi() {
        let r = ansi_runs("a\u{1b}[31mred\u{1b}[0m b\u{1b}[1;92mok\u{1b}[39m\u{1b}[2K");
        assert_eq!(r, vec![
            Run { text: "a".into(), color: 0, bold: false },
            Run { text: "red".into(), color: 31, bold: false },
            Run { text: " b".into(), color: 0, bold: false },
            Run { text: "ok".into(), color: 32, bold: true },
        ]);
        assert_eq!(strip_ansi("\u{1b}[33mwarn\u{1b}[0m: x"), "warn: x");
        let p = pieces("  GET /x \u{1b}[31mget\u{1b}[0m", "get");
        assert_eq!(p.iter().map(|x| (x.text.as_str(), x.color, x.marked)).collect::<Vec<_>>(), vec![("  ", 0, false), ("GET", 0, true), (" /x ", 0, false), ("get", 31, true)]);
        assert!(pieces("", "").is_empty());
    }

    #[::core::prelude::v1::test]
    fn timestamps() {
        assert_eq!(split_ts("2026-11-15T10:20:30.123456789Z hello"), ("2026-11-15T10:20:30.123456789Z".into(), "hello".into()));
        assert_eq!(split_ts("plain line"), (String::new(), "plain line".into()));
        assert_eq!(ts_show("2026-11-15T10:20:30.123456789Z"), "2026-11-15 10:20:30.123");
    }

    #[::core::prelude::v1::test]
    fn charts() {
        assert_eq!(series(&[Some(1.0), None]), vec![1.0, 0.0]);
        assert_eq!(series(&vec![Some(1.0); 100]).len(), CHART_N);
        let s = vec![json!({ "at": 0, "netRx": 0 }), json!({ "at": 2000, "netRx": 1000 }), json!({ "at": 4000, "netRx": 500 })];
        assert_eq!(rates(&s, "netRx"), vec![Some(500.0), Some(0.0)]);
        assert_eq!(chart_max(&[&[Some(3.0), None], &[Some(7.0)]], 0.0), 7.0);
    }

    #[::core::prelude::v1::test]
    fn screen() {
        let mut s = Screen::new();
        s.write("\u{1b}]0;title\u{7}root@x:/# ls\r\nbin  etc\r\n\u{1b}[01;32mroot@x\u{1b}[00m:/# abc\u{8}\u{1b}[K");
        assert_eq!(s.text(), "root@x:/# ls\nbin  etc\nroot@x:/# ab");
        s.write("\r\u{1b}[Kdone");
        assert_eq!(s.text(), "root@x:/# ls\nbin  etc\ndone");
        s.write("\u{1b}[H\u{1b}[2J$ ");
        assert_eq!(s.text(), "$");
    }
}
