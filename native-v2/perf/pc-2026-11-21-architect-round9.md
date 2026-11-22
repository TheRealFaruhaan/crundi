# Round 9 (the Architect): F99 under Xvfb, base vs now

The owner's display was not used this round (the Manager asks first). The
same CPU (farhan-RP-15), under Xvfb with lavapipe, a frame-journal release
build of each tree (`--features frame-journal`, `CRUNDI_FRAME_JOURNAL=1`),
`tools/measure-pc.mjs` with `MEASURE_DRAWER=0` on a fresh stub server. The
frame totals are lavapipe's present (~45 ms) and mean nothing here; the
journal's draw phases (layout, prepaint, paint, the whole draw: p50 / p95
µs) are the UI thread's CPU work, the part F99 is about.

| scenario | base dfb32bcc (2 runs) | round 8 HEAD 40ba8ecb + Mini (2 runs) |
|---|---|---|
| scroll-chat draw | 2431 / 3652, 2363 / 3351 | 2976 / 3786, 2940 / 3633 |
| cr-sweep draw | 3210 / 4229, 3212 / 4393 | 3891 / 4970, 3775 / 5305 |
| cr-scroll draw | 3389 / 5005, 3536 / 5064 | 3841 / 5124, 3852 / 5041 |
| scroll-kanban layout | 376, 398 | 445, 460 |

So every frame that re-renders the Shell costs ~0.5 ms more than at base,
layout ~0.1-0.2 ms of it even on the Kanban tab: the cost is spread, not
one view's.

Experiments (one build with switches; scroll-chat draw p50, one run each,
noise about ±0.1 ms): as is 2832; without Mini's layer and spots 3028;
without the log's overlays (copy button, jump, dock, goal, media) 2751;
without the pane's drop wrapper 2832; without the Shell's overlay layers
2678; without all four 2662. Only the Shell's overlay layers counted.

Tried and reverted: the top bar's nine leaves in one cached view
(`TopHost`) and the overlay layers in one (`OverlayHost`), so a scrolling
frame would replay two views, not fifteen. scroll-chat draw p50 2754 and
2621 (from ~2850): ~0.15 ms. But a cached view that re-renders renders
everything inside it afresh (`window.refreshing` in gpui's view.rs), so one
leaf's notify (the connection badge's blink, Cru's screen) re-rendered all
its siblings: test/shell.mjs and test/desktop's Cru checks failed. Rule 3
stands; reverted.

Open (F99): ~0.5 ms a frame over base, spread over the Shell's replays, the
Workbench's and the panes' per-frame work. The release build on the owner's
display decides where cr-scroll stands (round 7: 7.02-7.43 p95); that run
waits for the Manager's go (the owner's screen on).
