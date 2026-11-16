//! The Git diff window's rules (the web's feOpenDiff, simpleDiff, myersMiddle,
//! wordDiffHtml and stageChunk): a line diff of the old and new text, its
//! changes grouped into chunks, the two columns' rows, the changed words of
//! a replaced line, and a chunk as a patch for /api/git/stageHunk.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op { Equal(String), Remove(String), Add(String) }

impl Op {
    pub fn text(&self) -> &str { match self { Op::Equal(t) | Op::Remove(t) | Op::Add(t) => t } }
    fn is_equal(&self) -> bool { matches!(self, Op::Equal(_)) }
}

/// Split on any line ending (CRLF in the working tree, LF in git's blobs).
pub fn lines(s: &str) -> Vec<String> {
    s.replace("\r\n", "\n").replace('\r', "\n").split('\n').map(str::to_string).collect()
}

/// Myers' O(ND) diff of the middle (no common prefix or suffix).
fn myers_middle(a: &[String], b: &[String]) -> Vec<Op> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    if n == 0 && m == 0 { return vec![]; }
    if n == 0 { return b.iter().map(|l| Op::Add(l.clone())).collect(); }
    if m == 0 { return a.iter().map(|l| Op::Remove(l.clone())).collect(); }
    let max = n + m;
    let off = max + 1;
    let mut v = vec![-1isize; (2 * max + 3) as usize];
    v[(1 + off) as usize] = 0;
    let mut trace: Vec<Vec<isize>> = vec![];
    let mut reached = -1;
    for d in 0..=max {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let vm = v[(k - 1 + off) as usize];
            let vp = v[(k + 1 + off) as usize];
            let mut x = if k == -d || (k != d && vm < vp) { vp.max(0) } else { vm.max(0) + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] { x += 1; y += 1; }
            v[(k + off) as usize] = x;
            if x >= n && y >= m { reached = d; break; }
            k += 2;
        }
        if reached != -1 { break; }
    }
    let mut ops = vec![];
    let (mut x, mut y) = (n, m);
    let mut d = reached;
    while d > 0 {
        let vp = &trace[d as usize];
        let k = x - y;
        let at = |kk: isize| vp[(kk + off) as usize];
        let pk = if k == -d || (k != d && at(k - 1) < at(k + 1)) { k + 1 } else { k - 1 };
        let px = at(pk).max(0);
        let py = px - pk;
        while x > px && y > py { ops.push(Op::Equal(a[(x - 1) as usize].clone())); x -= 1; y -= 1; }
        if x == px { ops.push(Op::Add(b[(y - 1) as usize].clone())); y -= 1; } else { ops.push(Op::Remove(a[(x - 1) as usize].clone())); x -= 1; }
        d -= 1;
    }
    while x > 0 && y > 0 { ops.push(Op::Equal(a[(x - 1) as usize].clone())); x -= 1; y -= 1; }
    while y > 0 { ops.push(Op::Add(b[(y - 1) as usize].clone())); y -= 1; }
    while x > 0 { ops.push(Op::Remove(a[(x - 1) as usize].clone())); x -= 1; }
    ops.reverse();
    ops
}

/// The line diff: the common prefix and suffix trimmed, Myers in between.
pub fn diff(a: &[String], b: &[String]) -> Vec<Op> {
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] { start += 1; }
    let (mut ea, mut eb) = (a.len(), b.len());
    while ea > start && eb > start && a[ea - 1] == b[eb - 1] { ea -= 1; eb -= 1; }
    let mut ops: Vec<Op> = a[..start].iter().map(|l| Op::Equal(l.clone())).collect();
    ops.extend(myers_middle(&a[start..ea], &b[start..eb]));
    ops.extend(a[ea..].iter().map(|l| Op::Equal(l.clone())));
    ops
}

/// A run of changes: from op `start` up to (not including) `end`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk { pub start: usize, pub end: usize }

pub fn chunks(ops: &[Op]) -> Vec<Chunk> {
    let mut out = vec![];
    let mut cur: Option<usize> = None;
    for (i, op) in ops.iter().enumerate() {
        if !op.is_equal() { if cur.is_none() { cur = Some(i); } }
        else if let Some(s) = cur.take() { out.push(Chunk { start: s, end: i }); }
    }
    if let Some(s) = cur { out.push(Chunk { start: s, end: ops.len() }); }
    out
}

