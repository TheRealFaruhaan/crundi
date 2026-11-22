# Architect, F243 and F244: planning+stream+scroll, and the idle sidebar

farhan-RP-15, GNOME Wayland, a 1920x1040 window on the PC's display, release
builds (`tools/build.sh --release`), one window at a time, closed by its PID.
`tools/stub-server.sh 4886` fresh for each run, a fresh config folder each
run, `MEASURE_QUICK=1 MEASURE_QUICK_TERM=1 MEASURE_DRAWER=0`. Each run
started only once the 1-minute load was below 4 (3.0-3.99 at every start;
the owner's Electron client and other worktrees were running). Builds:

- **base**: be72ff56, the commit of the earlier 97-98% measurements
  (round 13's B/C), from `git archive`.
- **head**: 84251ba2, this step's starting HEAD.
- **dc7eff5c**, **f87be7e8**: bisect points (below).
- **xon / xoff**: head plus the sidebar fix, with the planning area's find
  marks on (`xon`) or skipped at run time (`xoff`, an env switch).
- **cur**: this step's final tree (the find-marks change and the F244 fixes).

p95 ms / share of frames ≤ 7 ms (p50 in brackets where it matters).

## 1. Interleaved base and head (b1 h1 b2 h2 b3 h3)

| run | planning+stream+scroll | cr-sweep | cr-scroll | term-stream+scroll |
|---|---|---|---|---|
| base b1 | 6.97 / 95% (p50 4.92) | 6.83 / 96% | 7.27 / 92% | 6.46 / 99% |
| head h1 | 7.49 / 86% (p50 6.01) | 6.78 / 97% | 7.01 / 95% | 6.51 / 99% |
| base b2 | 6.83 / 97% (p50 4.68) | 6.29 / 100% | 6.92 / 96% | |
| head h2 | 6.92 / 96% (p50 5.79) | 6.84 / 97% | 7.02 / 94% | |
| base b3 | 6.79 / 97% (p50 4.57) | 6.59 / 97% | 7.35 / 92% | |
| head h3 | 7.34 / 89% (p50 5.92) | 6.78 / 96% | 6.99 / 95% | |

Base was better in every pair: p50 4.6-4.9 ms against 5.8-6.0 ms, at the same
render counts (log ~200, mindmap-map 130-170, kanban-stack ~90 in ~500
frames). So it is the code, not only the conditions.

## 2. Looking for the change

The suspect was F202's find marks (59bc8daa): every kanban card and map
idea at rest put its words' marks in place in its prepaint on **every**
frame, each mark inside two nested element namespaces and its own global
id and element state, plus a String made per item.

| run | planning+stream+scroll |
|---|---|
| xon1 (marks on) | 6.91 / 96% (p50 5.76) |
| xoff1 (marks off) | 7.06 / 95% (p50 5.75) |
| xon2 (marks on) | 7.53 / 85% (p50 5.31) |
| (xoff2 overlapped a build: discarded) | |

Not a clear difference in those runs. Bisect points, interleaved:

| run | planning+stream+scroll |
|---|---|
| dc7eff5c d1 | 7.26 / 91% (p50 5.82) |
| f87be7e8 f1 | 7.14 / 93% (p50 4.68) |
| dc7eff5c d2 | 7.17 / 93% (p50 4.59) |
| f87be7e8 f2 | 7.12 / 93% (p50 4.70) |

Both intermediate builds sit between base and head, and the commits from
be72ff56 to dc7eff5c (Desktop's lock and dialogs, the composer's Enter
switch, ref_drag's edge state, live.rs's test, Operations) hold nothing on
this scenario's render path. In this noise, a bisect to one commit would take
many more runs per point than the gaps between builds allow.

## 3. The cut: find marks put in place in one state per drawing

`crundi_widgets::find::words_at` (find.rs): many drawings' word lists
(`find::Words`, laid out with the drawing and cached with it) under **one**
element state. A list that was not laid out again (`Rc::ptr_eq`) keeps its
marks, so per frame each word costs one cell set: no namespace, no global
id, no state lookup and no String. The map calls it once for all its ideas
and ghosts (by list and occurrence, since ghosts share one drawing). A
kanban card calls it once, under its id hashed to an integer. What Cru's
highlight finds is unchanged.

## 4. The final tree (cur), interleaved with base

| run | planning+stream+scroll | cr-sweep | cr-scroll | term-stream+scroll |
|---|---|---|---|---|
| cur (profiled run, numbers before the sampler) | 6.73 / 97% (p50 4.46) | 6.42 / 99% | 6.75 / 97% | 6.46 / 98% |
| cur c1 | 6.76 / 97% (p50 4.83) | 6.52 / 98% | 6.76 / 97% | 6.42 / 99% |
| base b4 | 6.92 / 96% (p50 5.28) | 6.39 / 98% | 6.97 / 96% | 6.37 / 99% |
| cur c2 | 7.12 / 93% (p50 4.84) | 6.74 / 96% | 6.92 / 96% | 6.36 / 99% |
| base b5 | 7.18 / 92% (p50 5.96) | 6.63 / 97% | 7.43 / 91% | 6.33 / 99% |
| cur c3 | 6.85 / 96% (p50 4.62) | 6.64 / 98% | 6.98 / 95% | 6.49 / 98% |
| cur c4 | 6.59 / 98% (p50 4.49) | 6.40 / 99% | 6.61 / 97% | 6.43 / 99% |
| cur c5 | 7.04 / 94% (p50 4.86) | 6.37 / 99% | 6.75 / 97% | 6.41 / 99% |
| cur c6 | 7.33 / 88% (p50 6.24) | 6.12 / 99% | 7.14 / 93% | 6.48 / 99% |

Means of the share ≤ 7 ms for planning+stream+scroll: **cur 95.4%**
(7 runs), **base 95.4%** (5 runs), head 90.3% (3 runs). The final tree is
back at base's level, and cr-sweep and cr-scroll are at or above base. It
did not make **3 runs in a row ≥ 95%**: c2 (93%) broke the first streak, and
c5 (94%) and c6 (88%) came after c4 (98%).

## 5. What the frames over 7 ms are

- Same build, same work: c4 98% and c6 88%. The slow runs draw *fewer*
  frames with more in each: c6 had 486 frames, the map rendered 124 times,
  kanban-stack 66 and the log 224. c4 had 533 frames, with 173, 88 and 203.
  The good base runs, too, had 158-170 map renders. How the stub's stream
  batches line up with the wheel decides whether a frame carries the map's
  pan and a log update together. That is why p50 jumps by ~1.4 ms from one
  run to the next with no code change.
- The sampled profile (`PROFILE_AT=planning`, 600 samples, gdb on the UI
  thread; perf/README.md) has no single hot spot. Of 293 busy samples,
  ~180 are GPUI's own frame pipeline under the root (taffy layout of
  the tree, scene sorting and insertion of quads, paths and sprites,
  hitboxes), 35 are pane bodies' paint (mostly the replay of their
  cached scene), 25 the kanban stack and 13 the pane heads (replay). The
  sampler's stacks are cut at ~25 frames, so the deepest work can't be
  pinned to a view.
