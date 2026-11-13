# 2.2 design review, round 1: measurements (web shot / native)

Native: release build of 7feede2 on DISPLAY=:99, 1400×860, isolated dev server
:4811 with Demo, "Fruit list and directory" (2 grants, auto long title "…listing"),
"Second chat" (typed long title), "Third chat". Pane widths set with real gutter
drags (xdotool): 378 ×3, 340, 567.15 / 568.15. Pixel runs from the PNGs (border
#2a2a3d, accent #6366f1, yellow #f59e0b, selection #0236a1). Crops: `<state>-<geom>.png`,
web on top.

| Item | Web | Native | |
|---|---|---|---|
| narrow header | top 90, fill 91–115, bottom border 116 | same | ok (F16) |
| full / … / × (x) | 533–557 / 562–586 / 591–618, y94–112 | same | ok |
| "…" open (also hovered) | accent border, icon #818cf8-ish | same | ok |
| title "Fruit list and directory" | text 298–446 (no cap) | 297–446 | ok (F17) |
| long typed title (pane 2) | text 683–899, full at 918 | 682–900, full at 918 | ok |
| armed × "Sure?" | red 574–618; full 516, … 545 | same | ok |
| edge 567 / 568 | narrow 26px / wide 28px + long title | same | ok (F15) |
| menu box | x264–618, y118–156 (39) | x263–618, y118–156 | 1px wider: A- |
| CHAT tag y | 130–144 | 131–145 | **+1** |
| lock y / x | 128–147 / 326–477 | 129–148 / 325–476 | **+1** y |
| A- y / x | 128–146 / 484–508 | 129–147 / 483–508 | **+1** y, 1px wider |
| reset y | 128–146 | 129–148 | **+1** |
| A+ y / x | 128–146 / 546–576 | 129–147 / 546–576 | **+1** y |
| pin y / x | 127–147 / 583–609 | 128–148 / 583–609 | **+1** y |
| menu, no secrets | x660–1003; lock 722–862 | x659–1003; lock 721–861 | A- 1px wider |
| wrapped 340: box | 251–580 × 118–182 | same | ok |
| wrapped row 1 lock / reset y | 127–146 | 128–147 | **+1** |
| wrapped row 2 pin y | 153–173 (8px to border) | 155–175 (6px) | **+2** |
| shadow under menu (x800, y157→180) | #060609 → #09090e | #0a0a0f flat | **none** |
| focus after × in pane 1 (pane 3 menu open) | pane 1 focused | pane 3 stays focused | **differs** |
| rename input scrolled to end (wide, F14) | sel x297–442 y98–111, text to 442, sliver of F | sel 297–440 y98–110, text to 439, no sliver | **2px short** |

Behaviour (driver + real X clicks): "…" toggles and focuses; one menu at a time;
pin/reset stay open; lock closes it; × closes and arms; Esc closes only the menu;
live chat-secrets revoke updates the count inside the open menu; a press on another
pane's title closes it and opens that rename; going wide closes it and it stays
closed when narrow again.
