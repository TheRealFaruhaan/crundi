# 2.9 Queued messages

What happens to a message sent while Claude is busy: the drawers above the
composer that show it, how it goes out, how it is taken back to edit, and
what becomes of it when the turn ends, fails, or the chat exits.

The composer itself (box, Send/Queue, meta row, narrow layout) is 2.5's
(`specs/composer.md`); the drafts it lands in are 2.8's
(`specs/attachments-drafts.md`). This spec adds the two drawers, the
Queue path, the take-back (click and ↑), and the queue's events.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below, never a pixel value.

**There is no list of queued messages and no per-message cancel.** The web
shows at most two drawers, each one block of text, and the only action is
"take it all back into the box". Cancelling is taking back and then
clearing the box. The native client does the same.

Source, `app/vendor/claude-chat.js` (grep with `-a`):
- CSS 262-273 (`.cc-queue`, `-head`, `-body`, `-hint`, `.sent`), 413
  (`.cc-queue.sent.recallable`);
- 1304-1315: `queued`, `queueNode`, `queueTimer`, `handedOver`, `sentNode`;
- 1687-1705: `setState()` (Send's label, the flush at turn end);
- 2454-2537: `postMessage()`, `giveBack()`, `recallQueued()` (the
  take-back, with `cancel-queued`);
- 2539-2635: `TICK_MS`, `enqueue()`, the ticker, `queuedText()`,
  `unqueue()`, `markTrimmed()`, `takeBackHint()`, `renderQueue()`;
- 2758-2822: `renderSent()` (the second drawer), `flushQueue()`;
- 2824-2853: `doSend()`, `sendText()` (idle → send; busy → enqueue);
- 3998-4043: `caretOnTopRow()` and the ↑ key;
- 4930-4935: `applyHistory` → `pendingInjections`;
- 5018-5045: the `queued-state` and `injected` events; 5072-5080: `exit`.
- Other ways into the queue: `voiceSend()` 3256 (`sendText`), `clearGoal()`
  4851 (`enqueue('/goal clear')`), the aside fallback 2742. `/btw …`
  (aside) and the suggestion (`sendSuggestion`, idle only) never queue.

Server, `src/claude-ui.js`: `sendMessage()` 2780-2856 (injected or not, the
uuid, `pendingInjections`), `takeInjectionAck()` 1943, the CLI's
`command_lifecycle` → `handleCommandLifecycle()` 1967, `settleInjections()`
1994 (called on idle, 1084), `cancelMessage()` 2996-3030, history's
`pendingInjections` 3187, the exit order 1517-1540. Routes,
`src/webapp.js` 6486-6548.

## Two drawers, two kinds of "queued"

The web has two stages, and a drawer for each:

| | **Held** (`queued`, `.cc-queue`) | **Handed over** (`handedOver`, `.cc-queue.sent`) |
|---|---|---|
| Where the text is | only in this view's memory | written to the CLI's stdin; the server keeps it in `pendingInjections` |
| When | typed while the chat **needs input** (a permission prompt or question is open: the CLI reads nothing until it is answered) | typed while the chat is **working**, or a held batch flushed |
| Look | dashed accent border, accent tint | solid slate border, slate tint (dimmer) |
| Head | `SENDING` + "N line(s) · as one message, at the next tool call" | `SENDING` + "not read yet" (recallable) / `SENT` + "waiting for Claude to pick it up" / `SENT` + "Claude is reading it" |
| Leaves when | flushed (state leaves needs-input, or turns idle), taken back, or the chat exits | the CLI reads it (`injected`), the turn ends (`injected`, assumed), cancelled, or the chat exits |
| Survives a reload / restart / reconnect | **no** (memory only; see "Native" below) | **yes**: the server sends it back in the history |

Both can show at once: the handed-over drawer on top, the held one under
it, then the input row (`web-both-drawers`).

## Screenshots

`specs/queued-messages/`, from the web UI at 1400×860 (layout: one wide
chat on top; under it a wide "Second chat" and a "Narrow chat").
`-2x`: the region ×2. Each chat is a real Claude turn on the test server
(a 900 s foreground `tail` behind a permission prompt), except where
"faked" says a server frame was played into the page to reach a state the
real CLI passes through too fast to catch.

| File | State |
|---|---|
| `web-needs-input-rest` | needs input, nothing queued: Send reads **Queue** (2.5) |
| `web-held-one-touchwording` | one held line. Hint "Tap to take it back": the hidden browser has no fine pointer. **Not the PC wording**, kept to show the difference |
| `web-held-two` | two lines held: "2 lines · as one message, at the next tool call", the body one line each. Hint "Click or press ↑ to take it back" (the PC's) |
| `web-held-two-hover` | the same, hovered: border accent, fill accent-dim |
| `web-held-long-trimmed` | a long held batch: the body clamped at 4 lines with "…", the hint "Shown trimmed · Click or press ↑ to take it back" |
| `web-taken-back` | ↑ with "half-typed next line" in the box: the drawer is gone, the box holds every held line, then the box's own text, caret at the end |
| `web-held-narrow` | held, in the narrow chat (415 wide) |
| `web-held-very-narrow` | held, in a 322-wide chat: the head's note wraps beside `SENDING`, which stays vertically centred |
| `web-handed-recallable` | the narrow chat working; what was held went out when the prompt was allowed: slate drawer, `SENDING` "not read yet", the take-back hint |
| `web-handed-taken-back` | ↑ on that drawer (a third line had been sent meanwhile): two `cancel-queued` calls, both `cancelled: true`; every line back in the box |
| `web-sent-waiting` | (faked `queued-state: sent`) handed over, the CLI has not said it is queued yet: `SENT` "waiting for Claude to pick it up", hint "Waiting for the session to confirm"; cursor default, ↑ does nothing |
| `web-sent-started` | (faked `queued-state: started`) the CLI is reading it: `SENT` "Claude is reading it", hint "Claude is reading this one — too late to edit" |
| `web-takeback-error` | (stubbed reply) take-back failed: a red error row "The session did not answer in time" in the log; the drawer stays |
| `web-send-failed` | (stubbed reply) the send of a flushed batch refused: the drawer goes, its text is put back at the top of the box, error row "Failed to write to the session" |
| `web-handed-after-reload` | a page reload with a message handed over (sent through the API from "another device", then a held line flushed by the state change): the drawer is back, from the history |
| `web-both-drawers` | (faked `state: needs-input`) one handed over and one held: slate drawer on top, dashed drawer under it |
| `web-exit-with-queue` | the chat's process killed with one handed over and one held. The handed-over line is in the log as a user message (the server's "assumed" settle); the held line is **lost** (see "Exit"), an error row "Session is not running"; the box "Session ended." |

Turn end, real, not a screenshot: with "Also say which shell you used"
handed over, the turn was interrupted; the drawer went at once (`state`
→ idle 2.6 s later, `injected`), the line appeared in the log as a user
bubble, and Claude answered it as the next turn ("It used bash…").

## The parts

Inside the composer's `.cc-wrap` (2.5's tree), before the input row:

