# 2.10 Scheduled messages

The composer's clock and the panel it opens: write a message now, and have
the server send it into this chat later (at a time, when the turn ends, or
when the 5-hour usage window resets). The panel holds the form and the list
of messages still to come for this chat, each with a delete.

The clock itself (its place on the meta row or the narrow button row, its
look and hover) is 2.5's (`specs/composer.md`, "Clock"); 2.5 drew it with
no action. This spec adds what it opens.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below, never a pixel value.

Source, `app/vendor/claude-chat.js` (grep with `-a`):
- CSS 395-452: `.cc-sched` (the clock), `.cc-schedpanel`, `-head`,
  `-title`, `-x`, `-body`, `-msg`, `-trigs`, `-trig` (`.on`, `.off`),
  `-at`, `-note`, `-list`, `-empty`, `-item` (`.next`), `-when`, `-text`,
  `-del`; `.cc-btn.primary` 132-135 (the Schedule button); `cc-in` 59;
- 1001-1007: the clock button; 1352-1363 `syncWidth()` (where it sits);
- 4609-4776: `trigLabel()`, `closeSched()`, `loadSched()`, `openSched()`
  (with `syncTrig()` and the save handler), `schedBtn` → `openSched`.

Server: `src/chat-schedule.js` (the store, the triggers, delivery);
routes `src/webapp.js` 6367-6401; turn end → `chatSchedule.onTurnEnd`
1741; limit warmer → `onLimitReset` 1787/1805; closing a chat drops its
items (`discardFor`, 3231/3237/6327/6518); the warmer switch
(`POST /api/settings {limitWarmup}`) 7525-7539.

## Screenshots

`scheduled-messages/web-<state>.png` (1400×860, a wide pane on the left,
two narrow ones on the right) and `-2x.png` crops of the panel.

| State | What it shows |
|---|---|
| `web-open-empty` | Opened on a wide pane with an empty composer, warmer off: title + ×; the message box focused (accent border + ring), placeholder "Message to send later…"; three triggers, **At a time** on, **Limit resets** dimmed (disabled); the time field (now + 30 min, rounded to the minute); no note; Schedule (accent gradient); a hairline, then "Nothing scheduled for this project." (italic, muted). The panel covers the composer. |
| `web-trig-turnend` | **Turn ends** on: the time field is gone, the note reads "Sends as soon as the current turn finishes. If nothing is running, it waits for the next turn to end." The panel is shorter (it grows up from the bottom). |
| `web-err-empty` | Schedule with an empty box: the note becomes "Write a message first." (muted, not red). |
| `web-err-notime` | At a time, the field cleared: "Pick a time." |
| `web-err-past` | A time earlier today: the server's "That time has already passed". |
| `web-with-items` | Reopened with text in the composer ("Remind me to deploy the release" carried into the box), warmer on (Limit resets enabled), four items: the first has an accent border and "**next** · when the turn ends"; a long text is cut at about three lines, no ellipsis; times as "11/14/2026, 3:30:00 PM" (mono, muted). |
| `web-hover-del` | An item's ✕ hovered: red. |
| `web-trig-limit` | **Limit resets** on: "Sends when the rolling 5-hour usage window rolls over." |
| `web-long-scrolled` | Seven items: the panel stops at 82% of the chat's height; the body (form and list together) scrolls under the fixed head. |
| `web-after-delete` | One ✕ clicked: the item is gone at once (no confirm), the rest re-listed. |
| `web-err-offline` | The network down: the list reads "Could not load scheduled messages."; Schedule gives "Could not schedule that: Failed to fetch". |
| `web-err-oldserver` | A server without the endpoint (404 `{error:"Not found"}`): the list reads "Nothing scheduled for this project.", Limit resets disabled, Schedule gives "Not found" (native: see Errors). |
| `web-narrow` | A narrow pane (the clock on the button row): the same panel, inset sp-8, triggers three across; the message box gives up height first. |
| `web-fired-stale` | A message scheduled for the current minute fired within a tick: it is in the chat as an ordinary user bubble, **but the open panel still lists it as "next"** (no live update, see Behaviour). |
| `web-fired-reopened` | The panel closed and reopened: the fired item is gone. |
| `web-fired-turnend` | A "Turn ends" item fired: after "first" was answered, "Now reply with just the word second" went in by itself and was answered. |
| `web-clock-hover` | The wide clock hovered: accent border, accent-hover icon. No badge or count when items exist (there is none in the web). |

