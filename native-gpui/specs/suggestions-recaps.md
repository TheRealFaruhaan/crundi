# 2.17 Suggestions and recaps

Two things Claude Code writes about a chat that are not part of the
conversation, shown floating over the bottom right of the log:

- **The suggestion** (`.cc-sug`): Claude Code's guess at the person's next
  message, after a turn ends. A ghost of the person's own bubble (same shape,
  right-aligned), dashed and muted so it never reads as sent, with a caption
  "Suggested, tap to send". A click **sends it straight away** (not into the
  box); its × drops it, here and on the server.
- **The recap** (`.cc-recap`): where the chat stands, for someone coming
  back. A card with an accent bar on its left, a small capital caption
  ("WHILE YOU WERE AWAY" or "WHERE THIS CHAT STANDS") and one or two
  sentences; its × drops it, here and on the server.

Both live in the pane's **floating dock** (`.cc-agents`), the same column
the sub-agent badges of 2.18 will sit in. Rule 2 of DESIGN.md applies: every
size and colour below is a theme token.

Source:
- `app/vendor/claude-chat.js`
  - CSS 309-311 (`.cc-agents`, the dock), 414-426 (`.cc-sug-row`,
    `.cc-sug-cap`, `.cc-sug`, `.cc-sug-x`, `.cc-recap-row`, `.cc-recap`,
    `.cc-recap-cap`), 297-306 (`.cc-jump`).
  - `renderSuggestion` 2640, `sendSuggestion` 2663, `dismissSuggestion`
    2672, `renderRecap` 2683, `dismissRecap` 2706.
  - `setState` 1702-1704 (a state other than idle drops the suggestion),
    `postMessage` 2453-2458 (any message going out drops both),
    `askAside` 2723 (a side question drops the recap, 2.21).
  - `dockLayout` 4189-4209 (the log's bottom padding, the jump button above
    the dock, the suggestion kept last).
  - `applyHistory` 4947-4952 (from `ui-history`: `session.suggestion`,
    `session.recap`), the `ui-event` switch 5052-5053 (`suggestion`,
    `recap`), the `exit` event 5084-5090 (both go).
- Server `src/claude-ui.js`
  - `handlePromptSuggestion` 1667 (the CLI's `prompt_suggestion` frame; kept
    only while idle), `clearSuggestion` 1676, `dismissSuggestion` 1684,
    `sendMessage` 2798-2801 (any send clears both).
  - `RECAP_PROMPT` / `RECAP_IDLE_MS` 76-78 (5 minutes; env
    `CRUNDI_RECAP_IDLE_MS`), `clearRecap` 2706, `dismissRecap` 2714,
    `scheduleIdleRecap` 2726 (after each turn), `makeRecap` 2740, the
    resumed recap 2580.
  - The session snapshot 3183-3184 (`suggestion`, `recap`).
- `src/webapp.js` 6486-6529 (the routes); `src/client-version.js` 143-151
  (capabilities `chat-recap`, `chat-aside`; the suggestion needs none).

**Not** `isDismissed` / `markDismissed` / `loadDismissed` /
`persistDismissed` (claude-chat.js 768-800, 4272-4301): those keep the
**agent badges'** dismissals (2.18, `crundi_chat_dismissed` in localStorage
plus `POST …/dismiss-agents`). A suggestion's or a recap's dismissal is kept
by the **server** alone (it clears its copy), which is why it survives a
reload or a client restart. The native client keeps nothing locally for
these two.

## Screenshots

`suggestions-recaps/web-<state>.png` at 1400×860, one wide pane unless
named. A `-2x` file is the named region cropped and enlarged. The recaps are
real (an idle recap with `CRUNDI_RECAP_IDLE_MS=25000`, a resumed one from a
chat that continued an earlier conversation); the suggestions were injected
through the page's WebSocket as the server's own `ui-event` frame, since
this machine's Claude Code sent none in four turns (the CLI decides when).

