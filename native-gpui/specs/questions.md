# 2.13 Questions (AskUserQuestion cards)

When Claude calls `AskUserQuestion`, the CLI holds the turn and asks the
person. The chat shows a **question card** in the conversation, under the
`AskUserQuestion` tool row. It holds one to four questions. Each has a chip,
the question, its choices (radio or checkbox, with a description and
sometimes a preview), an optional note on a choice, and an **Other** field.
Submit and Dismiss sit at the foot. Phase 1 built a basic card: the first
question only, one radio choice, no Other, no notes, and "Answered" text
afterwards. This screen covers the rest.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below. Reuse `CardBtn` (ui/chat.slint) for Submit and Dismiss, the focus-ring
pattern already used by the composer (an `accent-ring` rectangle outset by
`sp-4` behind the control), `Icon` with `ui/icons/note.svg` for the note
button and `x.svg` for removing a note, and `check.svg` for the checkbox
tick.

Source:
- `app/vendor/claude-chat.js`
  - `questionNode()` 2166-2408: the card, picks, `drawNote`, `syncSubmit`,
    `answerFor`, `saveAnswer` / `restoreAnswer` / `clearAnswer`, and the
    Submit and Dismiss handlers.
  - `respond()` 2411 and `answerOrphaned()` 2434.
  - CSS 121-170: `.cc-ask`, `.cc-ask-sub`, `.cc-btns`, `.cc-btn`, `.cc-q`,
    `.cc-q-chip`, `.cc-q-text`, `.cc-opt*`, `.cc-other`, `.cc-answered`,
    `.cc-ans-note`, `.cc-tag`.
  - `pruneDrafts()` 728: answer drafts expire after 30 days, like message
    drafts.
  - The `entry` and `patch` frames: 4972.
- Server `src/claude-ui.js`
  - `handleControlRequest()` 2390: the `can_use_tool` request for
    `AskUserQuestion` becomes an entry with kind `question` and status
    `pending`; the chat goes to needs-input.
  - `respond()` 2892.
  - `answerClosed()` 2962.
  - `expireStalePending()` 1799: a turn that ended takes its open questions
    to `cancelled`.
  - The process `exit` handler 1527: open questions go to `cancelled`.
  - The restore after a restart, 655-680: a replayed pending ask comes back
    `unanswered`, followed by a notice.
- `src/webapp.js` 6486 and 6549-6551: the `respond` and `answer-closed`
  routes.

## Screenshots

`questions/web-<state>.png` at 1400×860. A `-2x` file is the card (or the
named region) cropped and doubled. Three projects are seeded:
- **Pick** has "One question" (single choice) and "Multi select", two wide
  panes.
- **Many** has "Several questions" (three questions) and "Previews".
- **Demo** has six chats in a row, about 340 wide each, so its panes are
  narrow.

