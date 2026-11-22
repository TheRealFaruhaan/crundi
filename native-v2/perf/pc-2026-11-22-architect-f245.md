# Architect, F245: planning+stream+scroll, the scene replay cut

farhan-RP-15, GNOME Wayland, a 1920x1040 window on the PC's display, release
builds (`tools/build.sh --release`), one window at a time, closed by its
PID. `tools/stub-server.sh 4886` fresh for each run, a fresh config folder
each run, `MEASURE_QUICK=1 MEASURE_QUICK_TERM=1 MEASURE_DRAWER=0` (dfull:
the full run). Each run started only once the 1-minute load was below 4
(3.2-3.99 at every start; the owner's Electron client was running, and the
load sat at 5-13 between runs). Builds:

- **h**: this step's starting HEAD (ca8a4ffe), unchanged.
- **d**: HEAD plus GPUI patch (d), a replayed view keeps its draw orders
  (vendor/gpui/PATCH.md (d)). dp1 and dfull are d's final tree, with the
  `crundi_phases` diagnostic compiled in (off unless
  `CRUNDI_FRAME_PHASES=1`, which only dp1 set).

p95 ms / share of frames ≤ 7 ms (p50). Last column: mindmap-map /
kanban-stack / log renders in planning+stream+scroll's frames.

| run | planning+stream+scroll | cr-sweep | cr-scroll | term-stream+scroll | renders |
|---|---|---|---|---|---|
| h1 | 7.4 / 83% (p50 6.12) | 6.65 / 98% (p50 4.93) | 7.02 / 95% (p50 5.35) | 6.43 / 99% (p50 4.64) | 133/91/203 of 494 |
| d1 | 6.6 / 99% (p50 4.75) | 6 / 99% (p50 4.8) | 6.87 / 96% (p50 4.89) | 6.04 / 100% (p50 4.22) | 163/85/199 of 512 |
| h2 | 6.36 / 99% (p50 4.52) | 6.91 / 97% (p50 5.07) | 7.44 / 91% (p50 4.96) | 6.32 / 99% (p50 4.58) | 170/85/206 of 521 |
| d2 | 6.3 / 98% (p50 4.48) | 6.21 / 100% (p50 5.11) | 6.76 / 96% (p50 4.69) | 6.02 / 99% (p50 4.25) | 171/85/200 of 522 |
| h3 | 6.43 / 99% (p50 4.32) | 6.49 / 99% (p50 4.79) | 6.14 / 99% (p50 4.27) | 6.29 / 99% (p50 4.32) | 173/84/210 of 535 |
| d3 | 6.7 / 98% (p50 4.46) | 6.2 / 99% (p50 4.36) | 5.93 / 98% (p50 4.2) | 5.81 / 100% (p50 4.19) | 166/86/209 of 528 |
| dp1 | 5.89 / 100% (p50 4.28) | 6.23 / 99% (p50 4.37) | 5.95 / 99% (p50 4.4) | 5.99 / 100% (p50 4.25) | 165/86/225 of 542 |
| h4 | 6.21 / 99% (p50 4.25) | 6 / 100% (p50 4.19) | 6.91 / 96% (p50 4.81) | 6.38 / 99% (p50 4.37) | 169/85/222 of 540 |
| d4 | 6.67 / 98% (p50 5.05) | 5.72 / 100% (p50 4.21) | 6.46 / 99% (p50 4.57) | 5.82 / 100% (p50 4.23) | 153/86/201 of 512 |
| h5 | 6.41 / 99% (p50 4.38) | 6.45 / 98% (p50 4.87) | 6.56 / 98% (p50 5.04) | 6.31 / 99% (p50 4.32) | 173/88/211 of 544 |
| d5 | 6.34 / 99% (p50 4.55) | 5.82 / 99% (p50 4.27) | 5.75 / 100% (p50 4.25) | 5.9 / 100% (p50 4.11) | 163/87/201 of 526 |
| h6 | 6.6 / 98% (p50 4.59) | 6.37 / 99% (p50 4.36) | 6.83 / 96% (p50 4.67) | 6.3 / 99% (p50 4.23) | 173/84/204 of 524 |
| d6 | 6.19 / 100% (p50 4.4) | 6.15 / 99% (p50 4.37) | 6.84 / 96% (p50 4.53) | 5.93 / 100% (p50 4.17) | 172/87/206 of 534 |
| dfull | 6.79 / 97% (p50 5) | 6.58 / 98% (p50 4.76) | 7.12 / 94% (p50 5.46) | 5.88 / 100% (p50 4.12) | 155/86/204 of 512 |

Order: h1 d1 h2 d2 h3 d3, then dp1, then h4 d4 h5 d5 h6 d6, then dfull.

**planning+stream+scroll, share ≤ 7 ms:**
- d: 99, 98, 98, 100 (dp1), 98, 99, 100, 97 (dfull). **8 runs in a row ≥ 95%,
  mean 98.6%** (the 7 quick runs: 98.9%).