```
.cc-wrap
  .cc-queue.sent          handed-over drawer   (only while handedOver is not empty)
  .cc-queue               held drawer          (only while queued is not empty)
  .cc-inrow               the box and its buttons (2.5)
  .cc-up                  upload bar (2.7)
```

Each drawer, the same parts:

```
.cc-queue                 block; margin-bottom ~sp-8; padding sp-8 × sp-12; r-card; 1px border
  .cc-queue-head          one row, items centred, gap sp-6; margin-bottom sp-2
    label                 "SENDING" | "SENT": text-10 × fs, w-bold, uppercase, track-wide
    note                  text-10 × fs, w-medium, no uppercase, no tracking, 70% of the label's colour; wraps in its own box
  .cc-queue-body          the text: text-12 × fs (web 12.5), preserves newlines, wraps anywhere; at most 4 lines, "…" at the cut
  .cc-queue-hint          text-10 × fs, text-muted; sp-4 above
```

"× fs" is the pane's text scale (A− / A+, 2.5's `ChatPane.fs`): the web
scales all three texts by `--cc-fs`.

Measured (web, wide Second chat, 2 lines): drawer 689 × 94 at fs 1,
head 16 high, body 38 (two lines of 19.4), hint 16. The drawer spans the
composer's full content width in both wide and narrow layouts; nothing in
it changes with `.cc-narrow`.

### Colours

