# Terminal round 3 on the owner's PC (F46 / F50 / F51), 20 Nov

Release builds with frame pointers (`CRUNDI_BUILD_JOBS=12 RUSTFLAGS='-C
force-frame-pointers=yes' tools/build.sh --release`) of the run's HEAD
f5de9935 ("base") and of this round's terminal ("new"), on farhan-RP-15's
own display (1920x1040 window, Wayland, 144 Hz, Renoir), one window at a
time, a fresh config folder each run, against `tools/stub-server.sh 4320`
(a fresh server each run, no real Claude). `tools/measure-pc.mjs` with all
scenarios. The owner's Electron client ran throughout (load average 3.6–5.6);
during base2 and new1a the Planning developer was building on the same PC
(new1a is left out: every number in it doubled).

Windows on the PC's display (each closed by its PID by the harness):
10:50:35–10:53:46 (base1), 10:55:25–10:58:35 (new1a), 11:19:30–11:24:51
(base2), 11:24:55–11:28:06 (new2), 11:28:10–11:31:20 (base3),
11:31:24–11:34:36 (new3), 11:35:11–11:37:57 (profile, new),
11:38:32–11:41:19 (profile, base), 11:43:05–11:48:28 (new4),
11:49:05–11:54:33 (new5), 11:55:40–12:01:08 (exp1: no terminal glyphs),
12:03:19–12:06:35 (new6), 12:08:22–12:11:31 (new7). About 50 minutes.

## What changed

- vt.rs: every line is a `Line` with an id that changes whenever one of its
  cells changes (any mutable access stamps a fresh id). Lines keep their id
  when they scroll, into the scrollback too.
- terms.rs: the screen keeps each visible line's row (`RowPaint`: its
  backgrounds, glyphs and underlines) by line id. A batch derives only the
  lines it wrote, and the wheel only the lines that come into view. Each
  character is shaped once per face and text size (`CharShape`; a cell at a
  time, no ligatures, as xterm.js draws). The cache drops lines that leave
  the screen, and everything when the text size or cell width changes.
  Painting is unchanged: one paint layer, each row's glyphs painted
  straight in.
- The screen's own time (render + paint) is counted in the driver's props
  (`screenUs`, `screenPaints`, and `derived` for the rows derived). The
  harness reports it per paint (`report[term-*].screen`).
- Harness: `PROFILE_AT=term` samples the UI thread while the shell prints
  and the wheel sweeps. `MEASURE_TERM_CLEAR=1` adds the Kanban's region with
  the shell cleared and then refilled.

A row's look is unchanged: a grab of a shell printing bold, underline,
inverse, background, italic, CJK, ✓ ●, an emoji, 256-colour, truecolor and
dim text is pixel-identical between base and new (Xvfb, ffmpeg x11grab).

## Numbers (p95 ms, share ≤ 7 ms)

| Scenario | base1 | base2* | base3 | new4 | new5 | new6 | new7 | exp1 (no glyphs) |
|---|---|---|---|---|---|---|---|---|
| term-stream | 7.44, 90% | 7.17, 94% | 6.83, 97% | 6.66, 98% | 6.58, 98% | 6.50, 98% | 6.55, 98% | 6.03, 100% |
| term-stream+scroll | 7.98, 82% | 8.36, 82% | 7.85, 79% | 7.63, 84% | 7.69, 82% | 7.52, 87% | 7.55, 85% | 6.90, 96% |
| planning+stream+scroll (8 panes, with the shell) | 8.16, 76% | 7.52, 89% | 7.68, 85% | 7.66, 77% | 7.27, 91% | 7.73, 75% | 7.62, 86% | 6.96, 96% |
| idle-terminal | 1 frame | | | | | | 1 frame | |
| screen render + paint, per paint | | | | 513 µs | 480 µs | 440 µs | 431–452 µs | 130 µs |

*base2: a build was running on the PC for part of it. new4/new5 had the
row cache without the per-character shapes; new6/new7 are the final code.
The other scenarios in new7: scroll-panes 6.01, scroll-chat 6.02,
stream+scroll 6.41, stream 5.59, scroll-kanban 3.89, idle 0 frames.

## Where a terminal frame goes

- Deriving rows is no longer a cost. In the sampled profiles
  (profile-term-*.txt: 500 samples while the shell prints and the wheel
  sweeps) the screen's render and paint are 4–5 of ~300 busy samples in
  both builds. The time is in the chats' logs under the wheel, the
  workbench's layout and GPUI's frame.
- Measured exactly, a terminal frame costs the UI thread 430–450 µs
  (render + paint). Without the glyphs it is 130 µs (exp1). So ~300 µs per
  frame is GPUI's `paint_glyph`, about 150 ns for each of the ~2,200 glyphs
  of a 42 x 52 screen full of text. Then, every frame, those sprites are
  sorted, uploaded and drawn with the rest of the scene.
- exp1 painted no glyphs at all (an experiment build, not kept). It shows
  where the floor is: term-stream+scroll 6.90 (96%) and the eight-pane sweep
  6.96 (96%). Even a terminal that draws nothing is just under the budget.
  The other ~6.5 ms of those frames is the five chats under the wheel, the
  workbench re-laid out on every notify, and GPUI's frame.
- An idle terminal draws nothing (0 term-screen renders and 0 paints while
  other panes scroll: `renders` in every region). Like any pane's text, its
  sprites are replayed into each frame's scene. The 2-second region runs
  could not show a stable cost for that: the Kanban's region with the shell
  full against cleared was 6.5 vs 4.7, 6.66 vs 5.54, 4.42 vs 4.52 and 6.54
  vs 4.99 p50, and the refilled shell was 6.3, 6.24 (with no glyphs!) and
  4.44. A chat streams through those regions and ends at a different moment
  each run.
- The one idle-terminal frame is the top bar and the workbench, redrawn for
  a server state event (the projects store's terminal list). It is not the
  terminal's views, and PLAN.md already listed it before this round.

## What would reach ≤ 7 ms with a full terminal

Nothing left in the terminal's code. GPUI has no public way to draw a run
of glyphs as fewer sprites: `TextSystem::raster_bounds` and
`rasterize_glyph` are `pub(crate)`, so a row cannot be painted once into an
image, and a reused paint cannot be replayed at an offset when the lines
move up. The rest is the shared frame (the chats' logs, the workbench's
layout on each notify). Either needs the Manager's call: a change to the
GPUI fork, or work in the chat and workbench areas.
