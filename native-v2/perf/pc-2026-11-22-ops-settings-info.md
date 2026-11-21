# Settings and Info scrolled on the owner's PC (22 Nov, F62/F130)

Release build (`tools/build.sh --release`, this branch), the PC's own
display (one window, stopped by its PID), a fresh stub server
(`tools/stub-server.sh 4320`) and a fresh config folder each run,
`tools/measure-pc.mjs` with only the operations block:

    MEASURE_DRAWER=0 MEASURE_PLANNING=0 MEASURE_TERMINAL=0 MEASURE_CR=0 \
    MEASURE_STREAM=0 MEASURE_OPS=1 node native-v2/tools/measure-pc.mjs

`scroll-settings` is Settings with every section open, the wheel over the
page; `scroll-info` the Info tab, the wheel over the page and over its
server log; `scroll-docker` (untouched) is the control for how loaded the
PC was. The PC is shared with the other areas' suites (software-rendered
Xvfb clients, a busy loop): at load 12-22 every scenario, the control too,
lost 2-4 ms of p95, so only runs at load under ~8 are compared.

## Result (the last build, three runs back to back)

| run (load) | scenario | frames | p50 | p95 | ≤ 7 ms | max |
|---|---|---|---|---|---|---|
| 1 (8.1) | scroll-info | 632 | 4.35 | **6.24** | **98%** | 8.56 |
| | scroll-settings | 631 | 4.94 | **6.91** | **96%** | 9.07 |
| | scroll-docker | 693 | 3.56 | 5.22 | 100% | 7.42 |
| 2 (5.3) | scroll-info | 639 | 4.08 | **6.27** | **98%** | 8.54 |
| | scroll-settings | 647 | 4.57 | **6.81** | **96%** | 8.18 |
| | scroll-docker | 664 | 4.49 | 5.55 | 100% | 8.85 |
| 3 (3.3) | scroll-info | 635 | 4.40 | **6.31** | **98%** | 9.23 |
| | scroll-settings | 650 | 4.13 | **6.42** | **99%** | 9.26 |
| | scroll-docker | 681 | 4.17 | 5.16 | 100% | 6.58 |

idle-settings: 0 frames in every run. idle-info: 3 frames in 6 s (the
2 s poll's new figures), now only the Machine section's rows built again
(23 rows a poll; it was every row on the page, 18 ms frames).

Before (the same harness, the first build of this round, load ~3):
scroll-info p95 8.92, 77%; scroll-settings p95 9.6, 49%. The round before
measured 8.5 / 78% and 8.7 / 84% (F62).

## What changed

The cause (F130): a cached view is replayed moved (GPUI patch (b)) only
when it lies wholly inside its content mask. A Settings or Info section
taller than the page is never wholly inside, so the rows the list's edges
cut, each a whole section (Voice, Notification Settings, Sign-in, the
collaborators' invite form), were built, laid out and painted again on every
wheel frame.

- **A section is now several rows** (`crates/operations/src/rows.rs`,
  `Parts`): each child of its body's column is a row of its own in the
  page's virtual list, a cached view (`Rows`). A wheel frame builds again
  only the 2-3 rows the edges cut, each a small part. The card is drawn in
  slices (`slice`): the head (or title) has the top edge and corners, each
  part the sides, the last part the bottom; framed lists inside a body
  (the notification table, the skills lists) are sliced the same way
  (`Parts::framed`, `framed_with`), one row per table row or skill.
- **A row builds only its own part** (`Parts::wanting`, `part(|| …)`): a
  body built for part `j` builds that part and only counts the rest, so a
  row no longer builds its whole section to show one piece of it.
- **The page's layout** (Settings: which sections are open and how many
  parts each has; Info: each section's parts) is worked out once per
  change; the list is spliced where it changed, so the scroll place stays.
- **Info's server log**: each line is a cached row too; a line appended is
  a row more, and only the "logs" rows are built again. The machine's
  figures rebuild only the Machine rows (`with_only`).

Nothing a person sees or does changed: the same cards, gaps, controls and
words (checked under Xvfb at every scroll place, no seams at the slices),
and the operations suite passes (431 checks).

## Still per frame

A profile of scroll-settings (gdb sampler, `PROFILE_AT=settings`, added to
the harness with `PROFILE_AT=info`): the row builds are now ~10% of a
frame; the rest is the window's own layout and paint (the page's list is
the notified view, so every view above it renders: GPUI's "(c)", the
Architect's) and the replay of the rows that only moved.
