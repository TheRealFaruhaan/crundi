//! The Mind map's pure rules (src/webapp-html.js): the nodes from the
//! server, the project filter (mmNodePassesProject), the tree's layout
//! (layoutMindmap: columns by depth, parents centred on their children, the
//! "add idea" boxes, the idea being typed and a drag's placeholder), the
//! links, the keyboard's steps (mmStepTarget), the filter (nodeMatches) and
//! where a drag would land (mindmapDragHandlers). Unit-tested below.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const DRAFT: &str = "__draft__";
pub const DROP: &str = "__drop__";
pub const ADD: &str = "__add__";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub id: String,
    pub text: String,
    pub parent: Option<String>,
    pub notes: Vec<String>,
    /// Its own scope (a root's creation scope, or "Scope to project").
    pub project: String,
    /// The resolved scope: link > parent > own ("" general).
    pub eff: String,
    pub linked: bool,
    pub info: Value,
    /// Typed and shown, not saved yet (a temporary id).
    pub pending: bool,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// The nodes of GET /api/mindmap ({ ok, mindmap: { nodes } }).
pub fn parse(v: &Value) -> Vec<Node> {
    let Some(a) = v.get("mindmap").and_then(|m| m.get("nodes")).and_then(|n| n.as_array()) else { return vec![] };
    a.iter().map(|n| Node {
        id: s(n, "id"),
        text: s(n, "text"),
        parent: n.get("parentId").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(String::from),
        notes: n.get("notes").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
        project: s(n, "project"),
        eff: s(n, "effectiveProject"),
        linked: n.get("linkedTask").map(|l| !l.is_null()).unwrap_or(false),
        info: n.get("linkedTaskInfo").cloned().unwrap_or(Value::Null),
        pending: false,
    }).collect()
}

/// A node is hidden if its project or any ancestor's is toggled off (hiding
/// a project hides its whole subtree).
pub fn passes(nodes: &[Node], hidden: &HashSet<String>, n: &Node) -> bool {
    if hidden.is_empty() { return true; }
    let mut seen = HashSet::new();
    let mut cur = Some(n);
    while let Some(c) = cur {
        if !seen.insert(c.id.clone()) { break; }
        if hidden.contains(&c.eff) { return false; }
        cur = c.parent.as_ref().and_then(|p| nodes.iter().find(|x| &x.id == p));
    }
    true
}

/// The project bar's keys: every scope in use, projects by name, General
/// ("") last. Empty when there is no project at all (the bar hides).
pub fn project_keys(nodes: &[Node]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for n in nodes { if !keys.contains(&n.eff) { keys.push(n.eff.clone()); } }
    if keys.iter().all(|k| k.is_empty()) { return vec![]; }
    keys.sort_by(|a, b| match (a.is_empty(), b.is_empty()) { (true, false) => std::cmp::Ordering::Greater, (false, true) => std::cmp::Ordering::Less, _ => a.cmp(b) });
    keys
}

/// The id and everything under it.
pub fn subtree(nodes: &[Node], id: &str) -> HashSet<String> {
    let mut gone: HashSet<String> = [id.to_string()].into();
    let mut grew = true;
    while grew {
        grew = false;
        for n in nodes {
            if let Some(p) = &n.parent { if gone.contains(p) && !gone.contains(&n.id) { gone.insert(n.id.clone()); grew = true; } }
        }
    }
    gone
}

/// Is `id` the node `of` or under it?
pub fn is_under(nodes: &[Node], id: &str, of: &str) -> bool {
    let mut cur = nodes.iter().find(|n| n.id == id);
    let mut hops = 0;
    while let Some(c) = cur {
        if c.id == of { return true; }
        hops += 1;
        if hops > nodes.len() { break; }
        cur = c.parent.as_ref().and_then(|p| nodes.iter().find(|x| &x.id == p));
    }
    false
}

/// The filter (nodeMatches): words, notes, the linked task's words.
pub fn matches(n: &Node, q: &str) -> bool {
    if n.text.to_lowercase().contains(q) { return true; }
    if n.notes.join(" ").to_lowercase().contains(q) { return true; }
    let i = &n.info;
    if !i.is_null() {
        let all = format!("{} {} {} {}", s(i, "title"), s(i, "text"), s(i, "taskTitle"), s(i, "project"));
        if all.to_lowercase().contains(q) { return true; }
    }
    false
}

