# 2.5 Composer and Stop

The strip at the foot of every chat pane where the person writes to Claude:
the message box, the buttons beside it (attach, Stop, Send/Queue), and the
meta row under it (the state label, the permission-mode button, the Enter
switch, the schedule clock, the session id, the model, the cost). This spec
covers its layout in a wide and a narrow pane, typing and auto-grow, the
Enter switch, Send, Stop's two-step arming, the meta row's live values and
the text size.

Other screens own what these controls open:
- the mode button's chooser and its colours per mode: **2.6**;
- attaching a file (picker, drop, paste, upload bar `cc-up`): **2.7**;
- the draft surviving a switch or a restart: **2.8**;
- the queue drawer above the composer, Queue's effect and Up-arrow recall: **2.9**;
- the clock's panel (`cc-schedpanel`): **2.10**;
- the slash menu (`cc-slash`): **2.11**;
- the mic button (`cc-mic`, hidden unless the server can listen) and the
  recording strip in Send's place: **2.24**. Its slot is kept: between Stop
  and Send.

Rule 2 of DESIGN.md applies: no numbers from the web. Every size below is a
scale step (spacing, radius, font) or a structural size (control height,
icon size); the web's own values are in its CSS for anyone who wants them,
but they are not the spec.

Source, `app/vendor/claude-chat.js`:
- CSS 132-137 (`.cc-btn`, `.primary`), 174-206 (composer, narrow rules,
  input, meta, mode button), 227-261 (`.cc-meta-sp`, `.cc-busy`, the switch,
  Stop, attach), 278-279 (`.cc-sid`), 395-399 (`.cc-sched`), 455 (`.cc-mic`);
- `mount()` 963-1180: the DOM and its order;
- `syncEnterMode()` 1328, `syncWidth()` 1352, the switch's click 1373,
  `setSessionId()` + the id's click 1380-1398;
- `setState()` 1687: the label, Stop's visibility, Send ↔ Queue;
- `postMessage()` 2454, `doSend()` 2822, `sendText()` 2842;
- `disarmStop()` / `doStop()` 2857-2875;
- `autoGrow()` 3952, `caretOnTopRow()` 3998, `onKeyDown()` 4024;
- the listeners 4122-4135;
- the snapshot 4936-4952 (`applyHistory`) and the events 5052-5088
  (`init`, `meta`, `state`, `exit`).

Host, `src/webapp-html.js`: `--cc-fs` 8460/8841, `cellFont()` /
`applyCellFont()` / `setTermFont()` 9669-9705 (`CHAT_FONT_BASE` 13, range
9-24).

Server, `src/webapp.js` 6486-6568 (`/api/ui-sessions/:id/(send|interrupt|permission-mode)`),
9255 (`subscribe-ui`); `src/claude-ui.js` `sendMessage()` 2782,
`interrupt()` 3033, `setPermissionMode()` 3042, the `init` events 1710/2569,
`setState()` 1082, `exit` 1539.

## Screenshots

All in `composer/`, from an isolated server on :4810 at 1400×860.
`web-<state>.png` is the whole window; `-2x` is a crop of the composer
scaled ×2.

The data (project Demo):
| Chat | Used for |
|---|---|
| "Fruit list and directory listing" | the seeded chat: idle with a session id, model and cost; later asked an AskUserQuestion, so **needs input** |
| "Long job" | skipPermissions (Bypass all); sent, through the composer, a 590 s foreground command: **working** |
| "Gone" | never sent anything (no id, no model), then its `claude` process killed: **exited** |
| "Errors" | a fresh chat whose `/send` the page was made to fail: **send error** |

The layout: the wide pane is the top of a column split (1146px wide); the
bottom row holds two or three panes (570px, wide; or 376px, **narrow**).
Three side by side at 1400 makes every pane narrow.

