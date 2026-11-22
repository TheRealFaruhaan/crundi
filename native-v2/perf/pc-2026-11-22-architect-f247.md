# Architect, F247 and F213 item 2: the wide-clip model (GPUI patch (e))

farhan-RP-15, GNOME Wayland, a 1920x1040 window on the PC's display,
release builds (`tools/build.sh --release`), one window at a time, closed by
its PID. `tools/stub-server.sh 4886` fresh for each run, a fresh config
folder each run, `MEASURE_QUICK=1 MEASURE_QUICK_TERM=1 MEASURE_QUICK_PAN=1
MEASURE_DRAWER=0` (`MEASURE_QUICK_PAN`, new: the Mindmap tab's pan and the
idle after it in a quick run). Each run started once the 1-minute load was
below 4 (3.28-3.99 at every start). Builds:

- **h**: the step's starting HEAD bef84fa0, unchanged.
- **g**: the final tree (vendor/gpui/PATCH.md (e); the grid's wide clip
  while a pane is moved; the map split into its frame and a cached
  content with an adaptive margin).
- **e**, **f**: earlier versions of this tree, kept below for what they
  showed (two e runs that measured a stale binary, below, are left out).

p95 ms / share of frames ≤ 7 ms (p50). pan-mindmap renders: mindmap-map
(the content) / mindmap-frame (the frame, g only) of the scenario's frames.

## The final tree (g) against h

Order: f1 h8 f2 h9 f3 h10, then g1 h11 g2 h12 g3 h13 (interleaved).

| run | planning+stream+scroll | cr-sweep | cr-scroll | term-stream+scroll | pan-mindmap | idle-planning | pan-mindmap renders (map/frame) | p+s+s mindmap-map |
|---|---|---|---|---|---|---|---|---|
| h8 | 6.69 / 98% (p50 5.16) | 6.07 / 100% (p50 4.41) | 6.48 / 98% (p50 4.71) | 6.11 / 100% (p50 4.32) | 5.29 / 100% (p50 3.73) | 0 | 347/- of 347 | 153 of 514 |
| h9 | 6.84 / 96% (p50 5.48) | 6.52 / 98% (p50 5) | 6.23 / 99% (p50 4.65) | 6.09 / 100% (p50 4.25) | 5.36 / 100% (p50 4.44) | 0 | 340/- of 341 | 142 of 516 |
| h10 | 6.69 / 98% (p50 5.42) | 6.46 / 98% (p50 4.63) | 6.56 / 97% (p50 4.81) | 6.22 / 100% (p50 4.4) | 5.21 / 100% (p50 3.93) | 0 | 347/- of 348 | 144 of 506 |
| h11 | 6.56 / 99% (p50 4.79) | 6.24 / 99% (p50 4.57) | 6.02 / 99% (p50 4.33) | 6.05 / 99% (p50 4.36) | 5.36 / 100% (p50 4.8) | 0 | 314/- of 315 | 156 of 515 |
| h12 | 6.52 / 99% (p50 4.72) | 6.08 / 100% (p50 4.49) | 6.88 / 96% (p50 5.09) | 5.95 / 99% (p50 4.32) | 5.33 / 100% (p50 4.07) | 0 | 309/- of 309 | 158 of 514 |
| h13 | 7.06 / 94% (p50 5.89) | 6.38 / 99% (p50 4.94) | 6.18 / 99% (p50 4.32) | 5.94 / 100% (p50 4.17) | 4.99 / 100% (p50 3.8) | 0 | 340/- of 341 | 121 of 507 |
| f1 | 7.06 / 94% (p50 5.72) | 6.34 / 99% (p50 4.51) | 7.06 / 95% (p50 5.35) | 6.02 / 100% (p50 4.33) | 5.55 / 100% (p50 4.52) | 0 | 348/357 of 357 | 130 of 499 |
| f2 | 6.6 / 99% (p50 4.84) | 6.25 / 99% (p50 5.02) | 6.67 / 97% (p50 4.8) | 6.21 / 100% (p50 4.5) | 5.62 / 100% (p50 4.55) | 0 | 354/361 of 361 | 151 of 525 |
| f3 | 7.23 / 92% (p50 5.93) | 6.63 / 97% (p50 4.62) | 6.27 / 99% (p50 4.55) | 6.16 / 99% (p50 4.29) | 5.57 / 99% (p50 4.5) | 0 | 343/352 of 353 | 123 of 500 |
| g1 | 6.45 / 99% (p50 4.87) | 6.23 / 99% (p50 4.31) | 6.68 / 96% (p50 4.63) | 5.9 / 100% (p50 4.08) | 5.44 / 100% (p50 3.9) | 0 | 339/345 of 346 | 134 of 528 |
| g2 | 7.01 / 95% (p50 5.07) | 6.39 / 99% (p50 4.67) | 6.83 / 96% (p50 4.89) | 6.15 / 99% (p50 4.34) | 5.47 / 100% (p50 4.58) | 0 | 329/335 of 336 | 145 of 502 |
| g3 | 6.99 / 95% (p50 5.16) | 6.36 / 99% (p50 4.56) | 6.52 / 98% (p50 4.82) | 6.02 / 100% (p50 4.17) | 5.17 / 100% (p50 3.89) | 0 | 339/345 of 346 | 139 of 505 |

