# native-v2: the plan

Every area and screen of the PC client (`native-gpui/AREAS.md`,
`native-gpui/PLAN.md`; the reference is the web UI and the Electron app),
where it stands in v2, and its measurement. Built on `ARCHITECTURE.md`;
owned as `AREAS.md` says.

Status: **built** (in v2, its suite passes on the server), **slice** (in
the vertical slice: built for the architecture's proof, its full detail
still to port), **to do**. Measured: the suite's frames (p50 / p95 /
share ≤ 7 ms while it scrolls or streams; idle frames) — numbers that count
are the owner's PC, release build; "—" not yet.

## F. Foundation (Architect, this step)

- [x] `ARCHITECTURE.md` (the seven rules, the state model, the view tree
  and its caching, virtualised lists, the live link, background parsing,
  animations, the frame budget), `AREAS.md`, this plan
- [x] the Cargo workspace, versions pinned (gpui-pre =0.3.12 with x11 and
  wayland, gpui-component and gpui-kit-assets =0.10.0, cef =156.4.0 only
  in `crates/browser-host`, not a default member)
- [x] `tools/build.sh` (the shared target folder, two jobs, flock,
  oom_score_adj 1000, the 20 GB floor), `tools/check.sh` (no
  `refresh_windows()` / `window.refresh()`; run by build.sh and CI)
- [x] CI `.github/workflows/native-v2.yml`: the check, build, unit tests,
  smoke run (driver answers, sign-in page, idle draws no frames, a
  picture) and the Linux package, on Linux, Windows and macOS, as
  native-gpui's
- [x] the frame harness: `tools/frames.mjs` (ZED_MEASUREMENTS frame
  durations → p50, p95, share ≤ 7 ms, idle count), `test/lib.mjs`'s
  `T.frames`, `tools/smoke.mjs`'s idle check
- [x] core ported, with the live link's heartbeat (ping 10 s, silence
  25 s → reconnect, a new event stream with it; unit test against a
  silent fake server) and the conversation's changed-entry tracking
  (O(1) entry lookup, deltas appended in place)
- [x] the live hub: one channel awaited on the UI thread, one batch per
  frame (unit test: a burst of 201 events is one update; the next waits
  a frame)
- [x] the vertical slice (slice): the shell (top bar, sidebar, project
  bar, sign-in), the workbench (mosaic, heads, gutters, empty panes, pin,
  full screen, maximise, two-click ×), a chat pane (virtualised log over
  the composer, streaming through `ChatStore::apply_batch`; unit test: a
  batch of deltas splices one row)
