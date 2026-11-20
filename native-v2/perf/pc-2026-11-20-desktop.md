# The owner's PC, 20 Nov: the Desktop area's round (the browser drawer open)

Release build (`crundi-v2` and its `crundi-browser-host` beside it, CEF staged
with `tools/stage-browser.sh`), on the PC's own display (1920x1040 window,
Wayland, 144 Hz, Renoir), one window, its own fresh config folder and CEF
profile, a private D-Bus session (`dbus-run-session`: the tray and
notifications never reached the desktop's), against `tools/stub-server.sh
4303` (no real Claude). `tools/measure-pc.mjs` with its new drawer scenarios
(MEASURE_DRAWER, on by default): after the change request's five panes, the
drawer is opened beside them on a static 200-line page from the harness, and
the same sweep and scroll run again while the chat streams.

| Scenario | frames | p50 | p95 | max | ≤ 7 ms |
|---|---|---|---|---|---|
| idle-open | 0 | | | | |
| scroll-panes | 589 | 4.54 | 5.32 | 9.82 | 100% |
| scroll-chat | 599 | 4.59 | 5.36 | 6.10 | 100% |
| stream+scroll | 592 | 4.51 | 5.62 | 7.24 | 100% |
| term-stream+scroll | 2175 | 4.32 | 6.58 | 8.13 | 98% |
| planning+stream+scroll | 517 | 4.49 | 6.33 | 7.62 | 99% |
| cr-sweep (five panes, no drawer) | 575 | 4.59 | 5.89 | 8.39 | 99% |
| cr-scroll | 652 | 4.23 | 5.57 | 6.31 | 100% |
| **cr-drawer-sweep** (the drawer open, the wheel over the page too) | 558 | 4.64 | 6.11 | 8.13 | 99% |
| **cr-drawer-scroll** | 523 | 4.17 | 5.74 | 9.46 | 99% |
| idle-drawer (4 s) | 1 | | | | |
| idle-end, idle-planning | 0 | | | | |

- The page's frames redraw only its `PageView` (`page`: 150 renders in the
  drawer sweep, from the wheel scrolling the page); the drawer's bars drew
  nothing. The drawer itself is a layout view placed uncached by the Shell,
  so it re-runs its (few boxes') render with the Shell on every frame, as
  the Workbench does (`drawer` = `workbench` = 559).
- idle-drawer's one frame is the top bar and a composer once, the stream's
  end settling (as idle-terminal's one frame): nothing of the drawer's.
- The area's pump sleeps while nothing happens (the suite checks `pumps`).