| Part | Held | Held, hover | Handed over (`.sent`), any hover |
|---|---|---|---|
| Border (1px) | **dashed** accent at 55% → `accent-line` | dashed `accent` | **solid** slate at 45% → new token `slate-edge` (#94a3b873) |
| Fill | accent at 8% → `accent-haze` | `accent-dim` | slate at 9% → new token `slate-haze` (#94a3b817) |
| Label | `accent-hover` | same | `text-muted` |
| Note | label colour at 70% | same | `text-muted` at 70% |
| Body | `text-primary` | same | `text-secondary` |
| Hint | `text-muted` | same | `text-muted` |
| Pointer | hand | hand | hand only when recallable, else arrow |

The handed-over drawer has no hover change at all (the web pins its hover
to the rest look). Transition: the web eases border and fill over 140 ms;
native may switch at once.

The dashed border is `DashedBox` (`widgets.slint`), thick 1px, dash sp-4,
gap sp-2, corner r-card, as the dragged-pane outline uses.

### Wording (exact)

Held drawer:
- label `Sending`, shown uppercase;
- note `1 line · as one message, at the next tool call` / `N lines · as one
  message, at the next tool call`. **N counts sends, not text lines**: three
  Enters make "3 lines" even if one of them had five lines of text;
- hint `Click or press ↑ to take it back`;
- tooltip `Click to edit — brings every queued line back to the message box`.

Handed-over drawer, from its slots (the first that applies):
- any slot recallable (has a uuid and state `queued`): label `Sending`, note
  `not read yet`, hint `Click or press ↑ to take it back`, tooltip
  `Click to edit — brings it back to the message box`;
- else any slot `started`: label `Sent`, note `Claude is reading it`, hint
  `Claude is reading this one — too late to edit`;
- else: label `Sent`, note `waiting for Claude to pick it up`, hint
  `Waiting for the session to confirm`.
- Not recallable, tooltip: `Claude has this one; it joins the conversation
  as it is read`.
- Body: every slot's text, joined with newlines, oldest first.

Trimmed (either drawer): when the body is cut at 4 lines, the hint gets
`Shown trimmed · ` in front.

**The hint and Enter mode.** The hint does not change with the Enter
switch (Enter sends / Ctrl+Enter sends). It changes only with the pointer:
`Tap to take it back` on a touch-only device (`any-pointer: fine` false).
The PC client always has a mouse, so the native client always shows the
"Click or press ↑" wording. The Enter switch only decides which key sends,
and therefore which key queues (2.5): with "⏎ sends", Enter queues and
Shift/Ctrl+Enter is a newline; with "⌃⏎ sends", Ctrl+Enter queues.

## Behaviour

### Sending while busy (`doSend` → `sendText`)

Send (or Enter, per the switch) with text in the box, the box is cleared
and the draft with it (2.8), then by the chat's state:

- **idle:** a normal send (2.5): POST `/send`, the chat shows working at
  once. Nothing is queued.
- **working:** handed over at once. A slot `{text, uuid: null, state:
  'sent'}` is added to the handed-over drawer, POST `/send`, and the chat
  stays/goes working. There is **no delay and no batching** here: each
  Enter is its own slot, all shown in the one drawer.
- **needs input:** held. The text is added to the held list and the held
  drawer drawn; a ticker every 300 ms checks the state, and the moment it
  is anything but needs-input (the prompt answered: working; or idle) the
  whole held list goes out **as one message**, its lines joined with
  newlines (`flushQueue`), as a handed-over slot.
- Turning **idle** from anything else also flushes the held list at once
  (`setState`), and if that send goes out while the server is idle, it is a
  plain send (`injected: false`, see below).

Send's label is `Queue` whenever the state is not idle (2.5). `/btw …` is
asked on the side and never queues. A voice message (2.24) and
`/goal clear` (2.19) take the same path as a typed one.

### The handed-over slot's life

1. POST `/send` → `{ok, injected, uuid}`.
   - `ok: false`, or a network error: **give back** (below) and an error row.
   - `injected: false`: the server was idle after all and put it straight
     into the log; the slot is dropped (no drawer for it).
   - `injected: true`: the slot takes the `uuid` if it has none yet.
2. Event `queued-state {uuid, state, text}` (from the CLI's
   `command_lifecycle`): the slot is found by uuid, or else the oldest slot
   with no uuid and the same text (and it adopts the uuid). `queued` and
   `started` set its state; `cancelled`, `discarded`, `refused` remove it.
3. Event `injected {text, assumed?}`: the CLI took a message. The slot
   whose text matches is removed (the oldest slot if none matches). At the
   same moment the server adds the user entry to the log (`entry` event),
   at the point Claude read it, not where it was sent.
4. Turn ends (server state idle): the server settles every pending one as
   read (`injected` with `assumed: true`, plus their user entries).

### Take-back (`recallQueued`): click either drawer, or ↑

One action takes **everything still waiting** back to the box:
1. The held list (if any) is taken at once and its drawer goes.
2. Every handed-over slot that is recallable (uuid and state `queued`) is
   cancelled: POST `/cancel-queued {uuid}` for each, in parallel.
3. When all have answered:
   - `{ok: true, cancelled: true}`: the slot goes, its text comes back;
   - `{ok: true, cancelled: false}`: too late, the CLI already has it; the
     slot is marked `started` (the drawer then reads "Claude is reading
     it"); nothing comes back and **no error** is shown;
   - `ok: false` or a network error: the slot stays; one error row in the
     log with the first error (`error`, or "Could not take it back").
4. The box becomes: the cancelled texts (oldest first), then the held text,
   joined with newlines, then a newline and the box's own text if it had
   any (trimmed). The draft is saved (2.8), the box grows, gets focus, and
   the caret goes to the very end.

A click on a non-recallable handed-over drawer does the same call, which
then finds nothing to cancel: in effect nothing happens (only a held
batch, if any, comes back).

**↑ (Up arrow)** in the box does the take-back when all of these hold: no
Shift/Ctrl/Alt/Meta; no selection; the caret is on the box's first visual
row (position 0, or no newline before it and it has not wrapped to a
second row); and there is something to take back (a held list, or a
recallable slot). Then the key's default is suppressed. Otherwise ↑ moves
the caret as usual. When the slash menu (2.11) is open, ↑ moves in the
menu instead and never takes back.

### Give back (a refused send)

When `/send` of a handed-over slot fails, the slot leaves the drawer and
its text is put **in front of** what the box holds (text, newline, the
box's text), the draft saved, the box grown; an error row with the
server's `error` ("Failed to send" if none). The box does not take focus.
(`web-send-failed`.)

### The turn ends

- `state: idle` from working/needs-input: the held list is flushed (one
  message). The handed-over drawer empties through the server's settle
  (`injected` events), and the messages appear in the log.
- A turn that ends in an error (an `error` entry, then `state: idle`) is
  the same: the queue does not care how the turn ended. The flushed
  message starts the next turn.
- **Stop** (2.5's interrupt): the same as any idle; whatever was handed
  over is read as the next turn (seen live: interrupted, the line appeared
  as a user bubble and was answered).

### Exit (the chat's process ends)

What the web means to do (`case 'exit'`): hand the held text back into the
box (`unqueue`, which does **not** save the draft), clear the handed-over
drawer (beyond recall, never to be acknowledged), then the 2.5 exited look.

What it actually does: the server reports `state: idle` **before** `exit`
(`claude-ui.js` 1538-1539), so the idle flush has already sent the held
text to a dead session; `exit` clears that slot from the drawer, and when
`/send` answers "Session is not running" the give-back finds no slot and
drops the text. The held text is lost; only the error row remains
(`web-exit-with-queue`). The handed-over lines were settled by the server
as read and sit in the log as user messages.

**Native (a named difference, a web bug):** the held text survives an
exit. Either do not flush on an idle that arrives with the chat already
gone, or let give-back restore the text even when its slot was cleared.
The test below checks that it ends up in the box (and the draft).

### Reload, restart, reconnect, pane and project switches

- **Handed over:** server state. Any history load replaces the
  drawer's slots with `session.pendingInjections` (`{text, uuid, state}`).
  (The web gets it as the `ui-history` frame the server sends on every
  `subscribe-ui`, `webapp.js` 9255-9268; the native client loads history
  its own way, 2.4, and must take `pendingInjections` from it.)
  So it survives a reload, an app restart, a reconnect, a pane move and a
  project switch (`web-handed-after-reload`).
- **Held:** memory of that chat view only. In the web it is lost on a
  reload, on closing the pane, and on a project switch (the workbench
  destroys the other project's chat views; checked: held line, switch to
  "Other", back to "Demo", gone, box empty). Moving or resizing a pane
  keeps the view and the held list. On reconnect the history's
  `state` is applied (`setState(session.state)`), so if the chat went idle
  meanwhile the held list is flushed then.
- **Native:** held lines are per chat (keyed by chat id), not per view, so
  they survive pane moves, splits and fold changes, and **a project
  switch** (the chat is still running and the prompt still open). On app
  exit, or when the chat's pane is closed, the held text is not dropped:
  it is put into the chat's draft (2.8: in front of the draft's text, as
  give-back does), so it comes back in the box after a restart. A named
  difference (the web loses typed text in these cases).

### Narrow panes

Nothing changes but the width: the drawer spans the composer's content
width, the body wraps, the head's note wraps in its own box beside the
label (`web-held-very-narrow`). The 4-line clamp counts wrapped rows.

## Data and endpoints

| Call | Body | Reply |
|---|---|---|
| `POST /api/ui-sessions/:id/send` | `{text}` | `{ok: true, injected: bool, uuid}`, or `{ok: false, error}` ("Session is not running", "Message is empty", "Failed to write to the session", a token limit…). `injected` is true when the server was not idle: the text is then held in `pendingInjections` and goes into the log only when read |
| `POST /api/ui-sessions/:id/cancel-queued` | `{uuid}` | `{ok: true, cancelled: bool}`; `{ok: false, error}`: "No message id", "That message is no longer pending", "The session did not answer in time" (10 s), "Failed to write to the session", `No session "…"` |
| `GET /api/ui-sessions/:id/history` | | `session.pendingInjections: [{text, uuid, state: 'sent'|'queued'|'started'}]`, and `session.state` |

Live (`ui-event` frames, `{type: 'ui-event', id, event}`):

| `event.type` | Fields | Effect |
|---|---|---|
| `state` | `state` | Send's label; idle → flush held; leaving needs-input → the ticker flushes held |
| `queued-state` | `uuid, state, text` | `queued` / `started`: set the slot's state; `cancelled` / `discarded` / `refused`: drop it |
| `injected` | `text, assumed?` | drop the slot with that text (else the oldest) |
| `entry` | `entry {kind: 'user', text}` | the read message in the log (2.5's log) |
| `exit` | `code` | held → back in the box; handed-over cleared; exited look |

`GET /api/state`'s chat `pendingInjections` is a count (it keeps a
project "busy" in the sidebar, 2.4); not needed by this screen.

A server without `cancel-queued` (none exist today, but per the
backwards-compatible rule): a 404 or "not supported" reply is shown as an
error row "Taking back is not supported by this server", the slot stays.

## Tokens

accent-line, accent-haze, accent-dim, accent, accent-hover, text-primary,
text-secondary, text-muted, r-card, sp-2, sp-4, sp-6, sp-8, sp-12,
text-10, text-12, w-medium, w-bold, track-wide; new: **slate-edge**
(#94a3b873), **slate-haze** (#94a3b817). `DashedBox` for the dashed edge.

Slint notes (rule 5, PLAN.md "Known gaps" if they bite):
- the 4-line clamp with "…": a `Text` with `wrap: word-wrap`, `overflow:
  elide` and a max height of 4 × the line; if Slint will not elide a
  wrapped text, clip at 4 lines and keep "Shown trimmed ·" as the cue;
- "wraps anywhere" (`word-break: break-word`): Slint's word-wrap breaks a
  word longer than the row too;
- ↑ "on the first visual row": if the TextEdit cannot report the caret's
  row, measure the text before the caret in a hidden `Text` of the box's
  width.

## How to bring each state about (for the end-to-end test)

All on a `tools/test-server.sh` server with real Claude turns, as 2.4 and
2.5 do:

- **Working:** send "Run exactly this shell command in the foreground with
  timeout 600000 and wait for it to finish: timeout 900 tail -f /dev/null…"
  in a chat with `--dangerously-skip-permissions` (or allow the prompt).
- **Needs input:** the same in a chat that asks permission: the Bash prompt
  stays open until answered.
- **Handed over, recallable:** type and send in the working chat; within a
  second `queued-state: queued` arrives.
- **Turn end:** POST `/interrupt` (or a short `timeout 20`): the handed-over
  line lands in the log.
- **Exit:** kill the chat's `claude` process (a child of the test server
  whose environ has `CRUNDI_CHAT_ID=<id>`).
- **Started / waiting / cancel failures:** the native test hook can play a
  `queued-state` event into a chat, and stub replies, as the 2.7 tests do.

## End-to-end tests (phase2.mjs, 2.9)

1. **Queue while working, sent at the turn's end** — chat working; type
   "queued one", Enter: the box empties, a handed-over drawer shows the
   text; the server's `pendingInjections` has it. End the turn (interrupt,
   or a short wait): the drawer goes and the log has a user message
   "queued one".
2. **Held while needs input, flushed as one** — chat on a permission
   prompt; send "a" then "b": one held drawer "2 lines · as one message, at
   the next tool call", body "a⏎b"; the server has nothing pending. Allow
   the prompt: the held drawer goes, the server gets **one** send "a\nb"
   (pendingInjections or the log).
3. **Queue, then cancel: nothing is sent** — chat working; send "do not
   send me"; when the drawer reads `SENDING not read yet`, click it: POST
   `cancel-queued` → cancelled; the drawer goes; the box holds the text;
   clear the box; end the turn: the log has **no** "do not send me".
4. **Take back with ↑** — held "x" (needs input) and box "y", caret at 0:
   ↑ → box "x\ny", caret at the end, focus in the box, drawer gone,
   draft saved (restart the app: the box still says "x\ny"). With the
   caret on a lower row, ↑ only moves the caret.
5. **Both drawers, one take-back** — handed-over "h" (working) then held
   "k" (needs input, via the test hook): order slate-over-dashed; click
   the dashed one: box "h\nk".
6. **Too late** — stub `cancel-queued` → `{ok: true, cancelled: false}`:
   no error row; the drawer reads `SENT Claude is reading it`, hint "Claude
   is reading this one — too late to edit", arrow pointer, ↑ does nothing.
7. **Errors** — stub `cancel-queued` → `{ok: false, error: "E1"}`: an error
   row "E1", the drawer stays. Stub `/send` → `{ok: false, error: "E2"}`
   for a handed-over slot: the drawer goes, the text is in front of the
   box's text, an error row "E2".
8. **Survives** — handed over, then restart the app: the drawer is back
   (from history). Held, then switch project and back: still held (native).
   Held, then quit the app: after restart the text is in the box.
9. **Exit** — held "z", kill the chat's process: the box (disabled,
   "Session ended.") still holds "z" in its draft; the handed-over drawer is
   gone.
10. **Wording and look** — label/note/hint strings as above, for 1 and 3
    lines; a 6-line body is cut at 4 rows with the "Shown trimmed · " hint;
    narrow (< composer-stack) and very narrow panes: the drawer full width,
    the note wrapping.

## Checklist

- [ ] Held drawer (needs input): dashed accent-line edge, accent-haze fill,
      r-card, padding sp-8 × sp-12, above the input row
- [ ] Held hover: accent edge, accent-dim fill, hand pointer
- [ ] Handed-over drawer (working): solid slate-edge, slate-haze fill, label
      and note muted, body text-secondary; no hover change
- [ ] Both shown: handed-over on top, held under it, then the input row
      (and the upload bar after it)
- [ ] Head: label uppercase w-bold track-wide text-10 × fs; note w-medium
      at 70%, wraps in its own box; body text-12 × fs; hint text-10 × fs muted
- [ ] All wording exact: `SENDING` / `SENT`; the three notes; "N line(s) ·
      as one message, at the next tool call"; the three hints; tooltips
- [ ] Hint is always the "Click or press ↑" wording (never "Tap"); it does
      not change with the Enter switch
- [ ] Body clamped at 4 rows with "…", hint prefixed "Shown trimmed · "
- [ ] Line count counts sends, not text lines
- [ ] Send reads Queue when not idle; Enter/Ctrl+Enter per the switch
- [ ] Working: each send handed over at once (POST /send), a slot per send
- [ ] Needs input: held locally, flushed as ONE message (joined with
      newlines) when the state leaves needs-input or turns idle
- [ ] `/send` reply: injected false → no drawer; uuid adopted
- [ ] `queued-state` (queued / started / cancelled / discarded / refused) and
      `injected` (by text, else oldest) drive the handed-over drawer
- [ ] Take-back by click (either drawer) and by ↑ on the first visual row
      (no modifiers, no selection, something to take back); slash menu's ↑
      wins
- [ ] Take-back: cancel-queued per recallable slot; order cancelled texts,
      held text, then the box's text; focus, caret at end, draft saved
- [ ] cancelled false → "Claude is reading it", no error; failure → one
      error row, drawer stays
- [ ] Refused send → give back in front of the box's text + error row
- [ ] Turn end (normal, error, Stop) → handed-over drawer clears as the
      messages land in the log
- [ ] Exit: held text kept (named difference), handed-over cleared,
      exited look
- [ ] Handed-over survives reload/restart/reconnect (history
      `pendingInjections`)
- [ ] Held survives pane moves and project switches; on pane close / app
      quit it goes into the draft (named difference)
- [ ] Narrow and very narrow panes: full width, wrapping head and body
- [ ] Tokens only; slate-edge and slate-haze added to theme.slint
- [ ] Named differences in PLAN.md: exit keeps held text; held text kept
      across project switch / close / quit
- [ ] Side-by-side crops for every state in `queued-messages/review/`
