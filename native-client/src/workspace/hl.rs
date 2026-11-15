//! Syntax highlighting for the file editor and viewer (the web's CodeMirror
//! with its oneDark theme): a small tokenizer per language family, each
//! line turned into runs of a token kind. The colours are the theme's
//! (`Theme.ws-hl-*`, read once at start); StyledText draws a line from
//! `line_markup`.

/// A token's kind; its colour is the theme's for that kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok { Plain, Keyword, Str, Comment, Number, Func, Type, Prop, Op, Tag, Attr, Heading }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang { None, CLike, Rust, Python, Shell, Css, Markup, Json, Yaml, Markdown, Sql, Toml }

/// The language of a file, by its name (CodeMirror's getLangExtension).
pub fn lang_of(name: &str) -> Lang {
    let lower = name.to_ascii_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or("");
    let ext = base.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if base == "dockerfile" || base.starts_with("dockerfile.") || base == "makefile" { return Lang::Shell; }
    match ext {
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "java" | "c" | "h" | "cpp" | "cc" | "hpp" | "cs" | "go" | "kt" | "swift" | "dart" | "php" | "scala" | "slint" => Lang::CLike,
        "rs" => Lang::Rust,
        "py" | "pyw" => Lang::Python,
        "sh" | "bash" | "zsh" | "ps1" | "env" => Lang::Shell,
        "css" | "scss" | "less" => Lang::Css,
        "html" | "htm" | "xml" | "svg" | "vue" | "svelte" => Lang::Markup,
        "json" | "jsonc" => Lang::Json,
        "yaml" | "yml" => Lang::Yaml,
        "md" | "markdown" => Lang::Markdown,
        "sql" => Lang::Sql,
        "toml" | "ini" | "cfg" | "conf" => Lang::Toml,
        _ if base.starts_with(".env") => Lang::Shell,
        _ => Lang::None,
    }
}

const KW_CLIKE: &[&str] = &["abstract", "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for", "from", "func", "function", "go", "if", "implements", "import", "in", "instanceof", "interface", "let", "new", "null", "of", "package", "private", "protected", "public", "return", "static", "struct", "super", "switch", "this", "throw", "true", "try", "type", "typeof", "undefined", "var", "void", "while", "with", "yield", "int", "char", "float", "double", "long", "short", "unsigned", "signed", "bool", "boolean", "void", "namespace", "using", "template", "typename", "nil", "defer", "chan", "map", "range", "select", "fallthrough", "goto", "sizeof", "include", "define", "component", "property", "callback", "inherits", "global", "in-out", "out"];
const KW_RUST: &[&str] = &["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while"];
const KW_PY: &[&str] = &["and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield", "self"];
const KW_SH: &[&str] = &["if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in", "function", "return", "local", "export", "readonly", "set", "unset", "source", "echo", "exit", "cd", "FROM", "RUN", "CMD", "COPY", "ADD", "ENV", "WORKDIR", "EXPOSE", "ENTRYPOINT", "ARG", "USER", "VOLUME", "LABEL"];
const KW_SQL: &[&str] = &["select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table", "drop", "alter", "add", "index", "primary", "key", "foreign", "references", "not", "null", "and", "or", "join", "left", "right", "inner", "outer", "on", "group", "by", "order", "having", "limit", "offset", "as", "distinct", "union", "all", "default", "unique", "integer", "text", "varchar", "boolean", "true", "false", "begin", "commit", "if", "exists"];

