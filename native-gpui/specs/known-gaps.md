# 2.25 The chats' known gaps: an audit

This audits every chat item in `PLAN.md` "Known gaps" against the current
build (2026-11-15, after 2.16f) and against what Slint 1.18.1, the newest
release, can do. The Slint answers come from its source
(`i-slint-renderer-software`, `i-slint-core/textlayout/sharedparley`,
`parley 0.11.1`, `fontique 0.11.1`, `skrifa 0.44`), not from memory.

Each item is one of:

- **FIX**: Slint allows a fix or a clearly better approximation. Handed to
  the developer, ordered by how often a person sees it (below).
- **STALE**: no longer true on this build. Remove it or reword it in PLAN.md.
- **SLINT**: a real Slint limit. It stays named, with its workaround.
- **RULE 5**: a deliberate difference (DESIGN rule 5). It stays named.
- **LATER**: belongs to a later phase's screen. It stays named.

Screenshots (web 1400x860, native at the same size on :99, the same seeded
reply in both) are in `specs/known-gaps/`:

| File | What |
|---|---|
| `web-reply.png`, `native-reply.png` | The same reply (two paragraphs with inline code and a path, a list) and the user's bubble |
| `lh-web-over-native-2x.png` | Paragraph line height: web above, native below |
| `bubble-lh-web-over-native.png` | The user's bubble: web above, native below |
| `code-chip-web-over-native-4x.png` | Inline code and a path chip, 4x |
| `send-glow-web-over-native-4x.png` | The Send button's glow, 4x |
| `web-add-menu.png`, `native-add-menu.png`, `menu-shadow-web-left-native-right-2x.png` | A menu's shadow (the tab bar's + menu) |
| `recap-web-over-native-2x.png` | The recap card's text (StyledText, the same line-height gap) |

## What Slint 1.18.1 can and cannot do (checked in its source)

| Question | Answer | Where |
|---|---|---|
| Box shadow (`drop-shadow-*`) in the software renderer | No: `draw_box_shadow` is `// TODO` | renderer-software lib.rs 3059 |
| Clip with a radius | No: `combine_clip` ignores the radius (`// TODO: handle radius`) | lib.rs 3068 |
| Gradient with a border radius | No: filled as a square | lib.rs 1631 |
| Gradients with many stops | Yes: linear and radial, one segment per pair of stops (positions floored/ceiled to pixels) | lib.rs 1644-1715 |
| Rotate or scale an item or image | No (only whole-window rotation) | lib.rs 73 |
| Text filled with a gradient | No: text takes `brush.color()` | lib.rs 3253 |
| `line-height-factor` on `Text` and `TextInput` | Yes (already used: `Theme.lh-chat`) | builtin_elements.rs 1117, 2283 |
| `line-height-factor` on `StyledText` | **No**, not on master either | items/text.rs 240 |
| StyledText's line height | `FontSizeRelative((ascent − descent + leading) / upem)` of its **default font family**: the font's own metrics decide it, half the leading above and half below, as Chrome | sharedparley/shaping.rs 74-81, 244; parley line_break.rs 1067 |
| Registering font bytes at run time under another family name | Yes: `slint::fontique_011::shared_collection().register_fonts(blob, Some(FontInfoOverride { family_name, .. }))` (feature `unstable-fontique-011`) | slint lib.rs 750-785; fontique font.rs 466 |
| The inline code capsule's colours | Fixed in Slint: fill argb(28,128,128,128); a 1px border argb(88,170,170,170) when the StyledText's **default colour** is light (luminance > 140), argb(56,128,128,128) when it is dark; radius 22% of its height, 2-5px; code text at 0.85 of the size | sharedparley/draw.rs 255-350, shaping.rs 18 |
| Strike-through in StyledText | Yes: `~~text~~`, drawn per line under each glyph run | styled_text.rs 322, draw.rs 635 |
| Selection in StyledText | Not in 1.18.1; master has `selectable`, `copy()` (next release) | master builtin_elements.rs 1290-1315 |
| Animated GIF | Not by Slint; the `image` crate (gif feature, already a dependency) decodes its frames | Cargo.toml 25 |

## Fixes, by visibility

### K1. Paragraph line height inside StyledText: FIX (high: every reply, bubble, list, recap)