Not screenshotted (a hidden browser cannot show them): the browser's own
date-time popup, and the title tooltips.

## The parts

The panel lives inside the chat's root, over the bottom of the chat:

```
chat root (pane body under the header)
├── log …
├── composer …                (covered while the panel is open)
└── panel  (absolute: left/right/bottom sp-8, top free; max 82% of the root)
    ├── head   [Schedule a message ..................... ×]   hairline under
    └── body   (column, padding sp-10, gap sp-8, scrolls as one)
        ├── message box        3 rows, "Message to send later…"
        ├── triggers           [At a time] [Turn ends] [Limit resets]
        ├── time field         only for "At a time"
        ├── note               only when it has text
        ├── [Schedule]         full width, primary
        └── list               hairline above; items or the empty line
            └── item  [next · <when>                     ✕]
                      [<text, up to ~3 lines>             ]
```

- **Not modal.** No scrim; the rest of the app (the chat's header, other
  panes, the log above the panel) stays live. One panel per chat; several
  chats may have theirs open at once.
- **Placement.** Anchored to the bottom of the chat (over the composer and
  its meta row), inset sp-8 left, right and bottom; it grows upward with its
  content up to 82% of the chat's height, then the body scrolls. The head
  never scrolls.
- **Appears** with the chat's `cc-in` entrance (fade in, rise a step):
  `motion-fast`-ish; use the same entrance as 2.6's menus.

### Measurements → tokens

Measured on the wide pane (A = 1.0). Every font size scales with the chat's
text size (A−/A+), as the rest of the chat does.

| Part | Web | Native token |
|---|---|---|
| Panel | bg-primary, 1px border, radius 10, shadow 0 6 20 #0008, z over the composer | `bg-primary`, `border`, `r-card`, `shadow-menu` |
| Panel inset | 8 left/right/bottom | `sp-8` |
| Panel max height | 82% of the chat root | a new share token, `sched-share: 0.82` (structure) |
| Head | padding 8 × 10, hairline `border-subtle` under, 37 high | `sp-8` × `sp-10` |
| Title | "Schedule a message", 13 / 600, text-primary | `text-13`, `w-semibold` |
| × | text glyph 15, text-muted, padding 2 × 4; hover text-primary | the ×/close icon at `icon-md`, `text-muted` → `text-primary` |
| Body | padding 10, gap 8 | `sp-10`, `sp-8` |
| Message box | 3 rows (76 high at 13px), bg-secondary, border, radius 6, padding 7 × 9, 13px; focus: accent border + 3px accent ring; user-resizable (vertical) | the composer's input field look (2.5): `bg-secondary`, `border`, `r-control`, `sp-8` × `sp-8`, `text-13`; focus as 2.5's box. Min 3 rows; no resize grip (gap) |
| Triggers row | three buttons, flex 1 each, min 96, gap 6, wrap | `sp-6`; three equal widths; wrap to a new row only when a button would be narrower than `sp-48 + sp-48` |
| Trigger | 28 high, padding 6 × 8, 11.5px, bg-secondary, border, radius 6, text-secondary | `control-sm`, `text-12`, `bg-secondary`, `border`, `r-control`, `text-secondary` |
| Trigger on | accent-dim fill, accent border, text-primary | `accent-dim`, `accent`, `text-primary` |
| Trigger disabled | opacity 0.45, no pointer | `o-dim` |
| Trigger hover | none | none |
| Time field | 36 high, bg-secondary, border, radius 6, padding 6 × 8, 13px, calendar glyph at the right | `control-lg`, same field look, `text-13`, calendar icon `icon-md` |
| Note | 11px, text-muted; hidden when empty | `text-11`, `text-muted` |
| Schedule | `.cc-btn.primary`: 28 high, full width, 12 / 600, white on the accent gradient, radius 6, glow 0 2 12 accent .35; hover accent-hover | 2.5's Send look: `grad-accent`, `white`, `w-semibold`, `text-12`, `r-control`, `glow-send`, `control-sm` |
| List | hairline `border-subtle` on top, padding-top 8, gap 6 | `sp-8`, `sp-6` |
| Empty line | 12px italic text-muted | `text-12`, italic, `text-muted` |
| Item | bg-secondary, border-subtle, radius 6, padding 6 / 26 / 6 / 9 | `bg-secondary`, `border-subtle`, `r-control`, `sp-6` × `sp-10`, right `sp-24` |
| Item, next | accent border | `accent` |
| When | 10.5px mono, text-muted, gap 2 under | `text-11`, `Theme.mono`, `text-muted`; `sp-2` under |
| "next" | bold, accent-hover | `w-bold`, `accent-hover` |
| Text | 12px, text-secondary, wraps, cut at 52 (≈ 2.8 lines), no ellipsis | `text-12`, `text-secondary`, clipped at three lines (`3 × line height`), no ellipsis |
| Item ✕ | glyph 12, text-muted, top/right 4, padding 2 × 4; hover red | close icon `icon-sm`, `text-muted` → `red`, at `sp-4` from the top-right corner |

