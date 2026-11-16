//! Claude's reply text (2.16, specs/log-rest.md): the web's md(), inline()
//! and linkify() rules, as markdown for Slint's StyledText. Pure.

const FILE_EXT: &[&str] = &["js", "mjs", "cjs", "ts", "tsx", "jsx", "json", "md", "txt", "log", "py", "sh", "bash", "yml", "yaml", "toml", "ini", "conf", "cfg", "env", "html", "htm", "css", "scss", "xml", "svg", "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "csv", "sql", "go", "rs", "rb", "php", "java", "kt", "kts", "gradle", "swift", "c", "h", "cc", "cpp", "hpp", "vue", "lock", "dockerfile", "service"];

fn part_char(c: char) -> bool { c.is_alphanumeric() || "_.@+~%,=-".contains(c) }

/// Without a trailing ":12" or ":12:3".
fn bare(p: &str) -> &str {
    let mut b = p;
    for _ in 0..2 {
        if let Some((head, tail)) = b.rsplit_once(':') { if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) { b = head; } }
    }
    b
}

fn has_ext(p: &str) -> bool {
    p.rsplit_once('.').is_some_and(|(_, e)| FILE_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

/// The web's path rule for running text: an absolute path of two parts or
/// more, or a relative one starting ./ or ../, or with a known file
/// extension ("and/or" and "10/12" stay words); with its :line[:col].
pub fn text_path(tok: &str) -> bool {
    let b = bare(tok);
    let parts: Vec<&str> = b.trim_start_matches("../").trim_start_matches("./").trim_start_matches('/').split('/').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty() || !p.chars().all(part_char)) { return false; }
    b.starts_with('/') || b.starts_with("./") || b.starts_with("../") || has_ext(b)
}

/// The web's looksLikePath for a code span.
pub fn code_path(c: &str) -> bool {
    if c.is_empty() || c.chars().count() > 260 || c.contains(char::is_whitespace) || c.contains("://") || c.chars().any(|x| "<>&*|$`".contains(x)) { return false; }
    let b = bare(c);
    let rel = b.trim_start_matches("../").trim_start_matches("./");
    let parts: Vec<&str> = rel.trim_start_matches('/').split('/').collect();
    if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|x| x.is_alphanumeric() || "_.@+~-".contains(x))) { return true; }
    parts.len() == 1 && !parts[0].is_empty() && parts[0].chars().all(|x| x.is_alphanumeric() || "_.@+~-".contains(x)) && has_ext(parts[0])
}

