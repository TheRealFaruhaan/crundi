# 2.2 design review, round 2: measurements (web shot / native)

Native: release build of 26e25ea on DISPLAY=:99, 1400×860, a fresh isolated dev
server on :4811 (Demo; "Fruit list and directory" with 2 grants and the long
title "…listing"; "Second chat", later the long typed title; "Third chat").
Pane widths set with real gutter drags (xdotool): 378 ×3, 340, 566.9 / 567.9 / 568.9.
Crops: `<state>-<geom>.png`, web on top. Pixel runs from the PNGs.

| Item | Web | Native | |
|---|---|---|---|
| menu box | x264–618, y118–156 | x264–618, y118–156 | ok (F23) |
| CHAT tag text y | 133–140 | 133–140 | ok (F19) |
| lock x / y | 326–477 / 128–147 | 326–477 / 128–147 | ok (F19) |
| A- x / y | 484–508 / 128–146 | 484–508 / 128–146 | ok (F19, F23) |
| reset / A+ / pin y | 128–146 / 128–146 / 127–147 | 127–147 / 128–146 / 127–147 | ok (reset 19.6 high, AA) |
| menu, no secrets | x660–1003; lock 727–857 | x660–1003; lock 728–856 (inner run) | ok, side borders match |
| wrapped 340: box bottom | y182 | y182 | ok |
| wrapped row 1 y | 127/128–146 | 127/128–146 | ok (F19) |
| shadow under menu, x610 y157→180 | #000-ish gradient over bubble | 06060A→09090E in 5px steps | ok, named gap (F21) |
| shadow under wrapped menu, x300 y183→206 | 060609→09090E | 06060A→09090E | ok (F21) |
| corners (radius 6) | AA arc | same arc, AA differs by a grey level | ok |
| edge 566.9 / 567.9 / 568.9 | narrow / (between) tools + short title / long | narrow / tools + "Fruit list and direct…" / long, tm 347.8 | ok (F22) |
| long typed title (pane 2) | text 683–899 | 682–900 | ok |
| rename input as opened (wide) | sel x297–442 y98–111 | sel x297–442 y98–110 | ok; 1px height named gap (F14) |

Focus (F20), real X clicks: with pane 3's menu open, a click on pane 1's × focuses
and arms pane 1 and closes pane 3's menu. A press on pane 1's full, drag handle,
dot, spacer, right button and title each focuses pane 1 (the title also opens its
rename). While pane 1 is full screen, a click where pane 3 was keeps pane 1.

Not this screen: the lock opens no dialog yet (2.12). The chat log overlaps rows
in narrow panes (chatlog-overlap-native-*.png), raised as a nit for the chat log.