## Behaviour

### Opening and closing

- **The clock toggles** the panel: closed → open; open → closed
  (`openSched` returns through `closeSched` when one is open).
- **×** in the head closes it.
- **Esc does nothing** and **a click outside does nothing** (the web has
  neither; checked). The panel stays until the clock or × is pressed.
  Native: the same.
- **Nothing persists.** Closing throws the form away (text, trigger, time,
  note); reopening starts fresh. A reload / app restart closes it.
- **On open:**
  1. the message box gets the composer's text (`msg.value = input.value`,
     the raw text including whitespace); the composer keeps it too;
  2. the trigger is **At a time**, the time field is **now + 30 minutes,
     seconds dropped** (local time);
  3. the note is empty;
  4. the list loads (`loadSched`), which also sets Limit resets' state;
  5. the message box takes focus.

### Triggers (`syncTrig`, `trigLabel`)

The three buttons are a one-of-three choice; the chosen one is "on".

| Button | `trigger.type` | Time field | Note while chosen | Listed as (`trigLabel`) |
|---|---|---|---|---|
| At a time | `time` | shown | none (cleared) | the time, `toLocaleString()`: "11/14/2026, 3:30:00 PM" |
| Turn ends | `turn-end` | hidden | "Sends as soon as the current turn finishes. If nothing is running, it waits for the next turn to end." | "when the turn ends" |
| Limit resets | `limit-reset` | hidden | "Sends when the rolling 5-hour usage window rolls over." | "when the 5h window resets" |
| (unknown type from a newer server) | — | — | — | the raw type string |

- Choosing a trigger replaces the note with that trigger's note (so a
  validation message goes away).
- **Limit resets needs the limit warmer.** Each list load reads
  `limitWarmup`: when false the button is disabled (dimmed, no hover, title
  "Needs the limit warmer, which is off"); if it was the chosen one, the
  choice falls back to At a time. When true it is enabled, no title.
  Until the first load answers, it is enabled.
- The web's handler also has a note for clicking the disabled button ("Turn
  on the limit warmer in Settings to use this — it is what notices the
  window rolling over."), but a disabled `<button>` gets no click in
  Chromium, so it never shows (checked with a real click). Native: the same
  as what the person sees, i.e. a disabled button that ignores clicks and
  shows the title as its tooltip.

### The time field (the when-picker)

The web uses `<input type="datetime-local">`, minute resolution: it shows
the local date and time as "11/14/2026, 11:51 AM" (the system's locale
format), each part editable by typing or the arrow keys, and a calendar
glyph at the right that opens the browser's date-and-time popup (a month
grid and an hour/minute column). It can be emptied.

Native (closest Slint equivalent; **named gap** in PLAN.md):
- the field is a themed box showing the chosen moment in the same format
  (locale date, hour:minute, AM/PM as the system uses), editable by typing
  (parsed on change; an unparseable text counts as empty);