/// A bare http(s) URL becomes a link; trailing punctuation stays outside.
/// Code spans, markdown links and autolinks are left alone. An image
/// (`![alt](url)`) is not drawn: its text stays as written.
pub fn linkify(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut code = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            // A code span that is a path: its chip, as a path link.
            if let Some(len) = chars[i + 1..].iter().position(|x| *x == '`') {
                let inner: String = chars[i + 1..i + 1 + len].iter().collect();
                if code_path(&inner) { out.push_str(&format!("[`{inner}`](path:{inner})")); } else { out.push('`'); out.push_str(&inner); out.push('`'); }
                i += len + 2;
                continue;
            }
            code = !code; out.push(c); i += 1; continue;
        }
        // An image's "!" is a literal one (the web draws no pictures from
        // markdown: `![a](https://…)` is "!" and the link).
        if !code && c == '!' && chars.get(i + 1) == Some(&'[') { out.push_str("\\!"); i += 1; continue; }
        // [text](target) is a link only to http(s), whole as written; any
        // other '[' (a footnote's [1], a link to elsewhere) stays text.
        if !code && c == '[' {
            match real_link(&chars, i) {
                Some((t, end)) if is_http(&chars[t + 2..end - 1]) => { out.extend(&chars[i..end]); i = end; }
                _ => { out.push_str("\\["); i += 1; }
            }
            continue;
        }
        let rest: String = chars[i..chars.len().min(i + 8)].iter().collect();
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        if !code && (rest.starts_with("http://") || rest.starts_with("https://")) && prev != '(' && prev != '<' {
            let mut j = i;
            while j < chars.len() && !chars[j].is_whitespace() && chars[j] != '<' && chars[j] != '`' { j += 1; }
            let mut end = j;
            while end > i && ".,;:!?)]'\">".contains(chars[end - 1]) { end -= 1; }
            let url: String = chars[i..end].iter().collect();
            out.push('<');
            out.push_str(&url);
            out.push('>');
            out.extend(&chars[end..j]);
            i = j;
            continue;
        }
        // A path in running text (after a space, a bracket or the start).
        if !code && (c == '/' || c == '.' || part_char(c)) && (i == 0 || " \t([>\"'".contains(prev)) {
            let mut j = i;
            while j < chars.len() && (part_char(chars[j]) || chars[j] == '/' || chars[j] == ':') { j += 1; }
            let mut end = j;
            while end > i && ".,;:!?)]'\"".contains(chars[end - 1]) { end -= 1; }
            let tok: String = chars[i..end].iter().collect();
            if tok.contains('/') && text_path(&tok) {
                out.push_str(&format!("[{tok}](path:{tok})"));
                out.extend(&chars[end..j]);
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A user's message for StyledText: its text as typed (markdown characters
/// escaped, line breaks kept, leading spaces kept), its URLs as links.
pub fn plain_links(text: &str) -> String {
    text.split('\n').map(|line| {
        let lead = line.len() - line.trim_start_matches(' ').len();
        let mut out = "\u{a0}".repeat(lead);
        let rest = &line[lead..];
        let chars: Vec<char> = rest.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let head: String = chars[i..chars.len().min(i + 8)].iter().collect();
            if head.starts_with("http://") || head.starts_with("https://") {
                let mut j = i;
                while j < chars.len() && !chars[j].is_whitespace() { j += 1; }
                let mut end = j;
                while end > i && ".,;:!?)]'\">".contains(chars[end - 1]) { end -= 1; }
                let url: String = chars[i..end].iter().collect();
                out.push('<'); out.push_str(&url); out.push('>');
                for c in &chars[end..j] { out.push('\\'); out.push(*c); }
                i = j;
                continue;
            }
            let c = chars[i];
            if c.is_ascii_punctuation() { out.push('\\'); }
            out.push(c);
            i += 1;
        }
        if out.is_empty() { out.push('\u{a0}'); }
        out
    }).collect::<Vec<_>>().join("  \n")
}

/// The URL under a byte offset of a user's text (a click in its bubble).
pub fn url_at(text: &str, at: usize) -> Option<String> {
    let mut i = 0;
    while let Some(k) = text[i..].find("http") {
        let s = i + k;
        let rest = &text[s..];
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let e = s + rest.find(char::is_whitespace).unwrap_or(rest.len());
            let url = text[s..e].trim_end_matches(|c: char| ".,;:!?)]'\">".contains(c));
            if at >= s && at <= s + url.len() { return Some(url.to_string()); }
            i = e;
        } else { i = s + 4; }
        if i >= text.len() { break; }
    }
    None
}

/// A paragraph's lines: one paragraph, its line breaks kept (hard breaks).
pub fn paragraph(lines: &[&str]) -> String {
    lines.iter().map(|l| linkify(l.trim_end())).collect::<Vec<_>>().join("  \n")
}

/// What a line of a reply is.
#[derive(Debug, PartialEq)]
pub enum Line<'a> {
    Blank,
    Fence,
    /// `#` (true: level 1, drawn larger) or `##`+ (bold, body size).
    Heading(bool, &'a str),
    Rule,
    Quote(&'a str),
    Bullet(&'a str),
    Numbered(&'a str),
    Text(&'a str),
}

pub fn line(l: &str) -> Line<'_> {
    let t = l.trim_start();
    if t.is_empty() { return Line::Blank; }
    if t.starts_with("```") { return Line::Fence; }
    let tt = t.trim_end();
    if tt.len() >= 3 && ["-", "*", "_"].iter().any(|c| tt.chars().all(|x| x.to_string() == *c)) { return Line::Rule; }
    if t.starts_with('#') {
        let level = t.chars().take_while(|c| *c == '#').count();
        let rest = &t[level..];
        if rest.starts_with(' ') { return Line::Heading(level == 1, rest.trim()); }
    }
    if let Some(q) = t.strip_prefix("> ").or_else(|| (t == ">").then_some("")) { return Line::Quote(q); }
    if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|b| t.strip_prefix(b)) { return Line::Bullet(r); }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) { return Line::Numbered(r); }
    }
    Line::Text(l)
}

