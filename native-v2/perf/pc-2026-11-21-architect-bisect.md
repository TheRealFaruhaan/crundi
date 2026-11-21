# F99 / F143: the bisect and the first structural cut (the Architect, 21 Nov)

farhan-RP-15, the owner's display (Wayland, GNOME, 1920x1040 window, AMD
Renoir), release builds with frame pointers, one v2 window at a time, each
closed by its PID. Every run: a fresh `tools/stub-server.sh 4884`, a fresh
config folder, HEAD's `tools/measure-pc.mjs` (older binaries driven by the
same harness). The owner's Electron client ran throughout (~2 cores), as in
round 12. p50 / p95 ms / share ≤ 7 ms.

## 1. The commits (full runs, `MEASURE_DRAWER=0`)

| build | what it added | cr-scroll | cr-sweep | planning+stream+scroll | term-stream+scroll |
|---|---|---|---|---|---|
| 4deb5a41 | round 6 | 4.74 / 5.72 / 100% | 4.73 / 5.59 / 100% | 5.51 / 6.77 / 97% | 5.20 / 6.84 / 97% |
| 181eb694 | + Operations | 4.81 / 5.72 / 100% | 4.05 / 5.89 / 100% | 5.44 / 6.57 / 99% | 5.22 / 6.85 / 96% |
| dfb32bcc | + Desktop | 4.87 / 5.73 / 100% | 4.86 / 5.75 / 100% | 5.17 / 6.69 / 98% | 5.80 / 7.06 / 94% |
| 7acdefc4 | + d03673ce: the round-7 chat port and chrome | 5.77 / **7.50 / 83%** | 5.76 / 7.62 / 88% | 6.61 / 7.82 / 70% | 5.12 / 7.28 / 90% |
| 40ba8ecb | + round-8 merges | 5.66 / 7.30 / 91% | 5.84 / 7.24 / 92% | 5.43 / 8.21 / 64% | 5.37 / 7.48 / 87% |
| 8e44a94a (HEAD) | round 12's runs | 5.78 / 8.05 / 78%, 5.75 / 7.98 / 77% | | 7.83-8.39 | 7.98 |

Second runs, the same way:

| build | cr-scroll | cr-sweep | planning+stream+scroll | term-stream+scroll |
|---|---|---|---|---|
| dfb32bcc | 4.83 / 5.68 / 100% | 4.87 / 5.76 / 100% | 5.40 / 6.59 / 99% | 5.10 / 6.96 / 95% |
| 7acdefc4 | 4.65 / 7.15 / 93% | 5.58 / 7.00 / 95% | 5.97 / 7.79 / 75% | 5.55 / 7.36 / 90% |
| bac9b400 | 6.10 / 7.66 / 83% | 5.96 / 7.28 / 91% | 5.60 / 8.12 / 59% | 5.32 / 7.51 / 86% |

(7acdefc4 is d03673ce plus a test step that changed no crate. e5611489 was
not measured: the step is already pinned before it.)

The whole step is one commit: **d03673ce, the Architect's round 7** (the
chat's composer and log features ported, the shell's and workbench's chrome
of the UI review). Operations and Desktop cost nothing measurable. The
single-area scenarios hardly moved across it (scroll-chat 5.48 → 5.71,
scroll-kanban 4.01 → 4.03, pan-mindmap 5.08 → 5.22, scroll-panes 5.45 →
5.75): the ~1.8 ms is only where several panes draw together.

## 2. Inside d03673ce

