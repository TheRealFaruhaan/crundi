# 2.4 Live state: the pane's dot and badge, the project dot, live titles

What on the PC client shows a chat's live state, and how it updates without a
reload:

1. the pane header's **status dot** (`.term-status-dot`) and **agent badge**
   (`.term-agent-badge`);
2. the sidebar's **project dot** (`.sidebar-item .dot`, the `ts-*` classes),
   and the folded sidebar's **avatar ring** (`.proj-initials`), which follows
   the same state;
3. the chat's **title** in the pane header, changed by the server (Claude
   naming its chat, the auto-titler) or by a rename in another client.

The header's layout, the title's click to rename, its long form and the 160px
cap are 2.1 (`pane-header-wide.md`). The fold under 560px is 2.2. This spec
covers only what live state changes.

Source: `src/webapp-html.js`:
- CSS 527-539, the project dot (`.sidebar-item .dot`, `.active .dot`,
  `.ts-running/.ts-working/.ts-waiting/.ts-input .dot`);
- CSS 702-713, the folded rail (`.sidebar.collapsed … .proj-initials`);
- CSS 856-864, the pane dot and badge;
- `renderProjects()` 7462-7520, the project-state precedence;
- `headHtmlLive()` 9061-9082, the first draw of a header;
- `updateCellHead()` 9084-9150, the in-place update of the dot, title and
  badge;
- `updateLiveCellHeads()` 9157, and `applyState()` 11033-11064, run on every
  `state` event;
- `headHtmlParked()` 8787: the parked pane's grey dot, titled "Closed". That
  is 2.23.

Server: `src/claude-ui.js` `list()` 469 (`status`, `agentState`, `title`,
`longTitle`, `pending`), `setState()` 1082, `rename()` 3113; `src/webapp.js`
`buildState()` ~2090-2150, `broadcastState()` 2064, the auto-titler and the
retitle 2155-2215, `rename_chat` 8446.

## Screenshots

All are in `live-state/`, from an isolated server on :4810 at 1400×860.
`web-<state>.png` is the whole window. `-2x` and `-3x` are crops scaled ×2
and ×3.

The data: five projects. **Demo** has six chats:

| Chat | State | How it was made |
|---|---|---|
| "Fruit list and directory" → "Named by Claude" | idle | the seeded chat. The server titled it itself after its first turn; later renamed by `rename_chat` |
| "Long job" | working | skipPermissions; told to run `timeout 590 tail -f /dev/null` in the foreground |
| "Question" → "Renamed in another client" | needs input | AskUserQuestion. Renamed by `POST …/rename` from outside the browser |
| "Approval" | needs input | default mode; asked to run `touch approval-test.txt`, so a permission card |
| "Background" | waiting | skipPermissions; a background `sleep 3000` started, turn ended |
| "Gone" | exited | its `claude` process was killed |

The other projects:
- **Other**: one working chat and one idle chat (shows `ts-working`);
- **Third**: one waiting chat (`ts-waiting`);
- **Fourth**: one idle chat (`ts-running`);
- **Fifth**: no chats (no class).

| File | Shows |
|---|---|
| `web-mixed.png` | Demo active; panes idle, working, needs input (question), and approval (cut off). Sidebar with all five states |
| `web-sidebar-mixed-2x.png` | the sidebar rows of `web-mixed`: Demo (active, needs input), Other (working), Third (waiting), Fourth (running), Fifth (none) |
| `web-mixed-right.png` | the grid scrolled to its end: approval (needs input), Background (waiting), Gone (exited) |
| `web-head-{idle,working,question,approval,waiting,exited}-3x.png` | each pane header, 338×28, ×3 |
| `web-titles-live.png` | after the live renames: "Named by Claude" (server, `rename_chat`) and "Renamed in another client" (elided before the badge). No reload in between |
| `web-head-autotitled-3x.png`, `web-head-renamed-elided-3x.png` | those two headers ×3 |
| `web-full-autotitled-long.png` (+`-2x`) | the same chat in full screen: the wide header draws the long form "A title Claude gave this chat itself, long form" |
| `web-full-working.png` (+`-2x`) | a working chat, full screen: dot and badge in a wide header |
| `web-full-needs-input.png` (+`-2x`) | a needs-input chat, full screen: the typed title is capped at 160 (elided), then the badge |
| `web-active-working.png` (+`-2x`) | after answering both questions, Demo goes from needs input to working (active row, yellow dot with ring) |
| `web-sidebar-active-none.png` (+`-2x`) | Fifth active with no chats: its dot is **green** (`.active .dot`), with no ring. Demo, now inactive, is needs input |
| `web-folded-mixed.png` (+`-2x`) | folded rail, Demo active and needs input; OT working, TH waiting, FO running, FI none |
| `web-folded-active-running.png` (+`-2x`) | folded, Fourth active and running: a green border over the active gradient |
| `web-folded-active-none.png` (+`-2x`) | folded, Fifth active with no chats: the plain active look |
| `web-other-wide.png` (+`-2x`, both headers) | Other active and working (yellow dot with ring), two wide panes (about 570px): WORKING in a wide header, and "Other idle" turned exited **live** (its process was killed while the page watched; grey dot, composer "exited", "Session ended.") |