/// The web's trunc: at most `m` characters, the last one an ellipsis.
pub fn trunc(t: &str, m: usize) -> String {
    let c: Vec<char> = t.chars().collect();
    if c.len() > m { c[..m - 1].iter().collect::<String>() + "…" } else { t.to_string() }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draft { pub parent: Option<String>, pub after: Option<String> }

#[derive(Clone, Debug, PartialEq)]
pub struct DropAt { pub parent: Option<String>, pub index: usize, pub drag: String, pub h: f32 }

#[derive(Clone, Copy, Debug)]
pub struct Dims { pub col: f32, pub node: f32, pub pad: f32, pub gap: f32, pub min_h: f32 }

impl Default for Dims {
    fn default() -> Self { Dims { col: 235.0, node: 180.0, pad: 24.0, gap: 16.0, min_h: 48.0 } }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind { Node, Ghost, Draft, Drop }

#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub id: String,
    pub kind: Kind,
    /// A ghost's idea; a node's parent shown on the map.
    pub parent: Option<String>,
    pub depth: usize,
    pub x: f32,
    pub y: f32,
    pub h: f32,
    pub kids: bool,
    pub root: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub items: Vec<Placed>,
    /// SVG path data: the links to ideas, and the dashed ones to "add idea".
    pub edges: String,
    pub dashes: String,
    pub w: f32,
    pub h: f32,
    /// The draft's parent is gone: the draft closes.
    pub draft_lost: bool,
}

impl Layout {
    pub fn get(&self, id: &str) -> Option<&Placed> { self.items.iter().find(|p| p.id == id) }
}

fn edge(x1: f32, y1: f32, x2: f32, y2: f32) -> String {
    let m = (x1 + x2) / 2.0;
    format!("M {x1:.1} {y1:.1} C {m:.1} {y1:.1} {m:.1} {y2:.1} {x2:.1} {y2:.1} ")
}

/// A dashed link (stroke-dasharray 5 5): the same curve, as short strokes.
pub fn dashed_path(x1: f32, y1: f32, x2: f32, y2: f32, on: f32, off: f32) -> String {
    let m = (x1 + x2) / 2.0;
    let at = |t: f32| {
        let u = 1.0 - t;
        let x = u * u * u * x1 + 3.0 * u * u * t * m + 3.0 * u * t * t * m + t * t * t * x2;
        let y = u * u * u * y1 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y2;
        (x, y)
    };
    // The curve as a polyline, then cut into dashes along its length.
    let n = 48;
    let pts: Vec<(f32, f32)> = (0..=n).map(|i| at(i as f32 / n as f32)).collect();
    let mut out = String::new();
    let (mut pos, mut drawing, mut left) = (0.0f32, true, on);
    out += &format!("M {:.1} {:.1} ", pts[0].0, pts[0].1);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let mut done = 0.0f32;
        while len - done > 1e-3 {
            let step = left.min(len - done);
            done += step;
            left -= step;
            pos += step;
            let t = done / len;
            let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            out += &format!("{} {:.1} {:.1} ", if drawing { "L" } else { "M" }, p.0, p.1);
            if left <= 1e-3 { drawing = !drawing; left = if drawing { on } else { off }; }
        }
    }
    let _ = pos;
    out
}

