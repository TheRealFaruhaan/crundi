# Architect, F241: GPUI patch (b) for views partly or wholly outside their clip

farhan-RP-15, GNOME Wayland, a 1920x1040 window on the PC's display, release
(`tools/build.sh --release`), one window at a time, closed by its PID.
`tools/stub-server.sh 4886` fresh each run, a fresh config folder each run,
`MEASURE_QUICK=1 MEASURE_QUICK_TERM=1 MEASURE_DRAWER=0` (the CR's scenarios
and term-stream+scroll). Each run started only once the 1-minute load was
below 4 (it was 3.5-3.9 at every start: the owner's Electron client and other
worktrees were running). **base** is this branch's HEAD (the gpui patch
before F241), built from this tree with the F241 vendor changes reversed.
Runs interleaved: b1 m1 b2 m2 b3 m3, then b4 n4 b5 n5 b6 n6.

- **m**: the first F241 build. Its skip check scanned every cached view's
  scene operations on every frame, before any cheap test. That cost frames
  everywhere (planning+stream+scroll 74-85%).
- **n**: the final build. The scene is scanned only when the view's bounds
  alone already pass (bounds outside the clip, or a move that reveals
  nothing of the bounds). In the common case the check is a few comparisons.

p95 ms / share of frames ≤ 7 ms. No frame over 16 ms in any run.

| run | cr-scroll | cr-sweep | planning+stream+scroll | term-stream+scroll |
|---|---|---|---|---|
| base b1 | 7.23 / 94% | 6.73 / 98% | 6.70 / 98% | 6.97 / 95% |
| base b2 | 6.98 / 95% | 6.63 / 97% | 7.11 / 92% | 7.08 / 94% |
| base b3 | 6.96 / 95% | 7.09 / 94% | 7.47 / 85% | 6.83 / 96% |
| base b4 | 7.12 / 94% | 6.18 / 99% | 7.20 / 91% | 6.95 / 96% |
| base b5 | 7.31 / 92% | 6.56 / 97% | 7.27 / 91% | 6.86 / 97% |
| base b6 | 7.12 / 94% | 6.56 / 98% | 7.13 / 94% | 6.88 / 96% |
| m1 (first) | 7.46 / 90% | 7.26 / 89% | 7.97 / 74% | 7.05 / 94% |
| m2 (first) | 7.39 / 91% | 6.73 / 98% | 7.52 / 85% | 7.00 / 95% |
| m3 (first) | 7.09 / 94% | 7.63 / 89% | 7.91 / 82% | 6.88 / 96% |
| **n4 (final)** | 6.92 / 96% | 6.41 / 99% | 6.80 / 98% | 6.45 / 99% |
| **n5 (final)** | 7.02 / 95% | 6.52 / 98% | 7.24 / 90% | 6.43 / 99% |
| **n6 (final)** | 6.73 / 97% | 6.59 / 99% | 7.00 / 95% | 6.41 / 99% |

Means of the share ≤ 7 ms, final vs. base (all six base runs):
cr-scroll 96% vs 94%, cr-sweep 99% vs 97%, planning+stream+scroll 94% vs 92%,
term-stream+scroll 99% vs 96%.

term-stream+scroll is the clear gain. In this layout the terminal's pane is
out of the grid's view, so its screen is now skipped: term-screen and
term-cursor render 0 times (base: 2151-2203 each in ~2250 frames), and
pane-body renders about once a frame instead of twice. This is F205's
scenario, now 99% ≤ 7 ms and p95 6.4 ms.

## Idle (one full run of the final build, same set-up without MEASURE_QUICK)

| scenario | frames |
|---|---|
| idle-open (8 s) | 0 |
| idle-end (8 s) | 0 |
| idle-planning (6 s) | 0 |
| idle-terminal (6 s) | 1 (4.1 ms) |

The same run: scroll-panes 100%, scroll-chat 100%, stream+scroll 99%,
term-stream+scroll 99%, scroll-kanban 100%, pan-mindmap 99%, cr-sweep 98%,
cr-scroll 95%, planning+stream+scroll 86% (p95 7.55).

## planning+stream+scroll (F240)

It is under 95% in some runs of every build: base 85-98% (mean 92%),
final 90-98% (mean 94%; 86% in the full run). The work is the same in every
run of every build. Its renders are equal to within a few: workbench ~510,
log ~205 (the stream), mindmap-map 125-165, kanban-stack ~87, term-screen
~75 of ~510 frames. So the spread is frame time at the same work (load ~3.8,
other processes on the PC), not something drawn more often.

The suspect, Workspace's `find::marks` in terms.rs, runs only when the
terminal's screen renders: in ~15% of this scenario's frames, the same count
in base and final. It cannot account for 5-15% of frames over 7 ms on its own,
and base and final don't differ there. I did not find one cause for
the remaining frames over 7 ms. The frames that render are the map under the
wheel (region:mindmap p95 6.0-7.5) together with the streaming log. To go
further takes a sampled profile of this scenario (`PROFILE_AT=planning`),
not another A/B.

## What changed (vendor/gpui, PATCH.md (b))

- A moved cached view that is partly clipped is replayed when the move
  brings into view nothing its clip cut off (`reveals_nothing`, on its
  bounds and on what its last paint drew unclipped). A pane leaving across
  the grid's edge, or moving further out, is replayed; one coming in renders.
- A cached view wholly outside its clip (its extent included) is neither
  rendered nor painted, even when it is dirty. It renders when it is in view
  again or the window refreshes. Not skipped: a view holding the focus, one
  with a deferred draw, one that changed size.
- Workbench suite (debug, Xvfb): with the chat leaving across the grid's
  left edge during an edge scroll, base renders log 14, composer 14, Mini
  13 in 19 frames; the final build renders 0 of each in 15 frames.

Raw (on farhan-RP-15, not kept): /tmp/f241-runs/{b1..b6,m1..m3,n4..n6,full-n}/report.json.