Ignore the walking robot (the logo mascot) in some shots.

## States

### Pane dot and badge (per chat)

`as = status === 'exited' ? '' : (agentState || 'idle')`. Exited wins over
everything.

| State | Dot | Ring | Badge text (shown upper case) | Badge colours | Dot tooltip |
|---|---|---|---|---|---|
| idle (or no state yet) | `#10b981` green | none | none | none | `idle` |
| working | `#f59e0b` | **none** (only the sidebar has one) | `working` | bg `rgba(245,158,11,.15)` = `yellow-dim`, text `#f59e0b` | `working` |
| needs input (a question or a permission card pending) | `#6366f1` accent | 3px `rgba(99,102,241,.15)` (`accent-dim`), a box-shadow spread: no blur | `needs input` | bg `rgba(99,102,241,.15)`, text `#818cf8` (accent-hover) | `needs-input` (the raw state, with the hyphen) |
| waiting (turn over, a background task or wake-up still armed) | `#38bdf8` sky | none | `waiting` | bg `rgba(56,189,248,.16)`, text `#38bdf8` | `waiting` |
| exited (the process is gone) | `#5a5a78` text-muted | none | none | none | `exited` |

- The badge sits right after the title, 4px gap (the header's `gap`), and
  before the spacer. With no badge the spacer follows the title directly.
- The badge never hides, even in a narrow pane: the title elides first
  (`web-head-renamed-elided-3x.png`: the title's box shrinks to 102.7 for a
  179px text).
- No hover state on the dot or the badge. The dot's tooltip is the system
  tooltip (`title`), so native uses its TipBox, as 2.1 does. The badge has no
  tooltip.

### Project dot (sidebar, unfolded)

Precedence, over the project's chats **and Claude terminal panes** whose
`status === 'running'` (from the `state` event's `terminals`):

**needs-input > working > waiting > any running (`ts-running`) > none**

| Class | Dot (8×8) | Ring |
|---|---|---|
| none, inactive | `#5a5a78` | none |
| none, **active** | `#10b981` green (`.sidebar-item.active .dot`) | none |
| `ts-running` | `#10b981` | none |
| `ts-working` | `#f59e0b` | 3px `rgba(245,158,11,.15)` (`yellow-dim`) |
| `ts-waiting` | `#38bdf8` | 3px **`rgba(56,189,248,.18)`** (.18, not .16 as the badge has) |
| `ts-input` | `#6366f1` | 3px `rgba(99,102,241,.15)` |

The `ts-*` colour wins over the active row's green, and the ring is drawn on
the active row's `accent-dim` background too. The row's text colour does not
change with state.

Excluded from the count, because they are not `running`:
- exited chats;
- parked panes (`status: 'parked'`);
- a factory's employee chats. `buildState()` leaves them out of `terminals`.
  `GET /api/ui-sessions` **includes** them, so a client counting from that
  list would light a project's dot for a factory's hidden workers. See
  "Data" below.

### Project avatar (sidebar, folded)

The 38px round `.proj-initials`, mono 11.84px/700, letter-spacing .3552px:

| State | Background | Border (1px) | Text | Shadow |
|---|---|---|---|---|
| inactive, none or waiting | `#1a1a28` (bg-tertiary) | `#2a2a3d` (border) | `#8888a8` | none |
| inactive, `ts-running` | bg-tertiary | `#10b981` | `#e8e8f0` (text-primary) | none |
| inactive, `ts-working` | bg-tertiary | `#f59e0b` | `#e8e8f0` | `0 0 10px -2px #f59e0b` |
| inactive, `ts-input` | bg-tertiary | `#6366f1` | `#fff` | `0 0 12px -2px #6366f1` |
| active, none or waiting | accent gradient (135°, `#6366f1` → `#818cf8`) | `#6366f1` | `#fff` | `0 0 0 1px #6366f1, 0 0 12px -2px #6366f1` |
| active, `ts-running` | accent gradient | **`#10b981`** | `#e8e8f0` | the active shadow |
| active, `ts-working` | accent gradient | `#f59e0b` | `#e8e8f0` | `0 0 10px -2px #f59e0b` (replaces the active shadow) |
| active, `ts-input` | accent gradient | `#6366f1` | `#fff` | `0 0 12px -2px #6366f1` (measured on Demo: just the glow, no 1px ring) |

The web has no folded rule for `ts-waiting`, so a waiting project looks like
one with nothing running. Copy that as it is: it is the web's design, not a
slip we can be sure of.

The hidden `.dot` still carries its colour in the folded rail. It is not
drawn (box 0×0).

### Title (live)

- **Server-set title.** The auto-titler names a new chat after its first
  turn, and retitles it when the work changes. `rename_chat` is Claude naming
  its own chat. Both call `rename(id, title, {auto: true, long})`, then
  `broadcastState()`. The header shows the new short title, and in a wide
  header the long form (2.1's `data-long`). Seen here: "Design study" →
  "Fruit list and directory" (long "Fruit list and directory listing") by
  the server, then → "Named by Claude" / "A title Claude gave this chat
  itself, long form".
- **Rename in another client** (`POST /api/ui-sessions/:id/rename`):
  `titleByUser` is set and the long form is cleared, then `broadcastState()`.
  The header shows the typed title, capped at 160 and elided. A later
  automatic rename is refused (`{ok:false, error:'The person named this chat
  themselves…', keptTitle}`), and the header keeps the typed title.
- **Never while editing.** `updateCellHead` skips the title while it is the
  rename `<input>`. A live title arriving during a rename does not replace
  what the person is typing, and the input stays open. When they press Enter
  their title is sent and wins. Native: keep the edit field's text when a new
  title arrives.
- **No pane reload.** Titles are left out of the grid signature
  (`[id, project, order, status]`), so a rename updates the header in place.
  The log, the scroll position and the composer's draft and focus stay.
- Where else the title shows on the PC client:
  - the browser drawer's "send to chat" list (`brdChats`): that is the
    browser drawer, out of phase 2;
  - the inbox threads: phase 1's inbox, a snapshot taken when the item was
    made;
  - the phone live cards (`retitleLiveCards`): server side, not on the PC.

  So the only live title on the PC client's chat screens is the pane header.

### What else on the workbench reflects live state

| Where | Web | In 2.4? |
|---|---|---|
| Composer foot label (`.cc-meta` first span) | `idle` / `working…` / `needs your input` (class `cc-busy`, `#818cf8`), else `#5a5a78`; `exited` when the process is gone; 10.5px. **A waiting chat reads `idle` there** (`web-mixed-right.png`, Background: header WAITING, foot "idle"). Send reads "Queue" unless idle | Owned by **2.5** (the composer), which must copy this. Listed here because it changes with the same events |
| Stop button (shown only while working) | `.cc-stop` | 2.5 |
| Folded avatar ring | above | **yes** |
| Top bar approvals button (`#approvals-btn`, the shield with a count) | counts pending chat permission cards, secrets requests and collaborator requests; hidden at 0 | **No.** It is the top bar's approvals inbox, a screen of its own that is not chat-only. Not in phase 2's list, so it needs a line in PLAN.md for a later phase |
| Inbox count (sidebar footer) | the inbox's own `unread`, from the `inbox` event | Phase 1 (already live) |
| Cru's cells (`voiceOpts().cells`, webapp-html.js 6943) | the same precedence, for the voice assistant Cru's screen | **No.** Cru (the voice assistant) is not a chat screen and not in phase 2. 2.24 is the chat's own mic |
| Parked pane's grey dot, "Closed" | `headHtmlParked` | 2.23 |

There is no loading state. The headers are drawn from the `terminals` list
the first `state` event brings, so a dot never shows before its state. With
the event stream down, everything keeps its last state, and the next `state`
after reconnecting replaces it all.

## Measurements

From `tools/measure.js` at 1400×860, panes 338px wide. The header is at
y 91 (a box of 26, padding 3px 4px 3px 2px, gap 4px).

| Element | Box (x,y,w,h) | Text box | Style |
|---|---|---|---|
| `.term-drag` | 249,97.5,26,12 | 252,96.5,20,14 | 12px, letter-spacing −2px, line-height 12px, `#5a5a78`, padding 0 3px |
| `.term-status-dot` | 279,100,7,7 | — | radius 50%. Its x is the drag's right (275) + 4 gap; its y is the header's top + 9 |
| `.term-status-dot.input` | 973,100,7,7 | — | + `box-shadow: 0 0 0 3px rgba(99,102,241,.15)`: a 13×13 ring at 970,97 |
| `.term-title` (idle pane) | 290,94.5,164.2,18 | 296,96.5,152.2,14 | 2.1's: 12px/600 `#e8e8f0`, padding 2px 6px, radius 6. Its x is the dot's right (286) + 4 |
| `.term-agent-badge.working` | 711.2,97,69.7,13 | 718.2,98,55.7,11 | 9.6px (0.6rem)/700, upper case, letter-spacing .384px (0.04em), padding 1px 7px, radius 999 (a pill, 6.5 at this height). Its x is the title's right (707.2) + 4; its y is the header's top + 6 |
| `.term-agent-badge.input` "NEEDS INPUT" | 1060.7,97,90.5,13 | 1067.7,98,76.5,11 | as above, `#818cf8` on `rgba(99,102,241,.15)` |
| `.term-agent-badge.waiting` "WAITING" | 1774.8(−927 scroll),97,63.9,13 | 49.9 wide | `#38bdf8` on `rgba(56,189,248,.16)` |
| full screen: dot / title / badge | 3,11,7,7 / 14,5.5,160,18 / 178,8,90.5,13 | — | the same, with no drag handle: the dot's x is padding 2 + 1 |
| Sidebar `.sidebar-item` | 8,101,223,32 | — | padding 8px 12px, gap 10, radius 6. Active: bg `rgba(99,102,241,.15)`, text `#818cf8`, `::before` 3×18 gradient bar |
| Sidebar `.dot` | 20,113,8,8 | — | radius 50%. Ringed states add `0 0 0 3px <ring>`: a 14×14 ring at 17,110 |
| Sidebar `.name` | 38,109,181,16 | 38,109,…,16 | 14.08px, `#8888a8` (active `#818cf8`) |
| Folded `.sidebar-item` | 0,105,59,54 | — | padding 8px 0, gap 5 |
| Folded `.proj-initials` | 10.5,113,38,38 | 22,125,15,14 | see the folded table |
| Composer foot label | e.g. 22,795,89,16 | — | 10.5px/400. `#818cf8` when `cc-busy`, else `#5a5a78` (2.5) |

Colours as theme tokens: green `#10b981`, yellow `#f59e0b`, yellow-dim
`#f59e0b26`, accent `#6366f1`, accent-dim `#6366f126`, accent-hover
`#818cf8`, sky `#38bdf8`, text-muted `#5a5a78`. **Two new ones:**
- `sky` at .16 (`#38bdf829`), for the badge;
- `sky` at .18 (`#38bdf82e`), for the sidebar ring.

## Behaviour

- **Source of truth: the `state` event** on `/api/events`. On every event the
  web:
  - replaces `terminals`;
  - re-renders the sidebar (`renderProjects`);
  - updates every live header in place (`updateLiveCellHeads`): the dot
    classes, the dot's title, the badge (added, changed or removed), the
    title and its `data-long`;
  - rebuilds the grid only when `[id, project, order, status]` changes. A
    state change, a title change or a pending question never rebuild a pane.
- The server broadcasts `state`:
  - on each agent-state change (`setState` → `onStateChange` →
    `broadcastState`);
  - on create, close, rename, permission-mode and model;
  - on an auto title and on `rename_chat`;
  - on process exit.
- The chat's own stream (`/ws` subscription) also sends
  `{type:'state', state}` and `{type:'exit', code}`. The web's composer uses
  those (2.5), but the header uses only the `state` event's `terminals`.
- **Exited:** when the `claude` process ends, the server:
  - cancels the pending cards (pending becomes `[]`);
  - sets the agent state to `idle` (its stream says `state: idle`, then
    `exit`);
  - lists the chat with `status: 'exited', agentState: null`.

  The header must turn grey with no badge. A client that takes the pane's
  state from the stream's last `state` (idle) would wrongly draw it green.
  `status === 'exited'` has to win.

  An idle chat whose process dies also reaches the web live. The dot went
  grey 1.7s after the kill (`web-other-wide.png`).
- **Transitions seen in the web** (polled every 100ms; the times are from the
  answer):
  - allowing a permission: needs input → working (3.5s) → idle (5.0s). The
    project dot stayed `ts-input`, because another chat still asked;
  - answering the question: needs input → working (4.1s) → idle (5.2s). The
    project went `ts-input` → `ts-working`, because "Long job" was still
    working.
- Answering a card in the pane does not change the dot at once. It waits for
  the server's next `state`, about 1-4s later. Native should not flip it
  optimistically either.
- `updateCellHead`'s fallback for an empty title is `'Terminal'` (the first
  draw says `'Chat'`). A chat's title is never empty (`rename` stores
  `'Chat'`), so the slip can't be seen. Native: use "Chat".

## Data and endpoints

| What | Method, path | Shape |
|---|---|---|
| Live state | `GET /api/events?token=…` (SSE), event `state` | `{projects, services, terminals:[{id, project, title, longTitle, order, kind:'ui'\|'terminal'\|'parked'…, status:'running'\|'exited'\|'parked', agentState:'idle'\|'working'\|'waiting'\|'needs-input'\|null, pending:[…], permissionMode, planStatus, collaborator, factory}], scheduled, factoryRuns}`. `terminals` already leaves out factory employees |
| The chat list | `GET /api/ui-sessions` | `{sessions:[…same fields…, mark]}`. **Includes factory employees** (`factory.role === 'employee'`) |
| Rename | `POST /api/ui-sessions/:id/rename {title}` | `{ok, title}`. Then `state` |
| Claude names its chat | `POST /api/mcp/call {tool:'rename_chat', args:{sessionId, title, long}}` with `x-api-key: <internal key>` | `{ok, title, long}`, or `{ok:false, error, keptTitle}` after a typed rename. Then `state` |
| Answer a card | `POST /api/ui-sessions/:id/respond {requestId, behavior:'allow'\|'deny', updatedInput?}` | `{ok:true}`. A question's answer goes in `updatedInput.answers{question: label}` |
| Per-chat stream | `/ws` subscription | `{type:'state', state}`, `{type:'exit', code}`, `{type:'entry'…}` (phase 1) |

**Native today** (from reading `src/app.rs` and `ui/*.slint`; the review
checks these):
- `load_sessions()` refetches `GET /api/ui-sessions` on every `state` and
  works from that. So `project_state()`:
  - counts factory employees;
  - misses Claude terminal panes.

  Take `state.terminals` instead (filter `status == "running"`), as the web
  does.
- `agent_state()` maps a non-empty `pending` to needs-input. The server
  already sets `agentState: 'needs-input'` then, so this is harmless. Keep it
  or drop it.
- The pane's state prefers `cs.conv.state` (the stream's) over the list's.
  That draws an exited chat green, so check `status == "exited"` first.