- the calendar icon at its right opens Slint's std `DatePickerPopup`, then
  `TimePickerPopup` (both in Slint 1.18's std-widgets, fluent-dark style).
  The gap: those popups take the std style, not our theme tokens, and
  date and time are two steps instead of one popup.
- Minute resolution, local time, sent as UTC ISO (`new Date(local).toISOString()`).

### Save (Schedule)

In order (`.cc-sched-save` click):
1. text = message box, trimmed. Empty → note "Write a message first.", stop.
2. For At a time: no time → note "Pick a time.", stop.
3. `POST /api/chat-schedule` `{ project, text, trigger: {type[, at]}, chat: <this chat's id> }`.
   No busy state: the button stays enabled and nothing shows while it is in
   flight.
4. Reply `ok: false` → note = `error` or "Could not schedule that."; the form
   keeps everything.
   Network failure → note "Could not schedule that: <message>".
5. Reply `ok: true`:
   - if the composer's text, trimmed, equals the scheduled text, the
     composer is cleared (and its draft saved, the box shrunk), so the same
     thing is not sent twice; otherwise the composer is left alone;
   - the message box is cleared, the note is cleared; **the trigger and the
     time stay** as they were;
   - the list reloads.
- There is **no keyboard shortcut**: Enter in the message box is a new line
  (no Enter/Ctrl+Enter handling), Esc does nothing.

Validation on the server (each comes back as the note, verbatim):
"The message is empty", "That time has already passed" (more than 60 s in
the past: so the current minute is accepted and fires on the next tick),
"That is not a valid time", "Unknown trigger \"x\"", "That chat is not
open", "Unknown project", "Turn on the limit warmer in Settings to schedule
a message for the limit reset.", "Bad request body".

### The list (`loadSched`)

- `GET /api/chat-schedule?project=<alias>&chat=<chat id>`; shows `items`
  (pending only), in the server's order (soonest first; turn-end first of
  all, limit-reset at the reset time or now + 5 h).
- Only this chat's items, plus older items with no chat (they show in every
  chat of the project). Another chat's items are not shown (checked: the
  second chat's item is absent from the first chat's list).
- Empty → "Nothing scheduled for this project." (italic, muted; the
  wording says project even though the list is per chat; keep it).
- The first item is the **next**: accent border and "**next** · " before its
  when-label.
- Each item: when-label, then the text (pre-wrap, breaks long words, cut at
  about three lines with no ellipsis), and ✕ (title "Cancel this message")
  in the top-right corner. Items are not clickable otherwise.
- `recent` (sent and failed items) is returned but **not shown** by the web.
  Native: not shown either.
- Load failure (network) → "Could not load scheduled messages." in place of
  the list.

### Delete

✕ → `DELETE /api/chat-schedule/<id>`, no confirmation, no busy state; then
the list reloads whatever the answer (a 404 or a network error is silent;
the reload shows the truth).

### Firing (server side), and what the chat shows

- The server ticks every 15 s. A **time** item is due when its time has
  passed; a **turn-end** item when its chat's turn ends (one item per turn
  end; if the chat is parked it is woken first); a **limit-reset** item when
  the warmer reports the window rolled over.
- Delivery only into its own chat, and only when that chat is running and
  idle; otherwise it waits (a parked chat is resumed for it). It is sent with
  the ordinary send path, so **the chat shows it as an ordinary user message**
  followed by Claude's turn, through the same live events as a typed message
  (2.4/2.5). Nothing marks it as scheduled.
- A chat closed by the owner drops its pending items; turning the warmer off
  drops all limit-reset items.
- **The open panel does not update live.** The server has no event for the
  schedule (`chatSchedule.onChange` has no listener), and the web reloads the
  list only on open, save and delete. A fired item stays listed (even as
  "next") until one of those (`web-fired-stale` → `web-fired-reopened`).
  Native: the same (reload on open, save, delete). This is a web
  shortcoming, kept per rule 1; worth a word to the owner, not a fix here.

### The clock's own state

The clock never changes with the schedule: no badge, count or colour when
items exist (`web-clock-hover`, `web-with-items`). It does not show "open"
while the panel is open either. Native: the same.

### Narrow panes

- The clock sits on the button row (2.5). The panel is the same panel,
  inset sp-8 in the pane, triggers three across (they wrap only below
  ~3 × 96 px, which a pane at `pane-min` never reaches).
