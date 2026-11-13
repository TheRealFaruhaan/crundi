//! The workbench's tiling layout, as the web UI keeps it (src/webapp-html.js,
//! the "mosaic"): a tree of splits and leaves. A split lays its children out
//! in a row or a column, sized by percentages; a leaf holds one pane (a chat,
//! a launcher, a panel) by its key, or nothing (an empty leaf).
//!
//! Slint draws flat lists, so the tree is laid out here into rectangles: one
//! per leaf, one per gutter (the 7px grab strip between two siblings).
//! Sizes as the web UI's: 6px around the grid, 7px gutters, leaves at least
//! 340px wide (wider than that and the grid scrolls sideways).

use serde::{Deserialize, Serialize};

pub const PAD: f32 = 6.0;
pub const GUTTER: f32 = 7.0;
pub const MIN_W: f32 = 340.0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t")]
pub enum Node {
    #[serde(rename = "split")]
    Split { _id: u32, dir: Dir, sizes: Vec<f32>, kids: Vec<Node> },
    #[serde(rename = "leaf")]
    Leaf {
        _id: u32,
        key: Option<String>,
        /// Pinned: the pane keeps this size in px (the web UI's leaf.pin).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<Pin>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pin {
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Row,
    Col,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeafRect {
    pub id: u32,
    pub key: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GutterRect {
    /// The split it belongs to, and the gap after child `index`.
    pub split: u32,
    pub index: usize,
    pub dir: Dir,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The pair's extent along the split (for turning a drag into percentages).
    pub span: f32,
    /// A pinned pane is next to it: it can't be dragged.
    pub locked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mosaic {
    pub root: Node,
    pub next_id: u32,
}

impl Default for Mosaic {
    fn default() -> Self {
        Mosaic { root: Node::Leaf { _id: 1, key: None, pin: None }, next_id: 2 }
    }
}

fn node_id(n: &Node) -> u32 {
    match n { Node::Split { _id, .. } | Node::Leaf { _id, .. } => *_id }
}

impl Mosaic {
    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Every pane key in the tree, in reading order.
    pub fn keys(&self) -> Vec<String> {
        let mut out = vec![];
        fn walk(n: &Node, out: &mut Vec<String>) {
            match n {
                Node::Leaf { key: Some(k), .. } => out.push(k.clone()),
                Node::Leaf { .. } => {}
                Node::Split { kids, .. } => kids.iter().for_each(|k| walk(k, out)),
            }
        }
        walk(&self.root, &mut out);
        out
    }

    /// The minimum width the tree needs (leaves at MIN_W, gutters between).
    fn min_width(n: &Node) -> f32 {
        if let (Some(w), _) = Self::fixed(n) { return w; }
        match n {
            Node::Leaf { .. } => MIN_W,
            Node::Split { dir: Dir::Row, kids, .. } => kids.iter().map(Self::min_width).sum::<f32>() + GUTTER * (kids.len().saturating_sub(1)) as f32,
            Node::Split { kids, .. } => kids.iter().map(Self::min_width).fold(0.0, f32::max),
        }
    }

    /// A node's fixed size in px (w, h), or None where it flexes (the web UI's
    /// mosaicFixed): a pinned leaf fixes both; a split is fixed along its own
    /// axis only when every child is, and across it by its largest fixed child.
    pub fn fixed(n: &Node) -> (Option<f32>, Option<f32>) {
        match n {
            Node::Leaf { pin: Some(p), .. } => (Some(p.w), Some(p.h)),
            Node::Leaf { .. } => (None, None),
            Node::Split { dir, kids, .. } => {
                let fs: Vec<(Option<f32>, Option<f32>)> = kids.iter().map(Self::fixed).collect();
                let gut = GUTTER * (kids.len().saturating_sub(1)) as f32;
                let along = |f: &(Option<f32>, Option<f32>)| if *dir == Dir::Row { f.0 } else { f.1 };
                let across = |f: &(Option<f32>, Option<f32>)| if *dir == Dir::Row { f.1 } else { f.0 };
                let a = if fs.iter().all(|f| along(f).is_some()) { Some(fs.iter().filter_map(along).sum::<f32>() + gut) } else { None };
                let x = fs.iter().filter_map(across).fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))));
                if *dir == Dir::Row { (a, x) } else { (x, a) }
            }
        }
    }