- The next cut for this scenario is structural, not a hot spot: fewer
  primitives replayed per frame (the pane bodies and heads are re-inserted
  into the scene every frame even when replayed), or the map's pan drawn
  by moving its cached content instead of rendering MapView again. That is
  GPUI work (scene replay by reference instead of by copy).

## 6. F244: the sidebar in workspace.mjs's idle window

A trace on every notify path of the Sidebar, and on GPUI's reasons for
rendering a cached view again, showed the one render: `dirty=true` after
the Inbox badge went **0 → 1 unread**, ~1.2 s into the idle window. The
message was the test server's own: "Claude Code 2.1.330 is available (you
have 0.0.0)". src/webapp.js checks the Claude CLI's version 60 s after
the server starts, and the stub (tools/claude-stub.mjs) answered
`--version` with 0.0.0. So a real new Inbox message landed in whichever
suite's idle window was open about a minute in. The fix is at that source:
the stub now reports 999.0.0, which no release is newer than, so the test
server never posts the notice. (This also explains idle and pump flakes
that move with timing: F105's Inbox pump during idle, F122.)

Also changed, from the same look: the Sidebar's render now reads no entity
(rule 1). It read Projects (the icons) and the Inbox badge in render, and
GPUI redraws a view whenever an entity it read in render notifies. Projects
notifies on every terminals change, most of which the sidebar doesn't
show. The observers now copy the icons and the badge into the Sidebar, and
it redraws only when they changed.

## Checks on the final tree (debug, Xvfb, a fresh stub server each)

- workspace.mjs: 175 passed, 0 failed, 3 runs of 3 (before the fix: the
  idle check failed with `drew: {sidebar: 1}` in each run).
- planning.mjs ONLY=kanban,kanban-look,mindmap,mindmap-look: 174 passed.
- shell.mjs: 34 passed.
- desktop.mjs ONLY=cru,inbox (dbus-run-session): 120 passed, 1 failed:
  F211's ring check, which compares screenshots with ImageMagick; this PC
  has no xwd/convert (every snapshot there fails the same way), so it read -1.
- tools/check.sh clean; `tools/build.sh test --workspace` passes.

Raw (on farhan-RP-15, not kept): /tmp/f243-runs/*/report.json, prof/samples.txt.