/// The web's layoutMindmap: who is shown, where, and the links between.
/// `collapsed` loses the draft's and the drop's parent (they open it).
#[allow(clippy::too_many_arguments)]
pub fn layout(all: &[Node], collapsed: &mut HashSet<String>, hidden: &HashSet<String>, draft: Option<&Draft>, drop: Option<&DropAt>, heights: &HashMap<String, f32>, d: &Dims) -> Layout {
    let nodes: Vec<&Node> = all.iter().filter(|n| passes(all, hidden, n)).collect();
    let by: HashMap<&str, &Node> = nodes.iter().map(|n| (n.id.as_str(), *n)).collect();
    // Ids in order: children of each parent, and the roots.
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots: Vec<String> = vec![];
    let mut parent_of: HashMap<String, Option<String>> = HashMap::new();
    for n in &nodes {
        let p = n.parent.as_ref().filter(|p| by.contains_key(p.as_str())).cloned();
        match &p { Some(p) => children.entry(p.clone()).or_default().push(n.id.clone()), None => roots.push(n.id.clone()) }
        parent_of.insert(n.id.clone(), p);
    }
    let mut out = Layout::default();
    // The idea being typed: placed where it will be once saved.
    if let Some(dr) = draft {
        let pid = dr.parent.as_ref().filter(|p| by.contains_key(p.as_str())).cloned();
        if dr.parent.is_some() && pid.is_none() { out.draft_lost = true; } else {
            let list = match &pid { Some(p) => children.entry(p.clone()).or_default(), None => &mut roots };
            let at = dr.after.as_ref().and_then(|a| list.iter().position(|x| x == a)).map(|i| i + 1).unwrap_or(list.len());
            list.insert(at, DRAFT.to_string());
            parent_of.insert(DRAFT.to_string(), pid.clone());
            if let Some(p) = pid { collapsed.remove(&p); }
        }
    }
    // Where a dragged idea would land: a placeholder in the layout.
    if let Some(dp) = drop {
        let pid = dp.parent.as_ref().filter(|p| by.contains_key(p.as_str())).cloned();
        if dp.parent.is_none() || pid.is_some() {
            let list = match &pid { Some(p) => children.entry(p.clone()).or_default(), None => &mut roots };
            let others: Vec<String> = list.iter().filter(|x| **x != dp.drag && *x != DRAFT).cloned().collect();
            let at = others.get(dp.index).and_then(|r| list.iter().position(|x| x == r)).unwrap_or(list.len());
            list.insert(at, DROP.to_string());
            parent_of.insert(DROP.to_string(), pid.clone());
            if let Some(p) = pid { collapsed.remove(&p); }
        }
    }
    let visible = |id: &str| {
        let mut p = parent_of.get(id).cloned().flatten();
        let mut hops = 0;
        while let Some(pp) = p {
            if collapsed.contains(&pp) { return false; }
            hops += 1;
            if hops > all.len() + 2 { break; }
            p = parent_of.get(&pp).cloned().flatten();
        }
        true
    };
    let depth = |id: &str| {
        let mut dd = 0;
        let mut p = parent_of.get(id).cloned().flatten();
        while let Some(pp) = p { dd += 1; if dd > all.len() + 2 { break; } p = parent_of.get(&pp).cloned().flatten(); }
        dd
    };
    let mut vis: Vec<(String, Kind)> = nodes.iter().filter(|n| visible(&n.id)).map(|n| (n.id.clone(), Kind::Node)).collect();
    if parent_of.contains_key(DRAFT) { vis.push((DRAFT.to_string(), Kind::Draft)); }
    if parent_of.contains_key(DROP) { vis.push((DROP.to_string(), Kind::Drop)); }
    let mut pos: HashMap<String, (usize, f32)> = HashMap::new();
    let mut max_depth = 0;
    for (id, _) in &vis { let dd = depth(id); pos.insert(id.clone(), (dd, 0.0)); max_depth = max_depth.max(dd); }
    // An "add idea" box under every shown, open idea.
    let mut ghosts: Vec<(String, String)> = vec![];
    for (id, k) in &vis {
        if *k != Kind::Node || collapsed.contains(id) { continue; }
        let g = format!("{ADD}{id}");
        let dd = pos[id].0 + 1;
        pos.insert(g.clone(), (dd, 0.0));
        max_depth = max_depth.max(dd);
        ghosts.push((g, id.clone()));
    }
    let ghost_of: HashMap<String, String> = ghosts.iter().map(|(g, p)| (p.clone(), g.clone())).collect();
    let real_kids = |id: &str| -> Vec<String> { if collapsed.contains(id) { vec![] } else { children.get(id).cloned().unwrap_or_default() } };
    let height = |id: &str| -> f32 {
        if id == DROP { return heights.get(id).copied().unwrap_or_else(|| drop.map(|x| x.h.max(30.0)).unwrap_or(d.min_h)); }
        heights.get(id).copied().filter(|h| *h > 0.0).unwrap_or(d.min_h)
    };
    // Pack the middles, leaves top to bottom; a parent sits in the middle of
    // its real children (its trailing "add idea" takes a row of its own
    // but does not pull the parent down).
    let mut cursor = d.pad;
    fn place(id: &str, cursor: &mut f32, pos: &mut HashMap<String, (usize, f32)>, real_kids: &dyn Fn(&str) -> Vec<String>, ghost_of: &HashMap<String, String>, height: &dyn Fn(&str) -> f32, gap: f32) -> f32 {
        let real = real_kids(id);
        let mut kids: Vec<(String, bool)> = real.iter().map(|k| (k.clone(), false)).collect();
        if !real.is_empty() { if let Some(g) = ghost_of.get(id) { kids.push((g.clone(), true)); } }
        let y = if !kids.is_empty() {
            let cs: Vec<(f32, bool)> = kids.iter().map(|(k, g)| (place(k, cursor, pos, real_kids, ghost_of, height, gap), *g)).collect();
            let rc: Vec<f32> = cs.iter().filter(|c| !c.1).map(|c| c.0).collect();
            let span: Vec<f32> = if rc.is_empty() { cs.iter().map(|c| c.0).collect() } else { rc };
            (span[0] + span[span.len() - 1]) / 2.0
        } else {
            let h = height(id);
            let y = *cursor + h / 2.0;
            *cursor += h + gap;
            y
        };
        if let Some(p) = pos.get_mut(id) { p.1 = y; }
        y
    }
    for r in roots.clone() { place(&r, &mut cursor, &mut pos, &real_kids, &ghost_of, &height, d.gap); }
    // A leaf's "add idea" sits beside it, on its row.
    for (g, p) in &ghosts {
        if real_kids(p).is_empty() { let y = pos[p].1; if let Some(x) = pos.get_mut(g) { x.1 = y; } }
    }
    out.h = (cursor + d.pad).max(120.0);
    out.w = d.pad + max_depth as f32 * d.col + d.node + d.pad;
    let px = |dd: usize| d.pad + dd as f32 * d.col;
    for (id, k) in &vis {
        let (dd, y) = pos[id];
        let p = parent_of.get(id).cloned().flatten();
        out.items.push(Placed { id: id.clone(), kind: k.clone(), root: p.is_none(), parent: p, depth: dd, x: px(dd), y, h: height(id), kids: children.get(id).map(|c| !c.is_empty()).unwrap_or(false) });
    }
    for (g, p) in &ghosts {
        let (dd, y) = pos[g];
        out.items.push(Placed { id: g.clone(), kind: Kind::Ghost, parent: Some(p.clone()), depth: dd, x: px(dd), y, h: height(g), kids: false, root: false });
    }
    // The links: solid to real children, dashed to the "add idea" boxes.
    for (id, _) in &vis {
        let (dd, y1) = pos[id];
        for k in real_kids(id) {
            if let Some((kd, y2)) = pos.get(&k) { if out.items.iter().any(|p| p.id == k) { out.edges += &edge(px(dd) + d.node, y1, px(*kd), *y2); } }
        }
    }
    for (g, p) in &ghosts {
        let (dd, y1) = pos[p];
        let (gd, y2) = pos[g];
        out.dashes += &dashed_path(px(dd) + d.node, y1, px(gd), y2, 5.0, 5.0);
    }
    out
}

