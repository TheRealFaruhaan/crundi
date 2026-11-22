# Terminal streaming on the owner's PC (F37), 20 Nov

Release build of the run at ab831a80 plus this round's workspace changes, on
farhan-RP-15's own display (1920x1040 window, Wayland, 144 Hz, Renoir), one
window at a time with a fresh config folder, against `tools/stub-server.sh
4332` (no real Claude). `MEASURE_PLANNING=0 node tools/measure-pc.mjs`: the
four seeded chats, the fifth that streams, then the workspace's scenarios:
an Empty Shell pane beside the chats (six panes), the shell printing 3000
lines about 2 ms apart (`term-stream`, ~29 s), again with the wheel swept
over every pane (`term-stream+scroll`), then 6 s idle (`idle-terminal`).

Windows on the PC's display: 06:12:54-06:14:23, 06:34:12-06:36:35,
06:49:45-06:52:09 (each closed by PID; config folder removed).

| Scenario | frames | p50 | p95 | ≤7 ms | max | renders |
|---|---|---|---|---|---|---|
| term-stream (run 3) | 2106 | 4.47 | 6.95 | 95% | 17.3 | term-screen 2106, term-cursor 2106, workbench 2106; no input bar, no head |
| term-stream+scroll (run 3) | 2061 | 5.18 | 8.09 | 80% | 38.8 | + log 2079 (the wheel over the chats) |
| term-stream (run 2) | 2139 | 4.18 | 6.62 | 97% | 25.6 | |
| term-stream+scroll (run 2) | 2138 | 4.69 | 7.71 | 91% | 13.5 | |
| idle-terminal | 1 | | | | 4.9 | top-bar 1, workbench 1 |

The same run's chat scenarios (run 3, load average ~3.5): scroll-panes p95
6.06 (100% ≤7 ms), scroll-chat 5.82 (100%), stream+scroll 6.49 (99%), stream
6.91 (95%), idle 0. Run 1 (load average 7.7: three crundi-client processes at
~230% CPU) gave 9.6-11.2 for the chats: those numbers are the machine's load.

What a streaming frame costs: the output is batched (2719 batches in 29 s,
one a frame), and only the screen, the cursor and the workbench's layout
redraw (never the input bar, a head or another pane). Alone it holds the
budget; with the wheel moving the five chat logs in the same frames it is
over (p95 7.7-8.1). Run 3 paints the screen in one paint layer with glyphs
straight in (as the chat's log): no measurable change from run 2.

Next, if the terminal is to hold the budget under a sweep: re-derive and
re-shape only the rows the batch changed (the emulator would report its
dirty rows), and lay each row out once per change (one element per row, as
the chat's BlockEl) instead of every run every frame.
