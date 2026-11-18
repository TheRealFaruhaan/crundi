//! Project icons, as the web's projIconSrc: a project's `icon` address
//! (GET /api/project-icon?project=…&v=…, the server's pick of the repo's
//! icon) fetched once with the sign-in and kept as a picture; the address
//! changes when the file does, so a new icon is a new entry. Until it has
//! arrived, and for good if it can't be had, the project keeps its dot and
//! letters. The owner's 18 Nov report: no project logos in the sidebar.

use crate::session;
use gpui::*;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
struct Icons(HashMap<String, Option<Arc<Image>>>);
impl Global for Icons {}

/// The picture for a project's icon address, or None (none, not yet, or failed).
pub fn icon(cx: &mut App, url: &str) -> Option<Arc<Image>> {
    if url.is_empty() || !url.starts_with("/api/project-icon?") { return None; }
    if let Some(v) = cx.default_global::<Icons>().0.get(url) { return v.clone(); }
    cx.default_global::<Icons>().0.insert(url.to_string(), None);
    let Some(api) = session::api(cx) else { return None };
    let u = url.to_string();
    session::run(cx, move || api.get_bytes(&u).ok(), {
        let u = url.to_string();
        move |r: Option<(u16, Vec<u8>)>, cx: &mut App| {
            let Some((200, bytes)) = r else { return };
            let Some(format) = sniff(&bytes) else { return };
            cx.default_global::<Icons>().0.insert(u, Some(Arc::new(Image::from_bytes(format, bytes))));
            cx.refresh_windows();
        }
    });
    None
}

/// The icon of project `alias` (its `icon` field in the projects list).
pub fn of_project(cx: &mut App, alias: &str) -> Option<Arc<Image>> {
    let url = session::get(cx).projects.iter().find(|p| p.get("alias").and_then(|a| a.as_str()) == Some(alias))
        .and_then(|p| p.get("icon").and_then(|i| i.as_str()).map(String::from)).unwrap_or_default();
    icon(cx, &url)
}

/// The picture's format from its first bytes.
fn sniff(b: &[u8]) -> Option<ImageFormat> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) { return Some(ImageFormat::Png); }
    if b.starts_with(&[0xFF, 0xD8]) { return Some(ImageFormat::Jpeg); }
    if b.starts_with(b"GIF8") { return Some(ImageFormat::Gif); }
    if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" { return Some(ImageFormat::Webp); }
    if b.starts_with(&[0, 0, 1, 0]) { return Some(ImageFormat::Ico); }
    if b.starts_with(b"BM") { return Some(ImageFormat::Bmp); }
    let head = String::from_utf8_lossy(&b[..b.len().min(512)]).to_ascii_lowercase();
    if head.contains("<svg") { return Some(ImageFormat::Svg); }
    None
}

#[cfg(test)]
mod tests {
    // (Not `super::*`: gpui's `test` macro would shadow the standard one.)
    use super::sniff;
    use gpui::ImageFormat;
    #[test]
    fn sniffs_the_formats() {
        assert!(matches!(sniff(&[0x89, b'P', b'N', b'G', 0, 0]), Some(ImageFormat::Png)));
        assert!(matches!(sniff(&[0xFF, 0xD8, 0xFF]), Some(ImageFormat::Jpeg)));
        assert!(matches!(sniff(&[0, 0, 1, 0, 1]), Some(ImageFormat::Ico)));
        assert!(matches!(sniff(b"<?xml version=\"1.0\"?><svg xmlns="), Some(ImageFormat::Svg)));
        assert!(sniff(b"not an image").is_none());
    }
}
