# The shared frame cost on the owner's PC (F51, F55, F53), 20 Nov

Architect, round 6. Release builds of the run's HEAD 7010d59d ("base", plus
only the harness's CR layout hook, `measure-layout <stream> cr …`) and of
this round ("new"), with frame pointers (`CRUNDI_BUILD_JOBS=12
RUSTFLAGS='-C force-frame-pointers=yes' tools/build.sh --release`), on
farhan-RP-15's own display (1920x1040 window, Wayland, 144 Hz, Renoir), one
window at a time, a fresh config folder each run, against
`tools/stub-server.sh 4884` (a fresh server each run, no real Claude).
`tools/measure-pc.mjs`, every scenario. The owner's Electron client ran
throughout (load average 3.2–4).

Windows on the PC's display (each closed by its PID by the harness):
12:26:13–12:29:43 (base1), 12:36:16–12:40:08 (base, journal),
12:46:22–12:52:24 (base, journal and sampler), 12:58:38–13:02:29 (new
step 1, journal and sampler), 13:06:29–13:10:22 (new step 2, journal),
13:10:33–13:13:00 (sampler, terminal), 13:14:47–13:16:51 (a run stopped:
background tasks run at nice 5; numbers not used), 13:17:04–13:20:57 (base2),
13:20:59–13:27:04 (new1), 13:27:09–13:30:59 (base3), 13:31:02–13:34:54
(new2), 13:34:59–13:37:06 (7 panes, base), 13:37:06–13:39:13 (7 panes,
new), 13:58:23–14:04:29 (new3, the final build). About 55 minutes.

## The change request's own scenario, now in the harness

`measure-pc.mjs` ends with it (MEASURE_CR=0 skips it): five panes, the
Kanban over a chat, the Mindmap over the shell, the streaming chat (the
cities reply, about 19 s with the stub, sent again for each part) in the
third column; the other seeded chats are closed on the server first (every
running chat has a pane). `cr-sweep` is the wheel sweep over the window
while the chat streams; `cr-scroll` the wheel over the chat, the Kanban
and the map in turn.

## Numbers (p95 ms, share ≤ 7 ms)

| Scenario | base2 | base3 | new1 | new2 | new3 (final) |
|---|---|---|---|---|---|
| cr-sweep (the CR's five panes, a chat streaming) | 6.34, 100% | 6.49, 98% | 5.79, 100% | 5.91, 100% | 5.76, 100% |
| cr-scroll (the wheel over the chat, Kanban, map) | 6.70, 97% | 6.31, 100% | 5.79, 100% | 5.92, 100% | 5.55, 100% |
| planning+stream+scroll (8 panes, with the shell) | 7.64, 79% | 7.65, 85% | 6.74, 97% | 6.73, 98% | 6.42, 99% |
| term-stream+scroll | 7.31, 89% | 7.30, 90% | 6.64, 98% | 6.74, 98% | 6.75, 98% |
| term-stream | 6.52, 99% | 6.34, 99% | 5.93, 100% | 5.85, 100% | 5.73, 100% |
| scroll-panes | 5.58 | 5.88 | 5.70 | 5.41 | 5.29 |
| stream+scroll | 6.54, 98% | 6.40, 99% | 5.84, 100% | 5.79, 99% | 6.21, 99% |
| scroll-kanban / pan-mindmap | 3.99 / 5.19 | 3.81 / 5.10 | 3.80 / 5.13 | 3.99 / 5.06 | 3.98 / 5.13 |
| idle-open, idle-end, idle-planning | 0 frames | 0 | 0 | 0 | 0 |
| idle-terminal | 1 frame | 1 | 1 | 1 | 1 |

Seven panes, no terminal (`MEASURE_TERMINAL=0`, F55): base 6.70 / 97%, new
5.93 / 100%. new1/new2 are the paint layers and the leaner workbench; new3
adds the Shell's and the Notes tab's copies of what they lay out (F53).
The reports are in `pc-2026-11-20-architect/` (`report-*.json`).

## Where the frame went (base)

The frame journal (`--features frame-journal`, perf/README.md) splits each
draw: in the CR scenario a frame was ~0.8 ms request_layout, ~1.0 ms
prepaint, ~1.9 ms paint and ~1.5–2 ms present. No view of ours took
0.5 ms of its own in any phase, except the Workbench's request_layout now
and then. The sampler (pprof, 1 kHz) over the eight-pane sweep, UI thread:
present 20%, paint 16%, replaying clean views 14%, request_layout 10%,
the scene's sort 4%, taffy 4%, the mouse's hit test 3%.

The replay was the lever. GPUI copies every clean cached view's last
paint into the new scene each frame, and a primitive painted outside a
paint layer is inserted into the scene's bounds tree again to get its draw
order (scene.rs `insert_primitive`). The logs, the Kanban's lists, the map
and the terminal already painted in layers. The chrome did not: samples in
those inserts, by view, were the 8 pane heads 132, the 5 composers 78, the
5 logs 49, the project bar 27, the Mindmap's toolbar 24, the terminal
input 18, the sidebar 13, the Kanban's toolbar 12, the top bar 7. A head
cost more per frame than a whole log.

## What changed

- `crundi_widgets::layer` (layer.rs): the one paint-layer element (the
  chat's and planning's private copies are gone). In a layer: every pane
  head, the top bar (the Telegram dot on its icon in a nested layer of its
  own, so it stays over the icon), the sidebar, the project bar, the
  Kanban's toolbar, the Mindmap's toolbar buttons and project chips, the
  composer's buttons and meta row, the terminal input's buttons. After:
  heads 12 samples (from 132).
- `w::text_box` / `w::watch_focus`: a text box is in a layer only while
  the focus is not in it. A focused box draws its caret (a quad) over its
  text, and in a layer every quad goes under every glyph. The composer and
  the terminal input re-render on focus and blur, so the layer comes and
  goes with the caret.
- The Workbench: a pane's leaf is its panel's box (`w::panel_box`), one
  element less per pane, and the panel's inner wrapper is gone (two fewer
  per pane, each laid out on every frame that draws a pane).
