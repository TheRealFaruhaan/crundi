//! A code block's syntax colours, as names (the chat area maps each to a
//! theme token). Syntect with bat's syntax set (two-face), the crates and
//! features gpui-component already builds, so nothing new is compiled; the
//! scope rules are a short form of gpui-component 0.10's syntect backend
//! (src/highlighter/syntect_backend.rs, Apache-2.0). Pure, and run where
//! the reply's blocks are cut (the background executor for a long reply).
//!
//! The owner's call (F19, 19 Nov): chat code blocks keep their colours, in
//! colours that read well on the dark background (theme tokens only).

use std::{ops::Range, sync::LazyLock};
use syntect::{
    parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

/// What a run of code is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Syn { Comment, String, Number, Keyword, Type, Function, Tag, Attribute }

/// Loading the set takes tens of milliseconds: `warm` does it on a
/// background thread at start, so no cut waits for it.
static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// Load the syntax set now, off the UI thread.
pub fn warm() { std::thread::spawn(|| { LazyLock::force(&SYNTAXES); }); }

fn find(lang: &str) -> Option<&'static SyntaxReference> {
    let l = lang.trim().to_ascii_lowercase();
    if l.is_empty() || l == "text" || l == "txt" || l == "plain" { return None; }
    let alias = match l.as_str() { "csharp" => "cs", "make" => "Makefile", "shell" | "zsh" | "console" => "sh", "jsonc" => "json", "ts" | "typescript" | "tsx" => "ts", "javascript" | "jsx" | "mjs" | "cjs" => "js", "golang" => "go", "rust" => "rs", "python" | "py3" => "py", "yml" => "yaml", other => other };
    let s = &*SYNTAXES;
    [lang, l.as_str(), alias].into_iter().find_map(|t| s.find_syntax_by_token(t)).filter(|x| x.name != "Plain Text")
}

/// The coloured runs of `code` in `lang` (ordered, not overlapping; what is
/// not in a run is plain text). An unknown language: none.
pub fn highlight(lang: &str, code: &str) -> Vec<(Range<usize>, Syn)> {
    let Some(syntax) = find(lang) else { return vec![] };
    let set = &*SYNTAXES;
    let mut parser = ParseState::new(syntax);
    let mut scopes = ScopeStack::new();
    let mut out: Vec<(Range<usize>, Syn)> = vec![];
    let mut names = std::collections::HashMap::<Scope, Option<Syn>>::new();
    let mut push = |r: Range<usize>, scopes: &ScopeStack, out: &mut Vec<(Range<usize>, Syn)>| {
        if r.is_empty() { return; }
        let Some(n) = scopes.scopes.iter().rev().find_map(|s| *names.entry(*s).or_insert_with(|| name(*s))) else { return };
        match out.last_mut() { Some((l, ln)) if l.end == r.start && *ln == n => l.end = r.end, _ => out.push((r, n)) }
    };
    let mut off = 0;
    for line in LinesWithEndings::from(code) {
        if let Ok(ops) = parser.parse_line(line, set) {
            let mut at = 0;
            for (i, op) in ops {
                push(off + at..off + i, &scopes, &mut out);
                let _ = scopes.apply(&op);
                at = i;
            }
            push(off + at..off + line.len(), &scopes, &mut out);
        }
        off += line.len();
    }
    // A run that ends in a line break stops before it (no colour past the text).
    out.retain(|(r, _)| !r.is_empty());
    out
}

fn name(scope: Scope) -> Option<Syn> {
    let s = scope.build_string();
    let is = |p: &str| s == p || s.strip_prefix(p).is_some_and(|r| r.starts_with('.'));
    Some(if is("punctuation.definition.string") || is("punctuation.definition.comment") || is("invalid") || is("meta") || is("source") || is("text") {
        return None;
    } else if is("comment") {
        Syn::Comment
    } else if is("string") || is("constant.character.escape") {
        Syn::String
    } else if is("constant.numeric") || is("constant.language") || is("constant") || is("support.constant") || is("entity.name.constant") {
        Syn::Number
    } else if is("keyword.operator") || is("punctuation") {
        return None;
    } else if is("keyword") || is("storage.modifier") {
        Syn::Keyword
    } else if is("storage.type") {
        if ["class", "enum", "function", "impl", "interface", "module", "namespace", "struct", "trait", "type", "let", "var", "const"].iter().any(|w| s.contains(w)) { Syn::Keyword } else { Syn::Type }
    } else if is("entity.name.function") || is("support.function") || is("variable.function") || is("meta.function-call") {
        Syn::Function
    } else if is("entity.name.type") || is("entity.name.class") || is("entity.name.struct") || is("entity.name.enum") || is("entity.name.trait") || is("entity.name.interface") || is("entity.other.inherited-class") || is("support.type") || is("support.class") {
        Syn::Type
    } else if is("entity.name.tag") {
        Syn::Tag
    } else if is("entity.other.attribute-name") {
        Syn::Attribute
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colours_a_snippet() {
        let code = "// hi\nconst a = \"x\";\nfunction f() { return 42; }\n";
        let h = highlight("js", code);
        let of = |t: &str| h.iter().find(|(r, _)| &code[r.clone()] == t).map(|x| x.1);
        assert_eq!(h.first().map(|x| x.1), Some(Syn::Comment));
        assert!(h.iter().any(|(r, s)| *s == Syn::String && code[r.clone()].contains('x')));
        assert_eq!(of("42"), Some(Syn::Number));
        assert!(h.iter().any(|(_, s)| *s == Syn::Keyword));
        assert!(highlight("nope-lang", code).is_empty());
        assert!(highlight("", code).is_empty());
    }
}