    /// The width the grid takes in an area this wide (wider: it scrolls).
    pub fn content_width(&self, area_w: f32) -> f32 {
        area_w.max(Self::min_width(&self.root) + PAD * 2.0)
    }

    /// Lay the tree out in a content area of w × h (from content_width).
    pub fn layout(&self, w: f32, h: f32) -> (Vec<LeafRect>, Vec<GutterRect>) {
        let (mut leaves, mut gutters) = (vec![], vec![]);
        fn place(n: &Node, x: f32, y: f32, w: f32, h: f32, leaves: &mut Vec<LeafRect>, gutters: &mut Vec<GutterRect>) {
            match n {
                Node::Leaf { _id, key, pin } => leaves.push(LeafRect { id: *_id, key: key.clone(), x, y, w, h, pinned: pin.is_some() }),
                Node::Split { _id, dir, sizes, kids } => {
                    let along = if *dir == Dir::Row { w } else { h };
                    let gaps = GUTTER * (kids.len().saturating_sub(1)) as f32;
                    let free = (along - gaps).max(0.0);
                    // A pinned child (or a split holding one) takes its px size
                    // along this axis; the others share what is left by their sizes.
                    let fixed: Vec<Option<f32>> = kids.iter().map(|k| { let f = Mosaic::fixed(k); if *dir == Dir::Row { f.0 } else { f.1 } }).collect();
                    let free = (free - fixed.iter().flatten().sum::<f32>()).max(0.0);
                    let total: f32 = sizes.iter().zip(&fixed).filter(|(_, f)| f.is_none()).map(|(s, _)| *s).sum::<f32>().max(0.0001);
                    let mut lens: Vec<f32> = sizes.iter().zip(&fixed).map(|(s, f)| f.unwrap_or(free * s / total)).collect();
                    // Rows keep every child at its minimum width.
                    if *dir == Dir::Row {
                        let mins: Vec<f32> = kids.iter().map(Mosaic::min_width).collect();
                        for i in 0..lens.len() {
                            if lens[i] < mins[i] {
                                let short = mins[i] - lens[i];
                                lens[i] = mins[i];
                                // Taken from the widest free sibling that can spare it.
                                if let Some(j) = (0..lens.len()).filter(|&j| j != i && fixed[j].is_none() && lens[j] - mins[j] > short).max_by(|&a, &b| lens[a].partial_cmp(&lens[b]).unwrap()) {
                                    lens[j] -= short;
                                }
                            }
                        }
                    }
                    let mut at = if *dir == Dir::Row { x } else { y };
                    for (i, kid) in kids.iter().enumerate() {
                        let len = lens[i];
                        let (cx, cy, cw, ch) = if *dir == Dir::Row { (at, y, len, h) } else { (x, at, w, len) };
                        place(kid, cx, cy, cw, ch, leaves, gutters);
                        at += len;
                        if i + 1 < kids.len() {
                            let span = lens[i] + lens[i + 1] + GUTTER;
                            let locked = fixed[i].is_some() || fixed[i + 1].is_some();
                            gutters.push(if *dir == Dir::Row {
                                GutterRect { split: *_id, index: i, dir: *dir, x: at, y, w: GUTTER, h, span, locked }
                            } else {
                                GutterRect { split: *_id, index: i, dir: *dir, x, y: at, w, h: GUTTER, span, locked }
                            });
                            at += GUTTER;
                        }
                    }
                }
            }
        }
        place(&self.root, PAD, PAD, (w - PAD * 2.0).max(0.0), (h - PAD * 2.0).max(0.0), &mut leaves, &mut gutters);
        (leaves, gutters)
    }