| File | Shows |
|---|---|
| `web-rest` | wide, idle, empty, unfocused: placeholder with the Enter hint; meta: idle · Ask permissions · switch · clock · id · model · cost. Below: two fresh chats (no id, model or cost), one Bypass all |
| `web-focus` | the box focused: accent border and the ring |
| `web-typing` | one line typed |
| `web-long-max` | ten lines: the box has grown to its maximum (180px at 1×) and scrolls inside; the buttons stay at the bottom |
| `web-enter-newline` | the switch on Ctrl+Enter: thumb on the right, the hint in the placeholder changed. The other panes still show the old hint (see Behaviour) |
| `web-hover-attach` / `-send` / `-mode` / `-switch` / `-clock` / `-sid` | each control hovered (crops) |
| `web-sid-copied` | after a click on the session id: the toast "Session id copied: <full uuid>" |
| `web-running` | wide, working: label "working…" in accent, Stop (a red square) between attach and Send, Send reads **Queue**. The model and id arrived live (the `init` event) |
| `web-stop-hover` | Stop hovered: red border |
| `web-stop-armed` | after one click: Stop widens to "■ Sure?" on a red-dim fill with a red border; the box gives up the width |
| `web-needs-input-and-exited`, `-needs-input-2x`, `-exited-2x` | needs input: "needs your input" in accent, Queue, no Stop, the cost wrapped to a second meta line. Exited: placeholder "Session ended.", the box dimmed and disabled, label "exited", Send still there |
| `web-narrow` | three narrow panes (working, needs input, exited): the box on its own row; under it attach · clock · (Stop) · Send/Queue filling the rest; meta: label · mode icon only · switch · model · cost; no session id |
| `web-narrow-armed` | narrow, Stop armed |
| `web-a-minus-9` | text size 9 (A− to the minimum): the text shrinks; attach, Stop, the clock and the box's minimum height do not |
| `web-a-plus-20` | text size 20: box text, Send, the mode button, the switch, the labels grow; attach, Stop and the clock do not |
| `web-send-error` | a send the server refused: the error box in the log ("Session is not running"), the box emptied, and the composer stuck on "working…" with Stop (a web accident, see Behaviour) |

## The parts and their order

```
.cc-composer   (bg-secondary, a 1px border line on top, padding 8)
  .cc-wrap
    .cc-inrow            one row, items aligned to the BOTTOM, gap 8
      .cc-input          the message box (stretches)
      .cc-actions        (wide: transparent, its children sit in the row)
        attach           34 square, paperclip icon
        [clock]          narrow only: moved here from the meta row
        Stop             34 high, only while working
        [mic]            hidden (2.24)
        Send / Queue     .cc-btn.primary
    .cc-up               upload bar, hidden (2.7)
  .cc-meta               one row, gap 8, wraps; text 11 muted
    state label          "idle" | "working…" | "needs your input" | "exited"
    mode button          icon + name (wide) / icon only (narrow)
    Enter switch         [ ⏎ | ⌃⏎ ] + "sends"
    [clock]              wide only
    (spacer)
    session id           first 8 chars, mono, dotted underline; wide only
    model                e.g. "claude-opus-5-5"
    cost                 "$0.4015"
```

### Wide (pane content ≥ 520px)

- **Row 1:** the box takes all the width left; attach, [Stop], Send at its
  right, bottom-aligned with it, so as the box grows they stay on its last
  line. Gap 8 between all.
- **Row 2 (meta):** gap 6 above it. Label, mode button (icon and name),
  switch, clock; then a spacer; then id, model, cost at the right edge. When
  the row is too narrow it wraps (the cost goes to a second line first,
  `web-needs-input-2x`).

### Narrow (pane content < 520px), `.cc-narrow`

The measure is the chat's own width (`root.clientWidth < 520`, a
ResizeObserver), not the window's; it flips live as a split is dragged.

- **Row 1:** the box, full width.
- **Row 2:** attach, clock, [Stop], Send. Send stretches to fill the rest;
  attach, clock and Stop keep their size. Gap 8. The clock is the same
  control, moved (not a second copy), and is a 34-high button here (bigger
  icon, more padding) instead of the meta row's small one.