| State | What it shows |
|---|---|
| `web-pending-single-multi` | **Pending, nothing picked.** Left is a single-choice card: chip DATABASE, the question, three choices with descriptions, Other, Submit (disabled, dimmed) and Dismiss. Right is a multi-select card with square checkboxes. A note button sits at the top right of every choice. |
| `web-hover-option` | The pointer on a choice: its border turns `accent-hover`. Nothing else changes. |
| `web-single-picked-submit-hover` | SQLite picked: the row is selected (accent border, `accent-dim` fill, accent ring) and the radio is filled. Submit is enabled; the pointer is on it, so it shows its hover glow. The keyboard focus is on the radio, which carries the ring. |
| `web-hover-note-button` | The pointer on MySQL's note button: it gets a `bg-tertiary` fill and a `border` edge, and its icon turns `text-primary`. |
| `web-note-open` | The note button pressed on SQLite. The choice is picked (if it wasn't already), the button turns accent-hover (on), and a note row opens under the description: a field with the placeholder "Note for Claude about this choice", which takes the focus, and a ✕ "Remove the note". Typed: "Keep the file under data/". |
| `web-note-follows-choice` | MySQL picked afterwards: the note moves to MySQL with its text kept (one note per question, under the picked choice). |
| `web-multi-picked-dismiss-hover` | Multi select with Sign-in and Export ticked: both rows selected, and Submit is enabled. The pointer on Dismiss shows the danger hover: `red` edge, `red-dim` fill, `red` text. |
| `web-restored-after-reload` | The page reloaded: both cards come back as they were left (MySQL picked with its note; Sign-in and Export ticked). This is the answer draft. |
| `web-other-focused` | The pointer in Other: accent border and ring. The ticks are still there. |
| `web-other-typed` | "Only export for now, as JSON" typed into Other: every tick in that question is cleared, and Submit stays enabled. |
| `web-answered-other` | Submitted: the card shrinks to one green tag holding the typed text. The pane goes to WORKING. |
| `web-answered-note-and-other` | Both Pick cards afterwards. Left: tag "MySQL" and the line "Note: Keep the file under data/". Right: tag "Only export for now, as JSON". |
| `web-several-and-previews` | Many: the three-question card scrolled to its end (TESTS multi, DEPLOY single, each with its own Other, one Submit / Dismiss for the whole card). Right: the preview card. Each preview is a monospace box under the description. |
| `web-preview-card-2x` | The preview card alone. |
| `web-several-partly-answered` | Rust and Unit picked, Deploy not yet: Submit is still **disabled**. Every question needs an answer. |
| `web-several-all-answered` | Unit and Integration plus "This server": Submit is enabled. |
| `web-answered-several-and-dismissed` | Left: three tags in question order (Rust · Unit, Integration · This server). Right: Dismiss pressed, so a red "dismissed" tag shows; Claude replies that nothing was picked. |
| `web-narrow-long-previews` | Demo, narrow panes (about 340): long labels and descriptions wrap, and preview text wraps inside its box (no sideways scroll). |
| `web-narrow-long-top` | The long question at the top of its card: the question wraps over seven lines. |
| `web-text-larger` | Previews pane full screen, A+ three times (×1.23): text, chip, buttons and the preview font scale. Paddings, the radio and the note button stay the same size. |
| `web-text-smaller` | A− down to ×0.77. |
| `web-orphaned-and-answered-elsewhere` | Left: the chat's process exited while asking (status `cancelled`). The card keeps every control and gains a line at the top: "This question outlived the session that asked it. Answering sends your choice as a message." Middle: the question was answered from somewhere else (`POST …/respond` by another client); the card turned into its tags live. |
| `web-orphaned-card-2x` | The orphaned card alone. |
| `web-orphaned-answered-session-gone` | The orphaned card answered (SQLite): the tag shows straight away (status `answered-late`). Sending the message failed because the chat's process is gone, so the chat shows the error line "Session is not running" under the card. |
| `web-keyboard-focus-radio` | The keyboard focus on a radio: the accent ring round the radio itself. |
| `web-approvals-entry` | For reference only: the top bar's approvals list shows a pending question as "CHAT AskUserQuestion / pick" with "Open the chat". That list is its own screen, not 2.13. |

No web state was found for **loading**. The card arrives whole with the
`entry` frame, so there is nothing to load.

## Parts and arrangement

From the top of a pending card:

1. **Orphaned line** (only when the status is `unanswered` or `cancelled`):
   `.cc-ask-sub`, `text-12` (scaled), `text-secondary`, wraps, `sp-8` below.
2. **Per question, in order** (`.cc-q`, `sp-12` between questions and
   before the buttons):
   - **Chip** (only if the question has a `header`). It is a pill, as wide
     as its text: `accent` fill, `white` text, `text-10` (scaled),
     `w-bold`, upper case, `track-wide`, padding `sp-2` × `sp-6`, `r-pill`,
     `sp-4` below.
   - **Question:** `text-13` (scaled), `w-semibold`, `text-primary`, wraps,
     `sp-8` below.
   - **Choices,** one per option, `sp-6` apart (`.cc-opt`):
     - a row with `r-control`, a 1px `border` edge and `bg-primary` fill,
       padded `sp-8` × `sp-10`, with room on the right for the note button;
     - on the left, the radio (single) or checkbox (multi): `icon-sm`,
       `sp-2` down from the top so it lines up with the label, `sp-8` before
       the text;
     - the text column:
       - label: `text-13`, `w-semibold`, `text-primary`, wraps;
       - description (if any): `text-12`, `text-secondary`, wraps, right
         under the label;
       - preview (if any): a box `sp-6` below, `bg-secondary`, a 1px
         `border-subtle` edge, `r-chip`, padded `sp-6` × `sp-8`, `mono`
         `text-11` (scaled), `text-secondary`, keeping line breaks and
         wrapping long lines (the web's `pre-wrap`);
       - the note row (when open): `sp-8` below. A field (`bg-primary`, 1px
         `border`, `r-control`, `text-12`, placeholder "Note for Claude about
         this choice") fills the width, with a ✕ button (`control-sm`
         square at most, `text-muted`, hover `text-primary` on a faint fill)
         `sp-6` after it.
     - **Note button,** at the top right inside the row, `sp-4` in from the
       top and right edges: `control-sm`-sized (the web's 26), `r-control`,
       with the `note` icon at `icon-md` in `text-muted`.
   - **Other:** a single-line field the full width of the question, `sp-4`
     below the last choice. It has `bg-primary`, a 1px `border`,
     `r-control`, padding `sp-6` × `sp-10`, `text-12` (scaled), and the
     placeholder "Other (type your own answer)…".
3. **Buttons** (`.cc-btns`, gap `sp-8`, left-aligned, wrap): **Submit**
   (`CardBtn` primary) then **Dismiss** (`CardBtn` danger).

The card itself: `accent` 1px edge, `accent-dim` fill, `r-card`, padding
`sp-12`, full width of the conversation column. (The web adds a 1px
`shadow-sm`, which can't be seen on the dark. Leave it out.)

**Answered card** (`.cc-answered`): the same box, but holding only a
wrapping row of tags (gap `sp-6`):
- one **green tag** per question, holding its answer text, in question
  order (a pill: `bg-tertiary` fill, 1px `green` edge, `green` text,
  `text-11` scaled, padding `sp-2` × `sp-8`);
- each note on its own full-width line under the tags: "Note: <text>",
  `text-11` (the web's 11.5), `text-muted`;
- "by Cru" (a neutral tag: `bg-tertiary`, `border-subtle` edge,
  `text-secondary`) when `answeredBy` is `cru`.

Dismissed: one **red tag** "dismissed" (`red` edge and text). A status that
is answered but carries no answers shows one neutral tag, "answered".

### Measurements (web → token)

Taken with `tools/measure.js` on the One question card at 568 pane width,
`--cc-fs` 1.

| Part | Web | Token |
|---|---|---|
| Card padding / radius / edge | 11×12, 10, 1px accent | `sp-12`, `r-card`, 1px `accent` |
| Chip | 18.3 high, 10.5px w650, pad 1×7, gap 5 | `text-10`, `w-bold`, `sp-2`×`sp-6`, `r-pill`, `sp-4` |
| Question → first choice | 7 | `sp-8` |
| Choice row | pad 7/36/7/10, gap 8, radius 6, 55.7 high with a description | `sp-8`, `sp-10`, right `sp-32`+`sp-4`, `r-control` |
| Between choices | 5 | `sp-6` |
| Radio / checkbox | 13 square, 3 down | `icon-sm`, `sp-2` |
| Label / description | 13 w600 / 12 secondary, 1 apart | `text-13` `w-semibold` / `text-12` |
| Preview box | pad 6×8, radius 4, 5 above, mono 11 | `sp-6`×`sp-8`, `r-chip`, `sp-6`, `text-11` |
| Note button | 26 square, 5 in, icon 14 | `control-sm`, `sp-4`, `icon-md` |
| Note row | 7 above, field 26 high pad 5×8, ✕ 24, gap 6 | `sp-8`, `sp-6`, `control-sm` |
| Other | 28 high, pad 6×9, 4 above | `sp-6`×`sp-10`, `sp-4` |
| Question → question; last Other → buttons | 11 | `sp-12` |
| Buttons | 28 high, pad 6×12, gap 7, 12px | `CardBtn`, gap `sp-8` |
| Selected ring | 0 0 0 3px accent .28 | `accent-ring` outset `sp-4` |
| Answered tag | 21 high, pad 1×8, 11px | `sp-2`×`sp-8`, `text-11`, `r-pill` |
| Orphaned line | 12px secondary, 8 below | `text-12`, `sp-8` |

## Controls and behaviour

Each card keeps its **picks** while pending. There is one per question:
`{chosen: [labels], other: text, note: text, noteOpt: label}`.

- **Choice row, single-choice question.** A press anywhere on the row
  (label, description, preview or radio) picks that choice. It also
  un-picks the others and **clears Other** (both its text and the pick).
- **Choice row, multi-select question.** A press toggles the checkbox.
  `chosen` is the ticked labels in option order.
  - *Named difference:* the web leaves Other's text in place when a box is
    ticked, and Other then wins over the ticks, which are silently ignored.
    The native client clears Other on a tick, the same as a single choice.
    Treat this as a web accident.
- **Selection look:** a picked or ticked row is selected (accent edge,
  `accent-dim` fill, `accent-ring`). Hover on any row turns the edge
  `accent-hover`.
- **Note button** ("Add a note to this choice"): picks the choice if it
  isn't picked, then opens the note row under that choice. The note field
  takes the focus and the button shows as on (`accent-hover` icon).
  - There is one note per question. Opening it on another choice moves it
    there, with its text kept.
  - If the noted choice stops being picked, the note moves to the
    last-picked choice. If nothing is picked, the note row closes but its
    text is kept in the picks.
- **Note field:** typing saves at once. A press inside it does not toggle
  the row. Enter does nothing (the web swallows it), and Space types a
  space.
- **Note ✕** ("Remove the note"): clears the note and closes the row.
- **Other:** typing sets `other`. Any text clears every pick in that
  question (no radio or tick stays). Emptying it leaves nothing chosen.
  - *Named difference, as the Manager asked:* **Enter in Other submits the
    card** when Submit is enabled. The web does nothing on Enter (there is
    no handler; checked in the browser). Say so in PLAN.md.
- **Answer of a question:** Other's trimmed text if it has any, otherwise
  the chosen labels joined with ", ".
- **Submit:** disabled (dimmed, `o-dim`, no hover) until **every** question
  has a non-empty answer. It re-checks on every change.
  - Live question: POST `respond` with `behavior: allow` and `updatedInput`.
    That is the question's own input plus `answers` (`{question text:
    answer}`) and, if any note is non-empty, `annotations` (`{question
    text: {notes}}`).
  - Orphaned question: POST `answer-closed` (see Data).
  - Either way the card shows its answered tags **at once** (an optimistic
    lock, so a double press can't answer twice), and the answer draft is
    removed.
- **Dismiss:**
  - Live question: POST `respond` with `behavior: deny` and the message
    "The user dismissed the question.". The card shows "dismissed" at once.
  - Orphaned question: POST `answer-closed` with no answers and no text.
    The card shows the neutral "answered" tag, and nothing is sent to
    Claude.
  - Either way the draft is removed.
- **Errors:**
  - `respond` failures are ignored by the web, and the live `patch`
    resyncs. Native: if the POST fails, put the card back to pending (with
    the picks) and show the chat's error line. *Named difference;* the web
    leaves a card that claims an answer nobody received.
  - `answer-closed` with `{ok:false, error}`, or a network error, appends a
    local error entry in the chat with that text (seen: "Session is not
    running"). The card stays answered (the server already marked it).
- **Keyboard** (from the web's native inputs):
  - Tab order through the card:
    1. each single-choice question's radio group (one stop: the picked
       radio, or the first);
    2. that question's note buttons;
    3. each checkbox in order;
    4. the open note field and ✕;
    5. Other;
    6. then the next question;
    7. Submit, then Dismiss.
  - In a radio group, ↑/↓ (and ←/→) move to and **pick** the next choice.
    Space picks.
  - On a checkbox, Space toggles.
  - On a button, Enter or Space presses it.
  - The focused control shows the accent ring (`web-keyboard-focus-radio`).
  - Esc does nothing to the card.
  - The composer is unaffected: typing there while a question is pending
    goes to the held drawer (2.9).
- **Text size** (pane A−, A+ and reset; 2.1): every font in the card scales
  with the pane's `fs`, including the chip, the buttons, the preview and
  the tags. Paddings, the radio or checkbox and the note button do not.
- **Narrow pane:** nothing reflows apart from wrapping. Labels,
  descriptions, the question and preview lines wrap; the buttons wrap onto a
  second line if they must. There is no sideways scrolling.

## States

- **pending:** as above. It may be partly answered (from the draft).
- **answered** (`allowed`, or `answered-late` after an orphaned answer):
  green tags, note lines and "by Cru". It is shown the same way whether
  this client answered it or another one did. The change arrives as a
  `patch` frame and the card redraws in place.
- **dismissed** (`denied`): the red "dismissed" tag.
- **orphaned** (`cancelled` or `unanswered`): it is answerable, with the
  extra line at the top.
  - `cancelled` comes from the chat's process exiting, or from the turn
    ending with the question still open.
  - `unanswered` comes from a restart that restored the conversation from
    Crundi's own record. The chat then also shows the notice "This
    conversation stopped waiting for an answer. That question closed with
    the previous session — say what you want done and Claude will pick it
    up again." as its last entry (an ordinary notice entry).
  - Seen on this server: a restart that resumes from the CLI transcript
    brings back no card at all, only the `AskUserQuestion` tool row. That
    is fine; the client just renders what the history holds.
- **Pane state:** while a question is pending, the chat is `needs-input`
  (NEEDS INPUT tag, "needs your input" status, and Send reads Queue). These
  come from 2.4 and 2.9 and are unchanged here.

## Answer drafts (pane switch and restart)

A half-answered card survives anything that rebuilds it: switching panes or
projects, closing and reopening the window, or a client restart. The web
keeps `crundi_chat_ans_<chatId>_<entryId>` = `{v: picks[], t: ms}` in
localStorage:
- written on every change;
- removed when no question has a choice, Other text or note;
- removed on Submit or Dismiss;
- restored when the card is drawn (picks, ticks, Other text and the note
  row);
- pruned after 30 days.

Native: keep them in the 2.8 drafts store (src/drafts.rs, saved per
server), keyed `ans:<chatId>:<entryId>` with the same `{v, t}` and the same
`TTL_MS`. Keep them apart from the composer drafts (`chat:` and `project:`
keys) and out of the project-claim logic. The native client restores them
when it builds a pending question card, and must not restore them into an
answered card.

## Data and endpoints

- **The entry** (from `GET /api/ui-sessions/<id>/history` →
  `session.messages[]`, then live `entry` and `patch` frames on the chat's
  socket):
  ```
  { id, kind: "question", requestId, toolUseId, toolName: "AskUserQuestion",
    status: "pending"|"allowed"|"denied"|"cancelled"|"unanswered"|"answered-late",
    input: { questions: [ { question, header, multiSelect,
             options: [ { label, description, preview? } ] } ] },
    answeredInput?: { …input, answers: {question: answer},
                      annotations?: {question: {notes}} },
    answeredBy?: "cru" }
  ```
  The CLI requires a `description` on every option. `preview` is optional.
  There are 1-4 questions with 2-4 options each.
- **Answer:** `POST /api/ui-sessions/<chatId>/respond`
  - Body: `{requestId, behavior: "allow", updatedInput: {...input, answers,
    annotations?}}`.
  - Dismiss body: `{requestId, behavior: "deny", message: "The user
    dismissed the question."}`.
  - Reply: `{ok: true}`, or `{ok: false, error: "That request is no longer
    pending"}` and similar.
  - The server then patches the entry to `allowed` or `denied` with
    `answeredInput`, and the chat goes to `working`.
- **Orphaned answer:** `POST /api/ui-sessions/<chatId>/answer-closed`
  - Body: `{entryId, answers, text}`. The text is "Answering the question
    from before the session restarted:" followed by one line per question,
    `<question> -> <answer>`, plus ` (note: <note>)` when there is a note.
    The lines are joined with `\n`.
  - Dismiss body: `{entryId, answers: {}, text: ""}`.
  - Reply: `{ok: true}`, or `{ok: false, error}` (for example "That
    question is still live — answer it normally." or "Session is not
    running"). The entry is patched to `answered-late` before the message
    is sent.
- **Live:** the `entry` frame (a new question), and the `patch` frame `{id,
  patch: {status, answeredInput, always, answeredBy?}}`. The patch arrives
  when this client, another client or Cru answers, when the process exits,
  or when the turn ends. The chat's `needs-input` state comes with the
  session state as before (2.4).
- **Older servers:** both routes have existed for a long time. A 404 on
  `answer-closed` shows "Answering an old question is not supported by this
  server" as the chat's error line (backwards-compatible clients).

## End-to-end tests (the CR's question flows)

Make the test chat ask with a message like the one used here:

> Call the AskUserQuestion tool exactly once with exactly this input JSON
> (verbatim, no changes), then after the answer reply with one short
> sentence repeating what I chose and stop. Input: {…}

Every option needs a `description`, or the CLI rejects the call. Wait for
the pane to show needs-input (about 60 s), then drive the card through
testctl. After each run, read `GET /api/ui-sessions/<id>/history` and check
the question entry's `status` and `answeredInput`:

| Flow | Do | Server received |
|---|---|---|
| One choice | pick SQLite, Submit | `allowed`, `answers {"Which database…?": "SQLite"}` |
| Choice with a note | note on MySQL "Keep the file under data/", Submit | `answers {"…": "MySQL"}`, `annotations {"…": {notes: "Keep the file under data/"}}` |
| Multi-select | tick Unit and Integration, Submit | `answers {"…": "Unit, Integration"}` |
| Several questions | Rust; Unit + Integration; This server, Submit (disabled until the third) | three keys in `answers` |
| Other | type "Only export for now, as JSON", Enter | `answers {"…": "Only export for now, as JSON"}` |
| Dismiss | Dismiss | `denied`, no `answeredInput.answers` |
| Draft survives a restart | tick two, quit the client, start it, Submit | the two ticks come back, and are what was sent |
| Orphaned | kill the chat's claude process (`CRUNDI_CHAT_ID` in its environment, a child of the test server) while asking | the card gets the orphaned line; Submit gives `answered-late` (and a `send`) |

## Checklist

- [x] One question and several questions in one card: chip, question,
      choices, Other per question; one Submit / Dismiss per card.
- [x] Single choice (radio) and multi-select (checkbox) look and toggle as
      described; selected rows have the accent edge, fill and ring; hover
      turns the edge `accent-hover`.
- [x] Descriptions under labels; previews as mono boxes that keep line
      breaks and wrap.
- [x] Note button on every choice: picks, opens the note under the choice,
      focuses it, shows on; the note follows the selection; ✕ removes it;
      Enter in the note does nothing.
- [x] Other: typing clears that question's picks; a pick or tick clears
      Other (named difference for multi); Enter in Other submits when
      enabled (named difference).
- [x] Submit disabled (dimmed) until every question has an answer; enabled
      hover glow; Dismiss danger hover.
- [x] Submit sends `respond` allow with `answers` (+ `annotations` only
      with a note); Dismiss sends deny with "The user dismissed the
      question."; the card locks into its answered look at once.
- [x] Answered: green tags in question order, "Note: …" lines, "by Cru";
      dismissed: red tag; empty answers: neutral "answered".
- [x] Answered elsewhere (another client, Cru) updates the card live from
      the `patch` frame.
- [x] Orphaned (`cancelled` / `unanswered`): the extra line, still
      answerable; Submit and Dismiss go to `answer-closed` with the right
      text; errors show as the chat's error line.
- [x] Answer drafts kept per chat and entry in the drafts store; restored
      after a pane switch, a project switch and a client restart; cleared
      on Submit / Dismiss; 30-day expiry.
- [x] Keyboard: Tab order, arrows pick in a radio group, Space toggles,
      Enter/Space on buttons, accent focus ring.
- [x] Text size scales the card's fonts (not paddings or the radio and
      note button); narrow panes only wrap.
- [x] A failed `respond` puts the card back to pending with an error
      (named difference).
- [ ] Tokens only; side-by-side crops of every state above saved under
      `specs/questions/review/`.
- [x] End-to-end flows above pass in `test/phase2.mjs`.