- [ ] the slice's suite on the server: `test/slice.mjs` (sign-in, a new
  chat streaming real server events with no other action, frames while
  streaming and scrolling, idle 0 frames, a silent connection reconnects
  within 45 s and the next reply shows) — **written; to run on the
  server** (the owner's PC runs no window and no test server)
- [ ] frame numbers on the owner's PC (release, `CRUNDI_FRAME_BUDGET=1`):
  when the owner allows a window there

## The areas and their screens

| Area (crate) | Screen / feature | native-gpui ref | v2 | Measured |
|---|---|---|---|---|
| Shell (shell) | Top bar: logo and project, usage bars (wk / 5h, over-pace, countdowns), "updated … ago" (→ Usage), Cru's mark, Telegram, connection badge, window buttons, drag / double-click | shell/lib.rs, usage.rs | slice (Cru's mark static: Desktop's hover / live / unread / push-to-talk to do) | — |
| Shell | Sidebar: PROJECTS, fold (220 ms ease), rows and dots, folded rail with initials, project icons, + Add project, Inbox | shell/lib.rs, project_icon.rs | slice | — |
| Shell | Project bar: the 17 tabs, "+" (→ add menu), Browser button and dot, sideways scroll with fades | shell/lib.rs | slice (Browser's dot to do with Desktop) | — |
| Shell | Sign-in: server step, password and code, Google / GitHub / Telegram (OAuth and app-login), the browser's code, pairing, resume, sign-out | shell/session.rs | slice (ported; suite: sign-in by password) | — |
| Shell | The drawer's place and divider (drag, double-click reset, left / right) | shell/lib.rs | slice (Desktop gives the view) | — |
| Shell | Push-to-talk shortcut (ptt.json), settings parts | shell/ptt.rs | to do | — |
| Workbench (workbench) | 2.1 pane header wide, 2.2 narrow (the "…" menu), rename on click | workbench/head.rs | slice (no rename, no "…" menu yet) | — |
| Workbench | 2.3 moving panes: drag, swap, split, empty pane and its chips, the "+" menu's layouts and Fit | workbench/lib.rs, view.rs, menu.rs | slice (split, gutters, empty pane; drag/swap, "+" menu to do) | — |
| Workbench | 2.4 live state (dots, badges) | workbench | slice | — |
| Workbench | Launcher, Claude terminal panes (their input inside each panel) | workbench, workspace | to do | — |
| Chat (chat) | The pane: log over composer, its kind `live:`, panes kept in step with the project's chats | chats | slice | — |
| Chat: log | 2.13 questions, 2.14 tool cards (groups, bodies), 2.15 thinking, 2.16 prose / code blocks / bubble / results / errors / notices, 2.16m media, 2.16f live frames, 2.17 suggestions and recaps, 2.18 agent dock, 2.19 goal and plan, 2.20 cards, 2.21 side questions, 2.22 resume picker, 2.23 parked panes, "Go to the newest message" | chat-log | slice: bubble, markdown, tool card (bash / JSON), thinking, result, error, notice; the rest to do | streaming: — |
| Chat: composer | 2.5 composer and Stop, 2.6 mode picker, 2.7 attachments, 2.8 drafts, 2.9 queued, 2.10 scheduled, 2.11 slash menu, 2.12 secrets dialog, 2.12b secret requests, 2.24 voice input, the Enter switch, session id | chat-composer | slice: box, Send / Queue, Stop (Sure?), state, model, cost; the rest to do | — |
| Workspace (workspace) | Git, Files (tree, editor), Media | workspace | to do | — |
| Planning (planning) | Kanban (and the card dialog), Mind map, Notes, Schedules, webhooks, Factories (editor, live run graph, factory panel) | planning | to do | kanban / mind map scroll: — |
| Operations (operations) | Services, forwards, Docker; Secrets, Skills, Usage; Settings, sign-in, backup; Add project; Homepool; Info; Panels (widgets); What's new; approvals | operations | to do | — |
| Desktop (desktop, webview, browser-*) | The browser drawer (tabs per project, address bar, Network / Device menus, Annotate, DevTools), the Browsers tab, the native host (`test/drive.mjs`), tray and notifications, Inbox, the chat bubble, saved passwords, the homepool node, Cru (voice screen, live screen, kid mode, "Hey Cru") | desktop, webview, browser-host, browser-link | browser-host / link ported (not built here: CEF); the rest to do | — |

## The frame budget's runs (to fill)

| Run | Build | Machine | Streaming p50 / p95 / ≤7 ms | Scrolling chat | Kanban | Mind map | Idle frames |
|---|---|---|---|---|---|---|---|
| slice | debug | server (Xvfb, lavapipe) | — | — | n/a | n/a | — |
| full | release | owner's PC (1920x1080, 144 Hz, Renoir), 5 panes, a chat streaming | — | — | — | — | — |

## Open questions for the Manager

- The slice's Cru mark is drawn but static (no hover screen, no live
  breathing): Cru is Desktop's; until it lands, a click runs the `cru`
  action, which nobody registers yet.
- Tabs with no area yet show "<Tab> is not built yet in the v2 client."
  (native-gpui's placeholder wording), scaffolding only until every area
  lands.