- There is no badge, no needs-input ring on the pane dot, and no tooltip on
  the dot.
- The sidebar:
  - has no ring for `waiting`;
  - draws the active row's dot muted instead of green when it has no chats;
  - in the folded rail, puts the active ring ahead of the `ts-*` border,
    which is the wrong way round;
  - has no glow and no gradient fill for the active avatar.

## How to bring each state about (for the end-to-end test)

Start the server with `tools/test-server.sh <port> [--bare]`. These use the
real `claude` that the server spawns.

| State | Recipe | Lasts |
|---|---|---|
| idle | create a chat (`POST /api/ui-sessions/create {project, title}`) | — |
| working | create with `skipPermissions: true`, then send *"Run exactly this shell command in the foreground with timeout 600000 and wait for it to finish: timeout 60 tail -f /dev/null. It is a deliberate wait for a UI test; do not run it in the background. Then say done."* Plain `sleep N` is **refused** by this machine's shell hook ("Blocked: standalone sleep"), so use `timeout N tail -f /dev/null` | N seconds, then idle |
| needs input, question | send *"Use the AskUserQuestion tool to ask me one question: "Which colour do you prefer?" with the options Red, Green and Blue. Do nothing else."* | until answered |
| needs input, permission | default mode (no skipPermissions); send *"Run the shell command: touch approval-test.txt"* | until answered |
| needs input → working → idle | in default mode, ask for `timeout 20 tail -f /dev/null`. The approval card makes it needs input; Allow (in the native UI) gives working for about 20s, then idle | — |
| waiting | `skipPermissions: true`; send *"Run the shell command `timeout 60 tail -f /dev/null` with run_in_background set to true. Do not wait for it; just reply "started" and end your turn."* | until the task ends. Claude is then woken (briefly working), then idle |
| exited | kill that chat's `claude` process. On Linux it is a child of the server's PID (`/tmp/crundi-devtest-<port>/server.pid`) whose `/proc/<pid>/environ` holds `CRUNDI_CHAT_ID=<chat id>`. `kill <pid>` (never by name). `POST …/close` is not the same thing: it removes the chat and its pane | for good |
| server-set title | `POST /api/mcp/call` with the internal key (`CRUNDI_API_KEY` from that same child's environ), `{tool:'rename_chat', args:{sessionId, title, long}}`. Or wait for the auto-titler after the first turn of a new chat; that is slower and not deterministic | — |
| rename in another client | `POST /api/ui-sessions/:id/rename {title}` | — |
| project states at once | chats in several projects as in the table above. Projects: `POST /api/projects {alias, name, path, create:true}` | — |

The test should, for each transition, check through `testctl`:
- the pane's state;
- the badge text (`""`, `working`, `needs input`, `waiting`);
- the pane title;
- the project's dot state (`needs-input`, `working`, `waiting`, `running`,
  `""`), for both the active project and an inactive one.