/// The top of a box as drawn: centred on its middle, kept on the canvas.
pub fn top(p: &Placed) -> f32 { (p.y - p.h / 2.0).max(4.0) }

/// The idea to step to from `from` (mmStepTarget), "" for nowhere; and
/// whether `from` has to unfold first (right into a folded idea).
pub fn step(all: &[Node], hidden: &HashSet<String>, collapsed: &HashSet<String>, lay: &Layout, from: &str, dir: &str) -> (String, bool) {
    let shown: Vec<&Placed> = lay.items.iter().filter(|p| p.kind == Kind::Node).collect();
    let Some(f) = shown.iter().find(|p| p.id == from) else {
        // Nowhere yet: the top-left idea.
        let first = shown.iter().min_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.y.partial_cmp(&b.y).unwrap()));
        return (first.map(|p| p.id.clone()).unwrap_or_default(), false);
    };
    match dir {
        "up" | "down" => {
            let mut level: Vec<&&Placed> = shown.iter().filter(|p| p.depth == f.depth).collect();
            level.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
            let i = level.iter().position(|p| p.id == from).unwrap_or(0) as i64 + if dir == "down" { 1 } else { -1 };
            if i < 0 { return (String::new(), false); }
            (level.get(i as usize).map(|p| p.id.clone()).unwrap_or_default(), false)
        }
        "left" => (f.parent.clone().filter(|p| shown.iter().any(|x| &x.id == p)).unwrap_or_default(), false),
        _ => {
            let kids: Vec<&Node> = all.iter().filter(|n| n.parent.as_deref() == Some(from) && passes(all, hidden, n)).collect();
            if kids.is_empty() { return (String::new(), false); }
            (kids[(kids.len() - 1) / 2].id.clone(), collapsed.contains(from))
        }
    }
}

