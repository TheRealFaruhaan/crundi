//! A drawer tab's picture with its marks drawn in (VD composite), and
//! clearing browsing data (BD drawer:clear).

use crate::browser;
use serde_json::{json, Value};

const YOU: [u8; 3] = [0xea, 0x58, 0x0c];
const CLAUDE: [u8; 3] = [0x63, 0x66, 0xf1];

/// The page as a PNG at device pixels, with the marks (the person's only
/// when [mine]) drawn and numbered in their order.
pub fn composite(i: usize, mine: bool, done: Box<dyn FnOnce(Result<Vec<u8>, String>)>) {
    capture(i, Box::new(move |r| {
        let png = match r { Ok(p) => p, Err(e) => return done(Err(e)) };
        let img = match image::load_from_memory_with_format(&png, image::ImageFormat::Png) { Ok(i) => i.to_rgba8(), Err(_) => return done(Err("could not read the capture".into())) };
        let marks: Vec<browser::Ann> = browser::with(|st| st.panels[i].ann.iter().filter(|a| !mine || a.by != "claude").cloned().collect());
        let (vw, _, ..) = browser::with(|st| st.panels[i].view());
        let k = img.width() as f32 / vw.max(1) as f32;
        done(Ok(encode(&draw(img, &marks, k))))
    }));
}

/// The page's picture (CDP: what Chromium renders, at device pixels).
pub fn capture(i: usize, done: Box<dyn FnOnce(Result<Vec<u8>, String>)>) {
    browser::cdp(i, "Page.captureScreenshot", json!({ "format": "png" }), Box::new(move |ok, v| {
        use base64::Engine;
        match v.get("data").and_then(|d| d.as_str()) {
            Some(d) if ok => match base64::engine::general_purpose::STANDARD.decode(d) { Ok(b) => done(Ok(b)), Err(_) => done(Err("capture failed".into())) },
            _ => match browser::screenshot(i) {
                Some(b64) => done(base64::engine::general_purpose::STANDARD.decode(b64).map_err(|_| "capture failed".to_string())),
                None => done(Err("capture failed".into())),
            },
        }
    }));
}

pub fn encode(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

/// Draw the marks onto the picture, k device px per page px.
pub fn draw(mut img: image::RgbaImage, marks: &[browser::Ann], k: f32) -> image::RgbaImage {
    let lw = (3.0 * k).max(1.0);
    for (n, a) in marks.iter().enumerate() {
        let c = if a.by == "claude" { CLAUDE } else { YOU };
        match a.kind.as_str() {
            "box" => rect(&mut img, a.x * k, a.y * k, a.w * k, a.h * k, lw, c, false),
            "element" => rect(&mut img, a.x * k, a.y * k, a.w * k, a.h * k, lw, c, true),
            "pen" => for w in a.points.windows(2) { line(&mut img, w[0].0 * k, w[0].1 * k, w[1].0 * k, w[1].1 * k, lw, c); },
            "arrow" => {
                line(&mut img, a.x * k, a.y * k, a.to.0 * k, a.to.1 * k, lw, c);
                let ang = (a.to.1 - a.y).atan2(a.to.0 - a.x);
                let hl = 12.0 * k;
                for d in [-0.45f32, 0.45] { line(&mut img, a.to.0 * k, a.to.1 * k, a.to.0 * k - hl * (ang + d).cos(), a.to.1 * k - hl * (ang + d).sin(), lw, c); }
            }
            _ => {}
        }
        // The numbered disc: on a pin, else at the mark's corner.
        let r = 11.0 * k;
        let (cx, cy) = if a.kind == "pin" { (a.x * k, a.y * k) } else { (a.x * k + r, a.y * k + r) };
        disc(&mut img, cx, cy, r, c);
        number(&mut img, cx, cy, r, n + 1);
    }
    img
}

fn put(img: &mut image::RgbaImage, x: i64, y: i64, c: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() { img.put_pixel(x as u32, y as u32, image::Rgba([c[0], c[1], c[2], 255])); }
}

fn line(img: &mut image::RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: [u8; 3]) {
    let steps = ((x1 - x0).abs().max((y1 - y0).abs()) * 2.0).ceil().max(1.0) as i64;
    let h = (w / 2.0).max(0.5);
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        for dy in (-h.ceil() as i64)..=(h.ceil() as i64) { for dx in (-h.ceil() as i64)..=(h.ceil() as i64) {
            if ((dx * dx + dy * dy) as f32) <= h * h + 0.5 { put(img, x as i64 + dx, y as i64 + dy, c); }
        } }
    }
}

