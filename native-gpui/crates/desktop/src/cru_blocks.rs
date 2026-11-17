//! Cru's blocks on the live screen (crundi-voice.js blocksHtml), drawn
//! natively: heading, text, list (ordered or not), checklist, table,
//! progress, image (https, or a data: picture), chips coloured by tone
//! (ok green, warn yellow, bad red, info sky, muted). Every size and colour
//! a token; `kid` takes the kid screen's colours.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

fn s(v: &Value, k: &str) -> String { v.get(k).map(|x| match x { Value::String(s) => s.clone(), Value::Null => String::new(), o => o.to_string() }).unwrap_or_default() }

/// A tone's colour (TONE_COLOURS).
pub fn tone(t: &str) -> Hsla {
    match t { "ok" => T::green(), "warn" => T::yellow(), "bad" => T::red(), "muted" => T::text_muted(), _ => T::sky() }
}

thread_local! {
    /// data: pictures, decoded once.
    static PICS: std::cell::RefCell<HashMap<String, Option<Arc<RenderImage>>>> = std::cell::RefCell::new(HashMap::new());
}

fn data_image(url: &str) -> Option<Arc<RenderImage>> {
    PICS.with(|p| p.borrow_mut().entry(url.to_string()).or_insert_with(|| {
        use base64::Engine as _;
        let b64 = url.split_once(";base64,")?.1;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
        let mut i = image::load_from_memory(&bytes).ok()?.to_rgba8();
        for px in i.chunks_exact_mut(4) { px.swap(0, 2); }
        Some(Arc::new(RenderImage::new([image::Frame::new(i)])))
    }).clone())
}

pub fn blocks(list: &[Value], kid: bool) -> Div {
    let (fg, sub) = if kid { (T::kid_text(), T::kid_text()) } else { (T::text(), T::text_secondary()) };
    let mut d = div().flex().flex_col().gap(T::SP_8).text_color(fg);
    for (n, b) in list.iter().enumerate() {
        let el: Option<AnyElement> = match s(b, "type").as_str() {
            "heading" => Some(div().text_size(if kid { T::KID_HEADING } else { T::TEXT_16 }).font_weight(T::W_BOLD).child(s(b, "text")).into_any_element()),
            "text" => Some(div().text_size(if kid { T::KID_TEXT } else { T::TEXT_13 }).text_color(sub).child(s(b, "text")).into_any_element()),
            "list" => {
                let ordered = b.get("ordered").and_then(|x| x.as_bool()).unwrap_or(false);
                let mut l = div().flex().flex_col().gap(T::SP_4).pl(T::SP_8);
                for (i, it) in b.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter().enumerate() {
                    let t = match it { Value::String(s) => s.clone(), o => o.to_string() };
                    l = l.child(div().flex().gap(T::SP_8).text_size(T::TEXT_13)
                        .child(div().text_color(T::text_muted()).child(if ordered { format!("{}.", i + 1) } else { "•".to_string() }))
                        .child(div().flex_1().child(t)));
                }
                Some(l.into_any_element())
            }
            "checklist" => {
                let mut l = div().flex().flex_col().gap(T::SP_4);
                for it in b.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    let done = it.get("done").and_then(|x| x.as_bool()).unwrap_or(false);
                    l = l.child(div().flex().items_center().gap(T::SP_8).text_size(T::TEXT_13)
                        .child(div().text_size(T::ICON_MD).text_color(if done { T::green() } else { T::text_muted() }).child(if done { IconName::SquareCheck } else { IconName::Square }))
                        .child(div().flex_1().when(done, |d| d.line_through().text_color(T::text_muted())).child(s(&it, "text"))));
                }
                Some(l.into_any_element())
            }
            "table" => {
                let mut t = div().flex().flex_col().rounded(T::R_CHIP).border_1().border_color(T::border()).text_size(T::TEXT_12);
                if let Some(h) = b.get("headers").and_then(|x| x.as_array()) {
                    let mut r = div().flex().bg(T::bg_tertiary()).font_weight(T::W_SEMIBOLD);
                    for c in h { r = r.child(div().flex_1().min_w_0().px(T::SP_8).py(T::SP_4).child(match c { Value::String(s) => s.clone(), o => o.to_string() })); }
                    t = t.child(r);
                }
                for row in b.get("rows").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    let mut r = div().flex().border_t_1().border_color(T::border_subtle());
                    for c in row.as_array().cloned().unwrap_or_default() { r = r.child(div().flex_1().min_w_0().px(T::SP_8).py(T::SP_4).child(match c { Value::String(s) => s, o => o.to_string() })); }
                    t = t.child(r);
                }
                Some(t.into_any_element())
            }
            "progress" => {
                let v = b.get("value").and_then(|x| x.as_f64()).unwrap_or(0.0).clamp(0.0, 100.0);
                Some(div().flex().flex_col().gap(T::SP_4)
                    .child(div().text_size(T::TEXT_12).text_color(sub).child(format!("{} {}%", s(b, "label"), v.round())))
                    .child(div().h(T::SP_6).rounded(T::R_PILL).bg(T::bg_tertiary()).child(div().h_full().rounded(T::R_PILL).bg(T::accent()).w(relative(v as f32 / 100.0))))
                    .into_any_element())
            }
            "image" => {
                let url = s(b, "url");
                if url.starts_with("data:image/") { data_image(&url).map(|i| gpui::img(ImageSource::Render(i)).max_w_full().max_h(T::CRU_SCREEN_H).object_fit(ObjectFit::Contain).into_any_element()) }
                else if url.starts_with("https:") { Some(gpui::img(SharedUri::from(url)).max_w_full().max_h(T::CRU_SCREEN_H).object_fit(ObjectFit::Contain).into_any_element()) }
                else { None }
            }
            "chips" => {
                let mut r = div().flex().flex_wrap().gap(T::SP_6);
                for c in b.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    let col = tone(&s(&c, "tone"));
                    r = r.child(div().px(T::SP_8).py(T::SP_2).rounded(T::R_PILL).border_1().border_color(col.opacity(T::CHIP_EDGE)).bg(col.opacity(T::CHIP_FILL)).text_color(col).text_size(T::TEXT_12).child(s(&c, "text")));
                }
                Some(r.into_any_element())
            }
            _ => None,
        };
        if let Some(e) = el { d = d.child(div().id(("cru-block", n)).child(e)); }
    }
    d
}