    fn find_mut(n: &mut Node, id: u32) -> Option<&mut Node> {
        if node_id(n) == id { return Some(n); }
        match n {
            Node::Split { kids, .. } => kids.iter_mut().find_map(|k| Self::find_mut(k, id)),
            _ => None,
        }
    }

    /// Drag gutter `index` of split `split` by `delta` px, along a span of `span` px.
    /// The pair stays within 10–90% of what the two share, as in the web UI.
    pub fn drag(&mut self, split: u32, index: usize, delta: f32, span: f32) {
        let Some(Node::Split { sizes, kids, dir, .. }) = Self::find_mut(&mut self.root, split) else { return };
        if index + 1 >= sizes.len() || span <= GUTTER { return; }
        // A divider next to a pinned pane is locked.
        let along = |k: &Node| { let f = Mosaic::fixed(k); if *dir == Dir::Row { f.0 } else { f.1 } };
        if along(&kids[index]).is_some() || along(&kids[index + 1]).is_some() { return; }
        let pair = sizes[index] + sizes[index + 1];
        let px_per = (span - GUTTER) / pair;
        let mut a = sizes[index] + delta / px_per;
        a = a.clamp(pair * 0.1, pair * 0.9);
        sizes[index] = a;
        sizes[index + 1] = pair - a;
    }

    /// Double-click on a gutter: the two sides even again.
    pub fn even(&mut self, split: u32, index: usize) {
        if let Some(Node::Split { sizes, .. }) = Self::find_mut(&mut self.root, split) {
            if index + 1 < sizes.len() {
                let pair = sizes[index] + sizes[index + 1];
                sizes[index] = pair / 2.0;
                sizes[index + 1] = pair / 2.0;
            }
        }
    }

    /// Split a leaf: a new empty leaf beside it (row) or under it (col).
    /// Returns the new leaf's id.
    pub fn split(&mut self, leaf: u32, dir: Dir) -> u32 {
        let new_leaf = self.id();
        let new_split = self.id();
        fn go(n: &mut Node, leaf: u32, dir: Dir, new_leaf: u32, new_split: u32) -> bool {
            match n {
                Node::Leaf { _id, .. } if *_id == leaf => {
                    let old = std::mem::replace(n, Node::Leaf { _id: 0, key: None, pin: None });
                    *n = Node::Split { _id: new_split, dir, sizes: vec![50.0, 50.0], kids: vec![old, Node::Leaf { _id: new_leaf, key: None, pin: None }] };
                    true
                }
                Node::Split { dir: d, sizes, kids, .. } => {
                    // Into a split of the same direction: a sibling, sizes shared.
                    if let Some(i) = kids.iter().position(|k| matches!(k, Node::Leaf { _id, .. } if *_id == leaf)) {
                        if *d == dir {
                            let half = sizes[i] / 2.0;
                            sizes[i] = half;
                            sizes.insert(i + 1, half);
                            kids.insert(i + 1, Node::Leaf { _id: new_leaf, key: None, pin: None });
                            return true;
                        }
                    }
                    kids.iter_mut().any(|k| go(k, leaf, dir, new_leaf, new_split))
                }
                _ => false,
            }
        }
        go(&mut self.root, leaf, dir, new_leaf, new_split);
        new_leaf
    }

    /// Close a leaf: it goes, and a split left with one child becomes that child.
    pub fn close(&mut self, leaf: u32) {
        fn go(n: &mut Node, leaf: u32) {
            if let Node::Split { sizes, kids, .. } = n {
                if let Some(i) = kids.iter().position(|k| matches!(k, Node::Leaf { _id, .. } if *_id == leaf)) {
                    kids.remove(i);
                    sizes.remove(i);
                } else {
                    kids.iter_mut().for_each(|k| go(k, leaf));
                }
                if kids.len() == 1 {
                    *n = kids.pop().unwrap();
                }
            }
        }
        if matches!(&self.root, Node::Leaf { _id, .. } if *_id == leaf) {
            if let Node::Leaf { key, pin, .. } = &mut self.root { *key = None; *pin = None; }
            return;
        }
        go(&mut self.root, leaf);
    }