/// One row of the two columns: a chunk's head (its index), or a line on
/// each side ("" number and kind "pad" where that side has none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub chunk: i32,
    pub old_ln: String,
    pub old: String,
    /// equal | del | pad
    pub old_kind: &'static str,
    pub new_ln: String,
    pub new: String,
    /// equal | add | pad
    pub new_kind: &'static str,
    /// The changed words of a replaced line (byte ranges in old / new).
    pub old_words: Vec<(usize, usize)>,
    pub new_words: Vec<(usize, usize)>,
}

fn row(old_ln: String, old: &str, old_kind: &'static str, new_ln: String, new: &str, new_kind: &'static str) -> Row {
    Row { chunk: -1, old_ln, old: old.to_string(), old_kind, new_ln, new: new.to_string(), new_kind, old_words: vec![], new_words: vec![] }
}

/// The rows of the side-by-side view (feOpenDiff's desktop layout): a chunk
/// head before each chunk; a removed line followed by an added one share a
/// row, with their changed words.
pub fn rows(ops: &[Op]) -> Vec<Row> {
    let ch = chunks(ops);
    let mut out = vec![];
    let (mut o, mut n) = (1, 1);
    let mut i = 0;
    let mut ci = 0;
    while i < ops.len() {
        if ci < ch.len() && ch[ci].start == i {
            out.push(Row { chunk: ci as i32, ..row(String::new(), "", "pad", String::new(), "", "pad") });
            ci += 1;
        }
        match &ops[i] {
            Op::Equal(t) => { out.push(row(o.to_string(), t, "equal", n.to_string(), t, "equal")); o += 1; n += 1; }
            // A run of removed lines and the run of added lines after it
            // pair up row by row (old 2 beside new 2…), with their changed
            // words; the longer run's extra lines face an empty filler.
            Op::Remove(_) => {
                let mut j = i;
                while matches!(ops.get(j), Some(Op::Remove(_))) { j += 1; }
                let mut k = j;
                while matches!(ops.get(k), Some(Op::Add(_))) { k += 1; }
                let (rem, add) = (&ops[i..j], &ops[j..k]);
                for x in 0..rem.len().max(add.len()) {
                    match (rem.get(x), add.get(x)) {
                        (Some(r), Some(a)) => {
                            let (ow, nw) = word_diff(r.text(), a.text());
                            out.push(Row { old_words: ow, new_words: nw, ..row(o.to_string(), r.text(), "del", n.to_string(), a.text(), "add") });
                            o += 1; n += 1;
                        }
                        (Some(r), None) => { out.push(row(o.to_string(), r.text(), "del", String::new(), "", "pad")); o += 1; }
                        (None, Some(a)) => { out.push(row(String::new(), "", "pad", n.to_string(), a.text(), "add")); n += 1; }
                        (None, None) => {}
                    }
                }
                i = k - 1;
            }
            Op::Add(t) => { out.push(row(String::new(), "", "pad", n.to_string(), t, "add")); n += 1; }
        }
        i += 1;
    }
    out
}

fn tokens(s: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start = 0;
    let mut ws: Option<bool> = None;
    for (i, c) in s.char_indices() {
        let w = c.is_whitespace();
        if ws.is_some_and(|p| p != w) { out.push((start, i)); start = i; }
        ws = Some(w);
    }
    if start < s.len() { out.push((start, s.len())); }
    out
}

/// The changed words of a replaced line (wordDiffHtml): an LCS over words
/// and runs of space; the byte ranges only in old, and only in new.
pub fn word_diff(old: &str, new: &str) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let a = tokens(old);
    let b = tokens(new);
    let (na, nb) = (a.len(), b.len());
    if na * nb > 40_000 { return (vec![(0, old.len())], vec![(0, new.len())]); }
    let ta = |i: usize| &old[a[i].0..a[i].1];
    let tb = |j: usize| &new[b[j].0..b[j].1];
    let mut dp = vec![vec![0u16; nb + 1]; na + 1];
    for i in 1..=na { for j in 1..=nb {
        dp[i][j] = if ta(i - 1) == tb(j - 1) { dp[i - 1][j - 1] + 1 } else { dp[i - 1][j].max(dp[i][j - 1]) };
    } }
    let (mut ow, mut nw) = (vec![], vec![]);
    let (mut i, mut j) = (na, nb);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && ta(i - 1) == tb(j - 1) { i -= 1; j -= 1; }
        else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) { nw.push(b[j - 1]); j -= 1; }
        else { ow.push(a[i - 1]); i -= 1; }
    }
    ow.reverse(); nw.reverse();
    (ow, nw)
}

