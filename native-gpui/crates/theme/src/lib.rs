//! The one constant theme (DESIGN.md): every colour, size, radius, gap and
//! weight the client draws with, as named tokens, with the Slint client's
//! values (native-client/ui/theme.slint). Screens never type a number; they
//! take it from here. The same tokens reach gpui-component's own controls as
//! a registered theme (`init`), for light and dark alike, so the system's
//! appearance can't swap it.
//!
//! Only the Lead changes the scales. An area adds a named structural size in
//! its own section at the end of `T` (AREAS.md).

use gpui::{px, rgb, rgba, App, FontWeight, Hsla, Pixels};

pub use gpui_kit_assets::{AllAssets, IconName};

/// The tokens.
pub struct T;

#[allow(dead_code)]
impl T {
    // ─── Colours (the web's palette) ───
    pub fn bg() -> Hsla { rgb(0x0a0a0f).into() }
    pub fn bg_secondary() -> Hsla { rgb(0x12121a).into() }
    pub fn bg_tertiary() -> Hsla { rgb(0x1a1a28).into() }
    pub fn bg_hover() -> Hsla { rgb(0x22223a).into() }
    pub fn bg_card() -> Hsla { rgb(0x14141f).into() }
    pub fn border() -> Hsla { rgb(0x2a2a3d).into() }
    pub fn border_subtle() -> Hsla { rgb(0x1e1e30).into() }
    pub fn text() -> Hsla { rgb(0xe8e8f0).into() }
    pub fn text_secondary() -> Hsla { rgb(0x8888a8).into() }
    pub fn text_muted() -> Hsla { rgb(0x5a5a78).into() }
    pub fn white() -> Hsla { rgb(0xffffff).into() }
    pub fn accent() -> Hsla { rgb(0x6366f1).into() }
    pub fn accent_hover() -> Hsla { rgb(0x818cf8).into() }
    pub fn accent_deep() -> Hsla { rgb(0x4f46e5).into() }
    pub fn accent_dim() -> Hsla { rgba(0x6366f126).into() }
    pub fn accent_edge() -> Hsla { rgba(0x6366f159).into() }
    pub fn amber() -> Hsla { rgb(0xfbbf24).into() }
    pub fn green() -> Hsla { rgb(0x10b981).into() }
    pub fn green_dim() -> Hsla { rgba(0x10b98126).into() }
    pub fn red() -> Hsla { rgb(0xef4444).into() }
    pub fn red_dim() -> Hsla { rgba(0xef444426).into() }
    pub fn yellow() -> Hsla { rgb(0xf59e0b).into() }
    pub fn yellow_dim() -> Hsla { rgba(0xf59e0b26).into() }
    pub fn sky() -> Hsla { rgb(0x38bdf8).into() }
    pub fn sky_dim() -> Hsla { rgba(0x38bdf829).into() }
    pub fn error_text() -> Hsla { rgb(0xfca5a5).into() }
    pub fn error_edge() -> Hsla { rgba(0xef444466).into() }
    pub fn placeholder() -> Hsla { rgb(0x757575).into() }
    pub fn annotate() -> Hsla { rgb(0xea580c).into() }
    pub fn selection() -> Hsla { rgb(0x0236a1).into() }
    pub fn scrim() -> Hsla { rgba(0x0000008c).into() }
    pub fn backdrop() -> Hsla { rgba(0x000000db).into() }
    pub fn frame_bg() -> Hsla { rgb(0x0f0f14).into() }
    pub fn usage_used() -> Hsla { rgba(0x6366f138).into() }
    pub fn usage_time() -> Hsla { rgba(0x7daaff29).into() }

    // ─── Spacing: 2 4 6 8 10 12 16 20 24 32 40 48 ───
    pub const SP_2: Pixels = px(2.);
    pub const SP_4: Pixels = px(4.);
    pub const SP_6: Pixels = px(6.);
    pub const SP_8: Pixels = px(8.);
    pub const SP_10: Pixels = px(10.);
    pub const SP_12: Pixels = px(12.);
    pub const SP_16: Pixels = px(16.);
    pub const SP_20: Pixels = px(20.);
    pub const SP_24: Pixels = px(24.);
    pub const SP_32: Pixels = px(32.);
    pub const SP_40: Pixels = px(40.);
    pub const SP_48: Pixels = px(48.);