| State | What it shows |
|---|---|
| `web-none` | Nothing in the dock: the log runs to the composer, no padding. |
| `web-suggestion` (+`-2x`) | **A suggestion, idle.** Caption "Suggested, tap to send" right-aligned above a dashed, muted bubble "Add due dates to each idea" with its × in the top right corner. The log is padded so the last reply's result line clears it. |
| `web-suggestion-hover` (+`-2x`) | **Hovered** (also its keyboard-focus look): solid accent edge, `accent-dim` fill, text `text-primary`. |
| `web-suggestion-x-hover` (+`-2x`) | **× hovered**: × in `text-primary` on a `slate-dim` rounded square; the bubble under it is hovered too. |
| `web-suggestion-long` (+`-2x`) | **A long suggestion**: the bubble at the dock's widest (320), wrapping, keeping the text's own line break; the × stays in the top right; the recap above it. |
| `web-suggestion-sent` | **After a click**: the text is in the log as the person's own message, the chat is WORKING, the dock is empty, the box untouched. |
| `web-recap-idle` (+`-2x`) | **An idle recap** ("WHILE YOU WERE AWAY"), alone. It floats over the end of the last reply (see Behaviour: the web pads the log only for a suggestion). |
| `web-recap-x-hover` (+`-2x`) | The recap's × hovered. |
| `web-recap-resumed` (+`-2x`) | **A resumed recap** ("WHERE THIS CHAT STANDS"), in a chat that continued a conversation. |
| `web-recap-and-suggestion` (+`-2x`) | **Both at once**: the recap above, the suggestion last (always), right-aligned. |
| `web-scrolled-up-jump` | **Read up** with both shown: the jump button sits above the dock, not beside it. |
| `web-narrow-370` (+`-2x`) | **A narrow pane** (370): the dock keeps its 320 at most, `pane − 20` at least; recap and suggestion over the log. Slint's `pane-min` (340) means the dock never goes under 320 on the desktop. |

## Parts and arrangement

### The dock

```
                                    ┌─ the log ──────────────────────────┐
                                    │ …                                  │
                                    │            [badges row, 2.18]      │
                                    │   ┃ WHILE YOU WERE AWAY          × │ ← recap, the dock's width
                                    │   ┃ You're testing … next action.  │
                                    │            Suggested, tap to send  │ ← caption
                                    │          ┆ Add due dates …      × ┆│ ← suggestion, right-aligned
                                    └────────────────────────────────────┘
                                      composer
```

