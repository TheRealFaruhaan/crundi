# Workspace, F205: the terminal drawn a line at a time (term-stream+scroll)

farhan-RP-15, GNOME Wayland, a 1920x1040 window on the PC's display, release
(`tools/build.sh --release`), one window at a time, `tools/stub-server.sh 4556`
fresh each run, a fresh config folder each run, `MEASURE_QUICK=1
MEASURE_QUICK_TERM=1 MEASURE_DRAWER=0` (the CR's scenarios and
term-stream+scroll). Load average 3.5-6 (the owner's Electron client and other
worktrees' suites were running). **base** is this branch's HEAD before the
change, built from the same tree (`git archive HEAD`), measured interleaved
with the new build.

p50 / p95 ms / share ≤ 7 ms.

## term-stream+scroll (the shell prints 3000 lines, a chat log under the wheel)

| run | p50 | p95 | ≤ 7 ms |
|---|---|---|---|
| base b1 | 5.21 | 7.46 | 86% |
| base b2 | 5.02 | 7.52 | 85% |
| base b3 | 5.18 | 7.51 | 86% |
| base b4 | 4.86 | 7.51 | 87% |
| **new m8** | 4.81 | 7.04 | **95%** |
| **new m9** | 4.95 | 7.02 | **95%** |
| **new m10** | 5.20 | 6.94 | **96%** |

No frame over 16 ms in any run.

## The CR's scenarios (same runs; noisy: ±5% between runs of the same build)

| | planning+stream+scroll ≤ 7 ms | cr-sweep | cr-scroll |
|---|---|---|---|
| base (b1-b9) | 97, 94, 97, 92, 96, 94, 94, 89, 83% | 95-100% | 92-97% |
| new (m8-m13) | 91, 89, 85, 93, 88, 88% | 95-98% | 93-97% |

The new build's planning+stream+scroll reads a few percent lower on average.
The baseline also dips, to 89% and 83%, and bisecting builds of the terminal change
alone measured 84-95% across runs, so with this noise I could not pin it on
one cause. The candidate is the find marks (F202) over the rows the screen
paints itself: one mark element per row, rebuilt on every render while the
shell prints. They are now one element for all rows, rebuilt only when those
rows change (`crundi_widgets::find::marks`). I did not measure this last
build: the Manager stopped the measuring.

## What changed (crates/workspace/src/terms.rs)

- The row height is a whole number of device pixels (as xterm.js floors its
  cell height), so a line that scrolls moves by whole pixels.
- While the shell prints (changes within 250 ms of each other), each line is
  a cached view (`TermLine`). A line that only moved up is replayed where it
  now is (GPUI patch (b)), and a frame renders only the lines written (the
  workspace suite: about 5 line renders a screen render while printing, not
  the whole screen). 400 ms after the printing stops, the screen goes back to
  one element. A still screen as 50 nested cached views costs a little on
  every frame the Workbench replays its pane.
- When the screen is not wholly inside its clip (in this harness, the
  terminal pane is out of the grid's view: its clip is 0 px wide), its lines
  can't be replayed moved (patch (b) replays only what is wholly in its
  clip). The screen then paints every row itself as one element, and paints
  nothing when none of it is in sight. Before this, the line views rendered
  every frame (52 a frame, p95 8.9 ms).
- What made the difference here: in this layout the terminal is off screen,
  and the frame no longer paints its glyphs or rebuilds its find marks.

Raw (on farhan-RP-15, not kept): /tmp/ws-run/{b1..b9,m1..m13,v1a..v4d}/report.json.