    // ─── Radius: chip, control, field, card / pane, bubble, pill ───
    pub const R_CHIP: Pixels = px(4.);
    pub const R_CONTROL: Pixels = px(6.);
    pub const R_FIELD: Pixels = px(8.);
    pub const R_CARD: Pixels = px(10.);
    pub const R_BUBBLE: Pixels = px(12.);
    pub const R_PILL: Pixels = px(999.);

    // ─── Font sizes: 10 11 12 13 14 15 16 20 26 ───
    pub const TEXT_10: Pixels = px(10.);
    pub const TEXT_11: Pixels = px(11.);
    pub const TEXT_12: Pixels = px(12.);
    pub const TEXT_13: Pixels = px(13.);
    pub const TEXT_14: Pixels = px(14.);
    pub const TEXT_15: Pixels = px(15.);
    pub const TEXT_16: Pixels = px(16.);
    pub const TEXT_20: Pixels = px(20.);
    pub const TEXT_26: Pixels = px(26.);

    // ─── Weights ───
    pub const W_REGULAR: FontWeight = FontWeight(400.);
    pub const W_MEDIUM: FontWeight = FontWeight(500.);
    pub const W_SEMIBOLD: FontWeight = FontWeight(600.);
    pub const W_BOLD: FontWeight = FontWeight(700.);

    // ─── Line heights (factors): chat 1.55, UI 1.3, composer 1.45 ───
    pub const LH_CHAT: f32 = 1.55;
    pub const LH_UI: f32 = 1.3;
    pub const LH_COMPOSER: f32 = 1.45;

    // ─── Letter spacing: tight and wide (em) ───
    pub const TRACK_TIGHT: f32 = -0.01;
    pub const TRACK_WIDE: f32 = 0.05;

    // ─── Named sizes, only for structure ───
    pub const WINDOW_W: Pixels = px(1400.);
    pub const WINDOW_H: Pixels = px(860.);
    pub const WINDOW_MIN_W: Pixels = px(900.);
    pub const WINDOW_MIN_H: Pixels = px(520.);
    pub const TOPBAR_H: Pixels = px(48.);
    pub const TABBAR_H: Pixels = px(36.);
    pub const SIDEBAR_W: Pixels = px(240.);
    pub const SIDEBAR_FOLDED: Pixels = px(60.);
    pub const PANE_HEAD: Pixels = px(28.);
    pub const PANE_HEAD_NARROW: Pixels = px(26.);
    pub const PANE_MIN: Pixels = px(340.);
    pub const PANE_FOLD: Pixels = px(560.);
    pub const GUTTER: Pixels = px(7.);
    pub const TITLE_CAP: Pixels = px(160.);
    pub const COMPOSER_STACK: Pixels = px(520.);
    pub const COMPOSER_MAX: Pixels = px(180.);
    pub const DRAWER_W: Pixels = px(560.);
    pub const DRAWER_MIN: Pixels = px(320.);
    pub const DRAWER_RESERVE: Pixels = px(420.);
    pub const DIVIDER: Pixels = px(5.);
    pub const DIALOG_W: Pixels = px(380.);
    pub const DIALOG_W_LG: Pixels = px(460.);
    pub const DIALOG_W_XL: Pixels = px(560.);
    pub const DIALOG_MAX_H: Pixels = px(560.);
    pub const CONTROL_SM: Pixels = px(28.);
    pub const CONTROL: Pixels = px(34.);
    pub const CONTROL_LG: Pixels = px(36.);
    pub const ICON_SM: Pixels = px(12.);
    pub const ICON_MD: Pixels = px(14.);
    pub const ICON_LG: Pixels = px(16.);
    pub const ICON_XL: Pixels = px(20.);
    pub const DOT: Pixels = px(8.);
    pub const MENU_MIN_W: Pixels = px(190.);

    // ─── Type ───
    pub const UI_FONT: &'static str = "DejaVu Sans";
    pub const MONO: &'static str = "DejaVu Sans Mono";