#[allow(clippy::too_many_arguments)]
fn rect(img: &mut image::RgbaImage, x: f32, y: f32, w: f32, h: f32, lw: f32, c: [u8; 3], dashed: bool) {
    let edges = [(x, y, x + w, y), (x + w, y, x + w, y + h), (x + w, y + h, x, y + h), (x, y + h, x, y)];
    for (x0, y0, x1, y1) in edges {
        if !dashed { line(img, x0, y0, x1, y1, lw, c); continue; }
        let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let (dash, gap) = (6.0 * lw / 3.0, 3.0 * lw / 3.0);
        let mut t = 0.0;
        while t < len {
            let e = (t + dash).min(len);
            line(img, x0 + (x1 - x0) * t / len, y0 + (y1 - y0) * t / len, x0 + (x1 - x0) * e / len, y0 + (y1 - y0) * e / len, lw, c);
            t += dash + gap;
        }
    }
}

fn disc(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, c: [u8; 3]) {
    let ri = r.ceil() as i64;
    for dy in -ri..=ri { for dx in -ri..=ri {
        if ((dx * dx + dy * dy) as f32) <= r * r { put(img, cx as i64 + dx, cy as i64 + dy, c); }
    } }
}

/// The disc's number in white: a 3x5 figure font, scaled to the disc.
fn number(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, n: usize) {
    const F: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7], [2, 6, 2, 2, 7], [7, 1, 7, 4, 7], [7, 1, 7, 1, 7], [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7], [7, 4, 7, 5, 7], [7, 1, 1, 1, 1], [7, 5, 7, 5, 7], [7, 5, 7, 1, 7],
    ];
    let digits: Vec<usize> = n.to_string().chars().filter_map(|c| c.to_digit(10)).map(|d| d as usize).collect();
    let px = (r * 1.1 / 5.0).max(1.0);
    let w = digits.len() as f32 * 4.0 * px - px;
    let (x0, y0) = (cx - w / 2.0, cy - 2.5 * px);
    for (di, d) in digits.iter().enumerate() {
        for (row, bits) in F[*d].iter().enumerate() { for col in 0..3 {
            if bits & (4 >> col) != 0 {
                let (x, y) = (x0 + (di as f32 * 4.0 + col as f32) * px, y0 + row as f32 * px);
                for yy in 0..px.ceil() as i64 { for xx in 0..px.ceil() as i64 { put(img, x as i64 + xx, y as i64 + yy, [255, 255, 255]); } }
            }
        } }
    }
}

/// Clear cookies and site data, and the cache, in every profile (one site
/// and its subdomains, or all): the drawer's "Clear browsing data".
pub fn clear_data(site: Option<&str>, cookies: bool, cache: bool) {
    let any = browser::with(|st| st.panels.iter().position(|p| p.browser.is_some() && !p.closed));
    let Some(i) = any else { return };
    if cookies {
        match site {
            None => browser::cdp(i, "Storage.clearDataForOrigin", json!({ "origin": "*", "storageTypes": "all" }), Box::new(|_, _| {})),
            Some(s) => for o in [format!("https://{s}"), format!("http://{s}")] {
                browser::cdp(i, "Storage.clearDataForOrigin", json!({ "origin": o, "storageTypes": "all" }), Box::new(|_, _| {}));
            },
        }
        for (_, ctx) in browser::contexts() {
            use cef::ImplRequestContext;
            if let Some(cm) = ctx.cookie_manager(None) {
                use cef::ImplCookieManager;
                match site {
                    None => { cm.delete_cookies(None, None, None); }
                    Some(s) => for o in [format!("https://{s}"), format!("http://{s}")] { cm.delete_cookies(Some(&cef::CefString::from(o.as_str())), None, None); },
                }
            }
        }
    }
    if cache {
        // Every page's network cache (CDP: per browser, the profile's).
        let all: Vec<usize> = browser::with(|st| st.panels.iter().enumerate().filter(|(_, p)| p.browser.is_some() && !p.closed).map(|(n, _)| n).collect());
        for n in all { browser::cdp(n, "Network.clearBrowserCache", Value::Object(Default::default()), Box::new(|_, _| {})); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_marks() {
        let img = image::RgbaImage::from_pixel(100, 80, image::Rgba([255, 255, 255, 255]));
        let marks = vec![
            browser::Ann { by: "you".into(), kind: "box".into(), x: 10.0, y: 10.0, w: 40.0, h: 30.0, ..Default::default() },
            browser::Ann { by: "claude".into(), kind: "pin".into(), x: 70.0, y: 60.0, ..Default::default() },
        ];
        let out = draw(img, &marks, 1.0);
        // The box's edge is orange; the pin's disc indigo.
        assert_eq!(out.get_pixel(30, 10).0[..3], YOU);
        assert_eq!(out.get_pixel(70, 50).0[..3], CLAUDE);
        assert!(encode(&out).len() > 100);
    }
}
