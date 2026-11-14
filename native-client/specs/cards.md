# 2.20 Cards

The cards a chat's log can carry besides replies, tools and questions. The
main one is **a factory's result card** (`kind: 'factory-result'`): Crundi
posts it into the factory Manager's chat when a run finishes, and the person
decides there what happens to the work (merge, request changes, keep,
discard, undo). The second is the **row of a widget check's screenshots**
under a `widget_render` call. Everything else a log can carry is already
specified elsewhere or belongs to a later phase (see "Every card a log can
carry").

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.
Where the web's number is given, it is only there to pick the token.

Source:
- `app/vendor/claude-chat.js`: CSS 42-58 (`.cc-fr-*`) and 121-136 (`.cc-ask`,
  `.cc-ask-title`, `-sub`, `-reason`, `.cc-btns`, `.cc-btn`, `.primary`,
  `.danger`, `:disabled`); `paint()` 1745 (`case 'factory-result'` 1785);
  `factoryResultNode()` 2035-2152.
- `app/vendor/crundi-widgets.js`: CSS 126-131 (`.wg-inline*`, `.wg-shots`,
  `.wg-shot`); `inlineNode()` 702, `shotsNode()` 718.
- `src/webapp-html.js`: `widgetToolExtra()` 8579 (which tool calls get a
  card under them); the Manager's pane and its **Factory** tag 9129-9138.
- Server: `present()` `src/factory-runner.js` 959 (the card's fields),
  `finishAction()` 1044, `runDiff()` 1087, `doMerge()` ~1003;
  `addCard()` / `patchCard()` `src/claude-ui.js` 1039/1048;
  `POST /api/factory-runs` and `GET /api/factory-runs/diff` `src/webapp.js`
  5169-5216; `fullDiff()` `src/factory-git.js` 243 (2 MB cap);
  `GET /api/widgets/renders`, `/api/widgets/render-image`
  `src/widget-api.js` 338/344.

Native today: `src/chat.rs` drops `factory-result` entries (no `Shown`
kind), and `sync_panes()` (`src/app.rs` ~1064) filters out every chat with
a `factory` field, so **the Manager's chat has no pane at all**. Fix that
first (see "Where the card lives").

## Screenshots

`cards/web-<state>.png`, 1400×860 (the narrow one at 1000×860), two panes:
the Manager's chat is the right one (~560 wide). `-2x` is the card cropped
and doubled. Every factory state except kept, discarded, conflict,
merged-automatically and the merge error is **real**: a real run on the test
server (project `demo` made a git repo, factory "Site builder", a real
Manager chat), finished through `factory_handoff`, and the buttons pressed
for real (Request changes, Merge, Undo). Those five were patched in with the
server's own patch event and the card's own fields. The busy and failed
states used a fetch stub.

| State | What it shows |
|---|---|
| `web-ask-two-panes` (+`-2x`) | **Waiting for a decision** (`state: 'ask'`). Title "Factory result · Site builder"; the summary (markdown); the closed files line "▸ +130 −33 in 8 files on factory/site-builder-405aa5"; "Look it over, then choose what happens to it."; the usage line "$0.18 · 1 turns · 1 handoffs · 1 chats"; "▸ 3 follow-ups from review"; buttons **Merge into dev** (primary), **Request changes**, **Keep branch**, **Discard**. |
| `web-files-open` (+`-2x`) | **Files opened:** one mono row per file, name left (ellipsis when long), "+N −N" right (green, red); then **Show the full diff** (full width). |
| `web-full-diff` (+`-2x`) | **The full diff loaded:** the button is gone; the unified diff as plain mono text (no colours, no box), at most half the window tall, scrolling both ways. |
| `web-followups-open` (+`-2x`) | **Follow-ups opened:** "**F1** text (where)", "**F3** blocking text": id bold, **blocking** red, where muted in brackets; smaller text. |
| `web-changes-form` (+`-2x`) | **Request changes pressed:** under the buttons, a 3-line note box ("What should change?", focused, accent ring) and a full-width primary **Send to the Manager**. |
| `web-discard-armed` (+`-2x`) | **Discard pressed once:** it reads **Sure? Discard** for 3 s, then goes back. |
| `web-discard-hover` (+`-2x`) | Discard hovered: red border, `red-dim` fill, red text. |
| `web-button-hover` (+`-2x`) | A plain button hovered (Request changes): accent border, `accent-dim` fill. |
| `web-busy` (+`-2x`) | **A choice being sent:** every button of the card at half opacity, not clickable; the note box stays editable. |
| `web-action-error` (+`-2x`) | **The server said no** (`ok:false`): buttons usable again; under everything, the error in muted small text ("The run is running; there is no result waiting for a decision."). |
| `web-changes` (+`-2x`) | **Changes requested** (real): the card updated in place; the summary is gone (the server clears it while the team works); files line; "Changes requested. The team is working on them; this card updates when the Manager finishes again."; usage; follow-ups; no buttons. |
| `web-long-summary` (+`-2x`) | **Long content** (real, the second finish): a long markdown summary (headings, bullets, numbered list, inline code) and 53 files. |
| `web-long-files-end` (+`-2x`) | The end of a long files list: 40 rows, then "and 13 more" (muted), then Show the full diff. Usage now "$0.20 · 2 turns · 2 handoffs · 1 chats · 1 min". |
| `web-merged` (+`-2x`) | **Merged** (real): "Merged into **dev** as one commit `6b00fea`." and one button, **Undo**. |
| `web-undone` (+`-2x`) | **Undone** (real): "Undone. dev is back where it was; the work is kept on **factory/site-builder-405aa5**." No buttons. The files line reads "+0 −0 in 0 files" (the run's worktree is gone by then; the server sends zeros). |
| `web-ask-empty` (+`-2x`) | **Nothing changed** (`diff.count` 0): "The team made no changes, so there is nothing to merge. Ask for changes, or close the run."; only **Request changes** (primary) and **Close the run**. |
| `web-kept-files-binary` (+`-2x`) | **Kept:** "Kept on the branch **…**. Nothing was merged."; files open with a binary file: "+? −?". |
| `web-discarded` (+`-2x`) | **Discarded:** "Discarded. The worktree and branch are gone." |
| `web-conflict` (+`-2x`) | **Conflict:** "Merging into dev conflicts. The Manager is bringing dev in and resolving it, then it will finish again." then the server's error line (muted); usage with hours "$4.37 · 58 turns · 14 handoffs · 5 chats · 2 h 25 min". |
| `web-merged-auto` (+`-2x`) | **Merged automatically:** "Merged automatically into **dev** as one commit a1b2c3d." + Undo. |
| `web-ask-merge-error` (+`-2x`) | **Merge refused** (dirty checkout): back to ask, with the server's `error` above the buttons; usage with no cost and under a minute: "3 turns · 2 handoffs · 1 chats". |
| `web-narrow` (+`-2x`) | **Narrow pane** (~370): the files line wraps; follow-ups wrap; the buttons wrap to a second row (Keep branch, Discard). |
| `web-text-large` (+`-2x`) / `web-text-small` (+`-2x`) | **Text sizes** (the pane's A+ / A-): every part of the card scales with the pane's text size, buttons and file rows included. |
| `web-widget-cards` (+`-2x`) | **Widget cards:** under a `widget_open` call with `slot: 'inline'`, the live widget (header with its title and an Open-in-Panels button, the widget's page below); under a `widget_render` call, the row of the check's screenshots ("inline", "mobile" captions). |

## Where the card lives

The card is an entry of the **Manager's chat** of a run. That chat is in
`state.terminals` (an ordinary `kind: 'ui'` chat with
`factory: { runId, role: 'manager', employeeId }`); employees are not in
`terminals`. The web gives it a pane like any chat, with the **Factory** tag
in its header (pane-header-wide.md) and a close that the server refuses
(409) while the run goes (already in `head.rs`).

- **Native must give the Manager's chat a pane.** `sync_panes()` filters
  `s(c, "factory").is_empty()`, which drops it (the field is an object).
  Keep only that the list is `terminals`' `kind: 'ui'` chats, as the web.
- The pane's header shows the **Factory** tag (pane-header-wide.md).

## Parts of the factory result card

The card is the question card's box (`.cc-ask`, already built for 2.13):
1px `accent` border, `accent-dim` fill, `r-card`, padding `sp-12`
(web 11/12), the small shadow. Top to bottom, each part only when it has
something:

| Part | Web | Native |
|---|---|---|
| Title | "Factory result" + " · <factory>" when `factory` is set; weight 650, `text-primary`, gap 3 below | as the question card's title: `w-semibold`, `text-13`, `sp-4` below |
| Summary | `md(summary)`, `text-primary`, margin 6 / 8 | the log's markdown block (the same one as assistant text): paragraphs, **bold**, `code`, lists, headings; `sp-6` above, `sp-8` below. **Named difference:** the web's lists are not indented (bullets hang outside the card's text, see `web-long-summary`) and headings have no gap; native draws lists and headings as in replies. |
| Files line (a disclosure) | "▸ " + "+A" (green) " −R" (red) " in N file(s)" + " on <branch>" (`text-muted`); `.92em`, `text-secondary`; margin 4 / 8 | a row with the chevron (`Icon` chevron-right / chevron-down, `icon-sm`), the counts at `text-12`; `text-secondary`, branch `text-muted`; the whole row toggles |
| File rows (open) | `.cc-fr-list`: gap 2, top 6; each row mono `.9em` (~11), name left with ellipsis, "+a −r" right (`?` when null), gap 10 | `mono`, `text-11`; name elides at the end; counts never wrap (the web wraps them under a long name, see `web-files-open`: accident); `sp-2` between rows, `sp-6` above |
| "and N more" | when `diff.count` > rows (the server sends at most 40): `.cc-ask-sub` style | `text-12`, `text-secondary` |
| Show the full diff | `.cc-btn` across the list's width; only in ask, conflict, changes | `CardBtn`, full width |
| The diff | `pre`, mono `.85em`, no background, `max-height:50vh`, scrolls both ways, no wrap | `mono` `text-11`, `text-primary`, no box; height at most half the window's height (derived from the window, not a typed number; `pre-max-h` is the permission card's 200 and too small), both scrollbars; lines never wrap. It can be 2 MB: draw it as a virtualised list of lines (a `ListView` of one-line `Text`s), not one huge `Text`. "\n… (cut short)" is appended when `truncated`. |
| State message (`.cc-ask-sub`) | `text-secondary`, 12, 8 below; bold and code as in the table below | `text-12`, `text-secondary`, `sp-8` below, wraps; the bold parts `w-bold` in the same colour; the commit `mono` |
| Usage line | `.85em` (~11), `text-secondary`, margin 2 / 6 | `text-11`, `text-secondary`, `sp-2` above, `sp-6` below |
| Follow-ups (a disclosure) | "▸ N follow-up(s) from review"; rows `.92em` / line 1.45: "**F1** " + red "blocking " + text + muted " (where)" | as the files line; rows `text-11`, wrap; id `w-bold`; "blocking" `red`; where `text-muted` |
| Server error (`e.error`) | `.cc-ask-reason`: `text-muted`, 11.5, 8 below | `text-11`, `text-muted`, wraps, `sp-8` below |
| Buttons (`.cc-btns`) | row, gap 7, wraps | `CardBtn`s in a wrapping row (the question card's), gap `sp-8` |
| Note form (hidden until Request changes) | column, gap 6, top 8: textarea 3 rows, `bg-primary`, 1px `border`, r 6, padding 6/8, placeholder "What should change?", resizable vertically; then primary **Send to the Manager** across the width | `sp-8` above, `sp-6` gap; the question card's text field (`QField`, multi-line, 3 lines tall, grows to at most 6, then scrolls; no resize grip), placeholder "What should change?"; `CardBtn` primary, full width |
| Action error (`errLine`) | `.cc-ask-reason`, hidden until a POST answers `ok:false` | as Server error, last in the card |

### Text of each state (`state` from the server)

| state | message |
|---|---|
| `ask`, `diff.count` > 0 | "Look it over, then choose what happens to it." |
| `ask`, `diff.count` 0 | "The team made no changes, so there is nothing to merge. Ask for changes, or close the run." |
| `merged` | "Merged into **<parent>** as one commit `<commit 7>`." ("Merged automatically into …" when `auto`; no commit part when `commit` is empty) |
| `undone` | "Undone. <parent> is back where it was; the work is kept on **<branch>**." |
| `kept` | "Kept on the branch **<branch>**. Nothing was merged." |
| `discarded` | "Discarded. The worktree and branch are gone." |
| `conflict` | "Merging into <parent> conflicts. The Manager is bringing <parent> in and resolving it, then it will finish again." |
| `changes` | "Changes requested. The team is working on them; this card updates when the Manager finishes again." |

`<parent>` is `parentBranch`, or "the parent branch" when empty. No branch →
no " on <branch>" in the files line. Plurals: "1 file" / "N files",
"1 follow-up" / "N follow-ups".

**Usage line:** shown when `usage.turns` or `usage.costUsd`:
`["$" + cost (2 decimals) + " · "] + turns + " turns · " + handoffs + " handoffs · " + chats + " chats" [+ " · " + time]`;
time from `activeMs` rounded to minutes, left out at 0, "N min" under an
hour, "H h M min" from an hour. **Named difference:** the web never
pluralises here ("1 turns · 1 handoffs · 1 chats"); native says "1 turn",
"1 handoff", "1 chat" (as with "1 tools" in 2.18).

### Buttons per state

| state | buttons |
|---|---|
| `ask`, changes | **Merge into <parent>** (primary), **Request changes**, **Keep branch**, **Discard** (danger) |
| `ask`, no changes | **Request changes** (primary), **Close the run** (danger) |
| `merged` | **Undo** |
| others | none |

## Behaviour

1. **Live, in place.** The server adds the card once (`{type:'entry'}`) and
   from then on patches the same entry (`{type:'patch', id, patch}` where
   `patch` is the whole card) at every step: changes → ask again, merge,
   undo, and so on. The card is redrawn from the patched entry; it never
   moves in the log. Every open copy of the chat shows the same state.
2. **Expanding.** The files line and the follow-ups line each toggle their
   list (closed at first). The note form toggles with Request changes. The
   full diff is fetched once (Show the full diff disables itself while it
   loads; on success the button goes and the diff shows; on a network
   failure the button comes back enabled; `ok:false` shows the server's
   error, or "Could not load the diff", in the diff's place).
   **Named difference:** the web rebuilds the card on every patch, so open
   lists close, a loaded diff is dropped and a half-typed note is lost.
   Native keeps, per card id, which lists are open and the note text across
   patches (as the tool cards do, 2.14); a loaded diff is dropped when the
   state changes (the worktree may be gone).
3. **Choices.** Every button posts `POST /api/factory-runs`
   `{ action: 'finish', id: runId, choice, [note] }`:
   - Merge → `merge`; Keep branch → `keep`; Undo → `undo`;
   - Discard: the first click turns it into **Sure? Discard** for 3 s; a
     second click in that time → `discard`; after 3 s it says Discard
     again. **Close the run** (no changes) → `discard` at once, no arming.
   - Request changes opens the note form (and focuses the note); pressing
     it again closes the form. **Send to the Manager** with an empty or
     blank note only focuses the note; otherwise → `changes` with
     `note` (the text as typed).
   - While a POST runs: every button in the card is disabled (dimmed);
     the error line hides. `ok:true` → nothing more: the patch that follows
     redraws the card. `ok:false` → the error line shows `error` (or "That
     did not work."), buttons enabled again. A network failure → buttons
     enabled again, no message (the web's; native also shows the error line
     "Could not reach the server.": named difference).
   - A 404 on the endpoint (an old server) → the error line "Not supported
     by this server." (backwards-compatibility rule).
4. **Keyboard.** The buttons and disclosures are focusable (Tab), Enter /
   Space press them. In the note, Enter is a new line (it is a textarea in
   the web); Ctrl/Cmd+Enter sends (native addition, named).
5. **Text size.** Everything scales with the pane's text size (`fs`), as
   the question card does: title, summary, rows, buttons, note.
6. **Narrow.** The buttons wrap onto more rows; the files and follow-up
   lines wrap; file names elide first, the counts stay whole.
7. **Restart and reload.** The card is kept in the server's memory
   (`factory-result` is not in `PERSIST_KINDS`), so a reconnect's
   `ui-history` brings it back with its latest state. After a server
   restart, a run still waiting for a decision is recovered and the card is
   posted again (`present(entry, { state: 'ask' })` in recovery).
8. **Unknown card kinds.** `addCard` takes any `kind` (default `'card'`);
   the web draws nothing for a kind it does not know. Native: the same
   (skip it, no "unknown entry" placeholder).

## The widget screenshots row (`widget_render`)

Under a finished, successful `widget_render` tool call (any `…widget_render`
name, `status: 'done'`, not `isError`, `input.id` set), the row of the
check's screenshots (`shotsNode`):
- `GET /api/widgets/renders?project=<alias>&id=<id>` →
  `{ ok, renders: { at, shots: [{ frame, state, width, height, label, … }] } }`.
  No shots (or not ok) → nothing at all.
- One figure per shot: the picture from
  `GET /api/widgets/render-image?project=<alias>&id=<id>&name=<frame>--<state>&t=<at>`
  (PNG), **150 tall**, width from the picture, 1px `border`, `r-control`,
  `bg-primary` behind; under it the caption `<frame>` (+ " · <state>" when
  the state is not "live"), `mono`, `text-10`, `text-muted`, `sp-4` gap.
- The row: `sp-8` between figures, `sp-8` above, scrolls sideways when it
  is wider than the pane.
- A click on a picture opens it in the picture viewer (2.16m's
  `viewer.slint`), titled "Render: <frame>".
- Native: the picture's height a named size (`shot-h`, the web's 150).

## Every card a log can carry

| Card | Kind / source | Where |
|---|---|---|
| Factory result | entry `factory-result` | **this spec** |
| Widget screenshots | under `widget_render` (`shotsNode`) | **this spec** |
| Inline widget | under `widget_open` with `slot: 'inline'` (`inlineNode`): a bordered card, header (panel icon, title, Open in the Panels tab), the live widget page below (auto height, at most 460) | **gap: phase 3** (Panels). It is a live HTML page, so it needs the in-log CEF view of 2.16f, and its Open button goes to the Panels tab, which phase 3 brings. Until then native shows nothing under the call (the tool row itself is there). |
| Permission (incl. the plan approval) | entry `permission` | 2.13 / F107 (done) |
| Question | entry `question` | 2.13 (done) |
| Goal verdict | entry `goal-verdict` | 2.19 (done) |
| Turn result, error, notice | entries `result`, `error`, `notice` | 2.16 (done) |
| Media under `show_*` | `toolMedia` | 2.16m (done); live frames 2.16f |
| Agent / task badges | not entries: the agent dock (`kindOf`, `badgeEvent`) | 2.18 (done) |
| Suggestions, recaps, side questions | `cc-sug`, `cc-recap`, `cc-aside` | 2.17 / 2.21 (done) |
| Cru's reply cards | `replyMedia` drops `kind: 'card'` items: Cru's own | phase 6 (Cru) |
| The factory's own screens (run view, its "Manager's chat" button, the run tab) | `crundi-factories.js` | phase 4 (Planning: factories) |

## Data and endpoints

- **The card** (an entry; `src/factory-runner.js` `present()`):
  ```
  { id, kind: 'factory-result', runId, state, error,
    factory, task, summary, branch, parentBranch, commit, auto,
    diff: { added, removed, count, files: [{ file, added|null, removed|null }] (≤ 40) },
    followUps: [{ id: 'F1', text, where, blocking }],
    usage: { costUsd, turns, chats, activeMs, handoffs } }
  ```
  Arrives as `{type:'entry', entry}` in the chat's `ui-event` stream and
  in `ui-history`; then `{type:'patch', id, patch: <the whole card>}`.
- `POST /api/factory-runs` `{ action: 'finish', id, choice: 'merge'|'changes'|'keep'|'discard'|'undo', note? }` →
  `{ ok: true, run }` or `{ ok: false, error }`. Known errors: "Run not
  found.", "The run is <status>; there is no result waiting for a
  decision.", "Say what should change.", "Only a merged result can be
  undone.", git's own (blocked, conflict). A merge that fails comes back as
  `ok:false` **and** a patch to `ask` (or `conflict`) with `error` set.
- `GET /api/factory-runs/diff?id=<runId>` → `{ ok: true, diff, truncated }`
  (`diff` up to 2 MB) or `{ ok: false, error }` ("Run not found.", "The
  run's worktree is gone.").
- Widgets: `GET /api/widgets/renders`, `GET /api/widgets/render-image`
  (above).

## End-to-end test (phase2.mjs)

Seed a real run on the isolated test server, as these screenshots were made:
1. In the test server's project folder: `git init -b dev`, a few files, a
   commit. Have `tools/test-server.sh` set
   `FACTORY_WORKTREES_DIR="$D/worktrees"` so runs stay under
   `/tmp/crundi-devtest-<port>` (the default is `~/crundi-factory-worktrees`).
2. `POST /api/factories` `{ action: 'create', name: 'Site builder', definition }`
   with roster Manager + Developer and nodes start → manager → end
   (`sourceHandle: 'finish'`), manager ⇄ employee (the `solo()` factory of
   `scripts/test-factory-result.mjs`). Persona and task tell the Manager to
   wait and call no tools.
3. `POST /api/factory-runs` `{ action: 'start', project, factoryId, task, mergeMode: 'ask' }`
   → `run.id`, `run.managerChatId`, `run.worktree`.
4. Write files into `run.worktree` (an edit, a new file with a long path, a
   deletion).
5. `POST /api/mcp/call` with an ordinary chat's key (`data/dev-mcp/<chat>.json`,
   as phase2.mjs already does) `{ tool: 'factory_handoff', args: { sessionId: managerChatId, summary, finish: true, findings: [ two nits, one blocking ] } }`.
   The server posts the card (state `ask`) into the Manager's chat.
6. Check in the native client: the Manager's chat has a pane with the
   Factory tag; the card's title, summary, "+A −R in N files on <branch>",
   the ask message, the usage line, "3 follow-ups from review", the four
   buttons in order.
7. Open the files line: N rows with their counts; press Show the full diff:
   the diff text appears (it holds "diff --git").
8. Open follow-ups: F1-F3, "blocking" on F3.
9. Press Request changes, Send with an empty note: nothing is posted. Type a
   note, Send: the card goes to `changes` (no buttons). Hand off again with
   `finish: true`: the same card (same entry id) is back to `ask`.
10. Press Discard once: "Sure? Discard"; wait 3 s: "Discard".
11. Press Merge: the card says "Merged into dev as one commit <7>", one
    button Undo; `git log dev` has the commit. Press Undo: "Undone. …";
    `dev` is back at its first commit.
12. A refused choice: press a button on a card whose run is already decided
    (patch a copy, or post `finish` for an undone run through the client):
    the error line shows the server's error and the buttons are usable.

## Checklist

- [x] The Manager's chat gets a pane (not filtered for its `factory` field), with the Factory tag.
- [x] `factory-result` entries are drawn; unknown card kinds draw nothing.
- [x] Card box as the question card's; title "Factory result · <factory>".
- [x] Summary through the log's markdown (lists indented, headings spaced: named difference).
- [x] Files line: chevron, "+A" green, "−R" red, "in N file(s)", "on <branch>" muted; toggles the list.
- [x] File rows mono, name elides, counts right and whole, "?" for null; "and N more" past 40.
- [x] Show the full diff in ask / conflict / changes only; loads once; plain mono, no wrap, both scrollbars, at most half the window, virtualised; "… (cut short)"; error text in its place.
- [x] State messages exactly as the table, bold / mono parts, "the parent branch" fallback.
- [x] Usage line format (cost when set, minutes / hours, hidden when no turns and no cost), pluralised (named).
- [x] Follow-ups disclosure: count, rows with bold id, red "blocking", muted "(where)".
- [x] Server `error` line; action error line last.
- [x] Buttons per state, order and labels (Merge into <parent>, Request changes, Keep branch, Discard / Close the run, Undo); primary and danger looks; hover (accent / red).
- [x] Discard arms for 3 s ("Sure? Discard"); Close the run does not arm.
- [x] Request changes toggles the note form, focuses the note; empty note only focuses; Send posts `changes` with the note.
- [x] While posting: all card buttons disabled; `ok:false` → error, buttons back; network failure → buttons back + error (named); 404 → "Not supported by this server."
- [x] Patches redraw in place; open lists and the note survive a patch (named difference).
- [x] Text size scales the whole card; narrow pane wraps buttons and lines.
- [x] Widget screenshots row under `widget_render`: 150-tall pictures (`shot-h`), captions, sideways scroll, click → viewer; nothing when no shots.
- [x] Inline widget, Cru cards and the factory screens listed as named gaps in PLAN.md (phase 3 / 6 / 4).
- [x] phase2.mjs drives the run above (steps 1-12) against the test server.
- [x] Tokens only; side-by-side crops under `cards/review/` for every state above.