- The Shell keeps what it lays out (signed in, the tab, the fold, the
  drawer) and the Notes tab keeps `tab_project`, each copied by an observer
  that notifies only on a change. Neither reads a store while rendering, so
  a Nav, Session or Notes-store notify that changes no layout draws no frame.
- ARCHITECTURE.md: a "Paint layers" section (the rule, what may go in a
  layer, text boxes, why the Workbench cannot be cached and stays lean).

Nothing on screen changed. The suites' screens from base and new (slice,
workspace, planning: 80 pictures, Xvfb, lavapipe) were compared pixel by
pixel. 59 are identical. The rest differ only where the runs differ: the
scroll position of a column, how far a stream had got, a toast still
fading, random webhook ids, the launcher's animation. The one difference
from the layers is one colour step (±1 of 255) on 6 anti-aliased pixels of
the focused pane's rounded corner. All suites pass with the final build on
the PC (Xvfb): slice 41, log-element 18, workspace 106, planning 289.
`cargo test` 191.

## What is left in a frame

GPUI's own per-frame work, which grows with what is on screen: present
(wgpu's command encoding and submit, ~20% of the UI thread), the scene's
sort, the replay of the clean views' primitives, and painting the views
that really changed (the scrolled log's glyphs, the streaming terminal's
~2,200 glyphs at ~150 ns each). In term-stream+scroll the terminal and the
scrolled log are each repainted every frame, and both have to be. All the
measured scenarios are now under 7 ms at p95 with no change to GPUI.

## F53 (the Shell re-rendering after a Notes page opens)

On the PC's release build under Xvfb (a probe every 100 ms for 3 s after
the 400-block page opens), the Notes tab's layout view renders only with
a drawn frame: 1 at the open, 1 at 0.6 s, 7 between 1.9 and 2.2 s, then
none. Those frames re-render no Notes view, only the uncached chain (Root,
Shell, the tab's layout). GPUI draws a frame when any entity read during
the last draw is notified, even if the view that read it does not
re-render. So some store a cached view reads in render is notified there
with no visible change. The Shell's and the tab's own reads are gone now.
The remaining frames are cheap (no text re-laid out) and stop by 2.2 s.
The debug build's 40/s was not reproduced here.
