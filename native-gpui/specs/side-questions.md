# 2.21 Side questions

A side question is asked with **`/btw <question>`** in the chat's box, as
in Claude Code's terminal. Claude answers it from the conversation so far
(its cache, no tools), and **nothing is added to the conversation**: no
user entry, no turn, nothing queued. It works while a turn is running,
which is the point: ask what Claude is doing without interrupting it.

It shows as a card in the log (`.cc-aside`), at the end, where it was asked:

```
┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐
  ON THE SIDE                                       ×    ← head
  what file are you writing?                             ← the question
  ◌ Asking…            /  the answer (markdown)  /  error box
  Not added to the conversation.                         ← note
└ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘
```

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.

Source:
- `app/vendor/claude-chat.js`
  - CSS 427-436 (`.cc-aside`, `.cc-aside-head`, `.cc-aside-x`,
    `.cc-aside-q`, `.cc-aside-a`, `.cc-aside-a.busy`, `.cc-aside-note`),
    111 (`.cc-spin`), 172 (`.cc-error`).
  - `asideQuestion` 2715 (the `/btw` test), `askAside` 2720-2765,
    `doSend` 2821-2840 (checked before `sendText`, so never queued),
    3967-3969 (`/btw` offered in the slash menu, 2.11).
  - `applyHistory` 4899-4913 (a log rebuild wipes the cards).
- Server: `src/claude-ui.js` `askAside` 2625-2638 (a `side_question`
  control request, 60 s; `MAX_ASIDE_QUESTION` 2000, `MAX_ASIDE_ANSWER`
  8000), `src/webapp.js` 6530-6532, capability `chat-aside`
  (`src/client-version.js` 146-151).

## Screenshots

`side-questions/web-<state>.png` at 1400×860, one wide pane unless named;
`-2x` is the card cropped and enlarged. All against a real Claude Code
working on a real turn (writing a 1500-word file), except the two errors
(the fetch stubbed).