/// A side question: `/btw <question>` (case-insensitive; the question may
/// span lines). `/btw` alone is not one.
pub fn btw(text: &str) -> Option<String> {
    let t = text.trim_start();
    let head = t.get(..4)?;
    if !head.eq_ignore_ascii_case("/btw") { return None; }
    let rest = &t[4..];
    if !rest.starts_with(char::is_whitespace) { return None; }
    let q = rest.trim();
    (!q.is_empty()).then(|| q.to_string())
}

/// A markdown link at `chars[i]` (a '['): its ']' before any other '['
/// or a line break, then directly "(", a target without spaces, and ")".
/// (The '[' index of `](`, the index after `)`.) F145: "Footnote [1] … in
/// [the docs](https://…)" is a link only from "[the docs".
pub fn real_link(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    loop {
        match chars.get(j)? {
            '\\' => j += 2,
            '[' | '\n' => return None,
            ']' => break,
            _ => j += 1,
        }
    }
    if chars.get(j + 1) != Some(&'(') { return None; }
    let mut k = j + 2;
    loop {
        match chars.get(k)? {
            ')' => return Some((j, k + 1)),
            c if c.is_whitespace() => return None,
            _ => k += 1,
        }
    }
}

fn is_http(t: &[char]) -> bool {
    let h: String = t.iter().take(8).collect();
    h.starts_with("http://") || h.starts_with("https://")
}

/// The length of the backtick run at `chars[i]`.
fn ticks(chars: &[char], i: usize) -> usize { chars[i..].iter().take_while(|c| **c == '`').count() }

/// Whether a backtick run of `n` closes later (a code span), not counting
/// runs of another length.
fn closes(chars: &[char], from: usize, n: usize) -> bool {
    let mut k = from;
    while k < chars.len() {
        if chars[k] == '`' { let m = ticks(chars, k); if m == n { return true; } k += m; } else { k += 1; }
    }
    false
}

