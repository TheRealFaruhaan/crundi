//! Claude's reply text (2.16, specs/log-rest.md): the web's md(), inline()
//! and linkify() rules, as markdown for Slint's StyledText. Pure.

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
        if c == '`' { code = !code; out.push(c); i += 1; continue; }
        if !code && c == '!' && chars.get(i + 1) == Some(&'[') { out.push_str("!\\["); i += 2; continue; }
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
        out.push(c);
        i += 1;
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        assert_eq!(linkify("see https://example.com/a."), "see <https://example.com/a>.");
        assert_eq!(linkify("(https://x.io/b),"), "(https://x.io/b),");
        assert_eq!(linkify("a https://x.io/b) c"), "a <https://x.io/b>) c");
        assert_eq!(linkify("[docs](https://example.com/a)"), "[docs](https://example.com/a)");
        assert_eq!(linkify("`https://in.code`"), "`https://in.code`");
        assert_eq!(linkify("![alt](https://x/p.png)"), "!\\[alt](https://x/p.png)");
        assert_eq!(linkify("and/or 10/12"), "and/or 10/12");
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