    /// Put a pane in a leaf (or clear it).
    pub fn assign(&mut self, leaf: u32, key: Option<String>) {
        if let Some(Node::Leaf { key: k, .. }) = Self::find_mut(&mut self.root, leaf) { *k = key; }
    }

    pub fn leaf_of(&self, key: &str) -> Option<u32> {
        let (leaves, _) = self.layout(1000.0, 1000.0);
        leaves.iter().find(|l| l.key.as_deref() == Some(key)).map(|l| l.id)
    }

    /// Show this pane: in an empty leaf if there is one, else in a new column
    /// beside the last leaf (the web UI adds new panes to the right).
    pub fn place(&mut self, key: &str) -> u32 {
        if let Some(id) = self.leaf_of(key) { return id; }
        let (leaves, _) = self.layout(1000.0, 1000.0);
        if let Some(empty) = leaves.iter().find(|l| l.key.is_none()) {
            let id = empty.id;
            self.assign(id, Some(key.to_string()));
            return id;
        }
        let last = leaves.last().map(|l| l.id).unwrap_or(1);
        // A new column at the top level.
        let new_leaf = self.id();
        match &mut self.root {
            Node::Split { dir: Dir::Row, sizes, kids, .. } => {
                let share = 100.0 / (kids.len() + 1) as f32;
                for s in sizes.iter_mut() { *s *= 1.0 - share / 100.0; }
                sizes.push(share);
                kids.push(Node::Leaf { _id: new_leaf, key: Some(key.to_string()), pin: None });
                new_leaf
            }
            _ => {
                let id = self.split(last, Dir::Row);
                self.assign(id, Some(key.to_string()));
                id
            }
        }
    }

    /// Pin a leaf at this size in px, or unpin it (the header's pin button).
    pub fn toggle_pin(&mut self, leaf: u32, w: f32, h: f32) {
        if let Some(Node::Leaf { pin, .. }) = Self::find_mut(&mut self.root, leaf) {
            *pin = if pin.is_some() { None } else { Some(Pin { w: w.round(), h: h.round() }) };
        }
    }

    pub fn pin_of(&self, key: &str) -> Option<Pin> {
        fn go(n: &Node, key: &str) -> Option<Pin> {
            match n {
                Node::Leaf { key: Some(k), pin, .. } if k == key => *pin,
                Node::Split { kids, .. } => kids.iter().find_map(|k| go(k, key)),
                _ => None,
            }
        }
        go(&self.root, key)
    }

    fn set_pin(&mut self, key: &str, p: Pin) {
        fn go(n: &mut Node, key: &str, p: Pin) {
            match n {
                Node::Leaf { key: Some(k), pin, .. } if k == key => *pin = Some(p),
                Node::Split { kids, .. } => kids.iter_mut().for_each(|k| go(k, key, p)),
                _ => {}
            }
        }
        go(&mut self.root, key, p);
    }

    /// Double-click a pane's header: it takes its parent split (each sibling
    /// 8%, held at its floor); again, an even split. A lone pane: nothing.
    pub fn maximize(&mut self, leaf: u32) {
        fn go(n: &mut Node, leaf: u32) -> bool {
            let Node::Split { sizes, kids, .. } = n else { return false };
            if let Some(i) = kids.iter().position(|k| node_id(k) == leaf) {
                let len = kids.len();
                let big = 100.0 - (len as f32 - 1.0) * 8.0;
                let total: f32 = sizes.iter().sum::<f32>().max(0.0001);
                let already = sizes[i] * 100.0 / total >= big - 0.5;
                *sizes = (0..len).map(|j| if already { 100.0 / len as f32 } else if j == i { big } else { 8.0 }).collect();
                return true;
            }
            kids.iter_mut().any(|k| go(k, leaf))
        }
        go(&mut self.root, leaf);
    }