Round 7's own bisect (perf/pc-2026-11-20-round7.md) put crates/chat back to
its base in the merged tree: cr-scroll 7.05-7.14 → 6.42-6.71. So about
0.6 ms is the chat crate, the rest the shell, workbench and widgets chrome.
Reverting the shell, the workbench and the widgets one at a time does not
compile (the chat depends on the new widgets and on `register_head`), so
the rest was pinned on HEAD instead, by ablation: a scratch build of HEAD
with switches that each drop one per-frame piece (`CRUNDI_ABLATE`, never
committed), quick runs (`MEASURE_QUICK=1`, new: the CR's scenarios only,
about 3 minutes, the shell's screen filled as the full run leaves it).

| dropped | planning+stream+scroll | cr-sweep | cr-scroll |
|---|---|---|---|
| nothing | 4.77 / 7.35 | 5.74 / 7.40 | 5.45 / 7.07 |
| the window's overlay layers | 5.96 / 7.28 | 5.56 / 7.12 | 4.87 / 7.19 |
| the top bar | 5.86 / 7.14 | 5.68 / 6.68 | 5.35 / 6.87 |
| the five pane heads | 5.88 / 7.14 | 5.53 / 6.65 | 5.67 / 7.06 |
| the composers | 4.89 / 6.66 | 5.49 / 6.61 | 5.30 / 6.83 |
| Mini's layers | 6.08 / 7.68 | 5.82 / 7.61 | 5.59 / 7.23 |
| what floats over each log (dock, jump, copy, pointer tracking) | 5.99 / 7.41 | 5.47 / 6.81 | 4.80 / 7.11 |
| **all six** | **4.08 / 5.69** | **4.82 / 6.08** | **4.06 / 6.02** |

No single piece moves p95 by more than the noise (±0.3 ms); all six
together take 1.05 ms off cr-scroll's p95 and 1.4 ms off its p50. So the
regression is not a bug in one place: it is the sum of what the round-7
chrome put on screen, paid on every frame.

## 3. Why every frame pays for it (journal + pprof, cr-scroll, quick)

Main thread, the share of samples inside each view's prepaint and paint
(its subtree included): LogView 10.3% (the streaming log and the chat under
the wheel), MapView 3.9%, **Composer 2.9%** (never re-rendered here: only
replayed), StackView 2.3%, **PaneHead 2.1%** (replayed), TermScreen 1.3%,
ProjectBar 1.3%, the map's Dots 1.0%, and a long tail of chrome at 0.3-0.9%
each. Present is 18% (21% with the chrome dropped).

GPUI 0.3.12 keeps no retained scene: each frame rebuilds it, and a clean
cached view is *replayed*, not skipped: its primitives are pushed again
(`Scene::replay`; outside a paint layer each is also inserted in the
frame's bounds tree: the focused composer's text box is outside one, for
its caret), its hitboxes, listeners and dispatch nodes copied, its text
layouts carried over (`reuse_layouts`), and at the end of the frame every
element state it used is moved to the next frame (`remove_entry` on
`(GlobalElementId, TypeId)`, 3.1% of the main thread, 1.1% with the chrome
dropped). So a frame costs: the dirty views' work, plus replaying everything
else on screen, plus presenting it all. The chrome added in round 7 added
to the second and third terms on every frame.

And caching is one level deep: a notify dirties the view and every view
above it, and a cached view that re-renders draws every cached view inside
it again (`window.refreshing`, view.rs). The Shell is the window's root and
renders each frame; the Workbench is not cached (its leaves are). A pane
made one cached view, as the Manager's step suggested, would draw its log,
composer, head and Mini again on every frame any one of them changes: more
work, not less. The leaves are already the cached level, side by side.

## 4. The structural cut made this round

- **The streaming tail** (crates/chat/src/log/tail.rs; ARCHITECTURE.md,
  "The view tree"): while a reply streams and the log follows its end, the
  reply's last block is drawn by its own cached view at the log's foot, and
  the list keeps a room of its height. A batch that only grew that block at
  the same height notifies the tail's view alone: the log is replayed. In
  cr-scroll the streaming log's full redraws fell from ~230 to ~100 (the
  rest are new blocks and wraps), and the tail drew ~235 times instead.
  test/chat-tail.mjs (new, 17 checks) plays a stream and checks the tail
  redraws alone, a wrap redraws the log, the text sits where the list puts
  it after the stream, scrolling up and the stream's end give the block back
  to the list, and idle draws nothing.
