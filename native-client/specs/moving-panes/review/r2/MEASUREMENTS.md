# 2.3 Moving panes: design review, round 2

Fresh isolated server :4811 (project Demo; chats Fruit, Second, Third, then Fourth closed and Fifth
added), native on DISPLAY=:99 at 1400×860, driven by real X pointer events (xdotool). The web is
the same server in a 1400×860 Chromium. Crops: `<state>-<WxH+X+Y>.png`, web on top, native below.

## Round-1 findings

| # | Check | Web | Native | |
|---|---|---|---|---|
| F25 | Column gutter (cross4), 10 moves of +10 then −10 | follows | 570.5 → 580.5 → 620.5 → 670.5, back to 570.5 | fixed |
| F25 | Row gutter, 8 moves of +8 | follows | 378.5 → 442.5 | fixed |
| F25 | Gutter line at rest after drags | #2a2a3d | #2a2a3d (x819, y472) | fixed |
| F25 | Layout motion after a drag (2 columns preset, ffmpeg 50fps) | eases | changes over frames 52–74 | fixed |
| F26 | Preset picked with the mouse over another pane (cross4, top+bottom, 2 cols, left+right) | focus kept | focus kept (Fruit) | fixed |
| F26 | Split right clicked on Second while Fruit focused | focus kept | focus kept; left column 455.7 | fixed |
| F27a | Drag outline on a chat leaf (Second's x825 / x1391) | 17 / 4 dashed px, header band | 18 / 3, header band | fixed |
| F27b | Clone's header buttons | CHAT lock A− reset A+ full × | same, no pin | fixed |
| F28 | Empty pane, top-left corner | arc dash, gap, dashes | same (empty-corner-tl) | fixed |
| F28 | Far corners | Chromium's pattern runs round | up to ~5px off | named in PLAN.md "Dashed rounded borders" |

Drop (swap Third onto Fruit), the drop highlight, and the empty pane's outline during a drag are the same
as before.

## New differences

1. **The ⋮⋮ tooltip stays up during a drag** (drag-tooltip). Press on ⋮⋮ and drag away, with or
   without a pause first: "Drag to reorder" appears at (254,494), under the source header, and stays
   there until the release, while the pointer and clone are elsewhere. Web: no tooltip during a drag
   (Chromium hides title tooltips on a press and shows none while the button is held).
2. **A row that overflows shows its sizes** (four-row-sizes-45-5-25-25). Four chats in a row with
   sizes [45,5,25,25]:
   - web (`flex: 45 1 0px` …, min-width 340): every leaf 340, at x246/593/940/1287;
   - native: Fruit 612, the others 340 at x865/1212/1559 (the strip scrolls further).
   - During a drag from four 340s the native Fruit jumps 340 → 473.7 / 521.3 / 350 / 389.4 and back
     to 340 between pointer moves; the web stays at 340 throughout (its sizes change: 32.35/17.65
     after a +100px drag; native 33.36/16.64).