- **cr-sweep, term-stream+scroll**: parity (g p95 6.23-6.39 / 5.90-6.15
  against h11-13's 6.08-6.38 / 5.94-6.05).
- **pan-mindmap**: p50 parity (g 3.89-4.58, h 3.80-4.80), p95 5.17-5.47
  against 4.99-5.36 (about +0.1 ms). 100% under 7 ms in every run.
- **planning+stream+scroll**: g 99/95/95% (p95 6.45-7.01) against h11-13's
  99/99/94% (6.52-7.06); all twelve h runs here and before: 94-99%, mean
  97.6%. **cr-scroll**: g 96/96/98% (6.52-6.83) against h's 96-99% (mean
  97.9%). Within h's own run-to-run spread, but not better: a small cost
  can't be ruled out from three pairs.
- **Idle**: idle-planning 0 frames in every run, g and h.

So the patch buys no frame time in the CR scenarios, and costs none that
these runs can tell from noise. What it buys is work not done: a pane
entering across the grid's edge during an edge scroll and a small pan of
the map (both below) are replayed instead of drawn on every tick.

## What changed, and why it ended where it did

1. **Patch (e)** (PATCH.md): a wide clip records past itself, cuts
   primitives, hitboxes and layers to the real clip on insertion and on
   every replay, and patch (b)'s reveal check tests against where the last
   scene is whole (now tracked per view, `recorded`: it moves with each
   replay and is cut to the mask there; before, the check compared against
   the mask, which after a few moves no longer matched what the scene
   held). A view that rendered while a cached child was skipped records
   only what it showed (else a Kanban pane's out-of-view columns would stay
   blank when it is replayed into view). A clip that turns wide or plain
   leaves views wholly in view cached (`rekeyed`).
2. **e1 (the first version)**: the grid's clip always wide, the map's
   margin half its view (at most 400 px). term-stream+scroll, which has no
   map, went from h's 5.99-6.13 to 6.64 ms: a pane partly in view replays
   its whole body on every frame. planning+stream+scroll fell to 88%.
3. **e4-e6**: the grid's clip wide only while a pane is moved, the map's
   margin a quarter of its view (at most 200 px). Against h4/h6/h7:
   cr-scroll 95/95/95% (p95 6.91-7.2) against 98/98/99% (6.34-6.72);
   term-stream+scroll +0.15 ms. In a wheel sweep the content still renders
   every 2-3 frames, each time over 1.5-2.3x the area, and rendering the
   visible map was already about as cheap as replaying it:

| run | planning+stream+scroll | cr-sweep | cr-scroll | term-stream+scroll | pan-mindmap | idle-planning | pan-mindmap renders (map/frame) | p+s+s mindmap-map |
|---|---|---|---|---|---|---|---|---|
| h1 | 6.31 / 99% (p50 4.63) | 6.2 / 100% (p50 4.83) | 6.48 / 98% (p50 4.53) | 6.04 / 100% (p50 4.22) | 5.45 / 100% (p50 4.19) | 0 | 348/- of 348 | 166 of 517 |
| h2 | 6.52 / 99% (p50 5.51) | 6.16 / 99% (p50 4.61) | 6.56 / 98% (p50 4.83) | 6.13 / 99% (p50 4.34) | 5.25 / 100% (p50 3.86) | 0 | 346/- of 346 | 141 of 498 |
| h3 | 7.06 / 94% (p50 5.77) | 6.2 / 100% (p50 4.7) | 6.85 / 96% (p50 4.84) | 6.03 / 100% (p50 4.26) | 5.42 / 100% (p50 3.89) | 0 | 356/- of 357 | 130 of 493 |
| h4 | 6.24 / 99% (p50 4.64) | 6.18 / 99% (p50 4.65) | 6.72 / 98% (p50 4.99) | 6.08 / 100% (p50 4.32) | 5.43 / 100% (p50 4.37) | 0 | 356/- of 356 | 177 of 529 |
| h6 | 6.6 / 99% (p50 5.64) | 6.49 / 98% (p50 4.42) | 6.49 / 98% (p50 4.53) | 5.99 / 99% (p50 4.22) | 5.5 / 100% (p50 4.22) | 0 | 329/- of 330 | 130 of 503 |
| h7 | 6.68 / 98% (p50 4.5) | 6.75 / 97% (p50 4.78) | 6.34 / 99% (p50 4.61) | 6.1 / 99% (p50 4.3) | 5.2 / 100% (p50 3.79) | 0 | 351/- of 351 | 180 of 531 |
| e4 | 6.78 / 97% (p50 4.53) | 6.43 / 98% (p50 4.71) | 6.91 / 95% (p50 4.63) | 6.23 / 99% (p50 4.48) | 5.55 / 100% (p50 4.1) | 0 | 103/349 of 350 | 95 of 515 |
| e5 | 6.62 / 98% (p50 4.7) | 6.47 / 98% (p50 4.8) | 7.2 / 95% (p50 5.04) | 6.22 / 100% (p50 4.45) | 5.55 / 100% (p50 4.06) | 0 | 102/342 of 343 | 95 of 524 |
| e6 | 7 / 95% (p50 4.87) | 6.13 / 99% (p50 4.29) | 7.03 / 95% (p50 4.72) | 6.21 / 99% (p50 4.34) | 5.47 / 100% (p50 3.72) | 0 | 100/338 of 338 | 84 of 512 |

4. **f1-f3**: `Scene::draw` copies a primitive straight into its list
   (term-stream+scroll back to parity), and the margin only when pans
   don't follow each other within 20 ms (a wheel sweep or a touchpad): the
   content renders at the view's size during a sweep and once with the
   margin 150 ms after. pan-mindmap stayed +0.3 ms p95: in a sweep the
   content rendered every frame as a second view.
5. **g1-g3 (final)**: during a fast pan the frame builds the content inline,
   as one view, as before the split; the cached content (and its replay)
   only for slower and smaller pans.

## The two cases (the suites, under xvfb)

- workbench.mjs, the chat entering across the grid's left edge during an
  edge scroll (18-20 frames): log 1, composer 1, mini-panes 1, chat-mini 1
  (their one settle), from 15/15/14/14 at HEAD. Leaving: 0 (as F241).
- planning/mindmap.mjs, a 120 px pan in three wheel ticks 30 ms apart:
  mindmap-map 0 renders, mindmap-frame 3.

## A stale client (fixed in tools/build.sh)

Two e runs first measured the baseline's code: a release build of the
baseline tree failed part way (a missing asset), after compiling its crates
into the shared folder, and `build.sh` wrote its last-worktree marker only
after a successful build. So the next build from this tree touched nothing
and cargo kept the baseline's newer artifacts (and it never touched
`vendor/`). `build.sh` now writes the marker before cargo runs (and again
after it succeeds, so a build from the same tree touches nothing), and
touches `vendor/` with `crates/`.

## Checks on the final tree

- `tools/check.sh` clean; `tools/build.sh test --workspace` passes
  (gpui_patch: 10 tests, 7 new: entering replays and presses cut to the
  clip, a pan within the margin, a skipped child, a clip turning wide, and
  the two downstream checks of (a) below).
- Under xvfb against fresh stub servers: workbench 61/0, chat-log 66/0,
  planning (kanban, kanban-look, mindmap, mindmap-look, drops) 205/0,
  workspace 192/0. One full planning run of the near-final tree failed
  two drops checks (a Mindmap idea's drag didn't start after the map was
  panned to it); the same tree passed planning in full twice more and
  mindmap+drops and drops alone once each: a flake that the patch's map
  pan may have made likelier, not reproduced.

## Downstream checks of patch (a)

- radiosilence/koan#2031: a child made by its parent with `cx.new`,
  reused while the parent and the root render again, then notifying: the
  window draws on its own and the child renders
  (`a_child_made_in_render_and_reused_still_wakes_the_window`). Our
  per-scope access sets already cover it (with the reused view's
  `extend_accessed` removed, that test and five others fail).
- MaestrIA-rs#52: a cached view under an `invisible()` parent (hidden, the
  parent rendering again and reused whole, the leaf changing, shown again)
  and a `deferred` cached view in a cached ancestor reused whole (and
  moved): no panic, and each frame batches as a render does
  (`a_cached_view_under_an_invisible_parent`,
  `a_deferred_cached_view_in_a_reused_ancestor`). Nothing to fix.