/// A chunk as a unified patch (stageChunk): up to 3 lines of context each
/// side, the hunk header from the line numbers at its start.
pub fn chunk_patch(file: &str, ops: &[Op], ch: &Chunk) -> String {
    let (mut o, mut n) = (1usize, 1usize);
    for op in &ops[..ch.start] {
        match op { Op::Equal(_) => { o += 1; n += 1; } Op::Remove(_) => o += 1, Op::Add(_) => n += 1 }
    }
    let mut body = String::new();
    let (mut before, mut after, mut rem, mut add) = (0, 0, 0, 0);
    for op in &ops[ch.start.saturating_sub(3)..ch.start] {
        if let Op::Equal(t) = op { body.push_str(&format!(" {t}\n")); before += 1; }
    }
    for op in &ops[ch.start..ch.end] {
        match op { Op::Remove(t) => { body.push_str(&format!("-{t}\n")); rem += 1; } Op::Add(t) => { body.push_str(&format!("+{t}\n")); add += 1; } _ => {} }
    }
    for op in &ops[ch.end..(ch.end + 3).min(ops.len())] {
        if let Op::Equal(t) = op { body.push_str(&format!(" {t}\n")); after += 1; }
    }
    let header = format!("@@ -{},{} +{},{} @@\n", o - before, before + rem + after, n - before, before + add + after);
    format!("diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n{header}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(s: &[&str]) -> Vec<String> { s.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn diffs() {
        let a = l(&["a", "b", "c", "d"]);
        let b = l(&["a", "x", "c", "d", "e"]);
        let ops = diff(&a, &b);
        assert_eq!(ops, vec![Op::Equal("a".into()), Op::Remove("b".into()), Op::Add("x".into()), Op::Equal("c".into()), Op::Equal("d".into()), Op::Add("e".into())]);
        assert_eq!(chunks(&ops), vec![Chunk { start: 1, end: 3 }, Chunk { start: 5, end: 6 }]);
        // Every old line in order, every new line in order.
        let olds: Vec<&str> = ops.iter().filter(|o| !matches!(o, Op::Add(_))).map(|o| o.text()).collect();
        assert_eq!(olds, vec!["a", "b", "c", "d"]);
        assert!(diff(&a, &a).iter().all(|o| matches!(o, Op::Equal(_))));
        assert_eq!(diff(&[], &l(&["z"])), vec![Op::Add("z".into())]);
    }

    #[test]
    fn crlf() { assert_eq!(lines("a\r\nb\rc\nd"), l(&["a", "b", "c", "d"])); }

    #[test]
    fn side_by_side() {
        let ops = diff(&l(&["f(1)", "keep", "gone"]), &l(&["f(2)", "keep"]));
        let r = rows(&ops);
        assert_eq!(r[0].chunk, 0);
        assert_eq!((r[1].old_kind, r[1].new_kind, r[1].old_ln.as_str(), r[1].new_ln.as_str()), ("del", "add", "1", "1"));
        assert_eq!(r[2].old_kind, "equal");
        assert_eq!(r[3].chunk, 1);
        assert_eq!((r[4].old_kind, r[4].new_kind, r[4].new_ln.as_str()), ("del", "pad", ""));
    }

    #[test]
    fn words() {
        let (o, n) = word_diff("return \"Hello, \" + name;", "return \"Hi, \" + name;");
        assert_eq!(o.len(), 1);
        assert_eq!(&"return \"Hello, \" + name;"[o[0].0..o[0].1], "\"Hello,");
        assert_eq!(&"return \"Hi, \" + name;"[n[0].0..n[0].1], "\"Hi,");
    }

    #[test]
    fn patch() {
        let a = l(&["1", "2", "3", "4", "5", "6"]);
        let b = l(&["1", "2", "3", "four", "5", "6"]);
        let ops = diff(&a, &b);
        let ch = &chunks(&ops)[0];
        assert_eq!(chunk_patch("f.txt", &ops, ch), "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,6 +1,6 @@\n 1\n 2\n 3\n-4\n+four\n 5\n 6\n");
    }
}