- **Row 3 (meta):** label, the mode button as an **icon only** and
  finger-sized (34 wide, 28 high at 1×), switch, spacer, model, cost. The
  **session id is hidden** (the pane header already carries it, 2.2). Gap 6,
  text one step smaller (10 instead of 11 on the scale).
- The placeholder drops its hint: just "Message Claude…".

A window narrower than 768px also hides the mode button's name in every
pane (the web's phone media query); the PC client can be that narrow, so
the native client does the same.

## Every control

| Control | Look | Does |
|---|---|---|
| **Message box** | bg-primary, 1px border, radius control (6), padding 8 × 10, text 13 × fs, line height ~1.45. Placeholder muted. Focus: accent border + the ring (3px accent at ~28%). Disabled: 55% opacity | Typing grows it (below). Enter / Ctrl+Enter sends per the switch. Up on the top row recalls a queued message (2.9). A leading `/word` opens the slash menu (2.11). |
| **Attach** | 34 square, bg-primary, border, radius control, paperclip 16 in text-secondary. Hover: accent border, icon accent-hover. Title "Attach a file (uploads to crundi_attachments)" | Opens the file picker (2.7). While uploading: 55% and inert. |
| **Stop** | 34 high, at least 34 wide, bg-primary, border, radius control, a red rounded square 15. Hover: red border. **Armed:** red-dim fill, red border, and "Sure?" (11 × fs) slides in beside the square (the label grows from 0 width over ~180ms) | Shown only while the state is **working**. First click arms; a second click within 3 s interrupts (below). Title: "Stop this turn (click twice)", armed: "Click again to interrupt". |
| **Send** | `.cc-btn.primary`: the accent fill (the theme's accent gradient and glow, as the native Send already has), white 12 × fs semibold, padding 6 × 12, radius control. Hover: accent-hover. Narrow: stretches, padding 8 × 10 | Reads **Send** when idle, **Queue** when working or needing input. Never disabled. Click = the same as Enter (doSend). |
| **State label** | 11 × fs. "idle", "exited": muted. "working…", "needs your input": accent-hover (`.cc-busy`) | Read-only. A *waiting* chat (background task, turn over) reads "idle". |
| **Mode button** | 22 high (× fs), min 26 wide, padding 0 6, radius 5→**control**, border, bg-primary, icon 13 × fs + the mode's name (11 × fs) in text-secondary. Hover: accent border, text-primary. Its colour per mode is 2.6 (Bypass all is red with a red border: `web-rest`) | Opens the permission-mode chooser (2.6). Title "Permission mode: <name>. Click to change." |
| **Enter switch** | a pill track (bg-tertiary, padding 2) holding "⏎" and "⌃⏎" (11 × fs, bold), a thumb (accent-dim fill, 1px accent border, pill) under the live one, which is full opacity with its glyph in accent-hover; the other at 45%. Then "sends" (11 × fs, text-secondary). Hover: the label goes text-primary. The thumb slides in ~180ms | Click swaps Enter-sends ↔ Ctrl+Enter-sends, saves it, updates the placeholder, and puts the focus back in the box. Title: "Enter sends, Shift+Enter makes a newline — click to swap" / "Enter makes a newline, Ctrl+Enter sends — click to swap". |
| **Clock** | wide: a small bordered button (padding 3 × 5, radius control, clock icon 13, text-muted); narrow: 34 high (padding 7 × 9, icon 15). Hover: accent border, accent-hover. Title "Schedule this message". Does **not** scale with the text size | Opens / closes the schedule panel (2.10). |
| **Session id** | mono, 10.5→**10** or **11** × fs (pick one step), muted, a dotted 1px border-coloured underline; hover text-secondary; pointer cursor. Hidden until the CLI has reported an id; hidden in a narrow pane | Click copies the **full** uuid to the clipboard and toasts "Session id copied: <uuid>". Title "Claude session <uuid>  — click to copy (use it to resume this conversation)". |
| **Model** | 11 × fs, muted | Read-only. Empty until known. |
| **Cost** | 11 × fs, muted, "$" + 4 decimals | Read-only. Empty when 0. |

## Behaviour

### Typing and auto-grow
- The box starts one line high (its minimum: 34 at any text size) and grows
  with its content, wrapped lines included, up to **180** (at any text
  size), then scrolls inside. Measured: 35, 54, 73, 91, 110, 129, 148, 167,
  180 for 1-9 lines at 1×.
- It shrinks back as text is removed, and to one line when sent.
- Every change also saves the draft (2.8) and checks for the slash menu
  (2.11).
- Native: the minimum and the maximum are structural sizes (control height
  34, the composer maximum 180), the same at every text size.

### Enter, Ctrl+Enter and the switch
- **Enter sends** (the default): Enter with no Shift, Ctrl or Cmd sends
  (Alt+Enter sends too). Shift+Enter makes a newline. Ctrl+Enter does not
  send.
- **Ctrl+Enter sends:** Ctrl+Enter or Cmd+Enter sends; Enter and
  Shift+Enter make newlines.
- The choice is **one setting for every chat**, saved under the browser key
  `crundi_chat_enter` (`"newline"` for Ctrl+Enter, otherwise Enter). The web
  reads it when a pane is built, so other panes already open keep their old
  mode and hint until they are rebuilt (`web-enter-newline`). Native: keep it
  as one persisted setting in the client's settings (it survives a restart);
  applying it to every open pane at once is fine and is the intent of the
  web's comment ("Shared by every chat cell"). Name it in PLAN.md as a
  deliberate difference if it differs.
- The placeholder follows it: "Message Claude…  (Enter to send, Shift+Enter
  for newline)" / "Message Claude…  (Ctrl+Enter to send, Enter for newline)";
  narrow: "Message Claude…"; exited: "Session ended.".
- While the slash menu is open, Enter and Tab pick its item instead (2.11).

### Send (doSend)
1. Trailing whitespace is trimmed. An empty or all-blank box: nothing
   happens (Send is never disabled; there is no disabled look).
2. The box is cleared, the draft cleared, the box shrinks back.
3. The log jumps to the bottom and follows it again, even if the reader had
   scrolled up.
4. `/btw <question>` is asked on the side instead of sent (2.21).
5. **Idle:** POST `/send`, and the composer goes **working at once**
   (optimistic: "working…", Stop, Queue), before the server says so, so a
   quick second Enter queues instead of sending a competing message.
   **Working or needs input:** the text is queued (2.9).
6. The focus stays where it was: in the box after Enter; on the button after
   a click (the web does not refocus the box).
- **There is no separate "sending" look:** the optimistic switch to working
  is it (`web-running` is a few hundred ms after Enter).

### A send that fails
- The server answers `{ok:false, error}` (e.g. "Session is not running",
  "Message is empty", a collaborator's token limit) or the request fails: an
  error box appears in the log with that text ("Failed to send" if none).
- The web then stays on "working…" with Stop and Queue, because nothing from
  the server corrects the optimistic state, and the text is gone (only a
  queued message is given back). `web-send-error` shows it. That is an
  accident. Native: show the same error box in the log, then return to the
  server's real state (idle) and put the text back in the box. Name that in
  PLAN.md as a deliberate difference.

### Stop
- Shown only while the state is **working** (not needing input, not idle,
  not exited).
- **First click arms it:** "Sure?" appears, the red-dim fill, title "Click
  again to interrupt". It disarms by itself **3 s** later.
- **Second click within those 3 s:** disarms and POSTs `/interrupt`. The
  server sends the CLI an interrupt and reports idle straight away (a
  `state` event), which hides Stop and turns Queue back to Send. Errors are
  ignored.
- It also disarms whenever the state leaves working (the turn ended, a
  question came up). It does **not** disarm when the pointer moves away;
  only the timeout, a second click or the end of the turn do.
- No keyboard shortcut (Esc does not stop a turn).

### The meta row, live
| Value | Set from |
|---|---|
| state label, Stop, Send/Queue | the snapshot's `state` (`ui-history`), then every `state` event; `exit` sets "exited" |
| model | snapshot `model`; then `init` (`ev.model`) and `meta` (`ev.model`) events. A fresh chat has none until its first `init` |
| session id | snapshot `sessionId`; then `init` (`ev.sessionId`): the first `init` comes with an empty id, the one on the first turn carries it. Shown from then on |
| mode button | snapshot `permissionMode` (+ `skipPermissions` for the list); `init`/`meta` `permissionMode` (2.6) |
| cost | snapshot `totalCostUsd` only, so it refreshes when the pane is re-subscribed (a reload, a reconnect, a replay), not after each turn; each turn's own cost is in the log's result line |

### Exited
On the snapshot's `status != "running"` or an `exit` event: the box is
disabled and dimmed, its placeholder "Session ended.", the label "exited",
Stop hidden, Send back to "Send" (it does nothing with an empty box);
anything queued is put back in the box; voice stops. Attach, the mode
button, the switch and the clock stay as they are.

### Focus
- Nothing takes the focus when a pane opens.
- The box is focused: when the person clicks it; after the Enter switch is
  clicked; when the app goes to a chat (`goToChat`, the inbox's "open
  chat": the host calls `view.focus()`); after a recall, an attach or a
  give-back puts text in it (the caret at the end).
- Focus look: accent border plus the focus ring.

### Text size (A− / A+)
- Each pane has its own size, 9 to 24 (default 13, the last one chosen for
  that kind), set by the pane header's A−/A+ (2.1). The composer scales by
  `fs = size / 13`:
  - **scale:** the box's text, the placeholder, Send's text and height,
    "Sure?", the label, the mode button (height, min width, icon, text), the
    switch and "sends", the id, the model, the cost;
  - **do not scale:** attach (34), Stop (34 high), the clock, the box's
    minimum (34) and maximum (180), the paddings and gaps.
- It changes live when A−/A+ is clicked.
- Native: font sizes are a scale step × fs, as `ChatPane.fs` does today.

### Up arrow
Up with no modifier, nothing selected and the caret on the box's top
visual row (wrapped rows count) takes back what is queued (2.9). With
nothing queued, or anywhere lower, it moves the caret as usual.

## Data and endpoints

| What | Method, path | Body | Answer |
|---|---|---|---|
| send | POST `/api/ui-sessions/:id/send` | `{text}` | `{ok:true, injected:false\|true, uuid}` or `{ok:false, error}` (always HTTP 200) |
| stop | POST `/api/ui-sessions/:id/interrupt` | none | `{ok:true}` or `{ok:false, error:"No session \"…\""}`; then a `state: idle` event |
| mode | POST `/api/ui-sessions/:id/permission-mode` | `{mode}` | (2.6) |

Live, over the socket: `{type:"subscribe-ui", id}` → `{type:"ui-history",
id, session}` (the snapshot: `state`, `status`, `model`, `sessionId`,
`permissionMode`, `skipPermissions`, `totalCostUsd`, `pendingInjections`,
`slashCommands`…) then `{type:"ui-event", id, event}` with `event.type`:
- `state` `{state: "idle"|"working"|"needs-input"|"waiting"}`;
- `init` `{sessionId, model, slashCommands, permissionMode?}`;
- `meta` `{permissionMode?, model?, plan?}`;
- `history` `{session}` (a whole new snapshot);
- `exit` `{code}`.

The native client already reads these (`src/live.rs`, `src/chat.rs`,
`src/app.rs` 1200, `interrupt` 1381).

Stored on the client: the Enter mode (one setting); the pane's text size
(2.1); the draft (2.8).

## How to bring each state about (for the end-to-end test)

- **idle with id, model, cost:** `tools/test-server.sh <port>`; the seeded
  chat after its first turn.
- **fresh (no id, no model):** `POST /api/ui-sessions/create` and send
  nothing.
- **working:** send "Run exactly this shell command in the foreground with
  timeout 600000 and wait for it to finish: timeout 590 tail -f /dev/null…"
  to a `skipPermissions` chat. Interrupting it (Stop twice) ends it.
- **needs input:** ask the chat to use AskUserQuestion.
- **exited:** kill that chat's `claude` process (a child of the test server
  whose environment has `CRUNDI_CHAT_ID=<id>`), by PID.
- **send error:** send to an exited chat through the API answers
  `{ok:false, error:"Session is not running"}`; in the client, the same
  through the composer of a chat that exited between the click and the send,
  or a stub.
- **narrow:** three panes side by side in a 1400 window (each < 520).

## Checklist

Layout
- [ ] Wide: box, attach, [Stop], Send in one row, bottom-aligned; meta row
      under it: label, mode (icon + name), switch, clock, spacer, id, model,
      cost.
- [ ] The meta row wraps instead of clipping (cost to a second line).
- [ ] Narrow (< 520 of the pane's own width, live while dragging a split):
      box alone; then attach, clock, [Stop], Send stretched; meta: label,
      mode icon only (finger-sized), switch, model, cost; no id.
- [ ] The clock moves between the meta row (small) and the button row
      (34 high) with the width.
- [ ] Placeholder: the hint per Enter mode; "Message Claude…" when narrow;
      "Session ended." when exited.
- [ ] Window < 768: the mode button's name hidden everywhere.

Box
- [ ] Grows from one line (34) to 180, wrapped lines counted, then scrolls;
      shrinks back; one line after sending.
- [ ] Focus: accent border and the ring. Disabled (exited): dimmed.

Enter switch
- [ ] Enter sends / Shift+Enter newline; switched: Ctrl(Cmd)+Enter sends,
      Enter newline.
- [ ] The thumb slides; the live glyph in accent-hover, the other faint.
- [ ] One setting for every chat, persisted across a restart.
- [ ] Clicking it puts the focus back in the box.

Send
- [ ] Never disabled; a blank box does nothing.
- [ ] Clears the box and the draft; follows the log to the bottom.
- [ ] Goes working at once (label, Stop, Queue) before the server's event.
- [ ] Reads "Queue" while working or needing input, "Send" otherwise.
- [ ] A refused send: the error box in the log with the server's text; the
      composer returns to the real state and the text comes back (named
      difference).

Stop
- [ ] Only while working; between attach and Send.
- [ ] Click 1: armed ("Sure?", red-dim, red border, wider; the box gives
      way). Disarms after 3 s, or when the turn ends.
- [ ] Click 2 within 3 s: POST interrupt; Stop goes and Send returns on the
      server's idle.
- [ ] Hover: red border.

Meta
- [ ] Label: idle / working… / needs your input / exited, coloured as above;
      a waiting chat reads idle.
- [ ] Model and id appear live on `init` (no reload); the id only once
      known.
- [ ] A click on the id copies the full uuid and toasts "Session id copied:
      <uuid>".
- [ ] Cost "$x.xxxx" from the snapshot, empty when 0.
- [ ] Mode button shows the current mode's icon and name; opens 2.6.
- [ ] Hover looks: attach and clock (accent), mode (accent border, text
      primary), switch label (text primary), id (text secondary), Send
      (accent-hover), Stop (red).

Text size
- [ ] A−/A+ scales the text parts live (box text, Send, Sure?, the meta row,
      the mode button); attach, Stop, the clock and the box's 34/180 do not.

Theme
- [ ] Every size is a scale step or a structural size from `ui/theme.slint`;
      no typed numbers, no named colours (`white` → a Theme token).