| Part | Web | Token |
|---|---|---|
| Dock | absolute over the log: right 10, bottom 8; a column, items right-aligned, gap 5; at most `min(320, pane − 20)` wide, as wide as its widest item; above the log (z 15); the pointer passes through it except on its items | `sp-10`, `sp-8`, gap `sp-4`; a new structure token `chat-dock-w` 320 (2.18's badges share it) |
| Order | badges row (2.18, hidden while empty) · recap · suggestion. The suggestion is always last, whatever is added after it | |
| Log padding | while a suggestion (or a badge) is shown: the dock's height + 14 at the bottom of the log, so the newest line can scroll clear of it; 4 otherwise | dock height + `sp-12` (one step off) |
| Jump button | while a suggestion or a recap is shown, its bottom is the dock's height + 16 (above the dock); otherwise 10 | dock height + `sp-16` |

### The suggestion (`.cc-sug-row`)

| Part | Web | Token |
|---|---|---|
| Row | column, right-aligned, gap 3, margin-top 3 | gap `sp-2`, `sp-4` above |
| Caption | "Suggested, tap to send", 10.5 `text-muted`, 4 in from the right; on a `bg-primary` chip (padding 0 4, radius 4) so it reads over log text | `text-11` × fs, `sp-4`, `r-chip`, `bg-primary` |
| Bubble | padding 7 30 7 11; 1px **dashed** edge `#818cf8` at 55%; `bg-secondary` fill; corners 12 12 **3** 12 (the bubble's one sharp corner, bottom right, as the person's own bubble); shadow 0 4 14 black 35%; text `text-secondary`, the log's font and size (13 × fs, line 1.55), wraps anywhere, keeps newlines | `sp-8`/`sp-6` vertical, `sp-12` left, `sp-24 + sp-6` right; `DashedBox`; new colour token `sug-edge` (#818cf88c); `r-bubble` + the bubble's sharp-corner token; `shadow-menu`; `text-13` × fs, `line-chat` |
| Bubble width | as its text, at most the dock's width | |
| Hover / focus | edge solid `#6366f1` at 55%, fill `accent-dim`, text `text-primary`; 0.14 s | new `sug-edge-on` (#6366f18c), `accent-dim`, `motion-fast` |
| × | 22 × 22, top 4 right 4 inside the bubble; "✕" 11 × fs `text-muted`; radius 6; hover `text-primary` on `slate-dim` | `control-sm − sp-6`, `sp-4`, the `x` icon at `icon-sm`, `r-control`, `slate-dim` |
| Tooltips | bubble "Tap to send"; × "Dismiss" (aria "Dismiss suggestion") | the shared tooltip |

### The recap (`.cc-recap-row`)

| Part | Web | Token |
|---|---|---|
| Row | stretches to the dock's width; margin-top 3 | `sp-4` |
| Card | padding 8 30 8 11; 1px `border-subtle` edge with a **3px `accent` left edge**; `bg-secondary`; radius 8; shadow 0 4 14 black 35% | `sp-8`, right `sp-24 + sp-6`, left `sp-12`; bar `sp-2`+1 → use `sp-4` (one step off) or 3px named `bar-w`; `r-field`; `shadow-menu` |
| Caption | its own line, 10.5 / 600, upper case, letter spacing 0.04 em, `text-muted`, 2 below | `text-11` × fs, `w-semibold`, `track-wide`, `sp-2` |
| Caption text | `why == "resumed"` → "Where this chat stands"; otherwise "While you were away" (shown in capitals) | |
| Text | 12.5 × fs `text-secondary`, line 1.45, wraps anywhere, keeps newlines; at most 400 characters (the server cuts it) | `text-12` × fs, `line-chat` |
| × | the suggestion's × (same class), top 4 right 4; tooltip "Dismiss" (aria "Dismiss recap") | as above |
| Hover | none on the card (it does nothing on a click); only the × reacts | |

Both scale with the pane's A-/A+ (the web's `--cc-fs`); the dock's width,
gaps, the bar and the × box do not.

## Behaviour

### When they appear (the server's events)

- **Suggestion.** After a turn ends Claude Code may send a
  `prompt_suggestion` frame (not on the first turns, in plan mode, near a
  rate limit or while a prompt is pending: the CLI decides). The server keeps
  it only if the chat is **idle** (`s.suggestion = {text, uuid}`) and sends
  the `ui-event` `{type:'suggestion', suggestion:{text, uuid}}`. It goes with
  `{type:'suggestion', suggestion:null}` when any message is sent into the
  chat or the person dismisses it.
- **Recap.**
  - *Idle:* after each turn the server waits `RECAP_IDLE_MS` (5 min); if the
    chat is still idle on the same turn it asks Claude Code a side question
    (`RECAP_PROMPT`) and sends `{type:'recap', recap:{text, at, why:'idle'}}`.
    Once per turn. Not for factory or scheduled (background) chats.
  - *Resumed:* a chat that picked up an earlier conversation
    (`--continue` / `--resume`, not a fork, not a /compact opener, not a
    factory chat) gets one right after the CLI starts:
    `why:'resumed'`. A conversation with no history gets none.
  - It goes (`{type:'recap', recap:null}`) when a message is sent, the
    person dismisses it, or the chat exits.
- **On opening a chat** (`ui-history`): `session.suggestion` and
  `session.recap` are shown as they stand on the server. A dismissed one is
  `null` there, so it does not come back after a reload or a restart of the
  client. Both live in the server's memory only: a server restart loses them.
- **Exit** (`ui-event` `exit`): both go at once.

### When the suggestion shows

Shown only while **all** hold: there is one, the chat is **idle**, and the
box is enabled (the session is running). The moment the chat goes working,
needs input, waiting or exited, it is dropped (the web sets it to null on
any non-idle state; a later idle does not bring it back). A new suggestion
replaces the old one's text in place. There is **one** suggestion and **one**
recap per chat at most; each pane has its own dock.

### Sending a suggestion

A click (anywhere on the bubble but the ×), only while idle:
1. the log re-pins to the bottom (as a send from the box does);
2. `POST /api/ui-sessions/:id/send {text}` with the suggestion's text
   exactly — the same path as the composer's send (2.5), so a refused send
   behaves as one;
3. the suggestion and the recap go at once (locally; the server clears its
   copies too);
4. the chat shows working at once (optimistic, as the composer's send).

The composer's box and draft are **not** touched: the suggestion never goes
into the box, and whatever is typed there stays.

Keyboard: the web makes the bubble focusable (`tabindex=0`, role button,
the hover look on focus) but gives it no key handler, so Enter does nothing
there. **Native:** Tab reaches the bubble and its ×; Enter or Space on the
bubble sends, on the × dismisses (a named difference: the web's role=button
with no key is an accident).

### Dismissing

- **Suggestion ×:** it goes at once, then `POST
  /api/ui-sessions/:id/dismiss-suggestion` (no body). Errors are ignored.
- **Recap ×:** it goes at once, then `POST
  /api/ui-sessions/:id/dismiss-recap` with `{}`. Errors are ignored (it goes
  anyway with the next message). A server without `chat-recap` never sends a
  recap, so the call is never made there.
- **Also gone without a click:** both on any message sent (the box, a
  suggestion, a queued flush, a scheduled send arriving), the recap on a
  side question (2.21 calls `dismissRecap`, so that one does POST), both on
  exit.
- Surviving a restart: nothing to keep locally. After a restart the client
  shows what `ui-history` says; the server has `null` for a dismissed one.

### Scrolling

- A suggestion or recap appearing (or changing) while the log follows the
  bottom keeps it following (the web measures `atBottom()` first and
  scrolls down after); while reading up it does not move the log.
- The log's bottom padding grows with the dock while a suggestion (or a
  badge) is shown, so the newest message scrolls clear of the dock.
- **Recap alone:** the web does not pad the log for it (`dockLayout` counts
  only the suggestion and the badges), so a recap covers the end of the last
  reply until it is dismissed (`web-recap-idle`). That is an accident of
  `dockLayout` (its comment says the newest message must always scroll clear
  of the dock): **native pads for the recap too** — a named difference,
  PLAN.md.
- The jump button rises above the dock while a suggestion or a recap shows.

### Narrow panes

The dock is `min(320, pane − 20)` wide and right-aligned in every width;
nothing folds or moves. At the desktop's least pane (340) it is still 320.
The suggestion's caption stays one line; the bubble and the recap wrap.

## Data and endpoints

| What | Method, path | Body | Answer |
|---|---|---|---|
| The chat as it stands | WS `subscribe-ui` → `ui-history` | | `session.suggestion` `{text, uuid}` or null; `session.recap` `{text, at, why:'idle'|'resumed'}` or null. Also in `GET /api/ui-sessions/:id/history` (`{ok, session}`) |
| A suggestion arrives / goes | WS `ui-event` | | `{type:'suggestion', suggestion:{text, uuid} | null}` |
| A recap arrives / goes | WS `ui-event` | | `{type:'recap', recap:{text, at, why} | null}` |
| Send it | `POST /api/ui-sessions/:id/send` | `{text}` | `{ok, injected, uuid}` or `{ok:false, error}` (2.5) |
| Dismiss the suggestion | `POST /api/ui-sessions/:id/dismiss-suggestion` | none | `{ok:true}` / `{ok:false, error:'No such session'}` |
| Dismiss the recap | `POST /api/ui-sessions/:id/dismiss-recap` | `{}` | `{ok:true}` / `{ok:false, error}`; capability `chat-recap` |

Neither is in `GET /api/ui-sessions` (the list).

## Native notes

- `src/chat.rs` gets `suggestion: Option<{text, uuid}>` and `recap:
  Option<{text, at, why}>` from `load()` (history) and from the
  `suggestion` / `recap` events; `exit` clears both; a state other than idle
  clears the suggestion.
- The send goes through `app::send`'s plain path (`post_send`), not through
  the box: no draft is cleared or saved.
- The dock is a pane-level overlay in `ChatPane` (bottom right of the log);
  2.18 adds its badges row above.
- For tests: a `testctl` `chat-event` with `{type:'suggestion', …}` or
  `{type:'recap', …}` reaches the same handler as the server's frame.

## End-to-end tests (for `test/phase2.mjs`)

1. **A suggestion arriving, then sent.** On an idle chat, inject
   `{type:'suggestion', suggestion:{text:'Add due dates', uuid:'s1'}}` (the
   CLI cannot be made to send one); the pane shows it (caption, text). Type
   "keep me" in the box first. A real click on the bubble: the server gets
   `POST …/send {text:'Add due dates'}` (its history gains that user entry),
   the chat goes working, the dock is empty, the box still says "keep me".
2. **Hidden while busy:** with the chat working, inject a suggestion: none
   shows.
3. **Dismissed, and still dismissed after a restart.** Inject another, click
   its ×: it goes, and `POST …/dismiss-suggestion` is made (stub
   `dismiss-suggestion` to count it, or check the server's answer).
   Restart the client: no suggestion (the server's history has `null`).
4. **A recap shown and dismissed (real).** In a project, a chat with one
   finished turn is closed; a new chat in the same project continues that
   conversation and gets a **resumed** recap within ~15 s ("WHERE THIS CHAT
   STANDS" + the text from `GET …/history`). Click ×: it goes; `GET
   …/history` has `recap: null`; restart the client: not shown.
   (An idle recap needs `CRUNDI_RECAP_IDLE_MS`, which
   `tools/test-server.sh` strips with every `CRUNDI_*` variable; add a
   `--recap-ms N` option there if an idle recap is wanted.)
5. **Both at once:** inject a recap and a suggestion: recap above,
   suggestion last; the log's bottom clears the dock (the last row is fully
   visible above it); the jump button, when shown, is above the dock.
6. **A message sent from the box** drops both (the server clears them too).
7. **Narrow:** at a 340 pane the dock is 320 and inside the pane.

## Checklist

- [x] The dock: bottom right of the log, right `sp-10`, bottom `sp-8`,
      at most `chat-dock-w` (320) and `pane − 20`, right-aligned column.
- [x] Order: (badges) · recap · suggestion; the suggestion always last.
- [x] Suggestion: caption "Suggested, tap to send" on a `bg-primary` chip;
      dashed `sug-edge` bubble, `bg-secondary`, bubble corners with the sharp
      bottom right, shadow, `text-secondary`; × top right.
- [x] Hover and keyboard focus: solid edge, `accent-dim`, `text-primary`.
- [x] × hover: `text-primary` on `slate-dim`. Tooltips "Tap to send",
      "Dismiss".
- [x] Long text wraps inside 320, keeps its line breaks, × stays top right.
- [x] Shown only while idle and running; gone on working / needs input /
      waiting / exit; a new one replaces the old.
- [x] Click sends straight away (`POST …/send`), not into the box; the box
      and its draft unchanged; working at once; dock emptied; log re-pinned.
- [x] Enter / Space on the focused bubble sends; on the × dismisses (native).
- [x] × → gone at once + `POST …/dismiss-suggestion`; not back after a
      client restart.
- [x] Recap: `accent` 3px left bar, `border-subtle` edge, `r-field`,
      shadow; caption "WHILE YOU WERE AWAY" / "WHERE THIS CHAT STANDS"
      (upper case, `w-semibold`, `track-wide`, `text-muted`); text
      `text-secondary` `text-12`; × top right; no hover on the card.
- [x] Recap × → gone + `POST …/dismiss-recap {}`; not back after a restart.
- [x] Any sent message drops both; a side question drops the recap; exit
      drops both.
- [x] From `ui-history` on open: what the server holds is shown.
- [x] Log padding = dock height + `sp-12` while anything is in the dock
      (native also for a recap alone: PLAN.md named difference).
- [x] Jump button above the dock while a suggestion or recap shows.
- [x] Following the bottom stays following when one appears.
- [x] A-/A+ scale the texts, not the boxes.
- [x] Narrow pane: same arrangement, dock inside the pane.
- [x] Tokens only: new `chat-dock-w`, `sug-edge`, `sug-edge-on` in
      theme.slint.
