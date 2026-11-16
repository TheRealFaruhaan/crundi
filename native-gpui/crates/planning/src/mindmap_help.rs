//! The Mind map's shortcut sheet (the web's mmHelpHtml and mmDiagram): every
//! way of working the map, by section, a few with a little tree that opens
//! under them. The words are the web's. Moved over from the Slint client
//! (native-client/src/planning/mindmap_help.rs) as it was.

/// A piece of a row's keys: ("kbd" | "plus" | "or" | "word", text). A row's
/// keys are lines of these (the web's flex-wrap: the long ones break).
pub type Keys = Vec<Vec<(&'static str, String)>>;

/// A little tree: boxes (column, row, words, "" | "new" | "held"), links
/// (from, to, dashed), and the note under it.
pub struct Dia { pub boxes: Vec<(i32, i32, &'static str, &'static str)>, pub links: Vec<(usize, usize, bool)>, pub note: &'static str }

pub enum Row { Head(&'static str), Item { keys: Keys, text: &'static str, dia: Option<Dia> } }

fn key(k: &str) -> Vec<(&'static str, String)> {
    let mut out = vec![];
    for (i, p) in k.split('+').enumerate() {
        if i > 0 { out.push(("plus", "+".to_string())); }
        out.push(("kbd", p.trim().to_string()));
    }
    out
}
fn word(w: &str) -> Vec<(&'static str, String)> { vec![("word", w.to_string())] }
fn or(w: &str) -> Vec<(&'static str, String)> { vec![("or", w.to_string())] }
fn one(parts: Vec<Vec<(&'static str, String)>>) -> Keys { vec![parts.concat()] }
fn item(keys: Keys, text: &'static str, dia: Option<Dia>) -> Row { Row::Item { keys, text, dia } }

/// The sheet's rows, in the web's order.
pub fn rows() -> Vec<Row> {
    let t = |extra: Vec<(i32, i32, &'static str, &'static str)>| { let mut b = vec![(0, 1, "Trip", ""), (1, 0, "Flights", ""), (1, 1, "Hotel", "")]; b.extend(extra); b };
    vec![
        Row::Head("ADDING IDEAS"),
        item(one(vec![word("+ Add idea")]), "Start a new top-level idea, typed in place. A dashed \"add idea\" box starts one under that idea.", None),
        item(one(vec![key("Enter")]), "Save it and stop adding. The ring stays on it.", None),
        item(one(vec![key("Ctrl+Enter")]), "Save it and start the next idea at the same level.",
            Some(Dia { boxes: t(vec![(1, 2, "next idea", "new")]), links: vec![(0, 1, false), (0, 2, false), (0, 3, true)], note: "You typed \"Hotel\" and pressed Ctrl+Enter: the next box opens below it, under the same parent." })),
        item(one(vec![key("Tab")]), "Save it and start an idea nested under it.",
            Some(Dia { boxes: t(vec![(2, 1, "nested idea", "new")]), links: vec![(0, 1, false), (0, 2, false), (2, 3, true)], note: "You typed \"Hotel\" and pressed Tab: the next box opens one level in, under \"Hotel\"." })),
        item(one(vec![key("Shift+Tab")]), "Save it and start an idea one level out, beside its parent. From the top level, a new top-level idea.",
            Some(Dia { boxes: vec![(0, 0, "Trip", ""), (1, 0, "Hotel", ""), (2, 0, "Near beach", ""), (1, 1, "next idea", "new")], links: vec![(0, 1, false), (1, 2, false), (0, 3, true)], note: "You typed \"Near beach\" and pressed Shift+Tab: the next box steps back out, beside \"Hotel\"." })),
        item(one(vec![key("Shift+Enter")]), "A new line inside the idea.", None),
        item(one(vec![key("Esc")]), "Close the box without saving what is in it. Clicking elsewhere does the same.", None),
        Row::Head("EDITING"),
        item(one(vec![word("Click")]), "Edit the idea where it is. It opens to about seven lines, then scrolls.", None),
        item(one(vec![key("Enter")]), "Save the edit and stop. Clicking elsewhere saves it too.", None),
        item(one(vec![key("Ctrl+Enter")]), "Save the edit and start the next idea at the same level, as when adding.", None),
        item(one(vec![key("Esc")]), "Throw the edit away; the idea reads as it did.", None),
        item(one(vec![key("Tab"), or("or"), key("Shift+Tab")]), "Save the edit and carry on adding: nested under it, or one level out.", None),
        item(one(vec![word("Double-click")]), "Open the details: notes, a link to a task, media, delete.", None),
        Row::Head("MOVING ABOUT"),
        item(one(vec![key("Ctrl+Shift+Up / Down")]), "The idea above or below on the same level, carrying on into the next parent when one runs out.",
            Some(Dia { boxes: vec![(0, 0, "Trip", ""), (1, 0, "Hotel", "held"), (0, 1, "Work", ""), (1, 1, "Budget", "new")], links: vec![(0, 1, false), (2, 3, false)], note: "From \"Hotel\", Ctrl+Shift+Down goes to \"Budget\": the next idea on that level, though it belongs to \"Work\"." })),
        item(one(vec![key("Ctrl+Shift+Left")]), "Its parent.", None),
        item(one(vec![key("Ctrl+Shift+Right")]), "The middle idea under it (the earlier of the two middles when there is an even number).",
            Some(Dia { boxes: vec![(0, 1, "Trip", "held"), (1, 0, "Flights", ""), (1, 1, "Hotel", "new"), (1, 2, "Visa", "")], links: vec![(0, 1, false), (0, 2, false), (0, 3, false)], note: "From \"Trip\", Ctrl+Shift+Right lands on \"Hotel\", the middle of its three." })),
        item(one(vec![key("Enter")]), "With the ring on an idea: switch editing on. Enter again saves and switches it off. While editing, Ctrl+Shift and an arrow saves and takes the editing with you.", None),
        item(vec![[key("Ctrl+Enter"), or(","), key("Tab"), or(",")].concat(), key("Shift+Tab")], "With the ring on an idea and nothing being typed, these start a new idea just as they do while typing: at its level, nested under it, or one level out.", None),
        item(one(vec![key("Esc")]), "Let go of the ring. A click anywhere that is not an idea does the same.", None),
        item(one(vec![key("Delete")]), "With the ring on an idea: the ring turns amber and says what would go. Press Delete again while it is amber to delete the idea and everything under it. Any other key, a click, or a few seconds cancels.",
            Some(Dia { boxes: vec![(0, 0, "Trip", ""), (1, 0, "Hotel", "held"), (2, 0, "Near beach", ""), (1, 1, "Visa", "")], links: vec![(0, 1, false), (1, 2, false), (0, 3, false)], note: "With the ring on \"Hotel\", Delete twice removes \"Hotel\" and \"Near beach\". \"Visa\" stays." })),
        Row::Head("MOVING IDEAS"),
        item(one(vec![word("Drag onto an idea")]), "It becomes a child of that idea.",
            Some(Dia { boxes: vec![(0, 0, "Trip", ""), (1, 0, "Hotel", ""), (0, 1, "Visa", "held"), (1, 1, "Visa", "new")], links: vec![(0, 1, false), (0, 3, true)], note: "Drop \"Visa\" on the middle of \"Trip\": it moves under \"Trip\"." })),
        item(one(vec![word("Drag to the edge of an idea")]), "Dropped on the top or bottom edge of an idea, it goes just before or after it.",
            Some(Dia { boxes: vec![(0, 1, "Trip", ""), (1, 0, "Visa", "new"), (1, 1, "Flights", ""), (1, 2, "Hotel", "")], links: vec![(0, 1, true), (0, 2, false), (0, 3, false)], note: "Drop \"Visa\" on the top edge of \"Flights\": it becomes the one before it." })),
        item(one(vec![word("Drag onto a chat or terminal")]), "Puts a reference to the idea in the message.", None),
        Row::Head("LOOKING AROUND"),
        item(one(vec![key("Ctrl+Scroll"), or("or"), word("pinch")]), "Zoom, around the pointer.", None),
        item(one(vec![key("Ctrl++"), or("or"), key("Ctrl+\u{2212}")]).into_iter().map(|l| fix_plus(l)).collect(), "Zoom in or out, about the middle of the map.", None),
        item(one(vec![vec![("kbd", "+".to_string())], or("or"), vec![("kbd", "\u{2212}".to_string())]]), "With the ring on an idea: + unfolds what is under it, \u{2212} folds it away.", None),
        item(one(vec![word("\u{2212} / +")]), "The small button on an idea folds or unfolds what is under it.",
            Some(Dia { boxes: vec![(0, 0, "Trip  +", "held"), (1, 0, "Flights", ""), (1, 1, "Hotel", "")], links: vec![(0, 1, true), (0, 2, true)], note: "Folded, \"Trip\" keeps its ideas but hides them until you unfold it." })),
        item(one(vec![word("Hover")]), "Shows the whole of a long idea.", None),
        item(one(vec![word("Filter ideas")]), "Lights up the ideas that match and dims the rest.", None),
    ]
}

/// "Ctrl++" splits into Ctrl, +, "", "": put the + key back.
fn fix_plus(l: Vec<(&'static str, String)>) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = vec![];
    let mut i = 0;
    while i < l.len() {
        if l[i].0 == "kbd" && l[i].1.is_empty() && i + 2 < l.len() && l[i + 2].0 == "kbd" && l[i + 2].1.is_empty() {
            out.push(("kbd", "+".to_string()));
            i += 3;
            continue;
        }
        out.push(l[i].clone());
        i += 1;
    }
    out
}

/// A tree's size and its lines, as SVG path data (solid, dashed): columns
/// `cw` apart, rows `rh`, boxes `bw` by `bh`, `pad` around.
pub fn dia_paths(d: &Dia, cw: f32, rh: f32, bw: f32, bh: f32, pad: f32) -> (f32, f32, String, String) {
    let cols = d.boxes.iter().map(|b| b.0).max().unwrap_or(0) + 1;
    let rows = d.boxes.iter().map(|b| b.1).max().unwrap_or(0) + 1;
    let w = pad * 2.0 + (cols - 1) as f32 * cw + bw;
    let h = pad * 2.0 + (rows - 1) as f32 * rh + bh;
    let (mut solid, mut dash) = (String::new(), String::new());
    for (a, b, dashed) in &d.links {
        let (a, b) = (d.boxes[*a], d.boxes[*b]);
        let (x1, y1) = (pad + a.0 as f32 * cw + bw, pad + a.1 as f32 * rh + bh / 2.0);
        let (x2, y2) = (pad + b.0 as f32 * cw, pad + b.1 as f32 * rh + bh / 2.0);
        if *dashed { dash += &super::tree::dashed_path(x1, y1, x2, y2, 3.0, 3.0); } else {
            let m = (x1 + x2) / 2.0;
            solid += &format!("M {x1:.1} {y1:.1} C {m:.1} {y1:.1} {m:.1} {y2:.1} {x2:.1} {y2:.1} ");
        }
    }
    (w, h, solid, dash)
}

/// The same lines as end points and whether dashed, for GPUI's paths.
pub fn dia_links(d: &Dia, cw: f32, rh: f32, bw: f32, bh: f32, pad: f32) -> Vec<([f32; 4], bool)> {
    d.links.iter().map(|(a, b, dashed)| {
        let (a, b) = (d.boxes[*a], d.boxes[*b]);
        ([pad + a.0 as f32 * cw + bw, pad + a.1 as f32 * rh + bh / 2.0, pad + b.0 as f32 * cw, pad + b.1 as f32 * rh + bh / 2.0], *dashed)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_reads_as_the_web() {
        let r = rows();
        let heads: Vec<&str> = r.iter().filter_map(|x| if let Row::Head(h) = x { Some(*h) } else { None }).collect();
        assert_eq!(heads, vec!["ADDING IDEAS", "EDITING", "MOVING ABOUT", "MOVING IDEAS", "LOOKING AROUND"]);
        let dias = r.iter().filter(|x| matches!(x, Row::Item { dia: Some(_), .. })).count();
        assert_eq!(dias, 9);
        // Ctrl and +: the + key survives the split.
        let zoom = r.iter().find_map(|x| if let Row::Item { keys, text, .. } = x { text.starts_with("Zoom in").then(|| keys.clone()) } else { None }).unwrap();
        let caps: Vec<String> = zoom[0].iter().filter(|k| k.0 == "kbd").map(|k| k.1.clone()).collect();
        assert_eq!(caps, vec!["Ctrl", "+", "Ctrl", "\u{2212}"]);
        let ctrl_shift = key("Ctrl+Shift+Up / Down");
        assert_eq!(ctrl_shift.iter().filter(|k| k.0 == "kbd").count(), 3);
    }

    #[test]
    fn a_tree_has_its_size_and_lines() {
        let d = Dia { boxes: vec![(0, 1, "Trip", ""), (1, 0, "A", ""), (1, 2, "B", "new")], links: vec![(0, 1, false), (0, 2, true)], note: "" };
        let (w, h, s, dsh) = dia_paths(&d, 92.0, 30.0, 74.0, 20.0, 6.0);
        assert_eq!((w, h), (6.0 * 2.0 + 92.0 + 74.0, 6.0 * 2.0 + 60.0 + 20.0));
        assert!(s.starts_with("M 80.0 46.0 C"));
        assert!(dsh.contains('L'));
        let l = dia_links(&d, 92.0, 30.0, 74.0, 20.0, 6.0);
        assert_eq!(l[0], ([80.0, 46.0, 98.0, 16.0], false));
        assert!(l[1].1);
    }
}
