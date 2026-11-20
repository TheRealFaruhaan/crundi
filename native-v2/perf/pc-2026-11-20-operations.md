# The operations area on the owner's PC (20 Nov)

Release build (`crundi-v2`, this branch), the PC's own display (Wayland,
1920x1040 window), a fresh stub server (`tools/stub-server.sh 4888`) and a
fresh config folder, `tools/measure-pc.mjs` with `MEASURE_OPS=1` (its
operations block). No browser drawer: the Desktop area's browser is not in
this branch (Panels and What's new draw their pages through
`crundi_operations::web::set_host`, which Desktop sets).

## The change request's scenario, with the operations area in

The full run (all scenarios, then the operations block):

| scenario | frames | p50 | p95 | ≤ 7 ms | max |
|---|---|---|---|---|---|
| idle-open | 0 | | | | |
| scroll-panes | 624 | 3.83 | 5.49 | 100% | 7.81 |
| stream+scroll | 622 | 4.00 | 5.98 | 100% | 7.98 |
| idle-end | 0 | | | | |
| term-stream+scroll | 2317 | 4.36 | 6.63 | 98% | 10.13 |
| planning+stream+scroll | 516 | 4.65 | 6.74 | 97% | 8.09 |
| **cr-sweep** (five panes, a chat streaming) | 593 | 4.34 | **5.99** | **99%** | 7.77 |
| **cr-scroll** | 645 | 4.93 | **6.33** | **98%** | 8.22 |

The area adds nothing to a frame of the workbench: its tabs are not on
screen there, its two window layers (the approvals panel, the backup
notice) are cached and empty, and the live batch's chat events never reach
its screens (`compat::events` drops them).

## The area's heavy screens

| scenario | frames | p50 | p95 | ≤ 7 ms | notes |
|---|---|---|---|---|---|
| scroll-settings, every section open (first port: one view, the whole page per frame) | 312 | 19.1 | 21.3 | 0% | |
| scroll-settings, the page a virtual list of its sections | 580 | 5.35 | 8.72 | 84% | a wheel frame builds the sections on screen |
| idle-settings | 0 | | | | |
| scroll-info (first port) | 528 | 6.4 | 10.2 | 67% | 2000 log lines as divs before |
| scroll-info, sections and the server log virtual lists | 569 | 5.30 | 8.54 | 78% | |
| idle-info | 3 in 6 s | | | | the live machine figures (2 s poll), only when they changed |
| scroll-docker (containers) | 656 | 3.37 | 4.78 | 100% | |

Each section in a paint layer made no difference (p95 9.2 / 9.2): the cost
of a scroll frame there is building and laying out the open sections'
controls, not painting them. Settings with all sixteen sections open is the
worst case; with one or two open the visible content is far smaller.
