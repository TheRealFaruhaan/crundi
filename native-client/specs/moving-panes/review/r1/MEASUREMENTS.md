# 2.3 Moving panes: design review, round 1

Isolated server :4811 (project Demo; chats Fruit list and directory, Second, Third, Fourth),
native on DISPLAY=:99, 1400×860, driven by real X pointer events (xdotool) and testctl.
Crops: `<state>-<WxH+X+Y>.png`, web on top, native below.

## Matches (web = native)

| Item | Web | Native |
|---|---|---|
| Four in a row | 340×758 at x246/593/940/1287 | same |
| Presets, 4 chats: cross4 / left-right2 / top-bottom2 | spec table | same, leaf for leaf |
| Presets, 3 / 2 / 1 chats (all five each) | spec table | same |
| Split down then split right | left column 454, C 340 + ∅ 340 | same |
| Hover controls, narrow | box 392.5–414.5, y93–110 | 393–415, y93–110 (½px, nit) |
| Hover controls, wide (570) | over CHAT's left | same |
| "+" menu text rows (11 items) | y98…439, x418 | same ±1 |
| "+" menu icons | x393–404, same rows | same |
| Layout submenu left / top | 522 / 423 | 522 / 423 (wider and shorter: named one-line gap) |
| Empty pane buttons | 1014–1085, 1092–1163, 1170–1202; 26 high | same |
| Terminal chip | 26 high, icon +13, text +29 | 26, +13, +30 (text 4px wider: named font gap) |
| Empty pane dashes | 6 on / 4 off, 2px | same along the edges (corners differ: finding) |
| Gutter line, hover / all-accent while held | #6366f1 at x820 | same |
| Fit after resize | 570.5 / 378.5 everywhere | same; scrolls back to x246 |
| Fit that cannot fit | error toast, text | same |
| Pinned refused toast | text and box | same |
| Drop: swap / move / pinned target / pinned source | spec rules | same; dragged pane keeps focus |
| Auto-scroll | — | scrolled the full 233px within 60px of the edge |
| Esc during a drag | — (rule 5) | cancels (testctl key; xdotool's key goes to the X root here, no WM) |
| Double-click header / again / empty pane | 696.4 / 60.6, then even; 92/8 | same |
| × on an empty pane | collapse + hoist | same |
| Locked gutter | no drag, tooltip, .4 | same |
| Terminal chip → launcher → UI Mode chat | rule 5: clicked leaf, chat keeps it | same |
| Restart | layout kept | same tree after restart |
| ⋮⋮ | dots x257–258, y100/104/108 | same |
| F24 at 340px | no overlap | no overlap (inline-code paragraph, 3 wrapped paragraphs) |

## Differences

1. **Gutter drag follows only the first move** (gdrag.mjs: 10 moves of 10px → +10px total, both
   directions). After release every gutter line stays #6366f1 (web #2a2a3d; locked #2d2e69 vs
   #171721) and layout motion stays off (ffmpeg 50fps: a split / preset changes in one frame).
   On a fresh client motion runs over ~11 frames (native-motion-midflight).
2. **Overlay presses focus the pane beneath:** a Layout preset item focuses the pane that lands
   under the pointer; a split control focuses its pane. Web: focus unchanged.
3. **Unnamed differences:** dashed drag outline all round chat leaves (web: header band only);
   pin button in the clone (web's clone has none, `.pane-pin` is grid-scoped).
4. **Empty pane corners:** the corner arc runs into the first dash (an L about 11px); web has a
   separate arc dash and a gap (empty-corner-tl / -br).
5. Nits: split icons ½px right, middle line over 2px; clone hides what's under it (shadow layers
   under it) where the web lets 8% through; submenu icons start 1px right.