    /// The "+" menu's Layout presets, filled with these panes in order:
    /// cols2, cols3, grid (2×2), stack (left + right stack), rows (top + bottom pair).
    pub fn preset(name: &str, keys: Vec<String>) -> Mosaic {
        let mut m = Mosaic { root: Node::Leaf { _id: 1, key: None, pin: None }, next_id: 1 };
        let mut keys = keys.into_iter();
        let leaf = |m: &mut Mosaic, keys: &mut dyn Iterator<Item = String>| Node::Leaf { _id: m.id(), key: keys.next(), pin: None };
        let split = |m: &mut Mosaic, dir: Dir, kids: Vec<Node>| { let n = kids.len() as f32; Node::Split { _id: m.id(), dir, sizes: vec![100.0 / n; kids.len()], kids } };
        let root = match name {
            "cols3" => { let k = vec![leaf(&mut m, &mut keys), leaf(&mut m, &mut keys), leaf(&mut m, &mut keys)]; split(&mut m, Dir::Row, k) }
            "grid" => {
                let top = vec![leaf(&mut m, &mut keys), leaf(&mut m, &mut keys)];
                let bottom = vec![leaf(&mut m, &mut keys), leaf(&mut m, &mut keys)];
                let (t, b) = (split(&mut m, Dir::Row, top), split(&mut m, Dir::Row, bottom));
                split(&mut m, Dir::Col, vec![t, b])
            }
            "stack" => {
                let left = leaf(&mut m, &mut keys);
                let right = vec![leaf(&mut m, &mut keys), leaf(&mut m, &mut keys)];
                let r = split(&mut m, Dir::Col, right);
                split(&mut m, Dir::Row, vec![left, r])
            }
            "rows" => {
                let top = leaf(&mut m, &mut keys);
                let bottom = vec![leaf(&mut m, &mut keys), leaf(&mut m, &mut keys)];
                let b = split(&mut m, Dir::Row, bottom);
                split(&mut m, Dir::Col, vec![top, b])
            }
            _ => { let k = vec![leaf(&mut m, &mut keys), leaf(&mut m, &mut keys)]; split(&mut m, Dir::Row, k) }
        };
        m.root = root;
        // Panes that did not fit the preset go on the right.
        for k in keys { m.place(&k); }
        m
    }

    /// A preset that keeps each pinned pane's pin wherever it lands.
    pub fn preset_from(&self, name: &str) -> Mosaic {
        let mut m = Mosaic::preset(name, self.keys());
        for k in self.keys() { if let Some(p) = self.pin_of(&k) { m.set_pin(&k, p); } }
        m
    }

