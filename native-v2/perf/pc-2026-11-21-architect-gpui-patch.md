# The GPUI patch on the owner's display (Architect, round 11)

farhan-RP-15, Wayland/GNOME, a 1920x1040 window, AMD Renoir; release with
frame pointers, the worktree's own `.bin/release`, `tools/stub-server.sh
4884` fresh each run, and a fresh config folder each run. The owner's
Electron client was running throughout (about 2 cores), as in round 12.
Runs were started at a load average below 5 (C, D).

- **A**: the patch's first version, and no browser host beside the client.
  The drawer's scenarios measured a white drawer (below).
- **C, D**: the final patch (vendor/gpui/PATCH.md: (a) nested caching with
  the nested ranges shifted, (b) moved views replayed), F151 (the drawer
  cached at the Shell) and the browser host staged
  (`tools/stage-browser.sh --release`). The drawer shows its page
  ("measure", frames arriving).

p50 / p95 ms / share ≤ 7 ms.

| scenario | A | C | D |
|---|---|---|---|
| idle-open | 0 frames | 0 frames | 0 frames |
| idle-end | 0 frames | 0 frames | 0 frames |
| idle-planning | 0 frames | 0 frames | 0 frames |
| idle-terminal | 1 frames | 1 frames | 1 frames |
| idle-drawer | 1 frames | 0 frames | 0 frames |
| idle-info | 3 frames | 3 frames | 3 frames |
| idle-settings | 0 frames | 0 frames | 0 frames |
| scroll-panes | 4.99 / 5.79 / 100% | 3.98 / 5.68 / 100% | 4.95 / 5.91 / 100% |
| scroll-chat | 5.02 / 5.87 / 100% | 5.08 / 5.95 / 99% | 4.42 / 5.85 / 100% |
| stream+scroll | 4.54 / 6.38 / 99% | 4.92 / 6.4 / 99% | 5.22 / 6.53 / 99% |
| stream | 4.37 / 4.84 / 99% | 4.59 / 5.41 / 100% | 4.59 / 6.21 / 99% |
| term-stream | 4.18 / 6.42 / 99% | 4.29 / 6.54 / 98% | 4.26 / 6.55 / 98% |
| term-stream+scroll | 5.03 / 7.59 / 85% | 4.99 / 7.41 / 89% | 4.76 / 7.58 / 86% |
| scroll-kanban | 3.55 / 4.13 / 100% | 3.13 / 4.02 / 100% | 3.01 / 4.43 / 100% |
| pan-mindmap | 4.5 / 5.4 / 100% | 4.46 / 5.43 / 100% | 4.53 / 5.74 / 99% |
| planning+stream+scroll | 6.75 / 7.93 / 60% | 6.93 / 7.83 / 55% | 6.19 / 7.11 / 91% |
| cr-sweep | 5.99 / 7.13 / 93% | 6.19 / 7.56 / 82% | 5.05 / 6.71 / 98% |
| cr-scroll | 4.84 / 7.21 / 94% | 6.18 / 7.71 / 84% | 5.87 / 7.29 / 91% |
| cr-drawer-sweep | 5.47 / 6.86 / 96% | 5.29 / 7.64 / 86% | 4.9 / 7.11 / 94% |
| cr-drawer-scroll | 6.14 / 7.4 / 87% | 5.25 / 7.67 / 88% | 4.69 / 7.06 / 95% |
| scroll-info | 5.85 / 9.98 / 72% | 5.61 / 9.42 / 71% | 5.38 / 9.01 / 82% |
| scroll-settings | 5.86 / 9.52 / 75% | 5.89 / 10.11 / 72% | 5.77 / 9.93 / 75% |
| scroll-docker | 4.18 / 5.84 / 99% | 4.87 / 5.84 / 100% | 4.71 / 5.54 / 100% |
Round 12 (no patch) for comparison, runs A | B: cr-scroll 8.05 / 78% |
7.98 / 77%, cr-sweep 7.73 / 73% | 7.27 / 89%, planning+stream+scroll
8.39 / 51% | 7.83 / 70%, term-stream+scroll 7.98 / 78% | 7.98 / 72%.

## What the patch changed, and what it didn't

The render counts per scenario are the same in C and D (cr-scroll:
workbench every frame, log about 300, log-activity and log-tail about 230,
mindmap-map about 190, kanban-stack about 210). So the spread between C and
D is the machine, not the client. Against round 12, the CR mix moved from
p95 ≈ 8.0 ms / ≈ 77% to 7.1-7.7 ms / 82-98%:
- with the drawer cached (F151, which needs (a)), a chat's or a pane's notify
  no longer redraws the drawer (scroll-settings in A: drawer 561 renders in
  561 frames; in C and D: none);
- clean cached children inside a view that redraws are replayed instead of
  rendered.

It doesn't reach 95% everywhere. In every scenario that is still over
budget, the frame is spent in views that are themselves dirty:
- **The Workbench, every frame.** It is the ancestor of whatever changed
  (a log, a map under the wheel, the activity spinner, the terminal).
  gpui marks every ancestor of a notified view dirty, and a dirty view
  renders.
- **Under the wheel**, the log's visible rows, the kanban stack's cards and
  the map's ideas. These are elements, not cached views, so neither (a) nor
  (b) applies: a list's scroll notifies its view, which builds its visible
  rows again.
- **Settings and Info (F62).** Their sections are elements in a gpui
  `list`, not cached views, so (b) can't replay them (F130 assumed they
  were views).

## What else would reach the budget

1. **(c) in GPUI: a dirty view without dirty ancestors.** Replay the clean
   ancestors and splice the dirty child's new prepaint and paint in, instead
   of rendering every view above it. That saves the Workbench's and the
   Shell's render and layout on every frame. It's the largest piece left,
   and the biggest GPUI change: ranges become a tree.
2. **Rows as cached views where a list scrolls**: a kanban card, a mind-map
   idea, a Settings or Info section. Each would have a measured height, so
   its cached style is definite. With (b) they're replayed moved instead of
   rebuilt: a scroll costs the list's layout and the replay. The log's
   blocks are already single measured elements (BlockEl), so for the log the
   gain is smaller. These are area changes: Planning for kanban and the map,
   Operations for Settings and Info.
3. **term-stream+scroll**: the terminal's screen and the log under the
   wheel both redraw every frame. The terminal's batching is at one feed per
   frame already, so the gain would have to come from 1 and 2.

## The "white browser" the owner saw

It came from the measurement runs, not the client's paint path:
`tools/build.sh --release` doesn't build the browser host, and the runs'
`.bin/release` had no `crundi-browser-host`. The client log says "the
browser host did not start: … is not next to the app", and the drawer
showed its empty white page. The harness waited 30 s for the page's frames
and then measured anyway. The page it serves is on 127.0.0.1 on the PC
itself, so it was always reachable; the URL simply never loaded. Fixed in
`tools/measure-pc.mjs`:
- it refuses to run the drawer's scenarios without the host beside the
  client (`tools/stage-browser.sh --release`; or `MEASURE_DRAWER=0`);
- it fails if the drawer's page never shows;
- it logs the page's title and frames on its `drawer` line.

With the host staged, on the display, under Wayland: the page loads (title
"verify"), its frame reaches the app (`browserHost.frames` 1, the tab's
frames 1), and `idle-drawer` draws 0 frames. Those frames are drawn through
the same image path as every picture in the window. A screenshot of the display from here wasn't
possible: GNOME refuses `ScreenshotArea`, and an X11 grab of XWayland gives
black for a GPU window.