**Now:** every StyledText line sits at the font's own line. DejaVu Sans at
13px: 15.1px a line; the web's is 13 × 1.55 = 20.15px. One-line blocks are
padded to the web's box (chat.slint 2041-2086), so single lines, list items
and gaps match; wrapped lines are 5px a line tighter (`lh-web-over-native-2x.png`:
a two-line paragraph is 40.3px on the web, 30px native). The user's bubble
is the same (`bubble-lh-web-over-native.png`: web 20px a line, native 15px),
and so are the recap, the side answer, the plan, the cards' summaries and
the parked card.

**The fix: a chat font whose own line is 1.55.** Slint gives StyledText the
line height of its default font family's metrics (the table above). So:

1. At start, before the window: take the UI font's four faces (Linux: the
   bundled DejaVu Sans, `include_bytes!` of ui/fonts; Windows and macOS: the
   system UI family's blobs from `shared_collection()`, Segoe UI and San
   Francisco). Copy each and set its line gap so that
   (ascent − descent + gap) / unitsPerEm = `Theme.line-chat` (1.55):
   `hhea.lineGap` (hhea + 8, i16 BE) and `OS/2.sTypoLineGap` (OS/2 + 72),
   whichever skrifa reads (`OS/2.fsSelection` bit 7, USE_TYPO_METRICS, picks
   the typo metrics: skrifa metrics.rs 143-182). DejaVu Sans: upem 2048,
   ascent 1901, descent 483, so a gap of 790. A variable font (SF) may carry
   an `MVAR` `hlgp` delta: zero it or allow for it.
2. Register each copy under one new family, e.g. "Crundi Chat"
   (`FontInfoOverride { family_name: Some("Crundi Chat"), .. }`), and set
   `Theme.chat-font` to it (empty, as today, if anything fails).
3. Every chat StyledText takes `default-font-family: Theme.chat-font`; the
   one-line padding (chat.slint 2041-2086, "StyledText has no line height,
   so its one line is padded") goes, since the line box is now 1.55 with the
   half-leading above and below as Chrome's.
4. The bubble's transparent selectable copy (F122, `lh-plain`) takes the
   same family at `line-height-factor: 1`, so its lines still lie on the
   drawn ones.
5. Inline code (generic Monospace) gets the same ratio (it is set once for
   the paragraph from the default family), so a line with a chip is not
   taller.

Bitstream Vera's licence (DejaVu's) allows a modified copy under another
name; nothing modified is shipped for Segoe UI or SF (patched in memory only).

**Check:** a wrapped reply paragraph's lines 20px apart at 13px (web 20.15),
a 2-line paragraph 40px; a 3-line bubble 60px of text; a list item and a
one-line paragraph keep their heights; at A+ (fs) the pitch scales.

### K2. The line-height factors assume DejaVu's own line: FIX (high on Windows and macOS)

**Now:** `Theme.lh-chat` 1.33, `lh-ui` 1.12 and the composer's factor are
factors **over the font's own line**, tuned for DejaVu Sans (1.164 em:
1.164 × 1.33 = 1.55). On Windows the UI font is Segoe UI, whose own line is
(2210 + 514) / 2048 = 1.33 em, so `lh-chat` makes 1.77 em lines (web 1.55)
and `lh-ui` 1.49 (web 1.3); on macOS San Francisco is about 1.19 em, so
1.58 and 1.33. Not seen yet: CI's Windows and macOS snapshots
(drive-*) show no chat text. The owner's client runs on Windows.

**The fix:** set the factors at start from the UI font's own line (the
same metrics as K1: `lh = target / natural`), not as constants; or, once K1
is in, give chat `Text` items `Theme.chat-font` at factor 1. Add a unit test
of the computation (DejaVu → 1.33) and a chat snapshot in the Windows CI
job, so it is seen.

### K3. The inline code chip's outline: FIX, better approximation (high: most replies)

