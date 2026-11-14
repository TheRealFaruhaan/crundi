# 2.C Theme consolidation: design review

Fresh `tools/test-server.sh 4814`, seeded with 5 projects and 9 chats (a
long job, a question, an approval, a background task…). Two native release
builds on DISPLAY=:99, each from a fresh config, driven through the same
scripted sequence of 21 states: **old** = c8a7817 (before 2.C), **new** =
HEAD (20dfc2d). The web UI was in a hidden browser at 1400×860 on the same
server.

## Old over new (`old-over-new/`, top = before 2.C, bottom = after)

Sign-in: server, server error, login, login error, pairing; workbench,
sidebar hover, tab hover, folded rail and its hover; the "+" menu, the
Layout submenu and its hover; the hover controls, the narrow header's menu,
a full pane; the grip hover, a pane dragged, an empty pane and its chip
hover; the drawer. Plus close-ups of Send's glow and the Add project corner.

Every state keeps its parts, order and behaviour. The only moves are
token-step ones: tab labels a few px tighter (so one more tab label shows
before the Browser tab), the sidebar's PROJECTS row 2px up, the sign-in
card a little taller (centred, so it moves up 11px), the login fields 240
wide (web 260). Nothing is missing or reordered.

## Web over native (`web-over-native/`)

| Region | Result |
|---|---|
| Sidebar footer, wide and folded (F43) | The line is now at the footer's top (y761, web y765), above Add project. **Verified.** |
| Folded rail | Same avatars, rings, initials, order. |
| Top bar and tabs | Same parts and order. |
| Login card | Prompt, fields, Sign in, links centred, as on the web. |
| "+" menu (web, old, new) | Same items, order, icons; width from content, same as the web's. |
| Launcher (web, old, new) | Same parts; the column is centred; UI Mode's gradient has round corners. Nits below. |
| Card shadow | One smooth fade, no dark blocks. Under strong contrast the corner pieces show faint seams (one grey level); at normal contrast just visible at x4 (`card-shadow-corner-native-x4.png`). Nit. |

## Theme and tools

- `tools/tokens.sh` exits 0 on the tree; in a scratch copy it flags
  `white`, `black`, `darkgray`, `Colors.red`, `Colors.transparent`,
  `rgba(…)` and `13px` (F44 **verified**). It does not flag a sum of three
  steps (`sp-8 + sp-8 + sp-2`); the few in the tree are composite widths
  (a pill's parts, a menu's paddings), which is fine.
- theme.slint holds the scales and the structural list the PLAN entry asks
  for; the Rust constants match.

## Nits (not blocking)

1. Launcher ">_" is drawn at text-26; the web's is 48px, so it is about half
   the size (the old build matched). Being decorative, it could be an icon
   size (e.g. `sp-48`) rather than text.
2. Add project's dashed box has square corners (`Dashed`); the web's is
   rounded (radius 6) and the empty pane already uses `DashedBox` with round
   corners. Was like this before 2.C.
3. Launcher pane header in a narrow pane: the web shows Pin and × directly;
   native folds into "⋯" and ×. Was like this before 2.C.
4. SoftShadow corner seams (above).
