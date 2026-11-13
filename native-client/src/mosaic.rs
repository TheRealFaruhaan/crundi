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

/// The Layout presets, by the web's ids.
pub const PRESETS: [&str; 5] = ["cols2", "cols3", "cross4", "left-right2", "top-bottom2"];

/// What a drop did.
#[derive(Debug, PartialEq, Eq)]
pub enum Dropped {
    Nothing,
    Swapped,
    Moved,
    /// The dragged pane is pinned: refused.
    Pinned,
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
                            kids.insert(i + 1, Node::Leaf { _id: new_leaf, key: None, pin: None });
                            let n = kids.len();
                            *sizes = vec![100.0 / n as f32; n];
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

    /// Close a leaf (the web's mosaicCollapseLeaf): it goes, its siblings'
    /// sizes become even, and a split left with one child becomes that child.
    /// The root leaf alone becomes an empty root leaf.
    pub fn close(&mut self, leaf: u32) {
        fn go(n: &mut Node, leaf: u32) -> bool {
            let Node::Split { sizes, kids, .. } = n else { return false };
            if let Some(i) = kids.iter().position(|k| matches!(k, Node::Leaf { _id, .. } if *_id == leaf)) {
                kids.remove(i);
                let len = kids.len();
                *sizes = vec![100.0 / len.max(1) as f32; len];
                if len == 1 { *n = kids.pop().unwrap(); }
                return true;
            }
            kids.iter_mut().any(|k| go(k, leaf))
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

    /// Show this pane (the web's mosaicAddKey): in the first empty leaf in tree
    /// order if there is one, else appended to the root split, whose sizes
    /// become even (a root leaf becomes a row of two).
    pub fn place(&mut self, key: &str) -> u32 {
        if let Some(id) = self.leaf_of(key) { return id; }
        let (leaves, _) = self.layout(1000.0, 1000.0);
        if let Some(empty) = leaves.iter().find(|l| l.key.is_none()) {
            let id = empty.id;
            self.assign(id, Some(key.to_string()));
            return id;
        }
        let new_leaf = self.id();
        let leaf = Node::Leaf { _id: new_leaf, key: Some(key.to_string()), pin: None };
        if let Node::Split { sizes, kids, .. } = &mut self.root {
            kids.push(leaf);
            let n = kids.len();
            *sizes = vec![100.0 / n as f32; n];
        } else {
            let split = self.id();
            let old = std::mem::replace(&mut self.root, Node::Leaf { _id: 0, key: None, pin: None });
            self.root = Node::Split { _id: split, dir: Dir::Row, sizes: vec![50.0, 50.0], kids: vec![old, leaf] };
        }
        new_leaf
    }

    /// Fit to screen: every split's sizes even, all the way down (pins keep
    /// their px size: a pin overrides the share).
    pub fn fit(&mut self) {
        fn go(n: &mut Node) {
            if let Node::Split { sizes, kids, .. } = n {
                *sizes = vec![100.0 / kids.len() as f32; kids.len()];
                kids.iter_mut().for_each(go);
            }
        }
        go(&mut self.root);
    }

    /// Any pin in the tree.
    pub fn has_pin(&self) -> bool {
        fn go(n: &Node) -> bool {
            match n { Node::Leaf { pin, .. } => pin.is_some(), Node::Split { kids, .. } => kids.iter().any(go) }
        }
        go(&self.root)
    }

    fn leaf_mut(&mut self, leaf: u32) -> Option<(&mut Option<String>, &mut Option<Pin>)> {
        match Self::find_mut(&mut self.root, leaf) {
            Some(Node::Leaf { key, pin, .. }) => Some((key, pin)),
            _ => None,
        }
    }

    /// Drop the pane of leaf `src` on leaf `dst` (the web's mosaicAssignToLeaf):
    /// a pane there swaps with it (each leaf keeps its size and pin); an empty
    /// leaf takes it, and the old leaf collapses.
    pub fn drop_on(&mut self, src: u32, dst: u32) -> Dropped {
        if src == dst { return Dropped::Nothing; }
        let Some(src_key) = self.leaf_mut(src).and_then(|(k, _)| k.clone()) else { return Dropped::Nothing };
        if self.leaf_mut(src).is_some_and(|(_, p)| p.is_some()) { return Dropped::Pinned; }
        let Some((dst_key, dst_pin)) = self.leaf_mut(dst) else { return Dropped::Nothing };
        if dst_pin.is_some() { return Dropped::Nothing; }
        match dst_key.replace(src_key) {
            Some(other) => {
                if let Some((k, _)) = self.leaf_mut(src) { *k = Some(other); }
                Dropped::Swapped
            }
            None => {
                self.close(src);
                Dropped::Moved
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

    /// The "+" menu's Layout presets (the web's mosaicApplyPreset), filled with
    /// these panes in order; missing slots are empty leaves, panes beyond the
    /// slots are added as `place` does. Even sizes. The ids are the web's, so
    /// both share the saved tree:
    ///   cols2        row[0, 1]
    ///   cols3        row[0, 1, 2]
    ///   cross4       row[col[0, 2], col[1, 3]]
    ///   left-right2  row[0, col[1, 2]]
    ///   top-bottom2  col[0, row[1, 2]]
    pub fn preset(name: &str, keys: Vec<String>) -> Mosaic {
        let mut m = Mosaic { root: Node::Leaf { _id: 1, key: None, pin: None }, next_id: 1 };
        let slots: Vec<Option<String>> = (0..4).map(|i| keys.get(i).cloned()).collect();
        let used = match name { "cols2" => 2, "cols3" | "left-right2" | "top-bottom2" => 3, "cross4" => 4, _ => 0 };
        if used == 0 { return m; }
        let leaf = |m: &mut Mosaic, i: usize| Node::Leaf { _id: m.id(), key: slots[i].clone(), pin: None };
        let split = |m: &mut Mosaic, dir: Dir, kids: Vec<Node>| { let n = kids.len() as f32; Node::Split { _id: m.id(), dir, sizes: vec![100.0 / n; kids.len()], kids } };
        let root = match name {
            "cols2" => { let k = vec![leaf(&mut m, 0), leaf(&mut m, 1)]; split(&mut m, Dir::Row, k) }
            "cols3" => { let k = vec![leaf(&mut m, 0), leaf(&mut m, 1), leaf(&mut m, 2)]; split(&mut m, Dir::Row, k) }
            "cross4" => {
                let left = vec![leaf(&mut m, 0), leaf(&mut m, 2)];
                let right = vec![leaf(&mut m, 1), leaf(&mut m, 3)];
                let (l, r) = (split(&mut m, Dir::Col, left), split(&mut m, Dir::Col, right));
                split(&mut m, Dir::Row, vec![l, r])
            }
            "left-right2" => {
                let first = leaf(&mut m, 0);
                let right = vec![leaf(&mut m, 1), leaf(&mut m, 2)];
                let r = split(&mut m, Dir::Col, right);
                split(&mut m, Dir::Row, vec![first, r])
            }
            _ => {
                let top = leaf(&mut m, 0);
                let bottom = vec![leaf(&mut m, 1), leaf(&mut m, 2)];
                let b = split(&mut m, Dir::Row, bottom);
                split(&mut m, Dir::Col, vec![top, b])
            }
        };
        m.root = root;
        // Panes beyond the slots: the first empty leaf, or appended to the root.
        for k in keys.into_iter().skip(used) { m.place(&k); }
        m
    }

    /// A preset that keeps each pinned pane's pin wherever it lands.
    /// `order`: the panes in display order (the web's currentDesiredKeys: the
    /// chats in the server's order); panes in the tree but not in it follow.
    pub fn preset_from(&self, name: &str, order: &[String]) -> Mosaic {
        // An unknown preset leaves the layout as it is.
        if !PRESETS.contains(&name) { return self.clone(); }
        let have = self.keys();
        let mut keys: Vec<String> = order.iter().filter(|k| have.contains(k)).cloned().collect();
        keys.extend(have.into_iter().filter(|k| !order.contains(k)));
        let mut m = Mosaic::preset(name, keys);
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
        assert_eq!(m.preset_from("nonsense", &[]).keys(), m.keys());
        let p = m.preset_from("cols3", &[]);
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

    /// The tree as the spec writes it: row[a, col[b, ∅]].
    fn shape(n: &Node) -> String {
        match n {
            Node::Leaf { key: Some(k), .. } => k.trim_start_matches("live:").to_string(),
            Node::Leaf { .. } => "∅".into(),
            Node::Split { dir, kids, .. } => format!("{}[{}]", if *dir == Dir::Row { "row" } else { "col" }, kids.iter().map(shape).collect::<Vec<_>>().join(", ")),
        }
    }

    fn keys(n: usize) -> Vec<String> { (0..n).map(|i| format!("live:{i}")).collect() }

    #[test]
    fn presets_as_the_web() {
        let cases = [
            (4, "cols2", "row[0, 1, 2, 3]"), (4, "cols3", "row[0, 1, 2, 3]"),
            (4, "cross4", "row[col[0, 2], col[1, 3]]"),
            (4, "left-right2", "row[0, col[1, 2], 3]"), (4, "top-bottom2", "col[0, row[1, 2], 3]"),
            (3, "cross4", "row[col[0, 2], col[1, ∅]]"), (3, "top-bottom2", "col[0, row[1, 2]]"),
            (2, "cols3", "row[0, 1, ∅]"), (2, "cross4", "row[col[0, ∅], col[1, ∅]]"),
            (2, "left-right2", "row[0, col[1, ∅]]"), (2, "top-bottom2", "col[0, row[1, ∅]]"),
            (1, "cols2", "row[0, ∅]"), (1, "cols3", "row[0, ∅, ∅]"), (1, "cross4", "row[col[0, ∅], col[∅, ∅]]"),
        ];
        for (n, name, want) in cases {
            let m = Mosaic::preset(name, keys(n));
            assert_eq!(shape(&m.root), want, "{n} chats, {name}");
        }
        // Even sizes, and the 3-chat cross's geometry (spec: 570.5×378.5 leaves).
        let m = Mosaic::preset("cross4", keys(3));
        let (leaves, _) = m.layout(1160.0, 776.0);
        assert!((leaves[0].w - 570.5).abs() < 0.01 && (leaves[0].h - 378.5).abs() < 0.01);
        assert!((leaves[2].x - 823.5 + 240.0).abs() < 0.01, "{:?}", leaves[2]);
        // Four from left-right2: 378 wide, the root's sizes even.
        let m = Mosaic::preset("left-right2", keys(4));
        let (leaves, _) = m.layout(1160.0, 776.0);
        assert!((leaves[0].w - 378.0).abs() < 0.01 && (leaves[3].x - (1016.0 - 240.0)).abs() < 0.01, "{leaves:?}");
    }

    #[test]
    fn drop_swaps_or_moves() {
        // row[col[A, B], col[C, ∅]]: drag B onto ∅ gives row[A, col[C, B]].
        let mut m = Mosaic::preset("cross4", vec!["live:A".into(), "live:C".into(), "live:B".into()]);
        assert_eq!(shape(&m.root), "row[col[A, B], col[C, ∅]]");
        let (leaves, _) = m.layout(1160.0, 776.0);
        let id = |k: Option<&str>| leaves.iter().find(|l| l.key.as_deref() == k).unwrap().id;
        assert_eq!(m.drop_on(id(Some("live:B")), id(None)), Dropped::Moved);
        assert_eq!(shape(&m.root), "row[A, col[C, B]]");
        // A swap trades the panes; each leaf keeps its pin and size.
        let (leaves, _) = m.layout(1160.0, 776.0);
        let id = |k: &str| leaves.iter().find(|l| l.key.as_deref() == Some(k)).unwrap().id;
        let (a, c) = (id("live:A"), id("live:C"));
        assert_eq!(m.drop_on(c, a), Dropped::Swapped);
        assert_eq!(shape(&m.root), "row[C, col[A, B]]");
        // On itself: nothing. A pinned pane is refused; a pinned target takes nothing.
        assert_eq!(m.drop_on(a, a), Dropped::Nothing);
        m.toggle_pin(a, 400.0, 700.0);
        assert_eq!(m.drop_on(a, c), Dropped::Pinned);
        assert_eq!(m.drop_on(c, a), Dropped::Nothing);
        assert_eq!(shape(&m.root), "row[C, col[A, B]]");
    }

    #[test]
    fn split_close_place_and_fit_even_out() {
        // Split down on A of row[A, col[C, B]]: row[col[A, ∅], col[C, B]];
        // then split right on C: row[col[A, ∅], col[row[C, ∅], B]].
        let mut m = Mosaic::preset("left-right2", vec!["live:A".into(), "live:C".into(), "live:B".into()]);
        let id = |m: &Mosaic, k: &str| m.leaf_of(k).unwrap();
        m.split(id(&m, "live:A"), Dir::Col);
        assert_eq!(shape(&m.root), "row[col[A, ∅], col[C, B]]");
        m.split(id(&m, "live:C"), Dir::Row);
        assert_eq!(shape(&m.root), "row[col[A, ∅], col[row[C, ∅], B]]");
        // A sibling in a same-direction split: sizes even.
        let mut r = Mosaic::preset("cols2", vec!["live:A".into(), "live:B".into()]);
        r.split(id(&r, "live:A"), Dir::Row);
        let Node::Split { sizes, .. } = &r.root else { panic!() };
        assert!(sizes.iter().all(|s| (s - 100.0 / 3.0).abs() < 1e-3));
        // Closing: even again, a one-child split hoisted.
        r.close(id(&r, "live:B"));
        let Node::Split { sizes, .. } = &r.root else { panic!() };
        assert_eq!(sizes, &vec![50.0, 50.0]);
        // A new pane fills the first empty leaf, else goes on the root, even.
        let mut p = Mosaic::preset("cols2", vec!["live:A".into(), "live:B".into()]);
        p.place("live:C");
        assert_eq!(shape(&p.root), "row[A, B, C]");
        let Node::Split { sizes, .. } = &p.root else { panic!() };
        assert!(sizes.iter().all(|s| (s - 100.0 / 3.0).abs() < 1e-3));
        // Fit: every split even, all the way down.
        let (_, gutters) = m.layout(1160.0, 776.0);
        let g = &gutters[0];
        m.drag(g.split, g.index, 120.0, g.span);
        m.fit();
        fn all_even(n: &Node) -> bool {
            match n { Node::Leaf { .. } => true, Node::Split { sizes, kids, .. } => sizes.iter().all(|s| (s - 100.0 / kids.len() as f32).abs() < 1e-3) && kids.iter().all(all_even) }
        }
        assert!(all_even(&m.root));
    }
}