    // ─── The areas' named sizes (AREAS.md): each only in its own section ───
    // ── workspace ──
    // ── end workspace ──
    // ── planning ──
    /// secrets (2.12): the dialog's pick list stops here and scrolls.
    pub const SEC_PICK_MAX_H: Pixels = px(240.);
    /// secrets (2.12): a ticked secret's PIN or passphrase field.
    pub const SEC_LOCK_FIELD_W: Pixels = px(150.);
    /// secrets (2.12b): a request card's command box stops here and scrolls.
    pub const SREQ_CMD_MAX_H: Pixels = px(140.);
    /// media (2.7, 2.24): the faded look (attach while uploading; the Slint o-soft).
    pub const MEDIA_O_SOFT: f32 = 0.6;
    /// media (2.24): the recording dot's off half of a blink (the Slint o-dim).
    pub const MEDIA_O_DIM: f32 = 0.4;
    // The composer (2.5-2.11; colours named here by the composer, for the
    // Lead to review, shared/requests.md): the queue drawers' fills and
    // edges (slate at 9% / 45%, accent at 10%), Plan mode's sky edge.
    pub fn composer_slate_haze() -> Hsla { rgba(0x94a3b817).into() }
    pub fn composer_slate_edge() -> Hsla { rgba(0x94a3b873).into() }
    pub fn composer_accent_haze() -> Hsla { rgba(0x6366f11a).into() }
    pub fn composer_sky_edge() -> Hsla { rgba(0x38bdf866).into() }
    // ── end planning ──
    // ── operations ──
    /// A parked pane's card, at most (the web's .pk-card 330; 2.23).
    pub const PARK_CARD_W: Pixels = px(330.);
    // ── end operations ──
    // ── desktop ──
    /// An annotation stroke on a page.
    pub const ANNOTATE_STROKE: Pixels = px(3.);
    /// The marks' colours as bytes, for pictures drawn off screen (a page's
    /// screenshot with its marks): the person's (annotate) and a chat's (accent).
    pub const ANNOTATE_RGB: [u8; 3] = [0xea, 0x58, 0x0c];
    pub const ACCENT_RGB: [u8; 3] = [0x63, 0x66, 0xf1];
    /// A mark's numbered disc: its radius and width.
    pub const MARK_R: Pixels = px(11.);
    pub const MARK_D: Pixels = px(22.);
    /// An arrow mark's head: each stroke's angle from the shaft (radians).
    pub const ARROW_SPREAD: f32 = 0.45;
    /// Where the address suggestions sit: under the two bars, between the
    /// back/forward/reload buttons and the Network..DevTools ones.
    pub const DRAWER_BARS_H: Pixels = px(76.);
    pub const DRAWER_NAV_W: Pixels = px(96.);
    pub const DRAWER_TOOLS_W: Pixels = px(136.);
    /// The chat's cursor on a page.
    pub const PAGE_CURSOR: Pixels = px(18.);
    /// A drawer tab's widest, the address suggestions' and a popover's width.
    pub const DRAWER_TAB_MAX: Pixels = px(200.);
    pub const DRAWER_POP_W: Pixels = px(240.);
    pub const DRAWER_NOTE_W: Pixels = px(240.);
    /// A hidden browser's screenshot on the Browsers tab, at its tallest.
    pub const SHOT_H: Pixels = px(150.);
    /// The Inbox: the threads' width, a message's widest, a picture's tallest,
    /// and how strong a thread's colour is behind its glyph.
    pub const INBOX_LIST_W: Pixels = px(360.);
    pub const INBOX_MSG_W: Pixels = px(760.);
    pub const INBOX_PIC_H: Pixels = px(240.);
    pub const INBOX_TINT: f32 = 0.3;
    /// Cru's screen: its width and tallest, the field and the head in it,
    /// a bubble's widest, the chat view's height, and its place from the right.
    pub const CRU_W: Pixels = px(420.);
    pub const CRU_H: Pixels = px(760.);
    pub const CRU_FIELD: Pixels = px(120.);
    pub const CRU_HEAD: Pixels = px(60.);
    pub const CRU_TURN_W: Pixels = px(336.);
    pub const CRU_CHAT_H: Pixels = px(260.);
    pub const CRU_RIGHT: Pixels = px(192.);
    // ── end desktop ──
}

