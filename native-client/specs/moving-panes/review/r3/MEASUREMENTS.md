# 2.3 Moving panes: design review, round 3 (F32, F33)

Fresh isolated server :4811 (project Demo; chats Fruit, Second, Third, Fourth in one row), native
(HEAD build) on DISPLAY=:99 at 1400×860 driven by real X pointer events (xdotool); the web is the
same server in a 1400×860 Chromium. Crops: `<state>-<WxH+X+Y>.png`, web on top, native below.

## F33: an overflowing row keeps every leaf at its 340 floor

| Check | Web | Native |
|---|---|---|
| Sizes [45,5,25,25], leaves (width@x) | 340@246 340@593 340@940 340@1287 | 340@246 340@593 340@940 340@1287 |
| Gutter Fruit|Second from four 340s, real drag +100px in 10 moves: every move | 340 ×4, x unchanged | 340 ×4, x unchanged (all 10 moves) |
| Stored sizes after the drag | 32.35294117647059, 17.647058823529413, 25, 25 | 32.352943, 17.647057, 25, 25 |

Crops: four-row-sizes-45-5-25-25-*, gutter-drag-100-after-*. Fixed.

## F32: no header tooltip during a press or a drag

The web's tooltips are `title` attributes (term-drag "Drag to reorder", term-title "Click to rename"):
Chromium draws them as OS tooltips, which a page screenshot can't show, so the web reference is
Chromium's behaviour: hidden on a press, none while the button is held, shown again only on a
fresh hover. The web-over-native crops (drag-grip-no-tooltip-*) show the drag itself with no tooltip
on either side. The native sequences, each step after a 1.6 s pause:

| Step | ⋮⋮ (607,103) | title (670,103) |
|---|---|---|
| 1 hover | "Drag to reorder" | "Click to rename" |
| 2 pressed, not moved | none | none |
| 3 dragged to (+96,+200), paused | none | none |
| 4 dragged back over the source, still held | none | none |
| 5 released away | none | none |
| 6 fresh hover | "Drag to reorder" | "Click to rename" |
| q1/q2 pressed on arrival, dragged without a pause, then held | none | none |
| q3 fresh hover after release | shows | shows |

Crops: native-tooltip-grip-*, native-tooltip-gripq-*, native-tooltip-title-*, native-tooltip-titleq-*,
plus the two *-sequence.png strips. No drop happened in any sequence (order still Fruit, Second,
Third, Fourth). Fixed.