/// The runs of one line: (kind, text), in order; together the line.
/// `state` carries a block comment (or a markup comment, a Python triple
/// string) from one line to the next: call with each line in turn.
pub fn line_runs(lang: Lang, line: &str, state: &mut u8) -> Vec<(Tok, String)> {
    let mut out: Vec<(Tok, String)> = vec![];
    let push = |out: &mut Vec<(Tok, String)>, t: Tok, s: &str| {
        if s.is_empty() { return; }
        if let Some(last) = out.last_mut() { if last.0 == t { last.1.push_str(s); return; } }
        out.push((t, s.to_string()));
    };
    let c: Vec<char> = line.chars().collect();
    let n = c.len();
    let sub = |a: usize, b: usize| -> String { c[a..b].iter().collect() };
    let starts = |i: usize, p: &str| -> bool { let pc: Vec<char> = p.chars().collect(); i + pc.len() <= n && c[i..i + pc.len()] == pc[..] };
    let mut i = 0;
    match lang {
        Lang::None => { push(&mut out, Tok::Plain, line); return out; }
        Lang::Markdown => {
            let t = line.trim_start();
            if t.starts_with('#') { push(&mut out, Tok::Heading, line); }
            else if t.starts_with("```") { push(&mut out, Tok::Str, line); }
            else if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("> ") {
                let lead = line.len() - t.len();
                push(&mut out, Tok::Plain, &line[..lead]);
                push(&mut out, Tok::Keyword, &t[..2]);
                push(&mut out, Tok::Plain, &t[2..]);
            } else {
                // `code` spans in the string colour.
                let mut code = false;
                for part in line.split_inclusive('`') {
                    let tick = part.ends_with('`');
                    if code { push(&mut out, Tok::Str, part); }
                    else if tick { push(&mut out, Tok::Plain, &part[..part.len() - 1]); push(&mut out, Tok::Str, "`"); }
                    else { push(&mut out, Tok::Plain, part); }
                    if tick { code = !code; }
                }
            }
            return out;
        }
        _ => {}
    }
    // A comment carried on from the line before.
    if *state == 1 || *state == 2 || *state == 3 {
        let close = match *state { 1 => "*/", 2 => "-->", _ => "\"\"\"" };
        let mut j = 0;
        while j < n && !starts(j, close) { j += 1; }
        if j < n {
            let end = j + close.chars().count();
            push(&mut out, if *state == 3 { Tok::Str } else { Tok::Comment }, &sub(0, end));
            *state = 0;
            i = end;
        } else {
            push(&mut out, if *state == 3 { Tok::Str } else { Tok::Comment }, line);
            return out;
        }
    }
    let line_comment: &[&str] = match lang {
        Lang::CLike | Lang::Rust => &["//"],
        Lang::Python | Lang::Shell | Lang::Yaml | Lang::Toml => &["#"],
        Lang::Sql => &["--"],
        Lang::Css => &[],
        _ => &[],
    };
    let block = matches!(lang, Lang::CLike | Lang::Rust | Lang::Css | Lang::Sql);
    let kws: &[&str] = match lang { Lang::CLike => KW_CLIKE, Lang::Rust => KW_RUST, Lang::Python => KW_PY, Lang::Shell => KW_SH, Lang::Sql => KW_SQL, _ => &[] };
    // YAML / TOML: a key before ':' or '=' at the line's start.
    if matches!(lang, Lang::Yaml | Lang::Toml) {
        let t = line.trim_start();
        if !t.starts_with('#') {
            if lang == Lang::Toml && t.starts_with('[') { push(&mut out, Tok::Heading, line); return out; }
            let sep = if lang == Lang::Yaml { ':' } else { '=' };
            if let Some(p) = t.find(sep) {
                let key = &t[..p];
                if !key.trim().is_empty() && !key.contains(['"', '\'']) && (lang == Lang::Toml || !key.trim_start_matches("- ").contains(' ')) {
                    let lead = line.len() - t.len();
                    push(&mut out, Tok::Plain, &line[..lead]);
                    let (k, rest) = key.split_at(key.trim_end().len());
                    let k = k.strip_prefix("- ").map(|s| { push(&mut out, Tok::Op, "- "); s }).unwrap_or(k);
                    push(&mut out, Tok::Prop, k);
                    push(&mut out, Tok::Plain, rest);
                    push(&mut out, Tok::Op, &t[p..p + 1]);
                    let start = line.len() - t.len() + p + 1;
                    i = line[..start].chars().count();
                }
            }
        }
    }
    while i < n {
        let ch = c[i];
        // Markup: tags, attributes, comments.
        if lang == Lang::Markup {
            if starts(i, "<!--") {
                let mut j = i + 4;
                while j < n && !starts(j, "-->") { j += 1; }
                if j < n { push(&mut out, Tok::Comment, &sub(i, j + 3)); i = j + 3; } else { push(&mut out, Tok::Comment, &sub(i, n)); *state = 2; i = n; }
                continue;
            }
            if ch == '<' {
                let mut j = i + 1;
                if j < n && (c[j] == '/' || c[j] == '?' || c[j] == '!') { j += 1; }
                push(&mut out, Tok::Op, &sub(i, j));
                let s = j;
                while j < n && (c[j].is_alphanumeric() || c[j] == '-' || c[j] == ':' || c[j] == '_') { j += 1; }
                push(&mut out, Tok::Tag, &sub(s, j));
                // Attributes up to '>'.
                while j < n && c[j] != '>' {
                    if c[j] == '"' || c[j] == '\'' {
                        let q = c[j]; let s = j; j += 1;
                        while j < n && c[j] != q { j += 1; }
                        j = (j + 1).min(n);
                        push(&mut out, Tok::Str, &sub(s, j));
                    } else if c[j].is_alphabetic() {
                        let s = j;
                        while j < n && (c[j].is_alphanumeric() || c[j] == '-' || c[j] == ':' || c[j] == '_') { j += 1; }
                        push(&mut out, Tok::Attr, &sub(s, j));
                    } else { push(&mut out, Tok::Plain, &sub(j, j + 1)); j += 1; }
                }
                if j < n { push(&mut out, Tok::Op, ">"); j += 1; }
                i = j;
                continue;
            }
            let s = i;
            while i < n && c[i] != '<' { i += 1; }
            push(&mut out, Tok::Plain, &sub(s, i));
            continue;
        }
        if block && starts(i, "/*") {
            let mut j = i + 2;
            while j < n && !starts(j, "*/") { j += 1; }
            if j < n { push(&mut out, Tok::Comment, &sub(i, j + 2)); i = j + 2; } else { push(&mut out, Tok::Comment, &sub(i, n)); *state = 1; i = n; }
            continue;
        }
        if line_comment.iter().any(|p| starts(i, p)) && !(lang == Lang::Shell && i > 0 && !c[i - 1].is_whitespace() && c[i - 1] != ';') {
            push(&mut out, Tok::Comment, &sub(i, n));
            break;
        }
        if lang == Lang::Python && (starts(i, "\"\"\"") || starts(i, "'''")) {
            let q: String = sub(i, i + 3);
            let mut j = i + 3;
            while j < n && !starts(j, &q) { j += 1; }
            if j < n { push(&mut out, Tok::Str, &sub(i, j + 3)); i = j + 3; } else { push(&mut out, Tok::Str, &sub(i, n)); *state = 3; i = n; }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            // A Rust lifetime ('a) is not a string.
            if lang == Lang::Rust && ch == '\'' && i + 2 < n && c[i + 1].is_alphabetic() && c[i + 2] != '\'' {
                let s = i; i += 1;
                while i < n && (c[i].is_alphanumeric() || c[i] == '_') { i += 1; }
                push(&mut out, Tok::Type, &sub(s, i));
                continue;
            }
            let s = i; i += 1;
            while i < n && c[i] != ch { if c[i] == '\\' { i += 1; } i += 1; }
            i = (i + 1).min(n);
            // A JSON key: a string followed by ':'.
            let mut k = i;
            while k < n && c[k] == ' ' { k += 1; }
            let tok = if lang == Lang::Json && k < n && c[k] == ':' { Tok::Prop } else { Tok::Str };
            push(&mut out, tok, &sub(s, i));
            continue;
        }
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < n && c[i + 1].is_ascii_digit() && (i == 0 || !c[i - 1].is_alphanumeric())) {
            if i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_') { push(&mut out, Tok::Plain, &sub(i, i + 1)); i += 1; continue; }
            let s = i;
            while i < n && (c[i].is_ascii_alphanumeric() || c[i] == '.' || c[i] == '_') { i += 1; }
            push(&mut out, Tok::Number, &sub(s, i));
            continue;
        }
        if ch.is_alphabetic() || ch == '_' || ch == '$' || (ch == '@' && lang == Lang::Css) || (ch == '-' && lang == Lang::Css && i + 1 < n && c[i + 1].is_alphabetic()) {
            let s = i; i += 1;
            while i < n && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '$' || (c[i] == '-' && lang == Lang::Css)) { i += 1; }
            let w = sub(s, i);
            let mut k = i;
            while k < n && c[k] == ' ' { k += 1; }
            let next = if k < n { c[k] } else { '\0' };
            let tok = if lang == Lang::Css {
                if w.starts_with('@') { Tok::Keyword } else if next == ':' { Tok::Prop } else { Tok::Plain }
            } else if lang == Lang::Json {
                if w == "true" || w == "false" || w == "null" { Tok::Keyword } else { Tok::Plain }
            } else if kws.iter().any(|k| if lang == Lang::Sql { k.eq_ignore_ascii_case(&w) } else { *k == w }) {
                Tok::Keyword
            } else if lang == Lang::Rust && next == '!' {
                Tok::Func
            } else if next == '(' {
                Tok::Func
            } else if i > w.chars().count() && s > 0 && c[s - 1] == '.' {
                Tok::Prop
            } else if w.chars().next().is_some_and(|f| f.is_uppercase()) && lang != Lang::Sql {
                Tok::Type
            } else { Tok::Plain };
            push(&mut out, tok, &w);
            continue;
        }
        if "+-*/%=<>!&|^~?:".contains(ch) {
            push(&mut out, Tok::Op, &ch.to_string());
            i += 1;
            continue;
        }
        push(&mut out, Tok::Plain, &ch.to_string());
        i += 1;
    }
    out
}