/// The bundled fonts (DejaVu, as the Slint client), so the look is the same
/// on every machine (a Linux without desktop fonts has no ".SystemUIFont").
const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/DejaVuSans.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-Oblique.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-BoldOblique.ttf"),
    include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
    include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf"),
    // Emoji and symbols in content (a chat, a file): DejaVu has none, and a
    // Linux may have no emoji font, so it shows boxes. Noto Emoji (OFL,
    // monochrome) is the fallback. The no-emoji rule is for our own UI only.
    include_bytes!("../assets/fonts/NotoEmoji.ttf"),
];

/// The emoji and symbol fallback font's family.
pub const EMOJI_FONT: &str = "Noto Emoji";

/// The UI font with its fallbacks (emoji, symbols), for the window's root:
/// everything under it inherits the fallbacks, whatever family it sets.
pub fn ui_font() -> gpui::Font {
    gpui::Font {
        family: T::UI_FONT.into(),
        features: Default::default(),
        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec![EMOJI_FONT.to_string()])),
        weight: Default::default(),
        style: Default::default(),
    }
}

/// gpui-component's theme from the tokens: its colours are rebuilt from a
/// theme definition, so editing them directly does nothing.
pub fn theme_config(mode: &str) -> serde_json::Value {
    let hex = |c: Hsla| {
        let r = c.to_rgb();
        format!("#{:02x}{:02x}{:02x}{:02x}", (r.r * 255.) as u8, (r.g * 255.) as u8, (r.b * 255.) as u8, (r.a * 255.) as u8)
    };
    serde_json::json!({
        "name": "Crundi", "mode": mode,
        "font.family": T::UI_FONT, "font.size": 13.0,
        "mono_font.family": T::MONO, "mono_font.size": 12.0,
        "radius": 6,
        "colors": {
            "background": hex(T::bg()), "foreground": hex(T::text()), "border": hex(T::border()),
            "input.border": hex(T::border()), "ring": hex(T::accent()), "caret": hex(T::text()),
            "primary.background": hex(T::accent()), "primary.foreground": hex(T::white()), "primary.hover.background": hex(T::accent_hover()),
            "button.primary.background": hex(T::accent()), "button.primary.foreground": hex(T::white()), "button.primary.hover.background": hex(T::accent_hover()),
            "secondary.background": hex(T::bg_tertiary()), "secondary.foreground": hex(T::text()), "secondary.hover.background": hex(T::bg_hover()),
            "danger.background": hex(T::red()), "danger.foreground": hex(T::white()),
            "muted.background": hex(T::bg_card()), "muted.foreground": hex(T::text_muted()),
            "accent.background": hex(T::bg_hover()), "accent.foreground": hex(T::text()),
            "popover.background": hex(T::bg_secondary()), "popover.foreground": hex(T::text()),
            "list.active.background": hex(T::accent_dim()), "list.hover.background": hex(T::bg_hover()),
            "selection.background": hex(T::selection()),
            "scrollbar.thumb.background": hex(T::border())
        }
    })
}

/// Load the fonts and register the theme (light and dark the same: the PC
/// client is dark). Call once, after gpui_component::init.
pub fn init(cx: &mut App) {
    let _ = cx.text_system().add_fonts(FONTS.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect());
    let dark = serde_json::from_value::<gpui_component::ThemeConfig>(theme_config("dark"));
    let light = serde_json::from_value::<gpui_component::ThemeConfig>(theme_config("light"));
    let (Ok(dark), Ok(light)) = (dark, light) else { eprintln!("[gpui] the theme did not parse"); return };
    let (dark, light) = (std::rc::Rc::new(dark), std::rc::Rc::new(light));
    let t = gpui_component::Theme::global_mut(cx);
    t.dark_theme = dark.clone();
    t.light_theme = light;
    t.apply_config(&dark);
    gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_theme_parses_in_both_modes() {
        for m in ["dark", "light"] {
            assert!(serde_json::from_value::<gpui_component::ThemeConfig>(super::theme_config(m)).is_ok(), "{m}");
        }
    }
}