- **Rows below the reader**: while the reader is up the log ("Go to the
  newest message" showing, its dot unchanged), a batch that only touched
  rows below what the list last made redraws nothing.
- **The mosaic laid out once per change** (`Workbench::laid`).

### After (full runs on the display, drawer included)

| scenario | HEAD, round 12 (2 runs) | the cut, run A | the cut + rows below, run B |
|---|---|---|---|
| idle-open, idle-end, idle-planning | 0 | 0 | 0 |
| idle-terminal / idle-drawer | 0 / 0-2 | 1 / 1 | 1 / 1 |
| scroll-chat | 6.02, 5.96 | 4.48 / 5.74 / 100% | 5.13 / 6.09 / 100% |
| stream+scroll | 6.95, 6.91 | 5.49 / 6.53 / 98% | 4.93 / 6.41 / 99% |
| scroll-kanban | 4.52, 4.37 | 3.55 / 4.14 / 100% | 3.62 / 4.33 / 100% |
| pan-mindmap | 5.84, 5.78 | 4.57 / 5.31 / 99% | 4.89 / 5.48 / 99% |
| term-stream+scroll | 7.98 / 78%, 7.98 / 72% | 5.74 / 7.51 / 82% | 5.58 / 7.64 / 82% |
| planning+stream+scroll | 8.39 / 51%, 7.83 / 70% | 5.01 / 7.97 / 78% | 6.79 / 7.93 / 58% |
| cr-sweep | 7.73 / 73%, 7.27 / 89% | 5.81 / 7.33 / 90% | 6.23 / 7.09 / 94% |
| **cr-scroll** | **8.05 / 78%, 7.98 / 77%** | **5.95 / 7.41 / 90%** | **5.63 / 7.22 / 92%** |
| cr-drawer-scroll | 7.93 / 71%, 7.77 / 82% | 5.98 / 7.76 / 85% | 5.82 / 7.60 / 86% |

Quick A/B, back to back (cr-scroll p95): HEAD 7.50; the cut 6.74, 6.91
(cr-sweep 7.41 → 6.82, 6.86). A second HEAD quick run (7.96) overlapped a
build of mine and is left out.

So the cut takes about 0.6-0.8 ms off the CR's scroll and sweep, and they
are now near the line (92-94% ≤ 7 ms) but not under it. planning+stream+
scroll and term-stream+scroll did not move: their sweep goes over every
pane, the streaming chat too, which stops following its end (so no tail),
and the shell's screen redraws as it prints.

## 5. What is left, and the question for the owner

The target (95% ≤ 7 ms in all three) is not reached on GPUI 0.3.12 as the
client stands. What remains is mostly GPUI's per-frame model: every frame
rebuilds the whole scene from replays, so the cost grows with everything on
screen (§3), and the one-level cache means a view cannot keep its clean
children while it redraws. Two ways on:

1. **Inside the client, no fork** (another round, each worth tenths of a
   ms): the focused composer's box into a paint layer with its caret in a
   nested layer (today its primitives go through the bounds tree every
   frame); fewer ids and elements in the replayed chrome (heads, composer
   rows, project bar: fewer element states to move per frame); the
   terminal's screen drawn as the log's blocks are (one element per line).
   Each was small in the ablation, so together they may reach the line,
   with no margin.
2. **A small GPUI patch** (a fork of gpui-pre 0.3.12, the owner's call):
   let a re-rendering cached view keep the clean cached views inside it at
   the same bounds (no `refreshing` cascade), and replay a cached view at a
   new origin by an offset (F130). With those, a pane's log could keep its
   rows while it scrolls or streams and Settings/Info could scroll cheaply;
   it is the structural change the numbers point to.

Raw data on farhan-RP-15 (not kept in the tree): /home/farhan/crundi-work/v2bis
(m/ full runs, q/ quick runs, p/ profiles).
