# Planning round 3 on the owner's PC (20 Nov)

Release build of this branch (Notes, Factories and the Media modal added), on
farhan-RP-15's own display (1920x1080, 144 Hz), `tools/stub-server.sh 4696`
and a fresh `CRUNDI_CONFIG_DIR` for each run, `tools/measure-pc.mjs`. One
window at a time:

| run | window open | scenarios |
|---|---|---|
| 1 | 11:24:01–11:29:21 | all (terminal included) |
| 2 | 11:29:40–11:31:45 | `MEASURE_TERMINAL=0` |
| 3 | 11:31:55–11:33:59 | `MEASURE_TERMINAL=0` |

| scenario | run 1 | run 2 | run 3 |
|---|---|---|---|
| scroll-kanban p95 / ≤7 ms | 4.12 / 100% | 4.06 / 100% | 3.98 / 100% |
| pan-mindmap | 5.30 / 100% | 5.64 / 99% | 5.46 / 100% |
| idle-planning frames | 0 | 0 | 0 |
| planning+stream+scroll (7 panes) | — | 7.02 / 94% | 7.16 / 93% |
| planning+stream+scroll (8 panes, with the shell) | 8.14 / 75% | — | — |

- Seven panes (five chats, the Kanban, the Mindmap; a chat streaming): the
  same as round 2 (6.91–7.16). Under the wheel only the scrolled planning
  view renders (`kanban-stack` or `mindmap-map`), with the streaming `log`
  and the Workbench. Round 2's profile of this scenario
  (pc-2026-11-20-round2/profile-planning-7panes.txt) has no planning hot
  spot: the frame is GPUI's layout and paint of the panes on screen.
- With the shell pane in the layout (run 1), it is 8.14: F50/F51
  (Workspace's terminal screen), not planning.
- Run 1's terminal scenarios, for Workspace: term-stream 7.28 / 91%,
  term-stream+scroll 7.86 / 81%, idle-terminal 1 frame.
