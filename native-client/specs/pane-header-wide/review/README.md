# 2.1 design review, round 1

Native: release build of this branch on DISPLAY=:99, 1400×860, against an isolated dev
server (:4811) seeded like the spec's (project Demo, the fruit/ls chat, auto-titled
"Fruit list and directory listing"). States driven with the real pointer and keys
through the test driver. `native-<state>.png` are the full windows; each
`<state>-<WxH+X+Y>.png` is web (top) over native (bottom). Header row crops are
1150x32+245+89 (×2); tools 300x32+1100+89 and title 300x32+245+89 (×4);
full screen 1400x31+0+0.

## Measured (pixels from the screenshots, window coordinates)

| Element | Web | Native | |
|---|---|---|---|
| header bottom border | y118 #2a2a3d | y118 #2a2a3d | ok |
| dot | 279-285 × 101-107 | 279-285 × 101-107 | ok |
| title text | x298-492, y100-108 | x297-493, y101-108 | ok |
| title hover fill | 290-500 × 98-111 #1a1a28 | 290-500 × 98-111 #1a1a28 | ok |
| long typed title | "Investigate the flak…" | same, same width | ok |
| CHAT pill left edge | 1127 | 1125 | 2px wider (listed gap) |
| CHAT glyphs | cap y100-107 (8px), C 6px wide, bold | cap y101-107 (7px), C 5px wide, lighter | **differs** |
| lock (empty) | border x1178-1203 | 1177-1202 | 1px |
| lock .on 1 / 2 hovered | 1167-1203, #f59e0b, stays yellow | 1166-1202, #f59e0b, stays yellow | 1px |
| A- / reset / A+ | 1208-1233 / 1238-1262 / 1267-1296, h19 | 1207-1232 / 1237-1261 / 1266-1296, h19 | ok |
| pin / full / × | 1301-1327 h21 / 1332-1356 h19 / 1361-1388 h18 | same | ok |
| hover text/icon | #e8e8f0, border #6366f1 | #e8e8f0, border #6366f1 | ok |
| hover × | #ef4444 | #ef4444 | ok |
| armed, pinned, full, two panes, 801/340 maximise | | | match |
| pane top corners | rounded border+ring, header clipped (r9) | **square header fill over the border arcs; arc missing** | **differs** |
| Send at 9 / 13 / 24 | scales (`.cc-btn` 12px×cc-fs) | fixed 12px | **differs** |
| list item pitch at 9 / 13 / 24 | ~15.5 / 22 / 39-40 | 18 / 23 / 35 | **differs** |

After Esc in full screen, after the 3 s disarm and after reset, the header is
pixel-identical to rest (AE 0).

Not reachable here: no-secrets-capability (listed gap). Narrow neighbour in the
maximise shot is 2.2. Tooltips are Slint's (listed gap).