/// Where a drag would land.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Put back where it was (over itself, or its own "add idea").
    Back,
    /// Over the gap it opened: as it was.
    Stay,
    /// The empty canvas: the top level.
    Root,
    /// The middle of an idea (or its "add idea"): a child of it, outlined.
    Child(String),
    /// The top or bottom edge of an idea: a sibling, at this index among the
    /// others under that parent.
    Sib(Option<String>, usize),
}

/// The drag over the canvas at (x, y), unscaled (mindmapDragHandlers.onMove).
pub fn drop_target(all: &[Node], lay: &Layout, node_w: f32, drag: &str, x: f32, y: f32) -> Target {
    let over = lay.items.iter().rev().find(|p| p.kind != Kind::Draft && x >= p.x && x <= p.x + node_w && y >= top(p) && y <= top(p) + p.h);
    let Some(o) = over else { return Target::Root };
    match o.kind {
        Kind::Drop => Target::Stay,
        Kind::Ghost => match &o.parent { Some(p) if p != drag && all.iter().any(|n| &n.id == p) => Target::Child(p.clone()), _ => Target::Back },
        _ if o.id == drag => Target::Back,
        _ => {
            let rel = (y - top(o)) / o.h.max(1.0);
            if !(0.30..=0.70).contains(&rel) {
                let parent = all.iter().find(|n| n.id == o.id).and_then(|n| n.parent.clone());
                let sibs: Vec<&Node> = all.iter().filter(|n| n.parent == parent && n.id != drag).collect();
                let at = sibs.iter().position(|n| n.id == o.id).unwrap_or(0);
                Target::Sib(parent, if rel < 0.30 { at } else { at + 1 })
            } else { Target::Child(o.id.clone()) }
        }
    }
}

