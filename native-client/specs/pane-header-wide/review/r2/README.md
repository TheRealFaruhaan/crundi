# 2.1 design review, round 2

Release build of this branch (0fdd2c4) on DISPLAY=:99, 1400×860, against an isolated
dev server (:4811) with a freshly seeded fruit/ls chat ("Fruit list and directory
listing", automatic long form). The states were driven with the real pointer and keys:
rest, hover over each control, armed ×, pinned, full screen, renaming, font min/default/max,
secrets 1 and 2 (hovered), two panes and double-click maximise. Each crop here is web (top)
over native (bottom).

## The round-1 findings, re-measured

| Finding | Web | Native | |
|---|---|---|---|
| F1 pane corners (`corner-*`, `two-panes-40x36+*`) | arc at all 4 corners, 1px #6366f1 edge | the same; edge 1px at x246/x1393, y90/y853 | fixed |
| F2 Send at 9 / 13 / 24 | 49×24 / 60×28 / 89×40 | 50×24 / 60×28 / 89×39 | fixed (sits 3px higher at every size: the composer, 2.5) |
| F3 list bullet pitch at 9 / 13 / 24 | 16,16 / 22,22 / 40,39 | 16,16 / 22,22 / 39,39 | fixed |
| F4 CHAT glyphs (`chat-60x20+1120+94`) | caps y100-107 (8px), text x1136-1164 | caps y100-107 (8px), x1135-1164 | fixed; Small bold text gap in PLAN |
| CHAT pill | x1127-1173 | x1126-1172 (same width) | 1px left: the A-/A+ gap in PLAN |
| title text | x298-492, y100-108 | x297-493, y100-108 | ok |

Unfocused pane border #2a2a3d with no ring in two panes; full screen, armed, pinned,
secrets yellow-on-hover, hover colours: unchanged from round 1, all matching.

Still for later screens (nits): the chat log starts ~11px higher and the list bullets
don't move with the text size (web x267/264/258 at 9/13/24, native 268/267/266), which is
the chat log; Send's square corners and its 3px offset are the composer (2.5).