/// A block's markdown in an explicit colour (K3): `<font color>` spans
/// around it, so no text falls back to the StyledText's default (chip-key,
/// dark, for inline code's faint outline). Closed around http links
/// (`<https://…>`, `[x](https://…)`) so they keep the link colour, when
/// `split_links`; path links and code spans stay inside, in the text's
/// colour. Closed and opened again at each line break, as Slint starts a
/// paragraph there and a span cannot cross it. Links keep Slint's link
/// underline (drawn from 1 px: fonts::with_line). A link inside **bold**
/// can't be split out (an HTML close inside an open markdown style is an
/// error): app::md_in then asks again without `split_links`.
pub fn colorize(text: &str, color: &str, split_links: bool) -> String {
    let open = format!("<font color=\"{color}\">");
    let mut out = open.clone();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut code = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if code == 0 && c == '\\' && i + 1 < chars.len() && chars[i + 1] != '\n' { out.push(c); out.push(chars[i + 1]); i += 2; continue; }
        if c == '`' {
            let n = ticks(&chars, i);
            if code == 0 { if closes(&chars, i + n, n) { code = n; } } else if n == code { code = 0; }
            out.extend(&chars[i..i + n]);
            i += n;
            continue;
        }
        if code == 0 && c == '\n' {
            let kept = out.trim_end_matches(' ').len();
            let spaces = out.split_off(kept);
            out.push_str("</font>");
            out.push_str(&spaces);
            out.push('\n');
            out.push_str(&open);
            i += 1;
            continue;
        }
        if code == 0 && c == '<' && is_http(&chars[i + 1..]) {
            if let Some(k) = chars[i..].iter().position(|x| *x == '>' || x.is_whitespace()).filter(|k| chars[i + k] == '>') {
                if split_links { out.push_str("</font>"); }
                out.extend(&chars[i..i + k + 1]);
                if split_links { out.push_str(&open); }
                i += k + 1;
                continue;
            }
        }
        if code == 0 && c == '[' {
            if let Some((t, end)) = real_link(&chars, i) {
                let split = split_links && is_http(&chars[t + 2..end - 1]);
                if split { out.push_str("</font>"); }
                out.extend(&chars[i..end]);
                if split { out.push_str(&open); }
                i = end;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out.push_str("</font>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_spans() {
        let c = |t: &str| colorize(t, "#fff", true);
        assert_eq!(c("plain **bold**"), "<font color=\"#fff\">plain **bold**</font>");
        assert_eq!(c("see <https://x.io/a> now"), "<font color=\"#fff\">see </font><https://x.io/a><font color=\"#fff\"> now</font>");
        assert_eq!(c("a [doc](https://d.io) b"), "<font color=\"#fff\">a </font>[doc](https://d.io)<font color=\"#fff\"> b</font>");
        // Paths and code stay inside.
        assert_eq!(c("[`a/b.js`](path:a/b.js) and `<https://x>`"), "<font color=\"#fff\">[`a/b.js`](path:a/b.js) and `<https://x>`</font>");
        assert_eq!(c("\\[x](https://y)"), "<font color=\"#fff\">\\[x](https://y)</font>");
        // Not split: links inside, in the text's colour.
        assert_eq!(colorize("a [doc](https://d.io) <https://x.io>", "#fff", false), "<font color=\"#fff\">a [doc](https://d.io) <https://x.io></font>");
        // A line break: closed before its two spaces, opened after it.
        assert_eq!(c("one  \ntwo"), "<font color=\"#fff\">one</font>  \n<font color=\"#fff\">two</font>");
        // …but not inside a code span that crosses it; a lone backtick is text.
        assert_eq!(c("`a\nb` c"), "<font color=\"#fff\">`a\nb` c</font>");
        assert_eq!(c("it`s\nhere"), "<font color=\"#fff\">it`s</font>\n<font color=\"#fff\">here</font>");
    }

    /// F145: a '[' is a link only for a real [text](url).
    #[test]
    fn brackets_that_are_not_links() {
        let c = |t: &str| colorize(t, "#fff", true);
        let footnote = "Footnote [1] in the log says the cache is cold; the details are in [the cargo docs](https://doc.rust-lang.org/cargo/).";
        assert_eq!(c(footnote), "<font color=\"#fff\">Footnote [1] in the log says the cache is cold; the details are in </font>[the cargo docs](https://doc.rust-lang.org/cargo/)<font color=\"#fff\">.</font>");
        assert_eq!(linkify(footnote), "Footnote \\[1] in the log says the cache is cold; the details are in [the cargo docs](https://doc.rust-lang.org/cargo/).");
        // Nested: the outer '[' meets another before its ']'; the inner
        // one's ']' is not followed by '('.
        assert_eq!(real_link(&"[a [b] c](https://x.io)".chars().collect::<Vec<_>>(), 0), None);
        assert_eq!(c("[a [b] c](https://x.io)"), "<font color=\"#fff\">[a [b] c](https://x.io)</font>");
        assert_eq!(linkify("[a [b] c](https://x.io)"), "\\[a \\[b] c](https://x.io)");
        // …while a link after them is one.
        assert_eq!(c("[x [y](https://y.io)"), "<font color=\"#fff\">[x </font>[y](https://y.io)<font color=\"#fff\"></font>");
        // Unbalanced, across a line break, a space in the target.
        assert_eq!(c("]x[ y [z] (https://z.io)"), "<font color=\"#fff\">]x[ y [z] (https://z.io)</font>");
        assert_eq!(c("[a\nb](https://x.io)"), "<font color=\"#fff\">[a</font>\n<font color=\"#fff\">b](https://x.io)</font>");
        assert_eq!(c("[a](https://x.io b)"), "<font color=\"#fff\">[a](https://x.io b)</font>");
        assert_eq!(linkify("[z](not a link) and [w](ftp://w)"), "\\[z](not a link) and \\[w](ftp://w)");
        // A bracket at the end of the block.
        assert_eq!(c("the end ["), "<font color=\"#fff\">the end [</font>");
        assert_eq!(c("the end [x]("), "<font color=\"#fff\">the end [x](</font>");
        assert_eq!(linkify("the end ["), "the end \\[");
        assert_eq!(real_link(&['['], 0), None);
    }

    // (The Slint client checked the markup with Slint's own parser here;
    //  GPUI draws markdown with gpui-component, so that check stays behind.)
    #[test]
    fn links() {
        assert_eq!(linkify("see https://example.com/a."), "see <https://example.com/a>.");
        assert_eq!(linkify("(https://x.io/b),"), "(https://x.io/b),");
        assert_eq!(linkify("a https://x.io/b) c"), "a <https://x.io/b>) c");
        assert_eq!(linkify("[docs](https://example.com/a)"), "[docs](https://example.com/a)");
        assert_eq!(linkify("`https://in.code`"), "`https://in.code`");
        assert_eq!(linkify("![alt](https://x/p.png)"), "\\![alt](https://x/p.png)");
        assert_eq!(linkify("![logo](./assets/logo.png)"), "\\!\\[logo]([./assets/logo.png](path:./assets/logo.png))");
        assert_eq!(linkify("and/or 10/12"), "and/or 10/12");
    }

    #[test]
    fn side_questions() {
        assert_eq!(btw("/btw what file?").as_deref(), Some("what file?"));
        assert_eq!(btw("/BTW  two\nlines ").as_deref(), Some("two\nlines"));
        assert_eq!(btw("/btw"), None);
        assert_eq!(btw("/btw   "), None);
        assert_eq!(btw("/btwx y"), None);
        assert_eq!(btw("ask /btw x"), None);
    }

    #[test]
    fn paths() {
        assert_eq!(linkify("see /tmp/proj/client.js:12 now"), "see [/tmp/proj/client.js:12](path:/tmp/proj/client.js:12) now");
        assert_eq!(linkify("in src/app.js."), "in [src/app.js](path:src/app.js).");
        assert_eq!(linkify("./x/y and ../a"), "[./x/y](path:./x/y) and ../a");
        assert_eq!(linkify("and/or 10/12 client/server"), "and/or 10/12 client/server");
        assert_eq!(linkify("`src/server.js` and `obj.prop` and `app.js`"), "[`src/server.js`](path:src/server.js) and `obj.prop` and [`app.js`](path:app.js)");
        assert!(code_path("src/a.js") && code_path("/etc/z") && !code_path("a b/c") && !code_path("https://x/y"));
    }

    #[test]
    fn user_text() {
        assert_eq!(plain_links("see https://ex.com/a. *x*"), "see <https://ex.com/a>\\. \\*x\\*");
        assert_eq!(plain_links("a\n\n  b"), "a  \n\u{a0}  \n\u{a0}\u{a0}b");
        assert_eq!(url_at("go https://ex.com/a. ok", 5).as_deref(), Some("https://ex.com/a"));
        assert_eq!(url_at("go https://ex.com/a. ok", 1), None);
    }

    #[test]
    fn lines() {
        assert_eq!(line("# Title"), Line::Heading(true, "Title"));
        assert_eq!(line("## The fix"), Line::Heading(false, "The fix"));
        assert_eq!(line("#hashtag"), Line::Text("#hashtag"));
        assert_eq!(line("---"), Line::Rule);
        assert_eq!(line("***"), Line::Rule);
        assert_eq!(line("> quoted"), Line::Quote("quoted"));
        assert_eq!(line("- item"), Line::Bullet("item"));
        assert_eq!(line("3) third"), Line::Numbered("third"));
        assert_eq!(line("12. twelfth"), Line::Numbered("twelfth"));
        assert_eq!(line("```rust"), Line::Fence);
        assert_eq!(line("   "), Line::Blank);
        assert_eq!(paragraph(&["one", "two "]), "one  \ntwo");
    }
}