/// Apply a move locally (as the web does before the server answers): the
/// node goes under `parent`, at `index` among the others there (or last).
/// False if it would go under itself.
pub fn apply_move(nodes: &mut Vec<Node>, id: &str, parent: Option<&str>, index: Option<usize>) -> bool {
    if let Some(p) = parent { if is_under(nodes, p, id) { return false; } }
    let Some(at) = nodes.iter().position(|n| n.id == id) else { return false };
    let mut node = nodes.remove(at);
    node.parent = parent.map(String::from);
    let sibs: Vec<usize> = nodes.iter().enumerate().filter(|(_, n)| n.parent.as_deref() == parent).map(|(i, _)| i).collect();
    match index.and_then(|i| sibs.get(i)) { Some(&i) => nodes.insert(i, node), None => nodes.push(node) }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn n(id: &str, parent: Option<&str>, eff: &str) -> Node { Node { id: id.into(), text: id.into(), parent: parent.map(String::from), eff: eff.into(), ..Default::default() } }

    fn tree() -> Vec<Node> { vec![n("trip", None, ""), n("flights", Some("trip"), ""), n("hotel", Some("trip"), ""), n("beach", Some("hotel"), ""), n("work", None, "demo")] }

    #[test]
    fn parses_the_server() {
        let v = json!({ "ok": true, "mindmap": { "nodes": [
            { "id": "a", "text": "A", "parentId": null, "notes": ["x"], "project": null, "effectiveProject": "demo", "linkedTask": { "project": "demo", "taskId": "t" }, "linkedTaskInfo": { "kind": "task", "title": "T" } },
            { "id": "b", "text": "B", "parentId": "a", "notes": [], "effectiveProject": null, "linkedTask": null } ] } });
        let ns = parse(&v);
        assert_eq!(ns.len(), 2);
        assert!(ns[0].linked && ns[0].eff == "demo" && ns[0].notes == vec!["x"]);
        assert_eq!(ns[1].parent.as_deref(), Some("a"));
        assert!(!ns[1].linked && ns[1].eff.is_empty());
    }

    #[test]
    fn hiding_a_project_hides_the_branch() {
        let mut ns = tree();
        ns.push(n("deep", Some("work"), ""));
        let hidden: HashSet<String> = ["demo".to_string()].into();
        assert!(!passes(&ns, &hidden, &ns[5]));
        assert!(passes(&ns, &hidden, &ns[0]));
        assert_eq!(project_keys(&ns), vec!["demo".to_string(), "".to_string()]);
        assert!(project_keys(&tree()[..4]).is_empty());
    }

    #[test]
    fn lays_out_columns_by_depth() {
        let ns = tree();
        let d = Dims::default();
        let l = layout(&ns, &mut HashSet::new(), &HashSet::new(), None, None, &HashMap::new(), &d);
        let p = |id: &str| l.get(id).unwrap().clone();
        assert_eq!(p("trip").x, 24.0);
        assert_eq!(p("flights").x, 24.0 + 235.0);
        assert_eq!(p("beach").x, 24.0 + 2.0 * 235.0);
        // Flights is a leaf: its own row; Hotel sits on Beach's row (its only
        // real child), Trip between Flights and Hotel.
        assert_eq!(p("flights").y, 24.0 + 24.0);
        assert_eq!(p("hotel").y, p("beach").y);
        assert_eq!(p("trip").y, (p("flights").y + p("hotel").y) / 2.0);
        // Ghosts: under every open idea; a leaf's beside it.
        assert_eq!(p("__add__flights").y, p("flights").y);
        assert_eq!(p("__add__flights").x, p("beach").x);
        assert!(p("__add__trip").y > p("hotel").y);
        assert!(p("trip").kids && p("trip").root && !p("flights").root);
        assert!(l.edges.matches('M').count() == 3);
        assert!(!l.dashes.is_empty());
        assert_eq!(l.w, 24.0 + 3.0 * 235.0 + 180.0 + 24.0);
    }

    #[test]
    fn folding_hides_the_branch() {
        let ns = tree();
        let mut c: HashSet<String> = ["trip".to_string()].into();
        let l = layout(&ns, &mut c, &HashSet::new(), None, None, &HashMap::new(), &Dims::default());
        assert!(l.get("flights").is_none() && l.get("__add__trip").is_none());
        assert!(l.get("trip").unwrap().kids);
        assert!(l.edges.is_empty());
    }

    #[test]
    fn the_draft_takes_its_place_and_opens_its_parent() {
        let ns = tree();
        let mut c: HashSet<String> = ["trip".to_string()].into();
        let dr = Draft { parent: Some("trip".into()), after: Some("flights".into()) };
        let l = layout(&ns, &mut c, &HashSet::new(), Some(&dr), None, &HashMap::new(), &Dims::default());
        assert!(c.is_empty());
        let (f, d, h) = (l.get("flights").unwrap(), l.get(DRAFT).unwrap(), l.get("hotel").unwrap());
        assert!(f.y < d.y && d.y < h.y && d.x == f.x);
        let lost = layout(&ns, &mut HashSet::new(), &HashSet::new(), Some(&Draft { parent: Some("gone".into()), after: None }), None, &HashMap::new(), &Dims::default());
        assert!(lost.draft_lost && lost.get(DRAFT).is_none());
    }

    #[test]
    fn a_drop_opens_a_gap() {
        let ns = tree();
        let dp = DropAt { parent: Some("trip".into()), index: 0, drag: "beach".into(), h: 40.0 };
        let l = layout(&ns, &mut HashSet::new(), &HashSet::new(), None, Some(&dp), &HashMap::new(), &Dims::default());
        let (g, f) = (l.get(DROP).unwrap(), l.get("flights").unwrap());
        assert!(g.y < f.y && g.h == 40.0);
    }

    #[test]
    fn measured_heights_pack_rows() {
        let ns = vec![n("a", None, ""), n("b", None, "")];
        let hs: HashMap<String, f32> = [("a".to_string(), 100.0)].into();
        let l = layout(&ns, &mut HashSet::new(), &HashSet::new(), None, None, &hs, &Dims::default());
        assert_eq!(l.get("a").unwrap().y, 24.0 + 50.0);
        assert_eq!(l.get("b").unwrap().y, 24.0 + 100.0 + 16.0 + 24.0);
    }

    #[test]
    fn steps_with_the_keyboard() {
        let ns = tree();
        let (h, c) = (HashSet::new(), HashSet::new());
        let l = layout(&ns, &mut c.clone(), &h, None, None, &HashMap::new(), &Dims::default());
        assert_eq!(step(&ns, &h, &c, &l, "", "down").0, "trip");
        assert_eq!(step(&ns, &h, &c, &l, "flights", "down").0, "hotel");
        assert_eq!(step(&ns, &h, &c, &l, "hotel", "up").0, "flights");
        assert_eq!(step(&ns, &h, &c, &l, "flights", "up").0, "");
        assert_eq!(step(&ns, &h, &c, &l, "trip", "down").0, "work");
        assert_eq!(step(&ns, &h, &c, &l, "beach", "left").0, "hotel");
        // Right: the middle child (the earlier of two).
        assert_eq!(step(&ns, &h, &c, &l, "trip", "right").0, "flights");
        let folded: HashSet<String> = ["trip".to_string()].into();
        assert_eq!(step(&ns, &h, &folded, &l, "trip", "right"), ("flights".to_string(), true));
    }

    #[test]
    fn where_a_drag_lands() {
        let ns = tree();
        let l = layout(&ns, &mut HashSet::new(), &HashSet::new(), None, None, &HashMap::new(), &Dims::default());
        let f = l.get("flights").unwrap().clone();
        let t = top(&f);
        // The middle: a child; the top edge: before it; the bottom: after.
        assert_eq!(drop_target(&ns, &l, 180.0, "work", f.x + 10.0, t + f.h / 2.0), Target::Child("flights".into()));
        assert_eq!(drop_target(&ns, &l, 180.0, "work", f.x + 10.0, t + 2.0), Target::Sib(Some("trip".into()), 0));
        assert_eq!(drop_target(&ns, &l, 180.0, "work", f.x + 10.0, t + f.h - 2.0), Target::Sib(Some("trip".into()), 1));
        assert_eq!(drop_target(&ns, &l, 180.0, "flights", f.x + 10.0, t + 20.0), Target::Back);
        assert_eq!(drop_target(&ns, &l, 180.0, "work", 2.0, 2.0), Target::Root);
        let g = l.get("__add__flights").unwrap().clone();
        assert_eq!(drop_target(&ns, &l, 180.0, "work", g.x + 5.0, top(&g) + 5.0), Target::Child("flights".into()));
        assert_eq!(drop_target(&ns, &l, 180.0, "flights", g.x + 5.0, top(&g) + 5.0), Target::Back);
    }

    #[test]
    fn moves_locally() {
        let mut ns = tree();
        assert!(!apply_move(&mut ns, "trip", Some("beach"), None));
        assert!(apply_move(&mut ns, "work", Some("trip"), Some(0)));
        let kids: Vec<&str> = ns.iter().filter(|n| n.parent.as_deref() == Some("trip")).map(|n| n.id.as_str()).collect();
        assert_eq!(kids, vec!["work", "flights", "hotel"]);
        assert!(apply_move(&mut ns, "beach", None, None));
        assert_eq!(ns.last().unwrap().id, "beach");
        assert_eq!(subtree(&tree(), "trip").len(), 4);
    }

    #[test]
    fn filters_and_truncates() {
        let mut a = n("a", None, "");
        a.text = "Book the hotel".into();
        a.notes = vec!["near the beach".into()];
        assert!(matches(&a, "hotel") && matches(&a, "beach") && !matches(&a, "zzz"));
        a.info = json!({ "title": "Travel plan" });
        assert!(matches(&a, "travel"));
        assert_eq!(trunc("abcdef", 4), "abc…");
        assert_eq!(trunc("abc", 4), "abc");
    }
}