| State | What it shows |
|---|---|
| `web-typed-while-working` | `/btw what file are you writing, and roughly how long will it be?` in the box while the chat is WORKING (the button still says "Queue"). |
| `web-pending` (+`-2x`) | **Pending:** the card after Enter: "ON THE SIDE", the question (without `/btw`), a small spinner and "Asking…", the note. The box is empty; the "Working…" row stays below the card. |
| `web-answered` (+`-2x`) | **Answered** (about 3 s later): the answer in `text-secondary`, as markdown (bold, inline code, paths as links). |
| `web-x-hover` (+`-2x`) | The × hovered: `text-primary` on a `slate-dim` rounded box. |
| `web-several` | **Three at once:** one answered, two still asking, stacked in the order asked; each its own card. |
| `web-several-answered` | The same three, all answered. |
| `web-answered-markdown` (+`-2x`) | **A markdown answer with a list:** the web's bullets hang outside the card's text column (an accident: the reply list's indent is not applied inside the card). |
| `web-errors` | **Errors:** "This version of Claude Code cannot answer side questions." (the server's `{ok:false, error}`) and "Failed to fetch" (no network), each as the log's red error box inside the card. Also: the turn's later entries ("I wrote a design document…", the result line) arrive **below** the earlier card: a card stays where it was asked. |
| `web-vs-queued` | **A side question vs a queued message**, asked during the same turn: "Also add a short README.md…" went to the composer's **Sending** drawer (2.9) for the next pause; the `/btw` went into the **log** as a card and was answered at once. |
| `web-narrow` (+`-2x`) | **A narrow pane** (~350): the card spans the log's width, the question and the answer wrap; later entries (a tool row, the queued message joining, a Write) arrive below it. |
| `web-gone-after-reload` | **After a reload:** the cards are gone (they are local to the page; nothing on the server). |

## Parts

| Part | Web | Token |
|---|---|---|
| Card | the log's width; margin 6 above and below; padding 8 10; 1px **dashed** `border` edge; radius 6; `bg-secondary` | `sp-6`, `sp-8` / `sp-10`, `DashedBox`, `r-control`, `bg-secondary` |
| Head | a row, gap 8: the label (fills), then × | gap `sp-8` |
| Label | "On the side", 10.5 / 600, upper case, letter spacing 0.04 em, `text-muted` | `text-11` × fs, `w-semibold`, `track-wide`, `text-muted` |
| × | "✕" 11 × fs `text-muted`, padding 2 5, radius 5; hover `text-primary` on `slate-dim`; tooltip "Close" | the `x` icon at `icon-sm`, a `control-sm − sp-8` square or `sp-2`/`sp-6` padding, `r-control` (one step), `slate-dim` |
| Question | `text-primary`, the log's text (13 × fs, line 1.55), 4 above, 6 below, wraps anywhere, keeps newlines; the text after `/btw ` as typed (trimmed) | `text-13` × fs, `line-chat`, `sp-4`, `sp-6` |
| Answer, pending | a row, gap 7: the spinner (9 × 9, ring 1.5 `border` with an `accent` arc, 0.7 s a turn) and "Asking…" `text-secondary` | gap `sp-6`, the tool card's spinner (2.14: `spin`, `spin-stroke`) at `sp-10` |
| Answer, done | the answer as **markdown**, the same renderer as a reply (paragraphs, bold, code, lists, links, paths), in `text-secondary` | the reply blocks (2.16) in `text-secondary`; lists indented as in replies (not the web's hanging bullets) |
| Answer, error | the log's error box (`.cc-error`): 12 × fs `red` text, `red-dim` fill, 1px `red` edge, radius 6, padding 7 10; the server's error, or the network error's message | as 2.16's error row |
| Note | "Not added to the conversation.", 10.5 `text-muted`, 6 above | `text-11` × fs, `sp-6` |

Scales with A-/A+ (texts only).

## Behaviour

### Asking

- **How:** type `/btw <question>` in the box and send it (Enter, or the
  send key setting's Ctrl+Enter; the Send/Queue button the same). The test
  is `^/btw\s+(.*\S)\s*$` (case-insensitive, the question may span lines).
  `/btw` alone, or with only spaces, is **not** a side question: it is sent
  as an ordinary message.
- **The slash menu** offers `btw` ("Ask on the side: answered from the
  conversation, not added to it") when the CLI's list lacks it (2.11,
  already in `src/slash.rs`); choosing it puts `/btw ` in the box.
- **In any state** with a running session: idle, working, needs input,
  waiting. The check comes before the composer decides to send, queue or
  hold, so a side question is **never queued or held** and never changes
  the chat's state. (Native: test for `/btw` at the top of `app::send`,
  before `chat_state_now`.)
- **On asking:** the box is cleared and the draft saved empty (as any
  send); the log re-pins to the bottom; the slash menu closes; the **recap
  is dismissed** (2.17: `POST …/dismiss-recap`, the person is back); a card
  is appended at the end of the log, above the "Working…" row (which stays
  last); then `POST /api/ui-sessions/:id/aside {question}`.

### Answered, error, gone

- `{ok:true, answer}` → the answer (markdown) replaces "Asking…"; the log
  follows to the bottom if it was following.
- `{ok:false, error}` → the error box with `error` (or "No answer"). The
  server's errors: "No session …", "Session is not running", "Question is
  empty", "This version of Claude Code cannot answer side questions.", a
  timeout after 60 s, "No answer".
- A network failure → the error box with the failure's message.
- **404** (a server from before side questions): the card is removed and
  the text is **sent as typed** (`/btw …`, through the normal send, so it
  queues if busy) — what that server did with it before. The capability
  `chat-aside` says in advance whether the route exists; the native client
  does the same as the web on a 404 (this replaces the usual "not supported
  by this server", because the old behaviour is a working one).
- Several may be pending at once; each answers into its own card, in
  whatever order the answers come.

### Closing

The × removes that card, at once, locally. No call is made (the server
keeps nothing). Pending ones can be closed too; their answer is then
dropped.

### Where it lives

- **Local only.** Nothing is stored on the server and nothing is added to
  the conversation (the history's messages do not change). A reload or a
  client restart loses every card (`web-gone-after-reload`).
- **In the log, where asked:** entries that arrive later go **below** it.
  The web loses the cards when it rebuilds the log (a history it cannot
  reconcile, e.g. after a long disconnect); **native** keeps a card through
  a reconnect, anchored after the entry that was last when it was asked (a
  named difference: a rebuild wiping them is a side effect, not a choice).
- **Exit:** the cards stay (the box is disabled, so no new ones).

### How it differs from a queued message

| | Queued message (2.9) | Side question |
|---|---|---|
| Written as | anything, while Claude works | `/btw <question>`, in any state |
| Shown in | the composer's drawer ("Sending", then "Sent") | a card in the log |
| Reaches Claude | at its next pause, as a user turn | at once, as a side question (no tools, no turn) |
| In the conversation | yes, a user entry | never |
| Answer | in the turn's reply | in the card |
| Taken back | click the drawer | × closes the card |
| After a reload | kept (server) | gone |

### Narrow panes

Nothing changes but the width: the card spans the log, head row on one
line (the label is short), question and answer wrap.

## Data and endpoints

| What | Method, path | Body | Answer |
|---|---|---|---|
| Ask | `POST /api/ui-sessions/:id/aside` | `{question}` (≤ 2000 characters after trimming, the server cuts) | `{ok:true, answer}` (≤ 8000) or `{ok:false, error}`; 404 on a server without `chat-aside` |
| Dismiss the recap on asking | `POST /api/ui-sessions/:id/dismiss-recap` | `{}` | 2.17 |

No WebSocket event: the answer comes back on the POST (up to 60 s).

## End-to-end tests (for `test/phase2.mjs`)

1. **Asked during a turn, answered.** Start a real long turn ("Write a
   detailed 1500-word design document … to design.md"); while it is
   working, type `/btw what file are you writing?` and press Enter. At once:
   a card with "ON THE SIDE", the question, "Asking…", the note; the box
   empty; the drawer empty (nothing queued); the chat still WORKING. Within
   ~20 s the answer replaces "Asking…" (non-empty, mentions design.md in a
   real run). The server's history gains no user entry with "btw".
2. **Several:** two more in a row; three cards, in order.
3. **Error:** stub `aside` with `{ok:false, error:'E3'}` → the red box "E3"
   in the card; a network failure → its message.
4. **404:** stub `aside` with status 404 → no card, and `/btw …` goes out
   through `send` (queued while working).
5. **×** removes one card, the others stay.
6. **Recap:** with a recap shown, asking dismisses it (`POST
   …/dismiss-recap`).
7. **Restart:** the cards are gone after a client restart.
8. **`/btw` alone** is sent as a message, not asked.

## Checklist

- [x] `/btw <q>` from the box (Enter, Ctrl+Enter mode, the button) asks on
      the side in every running state; never queued, held or sent.
- [x] `/btw` with no question is sent as an ordinary message.
- [x] Box cleared, draft saved empty, log re-pinned, slash menu closed,
      recap dismissed.
- [x] Card at the end of the log, above "Working…": dashed `border`,
      `bg-secondary`, `r-control`, `sp-8`/`sp-10` padding, `sp-6` margins.
- [x] Head "ON THE SIDE" (upper case, `w-semibold`, `track-wide`,
      `text-muted`) and × (tooltip "Close", hover `slate-dim`).
- [x] Question `text-primary`, wraps, keeps newlines, without "/btw".
- [x] Pending: spinner + "Asking…" `text-secondary`.
- [x] Answered: markdown as replies, `text-secondary`, lists indented.
- [x] Error: the log's red error box with the server's or network's message.
- [x] 404: card removed, text sent as typed.
- [x] Note "Not added to the conversation." `text-muted` small.
- [x] Several at once, each answering into its own card.
- [x] × removes the card at once, no call.
- [x] Later entries arrive below the card; it keeps its place.
- [x] Kept through a reconnect (native); gone after a client restart.
- [x] Nothing added to the server's conversation.
- [x] A-/A+ scale the texts.
- [x] Narrow pane: same parts, wrapped.
- [x] Tokens only.
