//! The chat's font (2.25, specs/known-gaps.md K1 and K2). Slint's StyledText
//! has no line height of its own: it takes its default font's own line,
//! (ascent − descent + line gap) / units per em. So the chat gets a copy of
//! the UI font whose line gap makes that line 1.55 (the web's), registered
//! under its own family at start (src/main.rs). The same metrics give the
//! other texts' line-height factors (over the UI font's own line) on every
//! system, instead of constants tuned for one font. Pure.

/// The tables of an sfnt (TrueType / OpenType, not a collection): tag → (offset, length).
fn tables(b: &[u8]) -> Option<Vec<([u8; 4], usize, usize, usize)>> {
    let n = u16::from_be_bytes(b.get(4..6)?.try_into().ok()?) as usize;
    let mut out = vec![];
    for i in 0..n {
        let r = 12 + i * 16;
        let rec = b.get(r..r + 16)?;
        let tag: [u8; 4] = rec[0..4].try_into().ok()?;
        let off = u32::from_be_bytes(rec[8..12].try_into().ok()?) as usize;
        let len = u32::from_be_bytes(rec[12..16].try_into().ok()?) as usize;
        if off + len > b.len() { return None; }
        out.push((tag, off, len, r));
    }
    Some(out)
}

fn i16_at(b: &[u8], at: usize) -> Option<i16> { Some(i16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?)) }
fn u16_at(b: &[u8], at: usize) -> Option<u16> { Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?)) }

/// A font's line metrics as skrifa reads them (FreeType's rule): OS/2's
/// typo metrics when USE_TYPO_METRICS is set, else hhea's.
/// (units per em, ascent, descent (negative), line gap, typo?)
pub fn metrics(b: &[u8]) -> Option<(f64, f64, f64, f64, bool)> {
    let t = tables(b)?;
    let find = |tag: &[u8; 4]| t.iter().find(|x| &x.0 == tag).map(|x| x.1);
    let head = find(b"head")?;
    let upem = u16_at(b, head + 18)? as f64;
    let hhea = find(b"hhea")?;
    if let Some(os2) = find(b"OS/2") {
        let sel = u16_at(b, os2 + 62)?;
        if sel & (1 << 7) != 0 {
            return Some((upem, i16_at(b, os2 + 68)? as f64, i16_at(b, os2 + 70)? as f64, i16_at(b, os2 + 72)? as f64, true));
        }
    }
    Some((upem, i16_at(b, hhea + 4)? as f64, i16_at(b, hhea + 6)? as f64, i16_at(b, hhea + 8)? as f64, false))
}

/// The font's own line, in ems: (ascent − descent + gap) / upem.
pub fn natural_line(b: &[u8]) -> Option<f64> {
    let (upem, a, d, g, _) = metrics(b)?;
    (upem > 0.0).then(|| (a - d + g) / upem)
}

/// A copy of the font whose own line is `line` ems: both hhea's and OS/2's
/// line gaps set from their own ascent and descent; a variable font's MVAR
/// (which could move the gap) hidden by renaming its tag.
pub fn with_line(b: &[u8], line: f64) -> Option<Vec<u8>> {
    let t = tables(b)?;
    let mut out = b.to_vec();
    let find = |tag: &[u8; 4]| t.iter().find(|x| &x.0 == tag).map(|x| x.1);
    let upem = u16_at(b, find(b"head")? + 18)? as f64;
    let set = |out: &mut Vec<u8>, at_asc: usize, at_desc: usize, at_gap: usize| -> Option<()> {
        let (a, d) = (i16_at(b, at_asc)? as f64, i16_at(b, at_desc)? as f64);
        let gap = (line * upem - (a - d)).round();
        if !(0.0..=i16::MAX as f64).contains(&gap) { return None; }
        out[at_gap..at_gap + 2].copy_from_slice(&(gap as i16).to_be_bytes());
        Some(())
    };
    let hhea = find(b"hhea")?;
    set(&mut out, hhea + 4, hhea + 6, hhea + 8)?;
    if let Some(os2) = find(b"OS/2") { set(&mut out, os2 + 68, os2 + 70, os2 + 72)?; }
    if let Some(&(_, _, _, rec)) = t.iter().find(|x| &x.0 == b"MVAR") { out[rec..rec + 4].copy_from_slice(b"mvar"); }
    Some(out)
}

/// The factor over a font's own line that gives `want` ems (Slint's
/// line-height-factor): the web's 1.55 chat, 1.3 UI, 1.45 composer.
pub fn factor(natural: f64, want: f64) -> f64 { if natural > 0.0 { want / natural } else { 1.0 } }

#[cfg(test)]
mod tests {
    use super::*;

    const DEJAVU: &[u8] = include_bytes!("../ui/fonts/DejaVuSans.ttf");

    #[test]
    fn dejavu_metrics() {
        let (upem, a, d, _, _) = metrics(DEJAVU).unwrap();
        assert_eq!((upem, a, d), (2048.0, 1901.0, -483.0));
        let n = natural_line(DEJAVU).unwrap();
        assert!((n - 1.164).abs() < 0.001, "{n}");
        // The factors tuned by hand for DejaVu come out of its metrics.
        assert!((factor(n, 1.55) - 1.33).abs() < 0.005);
        assert!((factor(n, 1.3) - 1.12).abs() < 0.005);
    }

    #[test]
    fn a_chat_copy_with_a_155_line() {
        let c = with_line(DEJAVU, 1.55).unwrap();
        assert_eq!(c.len(), DEJAVU.len());
        let n = natural_line(&c).unwrap();
        assert!((n - 1.55).abs() < 0.001, "{n}");
        // Its ascent and descent (the glyphs' place) are as they were.
        let (_, a, d, g, _) = metrics(&c).unwrap();
        assert_eq!((a, d, g), (1901.0, -483.0, 790.0));
        // Every face of the bundled family copies.
        for f in [&include_bytes!("../ui/fonts/DejaVuSans-Bold.ttf")[..], &include_bytes!("../ui/fonts/DejaVuSans-Oblique.ttf")[..], &include_bytes!("../ui/fonts/DejaVuSans-BoldOblique.ttf")[..]] {
            assert!((natural_line(&with_line(f, 1.55).unwrap()).unwrap() - 1.55).abs() < 0.001);
        }
        assert!(with_line(b"not a font", 1.55).is_none());
    }
}
