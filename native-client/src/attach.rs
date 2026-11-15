//! Attachments (2.7, specs/attachments-drafts.md): where an uploaded path
//! goes in the message, what the upload's answer says, and whether the
//! server is on this machine. Pure: the app (src/app.rs) does the rest.

use serde_json::Value;

/// Put `path` into `text` in place of the selection [start, end) (byte
/// offsets; a caret when equal), as the web's insertPath: a space before it
/// unless the text before is empty or ends in whitespace, a space after it
/// unless the text after starts with whitespace. Returns the new text and
/// the caret right after what went in (after its trailing space).
pub fn insert_path(text: &str, start: usize, end: usize, path: &str) -> (String, usize) {
    let fix = |i: usize| { let mut i = i.min(text.len()); while !text.is_char_boundary(i) { i -= 1; } i };
    let (a, b) = (fix(start.min(end)), fix(start.max(end)));
    let (before, after) = (&text[..a], &text[b..]);
    let mut ins = String::new();
    if !before.is_empty() && !before.ends_with(char::is_whitespace) { ins.push(' '); }
    ins.push_str(path);
    if !after.starts_with(char::is_whitespace) { ins.push(' '); }
    (format!("{before}{ins}{after}"), before.len() + ins.len())
}

/// The upload's answer: the server's path and stored name, or the toast
/// that says why not ("Upload failed: …"). `status` 0 is no answer at all.
pub fn upload_answer(status: u16, body: &str, fallback_name: &str) -> Result<(String, String), String> {
    if status == 0 { return Err("Upload failed: Network error".into()); }
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        // Not JSON: something between this device and the server answered.
        let said = strip_tags(body);
        let said = if said.is_empty() { ", and nothing else".to_string() } else { format!(" ({said})") };
        return Err(format!("Upload failed: The upload was answered with HTTP {status}{said}, not by Crundi. Something between this device and the server may have blocked it."));
    };
    if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
        let path = v.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
        let name = v.get("name").and_then(|p| p.as_str()).filter(|n| !n.is_empty()).unwrap_or(fallback_name).to_string();
        return Ok((path, name));
    }
    let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
    // An older server without the endpoint (the backwards-compatibility rule).
    if status == 404 && err == "Not found" { return Err("Upload failed: not supported by this server".into()); }
    Err(format!("Upload failed: {}", if err.is_empty() { "?" } else { err }))
}

/// A body's text: tags out, whitespace collapsed, at most 80 characters.
fn strip_tags(body: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in body.chars() {
        match c { '<' => in_tag = true, '>' => { in_tag = false; out.push(' '); } _ if !in_tag => out.push(c), _ => {} }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect()
}

/// The server runs on this machine: a dropped file's own path will do.
pub fn is_local(base: &str) -> bool {
    let host = base.split("://").nth(1).unwrap_or(base);
    let host = if host.starts_with('[') { host.split(']').next().map(|h| format!("{h}]")).unwrap_or_default() } else { host.split(':').next().unwrap_or("").to_string() };
    matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// The toast for a local drop: "File added" or "N files added".
pub fn added_toast(n: usize) -> String {
    if n == 1 { "File added".into() } else { format!("{n} files added") }
}

/// The bar's number: "42%", then "saving" once the whole body is sent.
pub fn progress_label(sent: u64, total: u64) -> String {
    if total > 0 && sent >= total { "saving".into() } else if total == 0 { "0%".into() } else { format!("{}%", ((sent as f64 / total as f64) * 100.0).round() as u64) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_go_in_at_the_caret() {
        assert_eq!(insert_path("Please read andsummarise", 15, 15, "/p/x"), ("Please read and /p/x summarise".into(), 21));
        assert_eq!(insert_path("Please read and summarise", 15, 15, "/p/x"), ("Please read and /p/x summarise".into(), 20));
        assert_eq!(insert_path("Look at this: ", 14, 14, "/p/x"), ("Look at this: /p/x ".into(), 19));
        assert_eq!(insert_path("", 0, 0, "/p/x"), ("/p/x ".into(), 5));
        // A selection is replaced; offsets past the end or inside a character are held.
        assert_eq!(insert_path("a XYZ b", 2, 5, "/p"), ("a /p b".into(), 4));
        assert_eq!(insert_path("é", 1, 1, "/p"), ("/p é".into(), 3));
        assert_eq!(insert_path("ab", 99, 99, "/p"), ("ab /p ".into(), 6));
    }

    #[test]
    fn upload_answers() {
        assert_eq!(upload_answer(200, r#"{"ok":true,"path":"/a/b/1-n.txt","name":"1-n.txt"}"#, "n.txt"), Ok(("/a/b/1-n.txt".into(), "1-n.txt".into())));
        assert_eq!(upload_answer(404, r#"{"ok":false,"error":"Project not found"}"#, "n"), Err("Upload failed: Project not found".into()));
        assert_eq!(upload_answer(404, r#"{"error":"Not found"}"#, "n"), Err("Upload failed: not supported by this server".into()));
        assert_eq!(upload_answer(200, r#"{"ok":false}"#, "n"), Err("Upload failed: ?".into()));
        assert_eq!(upload_answer(0, "", "n"), Err("Upload failed: Network error".into()));
        assert_eq!(upload_answer(413, "<html><head><title>413 Request Entity Too Large</title></head><body><center><h1>413 Request Entity Too Large</h1></center><hr><center>nginx</center></body></html>", "n"),
            Err("Upload failed: The upload was answered with HTTP 413 (413 Request Entity Too Large 413 Request Entity Too Large nginx), not by Crundi. Something between this device and the server may have blocked it.".into()));
        assert_eq!(upload_answer(502, "", "n"), Err("Upload failed: The upload was answered with HTTP 502, and nothing else, not by Crundi. Something between this device and the server may have blocked it.".into()));
    }

    #[test]
    fn local_servers() {
        for b in ["http://localhost:4810", "http://127.0.0.1:8889", "http://[::1]:80", "http://localhost"] { assert!(is_local(b), "{b}"); }
        for b in ["http://127.0.0.2:4810", "https://dev.example.com", "http://10.0.0.5:4810"] { assert!(!is_local(b), "{b}"); }
        assert_eq!(added_toast(1), "File added");
        assert_eq!(added_toast(3), "3 files added");
        assert_eq!(progress_label(42, 100), "42%");
        assert_eq!(progress_label(100, 100), "saving");
    }
}
