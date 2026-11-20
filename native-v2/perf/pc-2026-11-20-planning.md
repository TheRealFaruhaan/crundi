# The planning area on the owner's PC, 20 Nov (farhan-RP-15)

`tools/measure-pc.mjs` (release build, one window on the PC's display, a
fresh config folder, `tools/stub-server.sh` 4342, never the owner's
sign-in), with the planning scenarios added: the board seeded with 80
cards (4 columns, a third with 3 subtasks), the map with 133 ideas.
`MEASURE_STREAM=0` for the iterations (the workbench scenario then sweeps
seven panes: four chats, the new chat, a Kanban and a Mindmap pane).

| Build | scroll-kanban p50 / p95 / ≤7 ms | pan-mindmap p50 / p95 / ≤7 ms | planning panes + sweep p95 | idle-planning |
|---|---|---|---|---|
| first port (div cards, every idea a div with its handlers) | 6.8 / 11.1 / 58% | 12.2 / 20.4 / 0% | 7.6 | 0 frames |
| + one paint layer per list and for the map, the dots in one layer, links built once each | 6.3 / 10.1 / 78% | 5.5 / 9.7 / 70% | 7.6 | 0 |
| + cards at rest drawn with look-alike controls (only the card under the pointer, or in use, has the live ones; pixel-identical: test/planning/kanban-look.mjs) | 6.2 / 7.9 / 72% | 6.9 / 11.8 / 54% | 7.1 | 0 |
| + ideas drawn without handlers (the map hit-tests the pointer), links cached per link | 5.8 / 8.0 / 69% | 5.4 / 10.9 / 67% | 7.4 | 0 |
| + heights estimated as drawn (no re-layout while panning: 22 → 0) | 6.6 / 8.0 / 70% | 6.8 / 7.9 / 60% | 8.9 (7.1–8.9 over runs) | 0 |

Experiments on the first port (release, same display): plain text-only
cards scroll at p95 5.0 ms (so the controls cost ~6 ms); swapping the five
per-card Buttons for divs: 7.4; the map without its links 15.4, without
its ideas 15.0, without the dots 16.0 (from 20.4).

The chat scenarios in the same runs: scroll-panes p95 5.7–6.2, scroll-chat
5.9, stream+scroll 6.4 (unchanged by the planning area).

Not met yet: the Kanban tab's scroll and the map's pan are at p95 ≈ 8 ms.
What is left per frame is building the visible cards' / ideas' element
trees; the next step is one element per card and per idea (the chat
log's BlockEl way: laid out once per width, only painted while scrolling).

Windows opened on the display (each closed by its PID by the harness):
    05:07:47
    05:09:40
    window 05:10:42 MMX_KANBAN_PLAIN=1 A=1
    closed 05:12:11
    window 05:12:20 MMX_KANBAN_PLAIN= MMX_NO_EDGES=1
    closed 05:13:49
    window 05:13:58 MMX_KANBAN_PLAIN= MMX_NO_NODES=1
    closed 05:15:26
    window 05:15:35 MMX_KANBAN_PLAIN= MMX_NO_DOTS=1
    closed 05:17:04
    window 05:18:20 A=1
    closed 05:19:49
    window 05:21:03 A=1
    window 05:22:30 MMX_KB_DIVBTN=1
    closed 05:22:32
    closed 05:23:59
    window 05:29:06 A=1
    closed 05:30:36
    window 05:33:50 A=1
    closed 05:35:19
    window 05:36:21 A=1
    closed 05:37:50
    window 05:39:43 A=1
    closed 05:41:11

(The first pair, 05:07:47–05:09:40, is the first full run. The 05:21:03
run was a profiling attempt; gdb could not attach (ptrace scope 1), and the
next run's script stopped its stub server, so that window and the 05:22:30
one overlapped for about 2 s.)