**Now** (`code-chip-web-over-native-4x.png`): Slint's capsule, a translucent
grey fill (#17171b over bg-primary) with a clearly visible 1px outline
(about #414144); the web's is a plain fill #1a1a28, radius 4, no outline.
Padding is already added (`pad_code`, no-break spaces).

**What Slint allows:** the capsule's colours are fixed, but its outline is
the faint one (argb 56,128,128,128, about #242428 here) when the
StyledText's **default colour** is dark. So:

1. `md()` (app.rs 2283) wraps the text in an explicit colour,
   `<font color="…">…</font>`, with the block's colour from the theme
   (text-primary, or text-secondary for quotes and side answers), and the
   StyledText's `default-color` becomes a dark token (luminance ≤ 140,
   e.g. a new `Theme.chip-key` = bg-primary). Nothing is drawn in the
   default colour then; it only picks the faint outline.
2. Links: Slint applies spans in the order the parser closes them, the later
   winning, so a Link **inside** the colour span takes that colour (and keeps
   its underline). Close the colour span around URL links so they keep
   `link-color` (accent-hover). Path links (`[`p`](path:p)`) stay inside it:
   at rest they become text-primary with an underline, which is the web's
   rest look (`.cc-path`: the text's colour, dotted underline;
   accent-hover and solid only on hover). The PLAN line "a path … in
   accent-hover" then changes. Verify the order with a snapshot.
3. The fallback in `md()` (strip `<>*_` on a parse error) must wrap again,
   or the text comes out in the dark default.

**Result:** outline ≈ #242428 on a #17171b fill (web: none on #1a1a28).
**Stays named:** the capsule's height and radius, its grey (not the web's
blue-grey) fill and the code size 0.85 (web 12/13 ≈ 0.92).

### K4. Shadows and glows: FIX, better approximation (medium: Send always, menus, toast, drag, sign-in card)

**Now** (`send-glow-web-over-native-4x.png`,
`menu-shadow-web-left-native-right-2x.png`): `SoftShadow` fades linearly
from the full colour at `radius` inside the box to clear at `blur` outside,
so its tokens were turned down by eye (menu .32 against the web's .5,
Send 8px at .2 against 12px at .35). Send's glow is visibly tighter and
weaker than the web's; menus are close.

**What Slint allows:** gradients with many stops (no blur). A CSS shadow
of blur B is a Gaussian of σ = B/2: 50% at the shadow box's edge, spanning
from B inside to B outside. With the fade drawn over 2B centred on the edge
and these stops (t across the fade, × the colour's alpha):

| t | 0 | .125 | .25 | .375 | .5 | .625 | .75 | .875 | 1 |
|---|---|---|---|---|---|---|---|---|---|
| alpha | .977 | .933 | .841 | .691 | .5 | .309 | .159 | .067 | .023 |

(the radial corners the same, along the radius), the tokens can take the
web's own values: Send 0 2px 12px accent .35; toast and most menus
`--shadow-md` 0 6px 20px .5; the dragged pane 0 12px 32px .55; the sign-in
card `--shadow-lg` 0 24px 60px .6 (each screen's spec has its own). Check
for seams between stops at scale 1 and 1.25 (the software renderer rounds
each segment's ends to pixels); five stops are enough if nine band.

**Stays named:** no real blur (a Gaussian profile from gradients); shadows
cut at a scrolling box's edge (sreq dock).

### K5. The composer's line height and tallest box: FIX (medium: every multi-line draft)

**Now:** the composer's TextInput uses `lh-chat` (1.55 lines); the web's
`.cc-input` is `line-height: 1.45`, so native lines are 1.3px taller and
`composer-max` (180) holds 8 lines against the web's 8.6 (PLAN: "eight
lines … the web's nine").

**The fix:** a token for 1.45 (over the font's own line, see K2) on the
composer's TextInput; its one-line height and tallest box then follow
(check them against `composer.md`'s measurements). The PLAN entry then goes.

### K6. A done to-do's strike: FIX (low: wrapped done to-dos)

**Now:** one rule across the whole text column for each wrapped line
(chat.slint 992: "Slint's text has no strike").
**What Slint allows:** StyledText draws `~~text~~` per line, as wide as
each line's text (web: `text-decoration: line-through`). Draw a done item
as StyledText with its text escaped for markdown and `~~` around it,
`default-font-family` the mono font. Its line height then needs K1's trick
for the mono font too (a "Crundi Chat Mono" copy of the mono face), or the
item keeps BodyText. Low priority.

### K7. The wheel at an open tool body's end: FIX (low)

**Now:** the wheel over an open body scrolls the body and never passes to
the log. 2.16f does this chaining for frames (`scroll-event`, reject at the
end); the same in the body's Flickable `scroll-event` (reject when the body
is at that end) would pass it to the log as the web does. Not a Slint limit;
the PLAN line goes once done.

### K8. Scroll thumbs can't be dragged: FIX (low)

Not a Slint limit: a TouchArea on `ScrollThumb` whose `moved` sets the
Flickable's `viewport-y`. The composer, the picker, the dock.

### K9. A GIF shows its first frame: FIX (low)

Not a Slint limit: decode the frames with the `image` crate's GIF decoder
(already a dependency), swap the box's image on a timer by each frame's
delay, only while the box is in view (as media load and let go now).

### K10. The agent badge's arrival and finish rings: FIX, partly (low)

The rings are an accent / green border fading out: border colour and width
animate in Slint. The arrival's scale cannot (no transforms): stays named.

### K11. Reduced motion: FIX (low)

Read the system's setting at start (Windows `SPI_GETCLIENTAREAANIMATION`,
macOS `accessibilityDisplayShouldReduceMotion`, GNOME
`org.gnome.desktop.interface enable-animations`) and stop the shimmer, the
caret's blink and the spinners' turning as the web's
`prefers-reduced-motion` does.

## Stale: remove or reword in PLAN.md

| PLAN entry | Why it no longer holds |
|---|---|
| Media (2.16m): "a frame (show_embed) is its box with the caption and an "Open" button … the live inline frame … is not built yet" | 2.16f built it (`src/frames.rs`, `LiveFrame`); the "Live frames" entry names what is left. |
| Live state (2.4): "the composer's foot label … is 2.5's; the parked pane's grey dot is 2.23's" | Both built (2.5, 2.23). Keep only the approvals button (phase 5) and Cru (phase 6). |
| Composer (2.5): "Queue sends to the server, which hands it to the running turn, until the client's own queue (2.9)" | 2.9 is in: the "Queued messages" entry says Queue and Send share one path. Drop the "until". |
| Goal and plan (2.19): "the plan's headings are bold body text in the panel (StyledText has no heading sizes)" | Not a difference: the web's `.cc-plan-body h1-h3` are 13px bold too (body size). Remove, or keep only "its lists hang as the reply's". |
| Cards (2.20): inline widget "needs the Panels tab (phase 3)" | The reason is now: it needs the widget runtime (`crundi-widgets.js createView`: the bridge, the data feeds, auto height up to 460) and Open goes to the Panels tab, both phase 3's; the in-log view it would sit in exists since 2.16f. Reword. |
| The rest of the log (2.16): the list ends "…in a few eased frames;" | Ends with a semicolon, the last item cut off. Tidy. |
| Inline code / Line height / Shadows (phase 1) | Reword after K1, K3, K4 as what is left (see each). |
| Tool cards (2.14): the strike and the wheel lines | After K6 / K7. |
| Composer's tallest box (2.5) | After K5. |

## Stays named: Slint limits (SLINT)

| Gap | Holds? | Workaround in place |
|---|---|---|
| Reply paragraphs not selectable | Yes in 1.18.1 | Code blocks, bubbles, tool and thinking bodies select. Slint master adds `selectable`/`copy()` to StyledText: revisit with the next Slint release. |
| Copy button on spans and links; a link's right-click menu | Yes: StyledText reports no span geometry or hover | Code blocks only; left click opens the link. |
| A path's dotted underline (and its hover look) | Yes: one solid underline, no per-span hover | Solid underline; colour per K3. |
| The bubble drawn as StyledText under a transparent selectable copy | Yes | As now (K1 step 4). |
| Code block: no sideways thumb | Yes (thumbs are drawn by us; see K8 for dragging) | Shift+wheel. |
| Thinking: the shimmer | Yes: text is one colour | A brighter band slides over it. |
| Thinking: the live caret | Yes: no position at the end of wrapped text | A glyph after the text. |
| Spinner | Yes: no rotation of images | Two paths. |
| Tabs in a body | Yes: no tab stops | Expanded to 8 in Rust. |
| Parked pane: no blur behind the card | Yes | `park-dim` under the veil. |
| Parked card's wrapping row | Yes: no wrapping row in Slint | Two rows. |
| Media box round corners | Yes: clip has no radius | Corner masks (relies on bg-primary behind). |
| Round gradients (Send, primary buttons, rail avatar, sign-in card) | Yes: gradients ignore the radius | Images / nine-slice PNGs. |
| Dashed rounded borders, the session id's dotted underline | Yes: no dashed/dotted borders | `DashedBox`, hairline dashes. |
| Turning the pinned pin; turning the dragged pane; a new pane's scale | Yes: no rotation or scale | A turned copy; drawn straight; fade only. |
| Small bold text a size up; header text widths ±1px; the rename input's selection 1px short | Yes (font rendering) | As named. |
| Blur on a click elsewhere | Yes (Slint keeps focus) | Counted presses end the rename. |
| Italic on Linux | Solved by the bundled DejaVu faces | Keep the entry as the reason for the bundled font. |
| A run's command breaks after `/` | Yes (Slint's line breaker) | As named. |
| Agent dock: the badge's scale on arrival | Yes | K10 for the rings. |
| Live frames: views made with the rows, the cap of 6, dialogs suppressed, full screen from a player, inner scrollers without chaining, the full-screen pane drawn twice | Yes (ListView rows; windowless CEF) | As named in 2.16f. |
| Video and audio in the system's player | Yes for video (no H.264/AAC in CEF's build, no playback in Slint). Audio could play in-app through `rodio` (a dependency since 2.24) | Fetch then open. Optional later. |
| Scheduled messages: std DatePickerPopup / TimePickerPopup, en-US formats | Yes (std widgets' style; no locale API) | As named. |
| Voice: echo gate not ported, device voice, mic focus, Linux spd-say | Yes / porting work, not a Slint limit; out of 2.25's visual scope | As named in 2.24. |
| File drops' position, pasting with the focus in the log, rfd portal | Yes (winit / platform) | As named. |
| Tooltips drawn by the client | Not a gap: the native client's own TipBox | Keep the entry as a note. |

## Stays named: deliberate differences (RULE 5)

Unchanged and still accurate on this build; each is the native choice where
the web has a bug or no behaviour: the narrow header's Esc and its split
items (2.2); moving panes' Terminal chip, one-line "Top + bottom split",
the focused clone's shadow, Esc to put back (2.3); the composer's refused
send, the shared Enter mode, the cost snapshot (2.5); the mode picker's
rollback and focus (2.6); attachments' header drop, busy until the last
upload, the 404 wording, drafts in settings (2.7, 2.8); held lines kept
(2.9); the scheduled 404 wording and no live update (2.10); the slash menu
(2.11); the secrets dialog's Tab and checkbox, the upgrade prompt (2.12,
2.12b); question cards (2.13); tool cards' open-on-patch and agent row
padding (2.14); the thinking body's kept scroll (2.15); the bubble's URL
click (2.16); Download asks first, decoded pictures kept (2.16m);
suggestions and recaps (2.17); side questions (2.21); "1 tool" (2.18);
the factory card (2.20); voice wording (2.24); the resume picker's keys
(2.22); parked panes' Esc in Peek, live "Closed · 5m ago", no terminal bar,
no Tab focus (2.23).

## Outside chats (noticed, not 2.25's)

- The connection dot's ring can pulse: a ring's size and opacity animate
  in Slint without transforms (phase 1's top bar).
- The login step under the title bar, Telegram sign-in, visual-only
  controls, the bundled server: unchanged, not chats.

## Checklist for the review

- [x] K1: wrapped reply lines 20px apart at 13px (web 20.15); a 2-line
      paragraph 40px; a list item, a one-line paragraph, a heading keep
      their heights; the bubble, recap, side answer, plan and card
      summaries the same; the bubble's selection lies on its text.
- [x] K1 on Windows and macOS: the chat family registers from the system
      font (or falls back to today's look without breaking); drive.mjs checks
      the registration on each CI system.
- [x] K2: line-height factors computed from the UI font; checked on each
      system by CI's drive test (the app logs the chat font and factors), not
      by a chat snapshot (drive has no chat).
- [x] K3: chip outline faint (≈#242428 on #17171b); paragraph text, bold,
      quotes and side answers in their colours; URL links accent-hover;
      a path text-primary with an underline; a parse-error reply readable.
- [x] K4: Send's glow, a menu, the toast, the drag clone and the sign-in
      card with the web's values and a Gaussian fade; no seams at 1 and
      1.25.
- [x] K5: the composer's lines 18.85px apart at 13px; its tallest box
      holds as many lines as the web's.
- [x] K6-K11 done or left named with a reason.
- [x] PLAN.md: the stale rows above removed or reworded; every remaining
      chat gap accurate.
