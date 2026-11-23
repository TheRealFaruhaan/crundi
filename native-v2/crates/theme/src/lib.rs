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

pub use gpui_kit_assets::IconName;

/// The client's asset source: the icon catalog, and the two fonts gpui's SVG
/// renderer asks for by path (IBM Plex Sans and Lilex, OFL, as Zed ships
/// them). They are compiled in, so every package (the .deb, the Windows
/// installer) carries them and an SVG's text never logs "could not find asset".
pub struct AllAssets;

/// The fonts gpui's SVG renderer loads through the asset source.
pub const SVG_FONTS: &[(&str, &[u8])] = &[
    ("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf", include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf")),
    ("fonts/lilex/Lilex-Regular.ttf", include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf")),
];

impl gpui::AssetSource for AllAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some((_, data)) = SVG_FONTS.iter().find(|(p, _)| *p == path) {
            return Ok(Some(std::borrow::Cow::Borrowed(*data)));
        }
        gpui_kit_assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        let mut all = gpui_kit_assets::AllAssets.list(path)?;
        all.extend(SVG_FONTS.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| (*p).into()));
        Ok(all)
    }
}

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
    // Kid mode's palette (desktop's; crundi-voice.js .cv-ov.kid): the sky
    // to the sun, the deep-indigo type, the white bubbles, the button indigo,
    // the "Wait for me!" amber, what the child said.
    pub fn kid_sky() -> Hsla { rgb(0xbae6fd).into() }
    pub fn kid_sun() -> Hsla { rgb(0xfef9c3).into() }
    pub fn kid_text() -> Hsla { rgb(0x312e81).into() }
    pub fn kid_button() -> Hsla { rgb(0x4f46e5).into() }
    /// The kid screen's buttons' flat shadow (rgba(49,46,129,.18)), and mute while on (#fee2e2 / #b91c1c).
    pub fn kid_btn_shadow() -> Hsla { rgba(0x312e812e).into() }
    pub fn kid_mute_on() -> Hsla { rgb(0xfee2e2).into() }
    pub fn kid_mute_on_fg() -> Hsla { rgb(0xb91c1c).into() }
    pub fn kid_wait() -> Hsla { rgb(0xb45309).into() }
    pub fn kid_heard_bg() -> Hsla { rgb(0xdbeafe).into() }
    pub fn kid_heard() -> Hsla { rgb(0x1e3a8a).into() }
    pub fn kid_break() -> Hsla { rgba(0x0a0a0feb).into() }
    pub fn kid_break_text() -> Hsla { rgb(0xfde68a).into() }
    /// KID_PALETTE's first colours (crundi-voice.js:64): the mic's top layer
    /// while listening, thinking, speaking and waking.
    pub fn kid_listen() -> Hsla { rgb(0xfacc15).into() }
    pub fn kid_think() -> Hsla { rgb(0xa3e635).into() }
    pub fn kid_speak() -> Hsla { rgb(0xf472b6).into() }
    pub fn kid_start() -> Hsla { rgb(0x22d3ee).into() }
    /// What went wrong or what to know, on the kid screen (.cv-ov.kid .cv-note).
    pub fn kid_note() -> Hsla { rgb(0xb45309).into() }
    /// The kid typing box's edge (.cv-ov.kid.kb .cv-type: 3px solid #a5b4fc).
    pub fn kid_type_border() -> Hsla { rgb(0xa5b4fc).into() }
    /// "Talk to Cru" over a video (.cv-talk: #f59e0b, #111 text, 0 6px 0 rgba(0,0,0,.3)).
    pub fn kid_talk() -> Hsla { rgb(0xf59e0b).into() }
    pub fn kid_talk_text() -> Hsla { rgb(0x111111).into() }
    pub fn kid_talk_drop() -> Hsla { rgba(0x0000004d).into() }
    /// An attachment chip's error edge (.cv-chip.bad) and a sent message's
    /// attachment tag (.cv-m .att: rgba(255,255,255,.08)).
    pub fn cru_att_bad() -> Hsla { rgb(0xf87171).into() }
    pub fn cru_att_tag() -> Hsla { rgba(0xffffff14).into() }
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
    /// The top bar's usage labels only (the web's .ublabel, 0.56rem).
    pub const TEXT_9: Pixels = px(9.);
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
    /// The browser's `line-height: normal` (the toast's lines; F123).
    pub const LH_NORMAL: f32 = 1.2;

    // ─── Letter spacing: tight and wide (em) ───
    pub const TRACK_TIGHT: f32 = -0.01;
    pub const TRACK_WIDE: f32 = 0.05;

    // ─── Named sizes, only for structure ───
    /// The test harness's window (every screen in screens/ is taken at it).
    pub const WINDOW_W: Pixels = px(1400.);
    pub const WINDOW_H: Pixels = px(860.);
    /// The app's window at launch and its least size: Electron's (app/main.js createWindow).
    pub const APP_W: Pixels = px(1100.);
    pub const APP_H: Pixels = px(750.);
    pub const WINDOW_MIN_W: Pixels = px(800.);
    pub const WINDOW_MIN_H: Pixels = px(500.);
    pub const TOPBAR_H: Pixels = px(48.);
    pub const TABBAR_H: Pixels = px(36.);
    pub const SIDEBAR_W: Pixels = px(240.);
    pub const SIDEBAR_FOLDED: Pixels = px(60.);
    pub const PANE_HEAD: Pixels = px(28.);
    pub const PANE_HEAD_NARROW: Pixels = px(26.);
    pub const PANE_MIN: Pixels = px(340.);
    pub const PANE_FOLD: Pixels = px(560.);
    pub const GUTTER: Pixels = px(7.);
    /// An empty pane (the web's .mosaic-empty): its inset, its chips' row
    /// width, a chip's padding (5 / 10) and 11.5px text, an action's
    /// sideways padding (9), and both their 13px icons.
    pub const LEAF_INSET: Pixels = px(3.);
    pub const LEAF_CHIPS_W: Pixels = px(300.);
    pub const LEAF_CHIP_PY: Pixels = px(5.);
    pub const LEAF_CHIP_GAP: Pixels = px(5.);
    pub const LEAF_CHIP_TEXT: Pixels = px(11.5);
    pub const LEAF_ACT_PX: Pixels = px(9.);
    pub const LEAF_ICON: Pixels = px(13.);
    /// A new pane's launcher (the web's .term-launch): its ">_" (2.4rem),
    /// buttons 9 px tall padding, Empty Shell at least 190 wide, the rows
    /// 300 wide, a shield 44 (172 open), a label 8 from its icon (UI Mode's
    /// caret 9), the Claude box's padding 16 / 20.
    pub const LAUNCH_MARK: Pixels = px(38.4);
    pub const LAUNCH_PY: Pixels = px(9.);
    pub const LAUNCH_MIN_W: Pixels = px(190.);
    pub const LAUNCH_ROW_W: Pixels = px(300.);
    pub const LAUNCH_SHIELD: Pixels = px(44.);
    pub const LAUNCH_SHIELD_OPEN: Pixels = px(172.);
    pub const LAUNCH_GAP: Pixels = px(8.);
    pub const LAUNCH_CHAT_GAP: Pixels = px(9.);
    pub const LAUNCH_BOX_PY: Pixels = px(16.);
    pub const LAUNCH_BOX_PX: Pixels = px(20.);
    pub const LAUNCH_LABEL: Pixels = px(11.52);
    pub fn launch_chat_edge() -> Hsla { rgba(0x818cf8d9).into() }
    /// UI Mode's gradient (the web's .btn-chat): its edge, middle and peak
    /// stops (#4338ca, #6366f1, #a5b4fc; the image is edge, middle, peak,
    /// middle, edge).
    pub fn launch_chat_stops() -> [Hsla; 3] { [rgb(0x4338ca).into(), rgb(0x6366f1).into(), rgb(0xa5b4fc).into()] }
    pub fn launch_normal_bg() -> Hsla { rgba(0x6366f11a).into() }
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
    /// A dragged row's ghost (the web's drag-clone opacity).
    pub const DRAG_GHOST_O: f32 = 0.92;
    /// The toast (the web's .toast: at most 420 px, 0.85rem).
    pub const TOAST_MAX_W: Pixels = px(420.);
    pub const TOAST_TEXT: Pixels = Self::TEXT_14;
    pub const CONTROL_SM: Pixels = px(28.);
    pub const CONTROL: Pixels = px(34.);
    pub const CONTROL_LG: Pixels = px(36.);
    /// The Schedule dialog's weekday button (the web's .sm-day, 34x32).
    pub const DAY_W: Pixels = px(34.);
    pub const DAY_H: Pixels = Self::SP_32;
    pub const ICON_SM: Pixels = px(12.);
    pub const ICON_MD: Pixels = px(14.);
    pub const ICON_LG: Pixels = px(16.);
    pub const ICON_XL: Pixels = px(20.);
    pub const DOT: Pixels = px(8.);
    pub const MENU_MIN_W: Pixels = px(190.);

    // ─── Controls: every control's dimensions (the owner, 16 Nov: "buttons
    // and such dimensions on the theme"). The widgets crate sets each control
    // from these, overriding gpui-component's own presets; no part sizes a
    // control. ───
    /// A button (text, icon and text, a dropdown, an armed button): its
    /// height, side padding, the gap between icon and label, the icon, the
    /// label's size and weight, the radius (the web's .svc-btn).
    pub const BTN_H: Pixels = Self::CONTROL_SM;
    pub const BTN_PAD_X: Pixels = Self::SP_10;
    pub const BTN_GAP: Pixels = Self::SP_6;
    pub const BTN_ICON: Pixels = Self::ICON_MD;
    /// A button's label: the theme's base size (gpui-component draws it at
    /// 1 rem, and the registered theme's font size is this token).
    pub const BTN_TEXT: Pixels = Self::TEXT_13;
    pub const BTN_WEIGHT: FontWeight = Self::W_MEDIUM;
    pub const BTN_RADIUS: Pixels = Self::R_CONTROL;
    /// The big button (Send, a form's main action): the control height.
    pub const BTN_H_LG: Pixels = Self::CONTROL;
    pub const BTN_PAD_X_LG: Pixels = Self::SP_12;
    pub const BTN_WEIGHT_LG: FontWeight = Self::W_SEMIBOLD;
    /// An icon-only button: a square of BTN_H, its icon.
    pub const ICON_BTN: Pixels = Self::BTN_H;
    pub const ICON_BTN_ICON: Pixels = Self::ICON_LG;
    /// A text box: its height, side padding and text size.
    pub const INPUT_H: Pixels = Self::CONTROL;
    pub const INPUT_PAD_X: Pixels = Self::SP_10;
    pub const INPUT_TEXT: Pixels = Self::TEXT_13;
    /// A switch (the web's .toggle): its track, the thumb, the inset between them (F69).
    pub const TOGGLE_W: Pixels = px(36.);
    pub const TOGGLE_H: Pixels = Self::SP_20;
    pub const TOGGLE_THUMB: Pixels = Self::ICON_MD;
    pub const TOGGLE_INSET: Pixels = Self::SP_2;
    /// A checkbox's box and the gap to its label (F69).
    pub const CHECK_BOX: Pixels = Self::ICON_MD;
    pub const CHECK_GAP: Pixels = Self::SP_8;
    pub const CHECK_TEXT: Pixels = Self::TEXT_13;
    /// The loading spinner (F69).
    pub const SPINNER: Pixels = Self::ICON_LG;
    /// The most frames a second a decorative animation (a spinner, a live
    /// label's pulse) asks for. Each of its frames draws its pane again; at
    /// the display's rate a running tool's spinner kept the window busy
    /// (the owner's PC, 19 Nov: ~13 fps while any chat worked).
    pub const ANIM_FPS: f32 = 24.0;

    // ─── Type ───
    pub const UI_FONT: &'static str = "DejaVu Sans";
    pub const MONO: &'static str = "DejaVu Sans Mono";

    // ─── The areas' named sizes (AREAS.md): each only in its own section ───
    // ── workspace ──
    /// An empty pane's chip: its icon and padding, and a character of its label.
    pub const WS_CHIP_BASE: Pixels = px(42.);
    pub const WS_CHIP_CHAR: f32 = 6.6;
    /// The diff window (feOpenDiff): its size, and the side-by-side's colours.
    pub const WS_DIFF_W: Pixels = px(1100.);
    pub const WS_DIFF_H: Pixels = px(620.);
    /// Media's cards (.media-card), their thumbnail, the search box.
    pub const WS_MEDIA_CARD: Pixels = px(150.);
    pub const WS_MEDIA_THUMB: Pixels = px(110.);
    /// A card's kind badge: the scrim at this.
    pub const WS_MEDIA_BADGE_O: f32 = 0.63;
    /// The lightbox's audio player's height.
    pub const WS_ML_AUDIO_H: Pixels = px(64.);
    pub const WS_MEDIA_SEARCH: Pixels = px(220.);
    /// A file row that is cut (waiting to be pasted).
    pub const WS_O_CUT: f32 = 0.45;
    /// The path bar's "/" between folders.
    pub const WS_CRUMB_SEP_O: f32 = 0.6;
    /// A diff row's line-number gutter.
    pub const WS_DIFF_LN: Pixels = px(44.);
    pub fn ws_diff_del() -> Hsla { rgb(0x2d1215).into() }
    pub fn ws_diff_add() -> Hsla { rgb(0x0f291a).into() }
    pub fn ws_diff_del_pad() -> Hsla { rgb(0x1a1215).into() }
    pub fn ws_diff_add_pad() -> Hsla { rgb(0x0f1a15).into() }
    pub fn ws_diff_del_text() -> Hsla { rgb(0xf87171).into() }
    pub fn ws_diff_add_text() -> Hsla { rgb(0x34d399).into() }
    pub fn ws_diff_chunk() -> Hsla { rgb(0x1e293b).into() }
    /// A replaced line's changed words (the web's wordDiffHtml backgrounds).
    pub fn ws_diff_del_word() -> Hsla { rgb(0x5c2020).into() }
    pub fn ws_diff_add_word() -> Hsla { rgb(0x1a4a2a).into() }
    /// The Claude terminal panels (3.2): the palette's colours beyond the
    /// theme's own (the web's TERM_THEME), the selection, a cell's line
    /// height (of its font size), the input box at its tallest, the cursor's
    /// blink (each half), and how many lines a wheel notch scrolls.
    pub fn ws_term_magenta() -> Hsla { rgb(0xa855f7).into() }
    pub fn ws_term_magenta_hi() -> Hsla { rgb(0xc084fc).into() }
    pub fn ws_term_cyan() -> Hsla { rgb(0x06b6d4).into() }
    pub fn ws_term_cyan_hi() -> Hsla { rgb(0x22d3ee).into() }
    pub fn ws_term_sel() -> Hsla { rgba(0x6366f14d).into() }
    pub const WS_TERM_LH: f32 = 1.2;
    pub const WS_TERM_INPUT_MAX: Pixels = px(120.);
    pub const WS_TERM_BLINK_MS: u64 = 500;
    pub const WS_TERM_WHEEL: i32 = 3;
    /// The key tools' buttons (.term-tool-btn min-width), and the d-pad's columns.
    pub const WS_TERM_TOOL_W: Pixels = px(40.);
    pub const WS_TERM_NAV_W: Pixels = px(52.);
    /// Dim (SGR 2) text: its colour at this opacity.
    pub const WS_TERM_DIM: f32 = 0.5;
    /// The media lightbox (.media-lightbox): its scrim, its buttons' fill
    /// (rest, hovered: white at these), the picture's share of the window.
    pub fn ws_ml_scrim() -> Hsla { rgba(0x000000db).into() }
    pub const WS_ML_BTN_O: f32 = 0.12;
    pub const WS_ML_BTN_HOVER_O: f32 = 0.25;
    pub const WS_ML_W: f32 = 0.9;
    pub const WS_ML_H: f32 = 0.82;
    /// The file windows (.fe-window): the top they keep under (the top bar),
    /// their least size, the snap zone, the resize handles (edge, corner),
    /// the Arrange bar's top, how long it lingers after a drag (ms), and
    /// after the pointer leaves it.
    pub const WS_WIN_TOP: f32 = 48.0;
    pub const WS_WIN_MIN_W: f32 = 360.0;
    pub const WS_WIN_MIN_H: f32 = 200.0;
    pub const WS_WIN_ZONE: f32 = 36.0;
    pub const WS_WIN_EDGE: Pixels = px(8.);
    pub const WS_WIN_CORNER: Pixels = px(16.);
    pub const WS_WIN_ARRANGE_TOP: Pixels = px(54.);
    pub const WS_WIN_ARRANGE_MS: u64 = 3500;
    pub const WS_WIN_ARRANGE_LEAVE_MS: u64 = 1200;
    /// The file list's marks (fileIcon): each kind's colour.
    pub fn ws_ft(kind: &str) -> Hsla {
        rgb(match kind {
            "docker" => 0x2496ed, "git" => 0xf1502f, "npm" => 0xcb3837, "env" | "lock" => 0xecd53f,
            "js" => 0xf7df1e, "ts" => 0x3178c6, "jsx" => 0x61dafb, "json" => 0xa8a8a2, "html" => 0xe34c26,
            "css" => 0x264de4, "scss" => 0xcd6799, "py" => 0x3776ab, "go" => 0x00add8, "rs" => 0xdea584,
            "java" => 0xf89820, "c" => 0x555555, "cpp" => 0x659ad2, "sh" => 0x4eaa25, "ps1" => 0x012456,
            "sql" => 0xe38c00, "md" => 0x519aba, "yaml" => 0xcb171e, "xml" => 0xe37933, "svg" => 0xffb13b,
            "pdf" => 0xe5252a, "vue" => 0x42b883, "svelte" => 0xff3e00,
            _ => 0x6b7280,
        }).into()
    }
    /// The Browser button's running dot (.brd-running::after): 7 px, a 2 px
    /// ring of the bar's colour round it (drawn as its border: the box is
    /// 11), 1 px after the label (the ring's 2 px reach back over it).
    pub const WS_BRD_DOT: Pixels = px(7.);
    pub const WS_BRD_DOT_RING: Pixels = px(2.);
    pub const WS_BRD_DOT_GAP: Pixels = px(1.);
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
    // The Mind map (4.3; the Slint client's mindmap-* and its o-* opacities).
    /// The map's columns (one per depth) and an idea's width.
    pub const MINDMAP_COL: Pixels = px(235.);
    pub const MINDMAP_NODE: Pixels = px(180.);
    /// The canvas's dot grid (the web's background-size).
    pub const MINDMAP_DOTS: Pixels = px(22.);
    /// How far from the canvas's edge an idea the keyboard goes to is kept.
    pub const MINDMAP_SCROLL_MARGIN: Pixels = px(70.);
    /// The toolbar folds to its narrow look below this width.
    pub const MINDMAP_FOLD: Pixels = px(480.);
    /// The idea dialog and the link picker; the picker's list at its tallest.
    pub const MINDMAP_DIALOG_W: Pixels = px(520.);
    pub const MINDMAP_LIST_MAX_H: Pixels = px(320.);
    /// The shortcut sheet: its keys column, its little trees' grid and boxes.
    pub const MINDMAP_HELP_KEYS: Pixels = px(176.);
    pub const MINDMAP_DIA_COL: Pixels = px(92.);
    pub const MINDMAP_DIA_ROW: Pixels = px(30.);
    pub const MINDMAP_DIA_BOX_W: Pixels = px(74.);
    pub const MINDMAP_DIA_BOX_H: Pixels = px(20.);
    /// Faded (held in a drag, dimmed by the filter), half seen (an "add
    /// idea" box), a project chip turned off, the idea in hand.
    pub const MINDMAP_O_FAINT: f32 = 0.3;
    pub const MINDMAP_O_DIM: f32 = 0.4;
    pub const MINDMAP_O_OFF: f32 = 0.7;
    pub const MINDMAP_O_CLONE: f32 = 0.92;
    /// schedule (4.5): a switched-off schedule's faded row (the web's .sched-item.off).
    pub const SCHED_O_OFF: f32 = 0.55;
    /// The Media modal (#media-modal .im-box: max-width 780px).
    pub const MEDIA_MODAL_W: Pixels = px(780.);
    // Kanban (4.1): a column's narrowest and widest (.kanban-col), the panel
    // width under which columns stack (@container 700px), the width under
    // which the toolbar folds (480), a card under which its action labels
    // fold (220; 320 compact), the attachments strip's bounds and thumbs,
    // the dragged card's look (o-dim), its clone (o-clone), Trash's faded
    // cards (o-soft), a subtask's resting "…" (o-caret), the slate edge,
    // a footer button's hovered edge.
    pub const KANBAN_COL_MIN: Pixels = px(220.);
    pub const KANBAN_COL_MAX: Pixels = px(360.);
    pub const KANBAN_STACK: Pixels = px(700.);
    pub const KANBAN_FOLD: Pixels = px(480.);
    pub const KANBAN_CARD_FOLD: Pixels = px(220.);
    pub const KANBAN_CARD_FOLD_COMPACT: Pixels = px(320.);
    pub const KANBAN_POP_MIN: Pixels = px(220.);
    pub const KANBAN_POP_MAX: Pixels = px(360.);
    pub const KANBAN_THUMB: Pixels = px(48.);
    pub const KANBAN_O_DIM: f32 = 0.4;
    pub const KANBAN_O_CLONE: f32 = 0.92;
    pub const KANBAN_O_SOFT: f32 = 0.6;
    pub const KANBAN_O_CARET: f32 = 0.55;
    pub fn kanban_slate_edge() -> Hsla { rgba(0x94a3b873).into() }
    /// A card's footer button hovered (.kanban-card .card-foot button:hover's edge).
    pub fn kanban_btn_edge() -> Hsla { rgb(0x3c3c58).into() }
    // Notes (4.4): the tab's page list (.nt-side), the width under which the
    // list goes over the page and the page's margins shrink, the page's
    // widest, the room under the last block, the picker's widest, a menu's
    // width and tallest, the link box; the list's share of the height when
    // stacked; a busy page in a list (o-soft), a dragged block's label (o-clone).
    pub const NOTES_SIDE_W: Pixels = px(280.);
    pub const NOTES_STACK: Pixels = px(700.);
    pub const NOTES_PAGE_MAX: Pixels = px(680.);
    pub const NOTES_PAGE_TAIL: Pixels = px(160.);
    pub const NOTES_PICK_MAX: Pixels = px(520.);
    pub const NOTES_MENU_W: Pixels = px(220.);
    pub const NOTES_MENU_MAX_H: Pixels = px(320.);
    pub const NOTES_LINK_W: Pixels = px(220.);
    pub const NOTES_SIDE_SHARE: f32 = 0.4;
    pub const NOTES_O_SOFT: f32 = 0.6;
    pub const NOTES_O_CLONE: f32 = 0.92;
    // factories (4.7, 4.8): a Library card at least (280), the editor's
    // palette (190) and the side (340), the dialogs (560, wide 680), the
    // PIN card (340), the transcript sheet (560), the width under which the
    // graph's bar keeps only icons (560) and a tab goes narrow (the web's
    // 640), the minimap (200 x 150), the Cleanup days box (80), the width
    // under which a dialog's pairs stack (520), an attachment chip (220);
    // the canvas's share of a narrow tab (52%), the panel's Details drawer at
    // most (92%); the faded looks (0.6, 0.4, 0.3); and the graph's rings and
    // edges (the Slint theme's green-ring, accent-ring, yellow-edge, slate-edge).
    pub const FACTORIES_CARD_MIN: Pixels = px(280.);
    pub const FACTORIES_LEFT: Pixels = px(190.);
    pub const FACTORIES_RIGHT: Pixels = px(340.);
    pub const FACTORIES_MODAL_W: Pixels = px(560.);
    pub const FACTORIES_MODAL_WIDE: Pixels = px(680.);
    pub const FACTORIES_PIN_W: Pixels = px(340.);
    pub const FACTORIES_SHEET_W: Pixels = px(560.);
    pub const FACTORIES_COMPACT: Pixels = px(560.);
    pub const FACTORIES_NARROW: Pixels = px(640.);
    pub const FACTORIES_MINI_W: Pixels = px(200.);
    pub const FACTORIES_MINI_H: Pixels = px(150.);
    pub const FACTORIES_DAYS_W: Pixels = px(80.);
    pub const FACTORIES_PAIR_STACK: Pixels = px(520.);
    pub const FACTORIES_CHIP_MAX: Pixels = px(220.);
    pub const FACTORIES_CANVAS_SHARE: f32 = 0.52;
    pub const FACTORIES_DRAWER_SHARE: f32 = 0.92;
    pub const FACTORIES_O_SOFT: f32 = 0.6;
    pub const FACTORIES_O_DIM: f32 = 0.4;
    pub const FACTORIES_O_FAINT: f32 = 0.3;
    pub fn factories_green_ring() -> Hsla { rgba(0x10b98134).into() }
    pub fn factories_accent_ring() -> Hsla { rgba(0x6366f147).into() }
    pub fn factories_yellow_edge() -> Hsla { rgba(0xf59e0b59).into() }
    pub fn factories_slate_edge() -> Hsla { rgba(0x94a3b873).into() }
    // ── end planning ──
    // ── operations ──
    /// A parked pane's card, at most (the web's .pk-card 330; 2.23).
    pub const PARK_CARD_W: Pixels = px(330.);
    /// homepool: its dialogs (.hp-dlg 560) and the node code's QR (180).
    pub const HOMEPOOL_DLG_W: Pixels = px(560.);
    pub const HOMEPOOL_QR: Pixels = px(180.);
    /// homepool: a decided request's card (the Slint o-soft).
    pub const HOMEPOOL_O_SOFT: f32 = 0.6;
    /// info: the server log's box (#server-logs-area max-height 300).
    pub const INFO_LOGS_H: Pixels = px(300.);
    /// info: a row's value takes at most this share of the row (.info-row).
    pub const INFO_VALUE_SHARE: f32 = 0.6;
    /// info: a person's token meter, at most (.collab-meter 160).
    pub const INFO_METER_W: Pixels = px(160.);
    /// info: an expired grant's row (the Slint o-soft).
    pub const INFO_O_SOFT: f32 = 0.6;
    /// info: the yellow edge of a warning note (.collab-sb.warn).
    pub fn info_yellow_edge() -> Hsla { rgba(0xf59e0b59).into() }
    /// services / docker: a chart's line (the widgets' spark; the Slint spin-stroke).
    pub const CHART_STROKE: Pixels = px(1.5);
    /// services: the inline logs' box, at its tallest (.svc-logs max-height).
    pub const SERVICES_LOGS_H: Pixels = px(200.);
    /// services: the usage strip's sparkline box (.stat-spark) and a value's width.
    pub const SERVICES_SPARK_H: Pixels = px(20.);
    pub const SERVICES_VALUE_W: Pixels = px(64.);
    /// docker: the rows' CPU and MEM columns, the filter, the pull field,
    /// the log filter, a key's column, a chart's height (.dk-*).
    pub const DOCKER_NUM_W: Pixels = px(64.);
    pub const DOCKER_MEM_W: Pixels = px(76.);
    pub const DOCKER_FILTER_W: Pixels = px(180.);
    pub const DOCKER_PULL_W: Pixels = px(300.);
    pub const DOCKER_LOG_FILTER_W: Pixels = px(280.);
    pub const DOCKER_KV_W: Pixels = px(160.);
    pub const DOCKER_CHART_H: Pixels = px(84.);
    /// docker: a bay-plan square (.dk-bay i 14).
    pub const DOCKER_BAY: Pixels = px(14.);
    /// docker: the job drawer's share of the tab's height.
    pub const DOCKER_JOB_SHARE: f32 = 0.46;
    /// docker: the log's ANSI colours the palette lacks, and a chart's fill.
    pub fn docker_red() -> Hsla { rgb(0xf87171).into() }
    pub fn docker_magenta() -> Hsla { rgb(0xc084fc).into() }
    pub fn docker_cyan() -> Hsla { rgb(0x22d3ee).into() }
    pub const DOCKER_FILL: f32 = 0.13;
    /// panels: the tab's list (.wg-tab-list), a row, the slot select, a
    /// frame's least / first / inline-most height, a chip's popover, a chip's
    /// text and face at most, the checks' shot height, a greyed row.
    pub const PANELS_LIST_W: Pixels = px(232.);
    pub const PANELS_ITEM_H: Pixels = px(40.);
    pub const PANELS_SELECT_W: Pixels = px(132.);
    pub const PANELS_FRAME_MIN: Pixels = px(36.);
    pub const PANELS_FRAME_START: Pixels = px(80.);
    pub const PANELS_INLINE_MAX: Pixels = px(460.);
    pub const PANELS_POP_W: Pixels = px(440.);
    pub const PANELS_POP_H: Pixels = px(560.);
    pub const PANELS_CHIP_MAX: Pixels = px(220.);
    pub const PANELS_SHOT_H: Pixels = px(150.);
    pub const PANELS_O_SOFT: f32 = 0.6;
    /// extras: the approvals panel (.appr-panel 420, its share of the window,
    /// a detail's box 140), Add project (400), What's new (the copy's least
    /// width and share, the stage 640x400, the frame at most, the title, a key column).
    pub const EXTRAS_APPR_W: Pixels = px(420.);
    pub const EXTRAS_APPR_MAX_SHARE: f32 = 0.7;
    pub const EXTRAS_APPR_DETAIL_H: Pixels = px(140.);
    pub const EXTRAS_ADD_W: Pixels = px(400.);
    pub const EXTRAS_WN_COPY_MIN: Pixels = px(340.);
    pub const EXTRAS_WN_COPY_SHARE: f32 = 0.42;
    pub const EXTRAS_WN_STAGE_W: f32 = 640.;
    pub const EXTRAS_WN_STAGE_H: f32 = 400.;
    pub const EXTRAS_WN_FRAME_MAX: Pixels = px(760.);
    pub const EXTRAS_WN_TITLE: Pixels = px(40.);
    pub const EXTRAS_WN_KEY_W: Pixels = px(150.);
    /// What's new: the title's line height (the web's .wn-title, a tight 1.1).
    pub const EXTRAS_WN_TITLE_LH: f32 = 1.1;
    pub fn extras_wn_haze() -> Hsla { rgba(0x6366f11f).into() }
    /// settings: Cru's selects and fields, the Telegram chat id, a matrix column, the idle-minutes box, the pairing picture.
    pub const SETTINGS_SELECT_W: Pixels = px(340.);
    pub const SETTINGS_CHAT_W: Pixels = px(260.);
    pub const SETTINGS_NTF_COL: Pixels = px(62.);
    pub const SETTINGS_NUM_W: Pixels = px(110.);
    pub const SETTINGS_PAIR_PIC: Pixels = px(300.);
    /// settings: a locked switch, the classes while "on its own" is off (the Slint o-dim).
    pub const SETTINGS_O_DIM: f32 = 0.5;
    /// skills: the backup status's key column, the sign-in restore's card, the editor's rows' height.
    pub const SKILLS_KEY_W: Pixels = px(92.);
    pub const SKILLS_RESTORE_W: Pixels = px(340.);
    /// vault: the PIN modal (.pin-box 320) and the lock dialogs (.csl-card 420).
    pub const VAULT_PIN_W: Pixels = px(320.);
    pub const VAULT_LOCK_W: Pixels = px(420.);
    /// vault: the PIN dialog's PIN (.pin-modal input: 1.4rem, mono, centred).
    pub const VAULT_PIN_TEXT: Pixels = px(22.4);
    /// vault: a request's separator (.secret-request rgba(245,158,11,0.25)).
    pub fn vault_yellow_edge() -> Hsla { rgba(0xf59e0b40).into() }
    // ── end operations ──
    // ── desktop ──
    /// Cru's mark (app/vendor/crundi-voice.js cruIcon): the stroke's gradient,
    /// sky to indigo to violet to pink, at these offsets; the dark layer
    /// behind it; its height in the top bar (its width is 150/90 of it).
    pub const CRU_MARK: [(f32, u32); 4] = [(0.0, 0x38bdf8), (0.4, 0x6366f1), (0.72, 0xa78bfa), (1.0, 0xe879f9)];
    pub const CRU_MARK_DARK: [(f32, u32); 3] = [(0.0, 0x0c4a6e), (0.45, 0x312e81), (1.0, 0x701a75)];
    pub const CRU_MARK_H: Pixels = px(24.);
    /// Crundi's mark (widgets::brand_mark): the head in the top bar (the web's 24),
    /// larger on the sign-in screen; the name 15/24 of the head's height, 8/24 apart.
    pub const BRAND_HEAD: Pixels = px(24.);
    /// Layout movement (panes moving or resizing, the sidebar folding): the web's 220 ms.
    pub const LAYOUT_EASE: std::time::Duration = std::time::Duration::from_millis(220);
    /// A project's icon in the sidebar row and the top bar (the web's 18).
    pub const PROJECT_ICON: Pixels = px(18.);
    /// A chat log's list builds this much beyond its view, above and below
    /// (no rows popping in while scrolling).
    pub const LOG_OVERDRAW: Pixels = px(800.);
    // Chat markdown (the web's `.cc-assistant` CSS): a list's indent (ul/ol
    // padding-left 20) and its marker's gap, a code block's padding (9 11)
    // and line height (1.5).
    pub const MD_LIST_INDENT: Pixels = px(20.);
    pub const MD_MARKER_GAP: Pixels = px(5.);
    pub const MD_CODE_PX: Pixels = px(11.);
    pub const MD_CODE_PY: Pixels = px(9.);
    pub const LH_CODE: f32 = 1.5;
    /// The browser's `hr` (a 1 px inset border, gray), as Chromium draws it
    /// on the web: its top line and its bottom line.
    pub fn md_rule_dark() -> Hsla { rgb(0x9a9a9a).into() }
    pub fn md_rule() -> Hsla { rgb(0xeeeeee).into() }
    /// Code colours on the dark background (the owner's call, 19 Nov: they
    /// stay, readable): from the palette, each at least 4.5:1 on bg_secondary.
    pub fn syn_keyword() -> Hsla { Self::accent_hover() }
    pub fn syn_string() -> Hsla { Self::green() }
    pub fn syn_number() -> Hsla { Self::amber() }
    pub fn syn_comment() -> Hsla { Self::text_secondary() }
    pub fn syn_type() -> Hsla { Self::error_text() }
    pub fn syn_function() -> Hsla { Self::sky() }
    pub const BRAND_HEAD_LG: Pixels = px(40.);
    pub const BRAND_WORD_OF_HEAD: f32 = 15.0 / 24.0;
    pub const BRAND_GAP_OF_HEAD: f32 = 8.0 / 24.0;
    /// A tooltip is never narrower than this: with less room right of the
    /// pointer it goes to the pointer's left instead.
    pub const TIP_MIN_W: Pixels = px(160.);
    /// Hovering Cru's mark opens it only on a window at least this wide.
    pub const CRU_HOVER_MIN_W: Pixels = px(900.);
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
    /// Cru's screen (crundi-voice.js .cv-ov on a wide window): 400 by 600
    /// under the top bar's mark, 580 by 760 with Cru's live screen up; the
    /// room it leaves under the window (100vh - 70); the field's least height.
    pub const CRU_W: Pixels = px(400.);
    pub const CRU_H: Pixels = px(600.);
    pub const CRU_W_SCR: Pixels = px(580.);
    pub const CRU_W_FULL: Pixels = px(1000.);
    pub const CRU_H_SCR: Pixels = px(760.);
    pub const CRU_ROOM: Pixels = px(70.);
    pub const CRU_FIELD_MIN: Pixels = px(230.);
    /// The field's share of the room against the words' (.cv-field flex 1.5, .cv-words flex 1).
    pub const CRU_FIELD_GROW: f32 = 1.5;
    /// The round buttons (.cv-x), the ⋯ menu's least width, a bubble's
    /// sharp corner (.cv-m border-bottom radius 5), the say-it button's room.
    pub const CRU_BTN: Pixels = px(34.);
    pub const CRU_BTN_ICON: Pixels = px(16.);
    pub const CRU_MENU_W: Pixels = px(220.);
    pub const CRU_BUBBLE_R: Pixels = px(16.);
    /// A card in a reply (.cv-card): 560px at most, 14px corners.
    pub const CRU_CARD_MAX_W: Pixels = px(560.);
    pub const CRU_CARD_R: Pixels = px(14.);
    pub const CRU_BUBBLE_TAIL: Pixels = px(5.);
    pub const CRU_SAY_ROOM: Pixels = px(40.);
    /// The × by his words (.cv-cx 26) and on a faded bubble (.cv-drop 28).
    pub const CRU_CX: Pixels = px(26.);
    pub const CRU_DROP_BTN: Pixels = px(28.);
    /// A bubble's widest share of the chat (.cv-m max-width 82%).
    pub const CRU_BUBBLE_SHARE: f32 = 0.82;
    /// The liquid mic (.cv-mic): its box (112, 64 in the chat view), the
    /// canvas around it (272 by 152), the sphere's radius.
    pub const CRU_MIC: Pixels = px(112.);
    pub const CRU_MIC_CHAT: Pixels = px(64.);
    pub const CRU_MIC_CANVAS_W: Pixels = px(272.);
    pub const CRU_MIC_CANVAS_H: Pixels = px(152.);
    pub const CRU_MIC_R: Pixels = px(30.);
    /// The field (drawField): the head's largest size unit (L), a cell's
    /// radius (idle, busy), the strands' and the drops' widths, the cells'
    /// ring from the field's edge, a cell's name under it.
    pub const CRU_HEAD_L: Pixels = px(38.);
    pub const CRU_CELL_R: Pixels = px(17.);
    pub const CRU_CELL_R_IDLE: Pixels = px(13.);
    pub const CRU_CELL_INSET_X: Pixels = px(46.);
    pub const CRU_CELL_INSET_Y: Pixels = px(34.);
    pub const CRU_STRAND: Pixels = px(4.);
    pub const CRU_DROP: Pixels = px(2.2);
    pub const CRU_CELL_NAME: Pixels = px(10.5);
    /// Cru's own colours (crundi-voice.js C, KIND, PALETTE, MIC_COLORS):
    /// violet (ask), fuchsia (wait), orange and soft red (the mic's offline
    /// and problem), the mic's dark layers, the disc, the cells' names, the
    /// mic's ripples and drops, the owner's bubble.
    pub fn cru_violet() -> Hsla { rgb(0xc084fc).into() }
    pub fn cru_fuchsia() -> Hsla { rgb(0xd946ef).into() }
    pub fn cru_orange() -> Hsla { rgb(0xfb923c).into() }
    pub fn cru_red_soft() -> Hsla { rgb(0xf87171).into() }
    pub fn cru_deep_blue() -> Hsla { rgb(0x1e3a8a).into() }
    pub fn cru_deep_indigo() -> Hsla { rgb(0x312e81).into() }
    pub fn cru_deep_violet() -> Hsla { rgb(0x4c1d95).into() }
    pub fn cru_disc() -> Hsla { rgb(0x05050a).into() }
    pub fn cru_label() -> Hsla { rgb(0xb8b8d0).into() }
    pub fn cru_drop() -> Hsla { rgb(0x7dd3fc).into() }
    pub fn cru_me() -> Hsla { rgb(0x2b2b55).into() }
    /// Cru's live screen: the panel's frame height, the full view's least
    /// window width, a chip's edge and fill strength.
    pub const CRU_SCREEN_H: Pixels = px(300.);
    /// The live screen's head buttons (.cv-scr-h .cv-x 28), Cru's words under its full view (66).
    pub const CRU_SCR_BTN: Pixels = px(28.);
    pub const CRU_FULL_WORDS_H: Pixels = px(66.);
    pub const CRU_FULL_MIN_W: Pixels = px(900.);
    pub const CHIP_EDGE: f32 = 0.45;
    pub const CHIP_FILL: f32 = 0.12;
    /// Kid mode (crundi-voice.js .cv-ov.kid): the type, the big mic, the
    /// round buttons and the PIN pad's keys and width.
    pub const KID_HEADING: Pixels = px(30.);
    /// The kid screen's friendly face ("Comic Sans MS", "Chalkboard SE", system-ui).
    pub const KID_FONT: &'static str = "Comic Sans MS";
    /// The kid typing box (.cv-type): 22px bold text, 28px corners, 14px 20px padding.
    pub const KID_TYPE_TEXT: Pixels = px(22.);
    pub const KID_TYPE_R: Pixels = px(28.);
    pub const KID_TYPE_PAD_X: Pixels = px(20.);
    pub const KID_TYPE_PAD_Y: Pixels = px(14.);
    /// "Talk to Cru" (.cv-talk): 14px 26px padding, 20px bold, its drop 6px down, 14px from the bottom.
    pub const KID_TALK_PAD_X: Pixels = px(26.);
    pub const KID_TALK_PAD_Y: Pixels = px(14.);
    pub const KID_TALK_TEXT: Pixels = px(20.);
    pub const KID_TALK_DROP: Pixels = px(6.);
    pub const KID_TALK_BOTTOM: Pixels = px(14.);
    /// An attachment chip (.cv-chip): at most 220px, 12px corners, a 28px
    /// thumbnail with 6px corners; the sent tag (.cv-m .att) 10px corners, 3px 8px.
    pub const CRU_ATT_MAX_W: Pixels = px(220.);
    pub const CRU_ATT_R: Pixels = px(12.);
    pub const CRU_ATT_THUMB: Pixels = px(28.);
    pub const CRU_ATT_THUMB_R: Pixels = px(6.);
    pub const CRU_ATT_TAG_R: Pixels = px(10.);
    pub const CRU_ATT_TAG_PAD_Y: Pixels = px(3.);
    pub const CRU_ATT_UP_O: f32 = 0.6;
    pub const KID_TEXT: Pixels = px(22.);
    pub const KID_REPLY: Pixels = px(28.);
    pub const KID_HEARD: Pixels = px(20.);
    pub const KID_MIC: Pixels = px(150.);
    /// The kid mic's largest size (clamp(110px, 15vmin, 170px)).
    pub const KID_MIC_MAX: Pixels = px(170.);
    pub const KID_MIC_ICON: Pixels = px(56.);
    pub const KID_BTN: Pixels = px(64.);
    pub const KID_KEY: Pixels = px(64.);
    pub const KID_PAD_W: Pixels = px(300.);
    pub const KID_BUBBLE_W: Pixels = px(900.);
    pub const KID_REPLY_R: Pixels = px(28.);
    /// The wake word training's level meter.
    pub const WAKE_METER_W: Pixels = px(160.);
    /// The kid screen's PIN pad (.cv-pin, .cv-pin-k button, .cv-pin-in).
    pub const KID_PIN_KEY_H: Pixels = px(56.);
    pub const KID_PIN_IN_W: Pixels = px(220.);
    pub const KID_PIN_IN_TEXT: Pixels = px(26.);
    /// The "…" while Cru thinks (min width), the kid buttons' icon, the typing row's top,
    /// the buttons' flat drop (0 5px 0).
    pub const KID_DOTS_W: Pixels = px(90.);
    pub const KID_BTN_ICON: Pixels = px(28.);
    pub const KID_TYPE_TOP: Pixels = px(6.);
    pub const KID_BTN_DROP: Pixels = px(5.);
    pub fn kid_pin_scrim() -> Hsla { rgba(0x0a0a0ff0).into() }
    pub fn kid_pin_key() -> Hsla { rgb(0x1a1a28).into() }
    /// The PC lock (src/webapp-html.js #pc-lock): the badge, the clock, the
    /// PIN box and its keypad, the full screen's night and the visible
    /// lock's see-through veil and ring.
    pub const PCL_TOP: Pixels = px(18.);
    pub const PCL_TIME: Pixels = px(140.);
    pub const PCL_DATE: Pixels = px(28.);
    pub const PCL_KEY_W: Pixels = px(64.);
    pub const PCL_KEY_H: Pixels = px(52.);
    pub const PCL_KEY_R: Pixels = px(12.);
    pub const PCL_BOX_R: Pixels = px(18.);
    pub const PCL_IN_W: Pixels = px(220.);
    pub const PCL_IN_W_VIS: Pixels = px(160.);
    /// .pcl-in's height: 24px text, 10px padding and a 1px edge (18px, 8px when visible).
    pub const PCL_IN_H: Pixels = px(52.);
    pub const PCL_IN_H_VIS: Pixels = px(40.);
    pub const PCL_IN_R: Pixels = px(10.);
    pub const PCL_ERR_H: Pixels = px(18.);
    pub const PCL_HINT: f32 = 0.7;
    pub const SP_14: Pixels = px(14.);
    pub const SP_22: Pixels = px(22.);
    pub const TEXT_18: Pixels = px(18.);
    pub const TEXT_24: Pixels = px(24.);
    pub fn pcl_full() -> Hsla { rgb(0x0b1022).into() }
    pub fn pcl_glow_a() -> Hsla { rgb(0x3b3f8f).into() }
    pub fn pcl_glow_b() -> Hsla { rgb(0x0e7490).into() }
    pub fn pcl_vis() -> Hsla { rgba(0x08_0a_18_61).into() }
    pub fn pcl_vis_ring() -> Hsla { rgba(0xa5b4fc8c).into() }
    pub fn pcl_badge() -> Hsla { rgba(0x00000073).into() }
    pub fn pcl_badge_edge() -> Hsla { rgba(0xffffff40).into() }
    pub fn pcl_box() -> Hsla { rgba(0x0f1228b8).into() }
    pub fn pcl_box_edge() -> Hsla { rgba(0xffffff24).into() }
    pub fn pcl_key() -> Hsla { rgba(0xffffff14).into() }
    pub fn pcl_key_hover() -> Hsla { rgba(0xffffff29).into() }
    pub fn pcl_key_edge() -> Hsla { rgba(0xffffff2e).into() }
    /// .pcl-in's edge (white at .3).
    pub fn pcl_in_edge() -> Hsla { rgba(0xffffff4d).into() }
    pub fn pcl_err() -> Hsla { rgb(0xfca5a5).into() }
    /// Cru's ring (#cru-spot): 6 px out from its target, a 2 px edge; its note
    /// (#cru-spot-note) at most 320 wide, 12 px from the target, its shadow
    /// 0 10px 30px rgba(0,0,0,.45).
    pub const CRU_SPOT_PAD: Pixels = px(6.);
    pub const CRU_SPOT_EDGE: Pixels = px(2.);
    pub const CRU_NOTE_MAX_W: Pixels = px(320.);
    pub const CRU_NOTE_GAP: Pixels = px(12.);
    pub const CRU_NOTE_SHADOW_Y: Pixels = px(10.);
    pub const CRU_NOTE_SHADOW_BLUR: Pixels = px(30.);
    /// #cru-spot-note: kept this far inside the window; its line height; its 1 px edges, both sides.
    pub const CRU_NOTE_EDGE: Pixels = px(8.);
    pub const LH_CRU_NOTE: f32 = 1.4;
    pub const CRU_NOTE_BORDERS: Pixels = px(2.);
    /// The kid screen's thinking dots (.kid-dots: 14 px dots, 10 px apart, an 18 px row).
    pub const KID_DOT: Pixels = px(14.);
    pub const KID_DOTS_GAP: Pixels = px(10.);
    pub const KID_DOTS_H: Pixels = px(18.);
    /// How far they bob up (cvkdots: translateY(-5px) at the top).
    pub const KID_DOTS_RISE: f32 = 5.;
    /// Cru's reply line under the screen (20 px lines).
    pub const CRU_REPLY_LH: Pixels = px(20.);
    pub fn cru_note_shadow() -> Hsla { rgba(0x00000073).into() }
    // ── end desktop ──
    // ── chat ──
    /// The chat log (claude-chat.js's CSS): "Go to the newest message" (38
    /// round, its arrow 18, its dot 10), the copy button on code and links
    /// (24 x 22, its icon 13), an open thinking block's height before it
    /// scrolls (260), the floating dock's widest (the web's 420).
    pub const LOG_JUMP: Pixels = px(38.);
    pub const LOG_JUMP_ICON: Pixels = px(18.);
    pub const LOG_COPY_W: Pixels = px(24.);
    pub const LOG_COPY_H: Pixels = px(22.);
    pub const LOG_COPY_ICON: Pixels = px(13.);
    pub const LOG_THINK_MAX: Pixels = px(260.);
    pub const LOG_DOCK_MAX_W: Pixels = px(420.);
    // ── end chat ──
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
        "font.family": T::UI_FONT, "font.size": f32::from(T::BTN_TEXT),
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
    // The code editor's highlighting for a dark ground (the config has no
    // highlight of its own, so it kept the light one: dark blue on dark).
    gpui_component::Theme::global_mut(cx).highlight_theme = gpui_component::highlighter::HighlightTheme::default_dark();
    // The caret blinks, then rests after a while without typing (as editors
    // do): each blink draws the window again, twice a second for as long as
    // a box had the keys, the window in the background included.
    gpui_component::Theme::global_mut(cx).cursor_blink = gpui_component::input::CursorBlink::blinking(std::time::Duration::from_millis(500)).with_idle_timeout(std::time::Duration::from_secs(10));
    // The toast where the web puts it: under the top bar, at the right edge
    // (top: the bar's height and sp-10; right sp-16), at most a dialog's width wide.
    let n = &mut gpui_component::Theme::global_mut(cx).notification;
    n.margins.top = T::TOPBAR_H + T::SP_10;
    n.margins.right = T::SP_16;
    n.width = T::DIALOG_W_LG;
    // What was written through global_mut reaches the base layer only when
    // pushed down: the caret (gpui_base's BlinkCursor) reads the base's
    // cursor_blink, so without this it never rested (F109).
    gpui_component::Theme::sync_base(cx);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_theme_parses_in_both_modes() {
        for m in ["dark", "light"] {
            assert!(serde_json::from_value::<gpui_component::ThemeConfig>(super::theme_config(m)).is_ok(), "{m}");
        }
    }

    /// T2: the paths gpui's SVG renderer loads (vendor/gpui/src/svg_renderer.rs
    /// load_bundled_fonts) are served, and the icons still are.
    #[test]
    fn serves_the_svg_renderer_fonts() {
        use gpui::AssetSource;
        for p in ["fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf", "fonts/lilex/Lilex-Regular.ttf"] {
            let d = super::AllAssets.load(p).expect(p).expect(p);
            assert!(d.len() > 100_000 && d[..4] == [0, 1, 0, 0], "{p}");
        }
        assert!(super::AllAssets.list("icons/").unwrap().len() > 100);
    }
}
