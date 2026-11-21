# The Architect's last round (F146): the owner's display, release

farhan-RP-15, Wayland/GNOME, a 1920x1040 window, AMD Renoir; release
(`tools/build.sh --release`), the worktree's own `.bin/release` with the
browser host staged, `tools/stub-server.sh 4884` fresh each run, a fresh
config folder each run. The owner's Electron client ran throughout (~2
cores). Each run started at a load average below 4.

p50 / p95 ms / share ≤ 7 ms.

| scenario | A: patch (c), Workbench cached at the Shell, Mini outside | B: A + each pane's body a cached view | C: B, quick run |
|---|---|---|---|
| idle-open, idle-end, idle-planning, idle-drawer | 0 frames | 0 frames | (drawer) 0 |
| idle-terminal | 1 frame | 1 frame | |
| scroll-panes | 3.93 / 5.67 / 100% | 4.92 / 5.77 / 100% | |
| scroll-chat | 4.12 / 5.87 / 100% | 4.97 / 5.66 / 100% | |
| stream+scroll | 4.10 / 6.47 / 99% | 5.27 / 6.02 / 100% | |
| stream | 3.44 / 4.52 / 99% | 4.00 / 5.14 / 99% | |
| term-stream | 4.03 / 6.30 / 99% | 4.21 / 6.26 / 100% | |
| term-stream+scroll | 4.85 / 7.41 / 87% | 5.45 / 7.42 / 88% | |
| scroll-kanban | 3.55 / 4.28 / 100% | 3.54 / 3.96 / 100% | |
| pan-mindmap | 3.70 / 5.08 / 100% | 4.34 / 5.21 / 100% | |
| **planning+stream+scroll** | 6.35 / 7.87 / 74% | **5.98 / 6.86 / 97%** | **4.41 / 6.73 / 98%** |
| **cr-sweep** | 5.15 / 7.10 / 94% | **5.14 / 6.48 / 98%** | **5.35 / 6.53 / 99%** |
| **cr-scroll** | 6.23 / 7.87 / 80% | **5.39 / 6.74 / 98%** | **5.37 / 6.73 / 98%** |
| cr-drawer-sweep | 5.51 / 7.61 / 87% | 4.91 / 6.58 / 97% | 4.53 / 6.54 / 98% |
| cr-drawer-scroll | 5.97 / 7.81 / 85% | 5.36 / 6.99 / 95% | 5.26 / 6.59 / 99% |

No frame over 16 ms in any run.

## What changed

- **A.** GPUI patch (c) (vendor/gpui/PATCH.md): a click's press and release
  redraw nothing, and its active style and a tooltip redraw their own view,
  not the window (F105). The Shell draws every tab's root cached, the
  Workbench included (F179). Mini's pane layers are a Shell overlay
  (`chat::mini::MiniPanes`) over the bounds the pane's body records, not
  children of the Workbench (F179). A pane moved by its head redraws its
  card alone, and the Workbench only when the lit leaf changes.
  Render counts in cr-scroll: workbench 586 in 586 frames (it is still the
  ancestor of every dirty leaf), log 283, tail 235, map 187, stack 211.
- **B.** Each pane's body is hosted in a cached view (`workbench::PaneBody`).
  The Workbench still renders on every frame (631 in 630), but it now
  replays the clean panes' bodies instead of building and laying out every
  pane's elements, and a dirty pane renders its body around its replayed
  leaves (pane-body 976 renders in cr-scroll: about 1.5 a frame). That was
  the step: the CR's three scenarios went from 74-94% to 97-99% ≤ 7 ms.

## What is left

term-stream+scroll (the terminal printing, a chat log under the wheel) is
7.42 / 88%. On every frame both the shell's screen and the log render
(term-screen 2055, log 2101 in 2114 frames), and with them two pane bodies.
It is not one of the CR's scenarios. The next cut would be the terminal's
screen drawn per line (as the log's blocks are), or GPUI's "(c) in GPUI: a
dirty view without dirty ancestors" from pc-2026-11-21-architect-gpui-patch.md.

Raw (on farhan-RP-15, not kept): /tmp/arch-run/m1, m2, m3 (report.json).