- In a short pane (82% of a small chat) the message box shrinks first
  (the web lets it collapse to nearly nothing, `web-fired-stale`); then the
  body scrolls. Native: keep the message box at least two lines high, then
  scroll the body (a deliberate floor, not a rearrangement).

### Busy, needs input, exited

The panel and the clock work in every state (scheduling while Claude works
is the point). The clock is not disabled when the chat has exited; the
server then answers "That chat is not open" if the chat is gone from the
server, which shows as the note.

### Errors and an older server

| Case | Web | Native |
|---|---|---|
| List load: network down | "Could not load scheduled messages." | the same |
| List load: 404 (server without the endpoint) | `{error:"Not found"}` → no items → "Nothing scheduled for this project.", Limit resets disabled | the list line reads "Scheduled messages are not supported by this server." and Schedule is disabled (backwards-compatible-clients rule: a newer client meeting a missing endpoint says "not supported by this server", never breaks). Named difference |
| Save: 404 | note "Not found" | note "Not supported by this server." |
| Save: network down | "Could not schedule that: Failed to fetch" | "Could not schedule that: <error>" |
| Save: `ok:false` | the server's `error`, or "Could not schedule that." | the same |
| Delete fails | silent, list reloads | the same |

## Data and endpoints

All under the bearer session (`apiFetch`), JSON.

**`GET /api/chat-schedule?project=<alias>&chat=<chatId>`**
→ `200 { ok: true, items: Item[], recent: Item[], limitWarmup: bool }`;
`400 { ok:false, error:"A project is required" }` without a project.

`Item`:
```
{ id: "hex16", project: "demo", text: "…",
  chat?: { id, conversation, title },          // absent on old items
  trigger: { type: "time", at: "ISO" } | { type: "turn-end" } | { type: "limit-reset" },
  status: "pending" | "sent" | "failed",
  createdAt: ms, firedAt: ms (0 = not yet), error: "",
  nextFireAt: ms }                              // items only; sort key
```

**`POST /api/chat-schedule`** body
`{ project, text, trigger: { type, at? }, chat: "<chatId>" }`
→ `200 { ok: true, item }` or `400 { ok: false, error }` (wording above).

**`DELETE /api/chat-schedule/<id>`** (id hex) → `200 { ok: true }` or
`404 { ok: false, error: "Not found" }`.

**`POST /api/settings { limitWarmup: true|false }`** (for tests: turns the
warmer on so Limit resets is enabled; turning it off drops limit-reset
items and answers `clearedLimitReset: n`).

Live events used: **none for the panel**. The delivered message reaches the
chat through the normal chat events (the user entry, then the turn) already
handled by 2.4/2.5.

Native wiring: `Composer.schedule(key)` (already a callback in
`ui/chat.slint`) toggles the panel for that chat; the panel's state (open,
text, trigger, time, note, items, warm, loading) is per chat key.

## Wording (exact)

- Title "Schedule a message"; × has no title.
- Placeholder "Message to send later…" (one ellipsis character).
- Triggers "At a time", "Turn ends", "Limit resets".
- Disabled Limit resets tooltip "Needs the limit warmer, which is off".
- Notes: "Sends as soon as the current turn finishes. If nothing is running,
  it waits for the next turn to end." / "Sends when the rolling 5-hour usage
  window rolls over." / "Write a message first." / "Pick a time." /
  "Could not schedule that." / "Could not schedule that: <message>".
- Button "Schedule".
- List: "Nothing scheduled for this project." / "Could not load scheduled
  messages." / "next · " / "when the turn ends" / "when the 5h window
  resets"; ✕ tooltip "Cancel this message".
- Clock tooltip "Schedule this message" (2.5).

## How to bring each state about

- Test server on a free port of the run's range, `tools/test-server.sh`;
  the seeded chat is idle.
- Warmer on/off: `POST /api/settings {"limitWarmup":true}`.
- Items: `POST /api/chat-schedule` with `chat` set to the chat id (a time
  tomorrow stays pending). A turn-end item stays pending until a turn ends.