- h: 83, 99, 99, 99, 99, 98, mean 96.2%. One run in six fell to 83%.

Today's h is better than F243's (95.4% mean). The load gate was the same,
but HEAD now has F246's stub fix: the test server no longer posts its
Inbox notice a minute in. d's p95 is lower than h's in 4 of 6 pairs, and
its worst run is 97%, not 83%.

**The other scenarios (no worse; mostly better):**
- term-stream+scroll: d p95 5.81-6.04 ms (99-100%), h 6.23-6.43 ms (99%).
  F205's scenario, 7.41 ms in round 13, has the most replay per frame.
- cr-sweep: d p95 5.72-6.58, h 6.00-6.91.
- cr-scroll: d p95 5.75-7.12 (94-100%), h 6.14-7.44 (91-99%).

**Idle (dfull):** idle-open, idle-end and idle-planning drew 0 frames.
idle-terminal and term-stream each drew 1 frame, and it was `usage-rows`:
the top bar's usage refresh (F122), which F241's full run also shows. The
suites' idle checks (below) pass.

## Cut (a): replay by keeping orders, not by the bounds tree (PATCH.md (d))

`Scene::replay` (and patch (b)'s `replay_offset`) put every replayed
primitive through `insert_primitive`: its clipped bounds went into the
frame's R-tree for a draw order, leaving a leaf. With the panes, heads and
shell views cached, almost all of a frame's ~4,500 primitives are replayed
on every frame, each glyph a sprite. Now one pass reads the range's lowest
and highest order and its extent, one `insert_span` puts one leaf over the
extent, and each primitive keeps its order shifted by the same amount
(PATCH.md (d) argues why that keeps every overlap's outcome). A replay
inside an open layer keeps upstream's path. `gpui_patch.rs`'s new
`replayed_views_draw_in_the_same_order` checks that a frame replaying a
cached view (in place, moved, and moved again) batches exactly as the frame
that rendered it. A deliberately broken shift (every order the same) fails
it.

"By reference" in the strict sense (the new frame pointing at the old
frame's primitive slices) isn't possible in this scene: `finish` sorts each
kind's list by order, so a view's primitives aren't contiguous, and the
renderer needs one sorted list per kind. The per-primitive cost that is left
is a copy into two vectors.

**Where a frame goes now** (dp1, `CRUNDI_FRAME_PHASES=1`, means over 2 s
windows during the sweeps): request layout 0.3-0.45 ms, layout and prepaint
1.0-1.25, paint 0.9-1.15, finish (scene sort, frame swap) 0.35-0.5, present
0.9-1.3. That is ~4.1 ms for ~4,500 primitives and ~5,000 scene operations.
No single phase dominates.

## Cut (b): pan the map by moving its cached content (not done, and why)

- With d, the target is met: 8 runs in a row ≥ 95%, the worst 97%.
- In the workbench, the map's frames cost what the kanban's do. Wheeling
  over one region alone for 2 s, with the chat streaming, gave p50 / p95:
  - region:mindmap: 4.30-4.50 / 5.81-6.35 ms;
  - region:kanban: 3.90-5.13 / 5.35-6.36 ms.

  In planning+stream+scroll, the map renders on ~30% of frames (153-172
  of ~520). A frame that renders it costs no more than one that renders the
  kanban stack, the log or the terminal instead. The ~4 ms floor is the
  pipeline above, which every frame pays.
- What (b) would take: the map culls to its view and its clip cuts away
  everything outside. So a pan always reveals content the last frame's
  scene doesn't hold, and patch (b) rightly renders it (F241's reveal
  check). Replaying a moved map would need the scene to keep primitives
  drawn outside the clip: a recorded wider mask, cut to the real one on
  insertion and on every replay, plus a reveal check against the wider
  region. That is a second clip model in GPUI's scene and hitboxes, for a
  frame share that is already 98-100%.
- Where it would show: the Mindmap tab's own pan (pan-mindmap, a full-size
  map, not a CR scenario) has p50 4.48 / p95 5.57 ms against the Kanban
  tab's column scroll at 3.28 / 4.02 (dfull). So a moved-content map would
  save about 1 ms a frame there.

The kanban stack: its frames cost the same as the map's (above), and
kanban-stack renders on ~16% of planning+stream+scroll's frames (84-88), so
the profile does not point at it.

## Checks on the final tree

- `tools/check.sh` clean; `tools/build.sh test --workspace` passes (the
  shell's gpui_patch: 3 tests, the new one included).
- Under Xvfb, a fresh stub server each, debug build: workbench 60 passed,
  chat-log 66, workspace 175, planning 486, operations 430; 0 failed. (This
  PC has no xwd/convert, so suites print "no snapshot"; no check compares
  them.)

Raw (on farhan-RP-15, not kept): /tmp/f245/runs/*/report.json, client.log
(dp1 has the phase lines).