    /// Drop panes whose keys are gone (a chat that ended elsewhere).
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        let (leaves, _) = self.layout(1000.0, 1000.0);
        for l in leaves {
            if let Some(k) = &l.key {
                if !keep(k) { self.close(l.id); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_place_split_close() {
        let mut m = Mosaic::default();
        m.place("live:a");
        m.place("live:b");
        let (leaves, gutters) = m.layout(1160.0, 770.0);
        assert_eq!(leaves.len(), 2);
        assert_eq!(gutters.len(), 1);
        assert_eq!(leaves[0].x, PAD);
        assert!((leaves[0].w - leaves[1].w).abs() < 1.0);
        assert!((leaves[1].x - (leaves[0].x + leaves[0].w + GUTTER)).abs() < 0.01);
        // Drag the gutter right by 100px.
        let g = &gutters[0];
        m.drag(g.split, g.index, 100.0, g.span);
        let (l2, _) = m.layout(1160.0, 770.0);
        assert!((l2[0].w - leaves[0].w - 100.0).abs() < 0.5);
        // A third pane, then close the first.
        m.place("live:c");
        assert_eq!(m.keys(), vec!["live:a", "live:b", "live:c"]);
        let a = m.leaf_of("live:a").unwrap();
        m.close(a);
        assert_eq!(m.keys(), vec!["live:b", "live:c"]);
        // Narrow area: leaves keep 340px and the grid gets wider.
        assert!(m.content_width(500.0) >= 2.0 * MIN_W + GUTTER + 2.0 * PAD);
        // Round trip through JSON (saved per project).
        let j = serde_json::to_string(&m).unwrap();
        let back: Mosaic = serde_json::from_str(&j).unwrap();
        assert_eq!(back.keys(), m.keys());
    }

    #[test]
    fn split_down_and_retain() {
        let mut m = Mosaic::default();
        let first = m.place("live:a");
        let below = m.split(first, Dir::Col);
        m.assign(below, Some("pend:1".into()));
        let (leaves, gutters) = m.layout(800.0, 600.0);
        assert_eq!(leaves.len(), 2);
        assert_eq!(gutters[0].dir, Dir::Col);
        assert!(leaves[1].y > leaves[0].y);
        m.retain(|k| k != "live:a");
        assert_eq!(m.keys(), vec!["pend:1"]);
    }

    #[test]
    fn pin_fixes_size_and_locks_dividers() {
        let mut m = Mosaic::default();
        m.place("live:a");
        m.place("live:b");
        m.place("live:c");
        let (leaves, _) = m.layout(1400.0, 800.0);
        let a = leaves[0].id;
        m.toggle_pin(a, 400.4, 788.0);
        assert_eq!(m.pin_of("live:a"), Some(Pin { w: 400.0, h: 788.0 }));
        let (leaves, gutters) = m.layout(1400.0, 800.0);
        assert!(leaves[0].pinned);
        assert_eq!(leaves[0].w, 400.0);
        // The others share the rest evenly.
        let rest = 1400.0 - 2.0 * PAD - 2.0 * GUTTER - 400.0;
        assert!((leaves[1].w - rest / 2.0).abs() < 0.5);
        // The divider by the pinned pane is locked, the other isn't.
        assert!(gutters[0].locked && !gutters[1].locked);
        let before = leaves[0].w;
        m.drag(gutters[0].split, 0, 120.0, gutters[0].span);
        assert_eq!(m.layout(1400.0, 800.0).0[0].w, before);
        // The pin survives JSON (it is saved) and a preset.
        let back: Mosaic = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back.pin_of("live:a"), Some(Pin { w: 400.0, h: 788.0 }));
        let p = m.preset_from("cols3");
        assert_eq!(p.pin_of("live:a"), Some(Pin { w: 400.0, h: 788.0 }));
        // Unpin: back to shares.
        m.toggle_pin(a, 0.0, 0.0);
        assert_eq!(m.pin_of("live:a"), None);
        assert!(!serde_json::to_string(&m).unwrap().contains("pin"));
    }

    #[test]
    fn maximize_then_even() {
        let mut m = Mosaic::default();
        m.place("live:a");
        // A lone pane: nothing happens.
        let lone = m.leaf_of("live:a").unwrap();
        m.maximize(lone);
        assert_eq!(m.keys(), vec!["live:a"]);
        m.place("live:b");
        let a = m.leaf_of("live:a").unwrap();
        m.maximize(a);
        let Node::Split { sizes, .. } = &m.root else { panic!() };
        assert_eq!(sizes, &vec![92.0, 8.0]);
        m.maximize(a);
        let Node::Split { sizes, .. } = &m.root else { panic!() };
        assert_eq!(sizes, &vec![50.0, 50.0]);
        // In 1400px, the small one sits at its 340px floor.
        m.maximize(a);
        let (leaves, _) = m.layout(1154.0, 770.0);
        assert_eq!(leaves[1].w, MIN_W);
    }
}