- Fire a time item now: schedule it for the **current minute** (accepted,
  ≤ 60 s past), it is sent within one 15 s tick.
- Fire a turn-end item: schedule it, then send the chat a short message;
  when that turn ends the item is sent.
- Old server: point the client at a stub that answers 404 for
  `/api/chat-schedule*`, or a test hook that makes the client see 404.

## End-to-end tests (phase2.mjs, 2.10)

1. **Open / close** — clock → panel, message box focused, the composer's
   text carried in, At a time on, time = now + 30 min (minute); clock again →
   closed; open, × → closed; Esc and a click on the log → still open.
2. **Empty** — "Nothing scheduled for this project."; warmer off → Limit
   resets disabled and unclickable; warmer on, reopen → enabled.
3. **Triggers** — each choice: time field shown only for At a time; the two
   notes exact.
4. **Validation** — empty → "Write a message first."; no time → "Pick a
   time."; a time an hour ago → "That time has already passed".
5. **Schedule and list** — text "e2e later" at tomorrow 09:00 → the box and
   note clear, trigger/time stay, the item is listed with the time label;
   the composer, holding the same text, is emptied; holding other text, is
   left alone.
6. **Order and next** — a turn-end item lists first with "next · when the
   turn ends" and the accent border.
7. **Fire** — schedule "Reply with just the word pong" for the current
   minute through the panel; within ~20 s it is a user message in the chat
   and Claude answers; the open panel still lists it; close and reopen → it
   is gone (the CR's "schedule").
8. **Delete** — schedule another, ✕ → gone from the list and from
   `GET /api/chat-schedule`.
9. **Per chat** — an item for chat B is not in chat A's list.
10. **Errors** — load failure → "Could not load scheduled messages."; save
    failure → "Could not schedule that: …"; 404 → "not supported by this
    server" wording, Schedule disabled.
11. **Narrow** — open in a narrow pane: the same parts, three triggers in a
    row, the body scrolls in a short pane, the message box at least two lines.

## Checklist

- [ ] Clock toggles the panel (open ↔ closed); × closes; Esc and outside
      clicks do not
- [ ] Panel: bottom-anchored over the composer, inset sp-8, grows upward,
      max `sched-share` (0.82) of the chat, body scrolls (RevealFlickable +
      ScrollThumb), head fixed; not modal (no scrim, Pointer.modal untouched)
- [ ] Head: "Schedule a message" w-semibold + × (muted → primary), hairline
- [ ] Message box: carries the composer's text, focused on open, 3 rows,
      field look and focus ring of 2.5; Enter = new line; never shorter than
      two lines
- [ ] Triggers: three equal buttons, one on (accent-dim, accent edge,
      text-primary); Limit resets disabled (o-dim, tooltip) when
      `limitWarmup` is false, falling back to At a time
- [ ] Time field: shown only for At a time; default now + 30 min (minute);
      themed box + calendar icon → DatePickerPopup then TimePickerPopup
      (named gap in PLAN.md)
- [ ] Notes per trigger and per error, exact wording, muted, hidden when empty
- [ ] Schedule: primary gradient look; validates (empty, no time); POST
      body with `chat`; on ok clears the box and note, keeps trigger/time,
      clears the composer only if it holds the same text, reloads the list
- [ ] List: per chat, server order, first = next (accent edge, bold
      accent-hover "next · "), when-label mono muted, text cut at three lines,
      ✕ (muted → red, "Cancel this message")
- [ ] Empty and load-error lines, exact wording
- [ ] Delete: no confirm, reload after
- [ ] No live list updates (reload on open/save/delete only), as the web
- [ ] Clock has no badge/count/open state
- [ ] Fired message appears in the chat as an ordinary user message
- [ ] Old server (404): "not supported by this server", never breaks
      (named difference)
- [ ] Narrow pane: same parts; short pane scrolls
- [ ] Every font scales with the chat's text size (A−/A+)
- [ ] Tokens only; `sched-share` added to theme.slint
- [ ] PLAN.md: 2.10 ticked; Known gaps: the date/time popups' std style and
      two-step picking, no resize grip on the message box; named
      differences: 404 wording, message box min height
- [ ] Side-by-side crops for every state in `scheduled-messages/review/`