It should also take a snapshot of each header and sidebar state for the
review.

## Checklist

- [ ] Pane dot 7×7 at the drag handle + 4 (no handle in full screen), header
      top + 9. Green for idle and no state, yellow for working, accent for
      needs input, sky for waiting, muted for exited.
- [ ] Needs input: the dot has a 3px `accent-dim` ring (13×13, no blur).
      Working and waiting have **no** ring on the pane dot.
- [ ] The badge after the title (+4): 13px tall, 9.6px/700 upper case,
      .384px tracking, padding 1px 7px, pill. WORKING is yellow on yellow-dim,
      NEEDS INPUT is `#818cf8` on accent-dim, WAITING is sky on sky .16.
      Widths about 69.7 / 90.5 / 63.9.
- [ ] No badge for idle or exited, and it goes when the state ends.
- [ ] Exited: grey dot and no badge, even though the stream's last state was
      idle (`status` wins).
- [ ] The dot's tooltip is the raw state: `idle`, `working`, `needs-input`,
      `waiting`, `exited`.
- [ ] A narrow pane keeps the badge; the title elides first.
- [ ] Project dot precedence: needs-input > working > waiting > running >
      none, over running chats (and Claude terminals) only. Exited chats,
      parked panes and factory employees don't count.
- [ ] Project dot rings: working yellow-dim, waiting sky .18, needs input
      accent-dim (all 3px). Running and none have no ring.
- [ ] The active project with nothing running has a **green** dot. Inactive
      with none is muted. A state colour wins over the active green.
- [ ] Folded rail: the border, text and glow per the folded table, the state
      over the active ring, and nothing for waiting.
- [ ] A rename in another client shows in the header within a second, with
      no pane reload: log, scroll and draft kept.
- [ ] A server-set title (`rename_chat`) shows live. Its long form shows in a
      wide header (full screen) and the short one in a narrow pane.
- [ ] A live title doesn't clobber an open rename field.
- [ ] Two projects in different states at once each show their own dot, and
      they change live when you switch the active project.
- [ ] The answer to a card is not drawn optimistically on the dot: it waits
      for the server's `state`.
- [ ] Out of 2.4, each named in PLAN.md:
  - the composer's foot label (2.5; it reads "idle" for waiting);
  - the top-bar approvals button (a later phase);
  - Cru's cells (not phase 2);
  - the parked dot (2.23).