/// Escape a run for StyledText's markdown: every ASCII punctuation mark
/// backslashed, spaces as no-break spaces (markdown would drop a line's
/// leading ones, or make an indented code block), a tab as `tab` spaces.
pub fn escape_md(s: &str, tab: usize) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            ' ' => o.push('\u{a0}'),
            '\t' => { for _ in 0..tab { o.push('\u{a0}'); } }
            c if c.is_ascii_punctuation() => { o.push('\\'); o.push(c); }
            c if c.is_control() => {}
            c => o.push(c),
        }
    }
    o
}

/// One line as StyledText markup: each run in its kind's colour (`colour`
/// gives the hex for a kind); an empty line is a no-break space (a line
/// must keep its height).
pub fn line_markup(runs: &[(Tok, String)], colour: &dyn Fn(Tok) -> String) -> String {
    let mut o = String::new();
    for (t, s) in runs {
        let e = escape_md(s, 4);
        if e.is_empty() { continue; }
        o.push_str(&format!("<font color=\"{}\">{}</font>", colour(*t), e));
    }
    if o.is_empty() { o.push('\u{a0}'); }
    o
}

/// Every line of a text, highlighted: the markup of each.
pub fn highlight(lang: Lang, text: &str, colour: &dyn Fn(Tok) -> String) -> Vec<String> {
    let mut state = 0u8;
    text.split('\n').map(|l| {
        let l = l.strip_suffix('\r').unwrap_or(l);
        line_markup(&line_runs(lang, l, &mut state), colour)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang: Lang, line: &str) -> Vec<(Tok, String)> { let mut s = 0; line_runs(lang, line, &mut s) }

    #[test]
    fn langs() {
        assert_eq!(lang_of("src/app.js"), Lang::CLike);
        assert_eq!(lang_of("main.rs"), Lang::Rust);
        assert_eq!(lang_of("Dockerfile"), Lang::Shell);
        assert_eq!(lang_of("a/b/README.md"), Lang::Markdown);
        assert_eq!(lang_of("notes.txt"), Lang::None);
        assert_eq!(lang_of("icon.svg"), Lang::Markup);
    }

    #[test]
    fn js_line() {
        let r = kinds(Lang::CLike, "  const n = 43; // hi");
        assert!(r.contains(&(Tok::Keyword, "const".into())));
        assert!(r.contains(&(Tok::Number, "43".into())));
        assert!(r.contains(&(Tok::Comment, "// hi".into())));
        let r = kinds(Lang::CLike, "return \"Hi, \" + name;");
        assert!(r.contains(&(Tok::Str, "\"Hi, \"".into())));
        let r = kinds(Lang::CLike, "function hello(name) {");
        assert!(r.contains(&(Tok::Func, "hello".into())));
        // The runs make up the line.
        let line = "x.y = f(\"a\\\"b\") /* c */ + 1;";
        assert_eq!(kinds(Lang::CLike, line).iter().map(|r| r.1.as_str()).collect::<String>(), line);
    }

    #[test]
    fn block_comment_carries() {
        let mut s = 0;
        let a = line_runs(Lang::CLike, "a /* start", &mut s);
        assert_eq!(s, 1);
        assert_eq!(a.last().unwrap().0, Tok::Comment);
        let b = line_runs(Lang::CLike, "still */ b", &mut s);
        assert_eq!(s, 0);
        assert_eq!(b[0], (Tok::Comment, "still */".into()));
    }

    #[test]
    fn json_and_markup() {
        let r = kinds(Lang::Json, "  \"name\": \"demo\",");
        assert!(r.contains(&(Tok::Prop, "\"name\"".into())));
        assert!(r.contains(&(Tok::Str, "\"demo\"".into())));
        let r = kinds(Lang::Markup, "<a href=\"x\">t</a>");
        assert!(r.contains(&(Tok::Tag, "a".into())));
        assert!(r.contains(&(Tok::Attr, "href".into())));
    }

    #[test]
    fn markup_escapes() {
        assert_eq!(escape_md("  *a*", 4), "\u{a0}\u{a0}\\*a\\*");
        assert_eq!(escape_md("\tx", 2), "\u{a0}\u{a0}x");
        let m = line_markup(&[(Tok::Keyword, "fn".into())], &|_| "#fff".into());
        assert_eq!(m, "<font color=\"#fff\">fn</font>");
        assert_eq!(line_markup(&[], &|_| "#fff".into()), "\u{a0}");
    }
}
