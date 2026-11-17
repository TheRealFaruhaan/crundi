# 2.14 Tool cards

Every tool call Claude makes is one **tool card** in the conversation: a
one-line head (status mark, tool name, a summary of its arguments, a caret)
over a body that opens on a click and holds the arguments in a readable form
and the tool's result. Claude Code also sums up a batch of calls in one line
("Read app.js", "Edited app.js port and host"); the web folds the batch
behind that line, a **group head** ("N tools"). A call that started a
sub-agent is an **agent row** with a way into the agent's transcript.

Phase 1 drew a basic card (`ToolRow` in ui/chat.slint): a yellow dot while
running, `mcp__x__y` renamed "y (x)", a summary from the first string
argument, the result only, and a group head built from the native client's
own rules. This screen replaces that with the web's card.

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.
Reuse `ScrollThumb` (ui/widgets.slint) for the body's scrollbar and `Icon`
for the to-do marks. The spinner is new: a ring image (`ui/shapes/`) turned
by `animation-tick()`; give its period a theme token (the web's is 0.7 s).

Source:
- `app/vendor/claude-chat.js`
  - CSS 100-119 (`.cc-collapsed`, `.cc-caret`, `.cc-tool*`, `.cc-spin`,
    `.cc-dot-ok/err`, `.cc-diff-add/del`, `.cc-todo*`), 364-366
    (`.cc-agentrow*`), 438-440 (`.cc-tgroup*`).
  - `shortPath()` 651, `truncate()` 657, `toolSummary()` 663,
    `toolDetailHtml()` 687.
  - `paint()` case `'tool'` 1766-1781: folding into a group.
  - `groupMembers()` 1872, `toolGroupHead()` 1878, `repaintGroup()` 1897,
    `toolNode()` 1901-1953 (the agent row 1905-1917, the card 1918-1952).
  - `taskToolNode()` 4400 and `findToolEntry()` 4392: the same card, open,
    inside a background task's panel (2.18).
  - `addEntry()` 4589 and the `entry` / `patch` frames 4979-4999.
  - `applyAgent()` 4306: an `agent` frame repaints the row that started it.
- Server `src/claude-ui.js`
  - `handleAssistant()` ~1880: a `tool_use` block becomes an entry
    `{kind:'tool', status:'running'}`.
  - `handleUser()` ~1918: its `tool_result` patches it to `status:'done'`
    with `result` (text blocks joined by a newline, any other block as
    `[type]`, at most 200 000 characters) and `isError`.
  - `handleToolUseSummary()` 2770: a `tool_use_summary` frame (Claude Code,
    `CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES`) patches every call it covers with
    `group` (the first call's entry id) and `groupSummary`.
  - The transcript reader 210-250: after a restart, calls come back with
    `status:'unanswered'` when their result was never written.
  - `getAgent()` 765 and `agentMeta()` 781: the `agent` frame.

## Screenshots

`tool-cards/web-<state>.png` at 1400×860, one wide pane (the "Narrow" shot:
three panes, the chat's 406 wide). A `-2x` file is the named region cropped
and doubled.

The chat ran a real turn (Bash, Read, Edit, a failing `cat`, Write, two
`grep`/`find` stand-ins, `seq 1 600`, a WebFetch agent, ToolSearch, an MCP
tool and a sub-agent). The CLI on this machine has no TodoWrite, Grep, Glob or
WebFetch tool and blocks a foreground `sleep`, so those cards, the running
spinner and the hand-made groups are entries injected into the page's
WebSocket (`entry` and `patch` frames, the same path the server's frames
take).

| State | What it shows |
|---|---|
| `web-groups-closed` | **Groups, closed.** Claude Code summed every batch up, so the turn reads as a column of group heads: a dot, the summary in `text-primary`, "1 tool" / "2 tools" and ▸ at the right. "Edited app.js port and host" has a red dot: one of its two calls failed. Agent rows (`web-fetch`, `general-purpose`) sit between them, never folded. |
| `web-groups-open` | Two groups open (▾): their calls show as ordinary cards under the head, the first `sp-4` below it, the next one entry gap (`sp-10`) further down. |
| `web-open-bash` | A **Bash card, open**: the head's caret turns to ▾; the body is the command, the description in `text-muted`, a hairline, then the output in `text-muted`. Below: an **Edit card, open**, with its diff. |
| `web-diff-edit-2x` | **The diff**: each old line "- …" in `red` on `red-dim`, each new line "+ …" in `green` on `green-dim`, each a full-width band, no gaps. |
| `web-failed-open` | **A failed call, open**: the name turns `red`, the dot is red, and the result under the hairline is `red` ("Exit code 1 / cat: missing-file.txt: No such file or directory"). |
| `web-open-read-scroll` | **A Read card, open**, with **the body at its limit**: the input as pretty JSON, then the file with its line numbers (tabs); the body stops at its maximum height and scrolls, with a thin scrollbar at its right. |
| `web-open-write` | **Write**: the whole new content as one `green` on `green-dim` block, then the result. |
| `web-long-output-bottom` | **Very long output** (`seq 1 600`): the body scrolled to its end; the card keeps its maximum height. |
| `web-open-mcp-unknown` | **Tools without a body of their own** (ToolSearch, `mcp__crundi__list_services`): the input as pretty JSON (`{}` when there is none), then the result. The name is the raw tool name. |
| `web-summaries-per-kind` | **One card per kind, closed**: running Bash (spinner), TodoWrite "4 items", Grep "port (*.js)", Glob "**/*.md in …/proj/docs", WebFetch the URL, WebSearch the query, Task the description, Skill the skill, NotebookEdit the short path, a Bash command cut at 140 characters with "…", an MCP tool's `key="value"` list cut with "…", an MCP tool with no arguments (no summary), the **pending AskUserQuestion** (spinner and `questions=[{"header":…`, F84), and a failed Edit. |
| `web-spinner-ask-4x` | The spinner, 4×: a ring in `border` with its top quarter in `accent`. |
| `web-todo-grep-open` | **TodoWrite open**: ☑ done items struck through in `text-muted`, ▸ the item in progress in `accent-hover` (its `activeForm`), ☐ the pending one in `text-secondary`; then the result. Under it **Grep open** (JSON input, result). |
| `web-todo-3x` | The to-do marks, 3×. |
| `web-task-open` | **The Task tool as a plain card** (no agent record for it: after a restart, or a call the server never saw start an agent): summary = its description, the body its JSON input, the result. |
| `web-failed-edit-open` | A failed Edit: the diff, then the `<tool_use_error>` text in `red`. |
| `web-groups-running-failed` | **Group heads' status**: a spinner while any call in it runs; a red dot when any failed. |
| `web-groups-open-states` | The same two groups open: the running Bash inside has its spinner; the failed Bash its red name. |
| `web-live-done-and-wrap` | **Live**: the running call was patched to done, and the group head's spinner turned to a green dot without a reload. Below, an open card whose output is one very long line: it wraps, breaking inside words. |
| `web-agent-row-hover` | **Agent row**, pointer on it: its border turns `accent`. |
| `web-agent-row-click` | The click opens the agent's panel over the log (2.18's screen; shown for reference). Inside, the agent's own calls are ordinary tool cards. |
| `web-text-larger` / `web-text-smaller` | **Text sizes**: A+ four times (×1.31) and A- to the floor (×0.69). Names, summaries, the body, "N tools" scale; the dots, the spinner, the paddings and the caret's place do not. |
| `web-narrow` | **A narrow pane** (406): summaries and the group's line elide, "2 tools" and the caret keep their place; to-do items wrap beside their mark; diff lines wrap inside their band. |

## Parts and arrangement

A tool entry is one item of the log (the log's entry gap below it, `sp-10`).
It is one of three things:

1. **Agent row**: the call started a sub-agent (the chat has an `agent`
   record for its `toolUseId` whose `kind` is not `task`). Never folded.
2. **Grouped**: the entry has `group` and `groupSummary`. The first call of
   the group (`group == id`) draws the **group head**, then, when the group
   is open, its own card `sp-4` under the head. Every other member draws its
   card only when the group is open, and takes no room at all (no entry gap)
   when it is closed.
3. **A card** otherwise.

### The card (`.cc-tool`)

```
┌──────────────────────────────────────────────────────────────────┐
│ ● Bash  ls -la                                                 ▾ │  head
├──────────────────────────────────────────────────────────────────┤
│ ls -la                                                           │  body:
│ List the project files                                           │  arguments
│ ──────────────────────────────────────────────────────────────── │
│ total 16 …                                                       │  result
└──────────────────────────────────────────────────────────────────┘
```

- Box: `bg-secondary`, a 1px `border-subtle` edge, `r-control`, clips its
  content. Full width of the log.
- **Head**, one row, padding `sp-6` × `sp-10`, spacing `sp-8`, centred
  vertically, the pointer cursor, not selectable:
  1. Status mark: the **spinner** while `status == 'running'`; otherwise a
     `sp-6` round dot, `red` if `isError`, else `green`. (Any other status,
     `done` or `unanswered`, is the green dot.)
  2. Name: the tool's name exactly as sent (`mcp__crundi__list_services`,
     not renamed), `mono`, `text-12`, `w-semibold`, `accent-hover`; `red` when
     `isError`. Never elided.
  3. Summary: `ui-font`, `text-12`, `text-secondary`, one line, elided at
     the end, takes the rest of the row.
  4. Caret: ▾ open, ▸ closed (the web turns one ▾ by -90°), `text-muted`,
     `text-13`.
- **Body**, shown when open: a 1px `border-subtle` line on top; padding
  `sp-8` × `sp-10`; `mono`, `text-12` (web 11.5), `lh-chat`,
  `text-secondary`; the text keeps its newlines and runs of spaces and wraps
  at the pane's edge, breaking inside a word when it must; tabs keep tab
  stops of 8 (expand them in Rust if Slint cannot). Selectable. At most
  `composer-max * 2` tall (web 340), then it scrolls (`ScrollThumb`); the
  arguments and the result scroll together.
  - **Arguments**, by tool (`toolDetailHtml`):
    - Bash: the command; then, if there is a `description`, it on its own
      line in `text-muted`, `sp-6` below.
    - Write: the `content` (first 4000 characters, then "…") as one
      `green` on `green-dim` band.
    - Edit: every line of `old_string` as "- line" in `red` on `red-dim`,
      then every line of `new_string` as "+ line" in `green` on
      `green-dim`; each line its own full-width band, wrapping inside it.
      (An empty string still gives one "- " or "+ " line.)
    - TodoWrite: the to-do list (below).
    - Anything else (Read, Grep, Glob, WebFetch, Task, MCP tools,
      AskUserQuestion…): the input as JSON, pretty-printed with a 2-space
      indent, first 4000 characters then "…". No arguments: `{}`.
  - **Result**, when there is one (not null, not empty): `sp-8` below the
    arguments, a 1px `border-subtle` line, `sp-8` below it the text with
    ANSI escapes removed, first 6000 characters then "…"; `text-muted`, or
    `red` when `isError`.
- **To-do list** (TodoWrite's arguments): one row per to-do, `sp-4` apart,
  a mark and the text side by side (`sp-8`), the text wrapping in its own
  column:
  - completed: a ticked box, the `content`, `text-muted`, struck through;
  - in_progress: ▸, the `activeForm` (the `content` if there is none),
    `accent-hover`;
  - pending: an empty box, the `content`, `text-secondary`.
  The web's marks are the characters ☑ ▸ ☐. Draw them as icons in the
  text's colour, the text's height (`square.svg`, `check.svg` inside it,
  `play.svg`), not emoji. Slint's `Text` has no strike-through: draw a 1px
  line across each line of a completed item, or, if a wrapped item cannot
  be struck, name it in PLAN.md Known gaps.

### The group head (`.cc-tgroup`)

The same box as a card, and the same head row (padding, spacing, pointer):
1. Status: the spinner while any member runs; else a `red` dot if any member
   failed; else `green`.
2. The `groupSummary`: `ui-font`, `text-12`, **`text-primary`**, one line,
   elided, takes the rest.
3. "1 tool" / "N tools": `text-11`, `text-muted`, never elided.
4. Caret ▸ closed, ▾ open, `text-muted`.
Its tooltip: "Show each tool call" when closed, "Fold these tool calls" when
open.

### The agent row (`.cc-agentrow`)

The card's box, one row, spacing `sp-8`:
1. The spinner while the agent runs (`status` `running` or empty), else a
   dot, `red` if `status == 'failed'`, else `green`.
2. `subagentType` (or "Agent"), as the card's name.
3. The agent's `description`, as the card's summary.
4. "transcript ›", `text-11`, `accent-hover`, at the right.
Hover: the edge turns `accent`. Click: opens that agent's panel (2.18; until
2.18 lands, the row is drawn and the click does nothing).

The web gives this row no padding (21 high, text on the edge, see
`web-agent-row-hover-2x`): an accident. The native row uses the card head's
padding. **Named difference.**

### Measurements (web → token)

| Part | Web | Token |
|---|---|---|
| Card / group head height | 34 (32 + edges) | head padding `sp-6` + text line |
| Card radius | 6 | `r-control` |
| Head padding | 6 × 9 | `sp-6` × `sp-10` |
| Head gap | 7 | `sp-8` |
| Dot | 6 | `sp-6`, round |
| Spinner | 12 outer (9 + 1.5 ring) | `icon-sm` |
| Spinner period | 0.7 s, linear | new theme duration |
| Name | mono 12 / 600 / `accent-hover` | `mono`, `text-12`, `w-semibold` |
| Summary | 12 / `text-secondary` | `text-12` |
| Group line | 12 / `text-primary` | `text-12` |
| "N tools", "transcript ›" | 11 / `text-muted`, `accent-hover` | `text-11` |
| Caret | 13 / `text-muted` | `text-13` |
| Body padding | 8 × 9 | `sp-8` × `sp-10` |
| Body text | mono 11.5, line 1.55 | `text-12`, `lh-chat` |
| Body max height | 340 | `composer-max * 2` |
| Result line | margin 7, padding 7 | `sp-8`, `sp-8` |
| Bash description gap | 5 | `sp-6` |
| To-do row gap / mark gap | 2+2 / 7 | `sp-4` / `sp-8` |
| Group head → first member | 4 | `sp-4` |
| Entries | 10 | the log's entry gap, `sp-10` |

Everything that is text scales with the pane's text size (`fs`, the web's
`--cc-fs`, A- / A+); the dot, the spinner, padding and gaps do not.

## Summaries (`toolSummary`)

`shortPath(p)`: backslashes to `/`; when it has more than three `/`-parts,
"…/" plus the last two (`/tmp/x/proj/app.js` → `…/proj/app.js`).
`truncate(s, n)`: the first n characters and "…".

| Tool | Summary | Seen |
|---|---|---|
| Read, Write, Edit | `shortPath(file_path)` | `…/proj/app.js` |
| NotebookEdit | `shortPath(notebook_path)` | `…/notebooks/analysis.ipynb` |
| Bash | `truncate(command, 140)` | `ls -la` |
| Glob | `pattern`, plus ` in ` + `shortPath(path)` if a path | `**/*.md in …/proj/docs` |
| Grep | `pattern`, plus ` (glob)` if a glob | `port (*.js)` |
| WebFetch | `url` | `https://example.com/docs/getting-started` |
| WebSearch | `query` | `slint scrollview max height` |
| Task, Agent | `description`, else `subagent_type`, else nothing | `Count lines in app.js` |
| TodoWrite | `N items` | `4 items` |
| Skill | `skill` | `crundi` |
| anything else | every key as `key=` + its JSON, space-separated, `truncate(…, 140)`; nothing if no keys | `title="Write tests…" column="backlog" …`, `questions=[{"header":"Database",…` |

The summary is one line: the head elides whatever does not fit. A missing
field reads as the web prints it (`undefined` for a Glob or Grep without a
pattern; the native client may print nothing there, it never happens).

Native today differs: it renames MCP tools, takes the first string argument
for unknown tools (so AskUserQuestion shows nothing, F84), shows "Update the
to-do list" for TodoWrite, does not shorten paths and leaves out Grep's glob
and Glob's path.

## Controls and behaviour

- **Card head click**: opens or closes that card. Starts closed, always
  (from history, while running, and a Task card too). No keyboard: the head
  is not a focus stop, as in the web.
- **Group head click**: opens or closes the group: its members' cards show
  or hide; the caret and tooltip flip. Groups start closed. A member card
  inside an open group opens and closes on its own.
- **Agent row click**: the agent's panel (2.18).
- The open / closed state of cards and groups lasts as long as the pane
  shows the chat; it is not saved.
- **Live**:
  - An `entry` frame with `kind:'tool'` adds a card (running, with its
    spinner).
  - A `patch` frame changes it in place: `status`, `result`, `isError`
    when the result arrives; `group`, `groupSummary` when Claude Code sums
    the batch up (the cards fold behind a new head). A member's patch also
    redraws its group head (count, spinner / dot).
  - An `agent` frame for a `toolUseId` turns that call's card into an agent
    row and keeps its spinner / dot current.
  - The web rebuilds the card on every patch, which **closes an open card
    when its result arrives** (checked: open a running card, patch it to
    done, it is closed again). That is an accident of the rebuild; the
    native card keeps its open state by entry id. **Named difference.**
- **Scrolling**: the log keeps following the bottom when a card grows (the
  log's own rule, 2.16). Wheel over an open body scrolls the body until it
  ends, then the log.
- Text in the body can be selected and copied; the head's cannot.

## States

| State | Look |
|---|---|
| Running | spinner; no result line; the body (if opened) has the arguments only |
| Done | green dot; result under a hairline |
| Failed (`isError`) | red dot, red name, red result |
| Unanswered (restored, no result) | green dot, no result line |
| Closed / open | ▸ / ▾, body hidden / shown |
| Long result | body at `composer-max * 2`, scrolls |
| Very long line | wraps, breaking in a word |
| Group closed / open, running / failed / done | see "The group head" |
| Agent row running / done / failed, hover | see "The agent row" |
| Text size ×0.69 … ×1.31 | text scales, marks and spacing do not |
| Narrow pane | summaries elide; name, "N tools", caret keep their width; bodies wrap |

After a restart the server puts back its own stored copy of the chat
(`restoreHistory`, src/claude-ui.js 640-700), which keeps groups and agent
records (a running agent comes back `stopped`, drawn with the green dot).
When Claude's transcript holds more than that copy, it reads the transcript
instead: then there are no groups (the summaries are not in it), no agent
records (a Task / Agent call is a plain card, as `web-task-open`), and calls
without a result are `unanswered` (green dot, no result line). Both come in
through the same `ui-history` frame; the native client draws whatever it is
given.

Out of scope here: what the host puts under a finished call (`toolExtra`:
images, video, files and frames from `show_*`, a widget's shots) is 2.16's
"images and media in replies"; the agent panel and the task panel's open
card are 2.18's; permission and question cards are 2.13 and earlier.

## Data and endpoints

No endpoints of its own. The cards come from the chat's existing stream
(specs/live-state.md, specs/composer.md):

- `ui-history` (on subscribe): `session.messages[]`, where a tool entry is
  ```
  { id, kind:'tool', toolUseId, name, input:{…},
    status:'running'|'done'|'unanswered', result:string|null, isError:bool,
    group?:string, groupSummary?:string }
  ```
  and `session.agents[]` is the agent records
  `{ toolUseId, kind:'agent'|'task', subagentType, description, status,
     dismissed?, … }`.
- `ui-event` with `event.type`:
  - `entry` `{ entry }` – a new call;
  - `patch` `{ id, patch }` – `{status, result, isError}` or
    `{group, groupSummary}`;
  - `agent` `{ agent }` – an agent record, new or changed.

## End-to-end test (phase2.mjs)

Against `tools/test-server.sh` (a real `claude`): a chat created with
`skipPermissions: true` in the seeded project (put an `app.js` in it with a
`const port = 3000;` line), sent:

> This is a UI test. Make exactly these tool calls, one per message, in this
> order: 1. Bash `ls -la` with description "List the project files";
> 2. Read app.js; 3. Edit app.js, replace `const port = 3000;` with
> `const port = 4000;`; 4. Bash `cat missing-file.txt` (it will fail).
> Then reply "Done."

Wait for idle, then check through the test control:
- four tool entries; Claude Code will usually have folded each batch behind
  a group head: open every group first, and check the head's count and dot
  (the batch with the `cat` has a red dot);
- Bash: green dot, name `Bash`, summary `ls -la`; open: the command, the
  description, the listing;
- Read: summary `…/<dir>/app.js`; open: the JSON input and the file;
- Edit: summary `…/<dir>/app.js`; open: "- const port = 3000;" in the del
  colour and "+ const port = 4000;" in the add colour;
- the failing Bash: red dot, red name, summary `cat missing-file.txt`, the
  result in red containing "No such file";
- open a card, inject (or wait for) a patch to it: it stays open.

The tools this CLI does not have (TodoWrite, Grep, Glob, WebFetch) and the
summary rules go in `cargo test`: one case per row of the Summaries table,
with the inputs used here (`web-summaries-per-kind`), plus the JSON body,
the Edit diff lines, the Write block, the 4000 / 6000 cuts and the to-do
rows.

## Checklist

- [x] Card box: `bg-secondary`, `border-subtle` edge, `r-control`.
- [x] Head: status mark, raw name (mono, semibold, `accent-hover`, red on
      error), summary (elided), caret ▸ / ▾; padding `sp-6`×`sp-10`, gap `sp-8`.
- [x] Spinner while running (ring `border`, arc `accent`, turning); dots
      `sp-6` green / red; `unanswered` is green.
- [x] Summaries match the table for every kind, including `shortPath`,
      the 140-character cut, Grep's glob, Glob's path, "N items" and the
      `key=value` fallback (F84: the pending AskUserQuestion row).
- [x] Body: hairline, mono, `lh-chat`, wraps (breaking words), keeps
      spaces and tabs, selectable, max `composer-max * 2`, scrolls with
      `ScrollThumb`.
- [x] Bash body: command + muted description.
- [x] Write: one green band; Edit: "- " red bands, then "+ " green bands.
- [x] Other tools: pretty JSON (`{}` when empty), cut at 4000.
- [x] Result: hairline, muted (red on error), ANSI removed, cut at 6000.
- [x] TodoWrite: icon marks, done struck and muted, active `accent-hover`
      with `activeForm`, pending `text-secondary`; wraps beside the mark.
- [x] Group head: status from its members, `text-primary` line, "N tool(s)",
      caret, tooltips; opens / closes its members; first member `sp-4` below.
- [x] Closed group members take no room.
- [x] Agent row: mark, subagent type, description, "transcript ›",
      accent edge on hover, padded like a head (named difference), never
      folded.
- [x] Live: entry adds a running card; patch updates it and its group
      head; summary patch folds the batch; agent frame turns a card into an
      agent row.
- [x] An open card stays open when its result arrives (named difference).
- [x] Text size scales the text only.
- [x] Narrow pane: summaries elide, counts and carets stay, bodies wrap.
- [x] After a restart: whatever `ui-history` holds draws the same way
      (groups and agent rows from the stored copy; plain cards, `unanswered`
      green dots and a Task card from a transcript).
- [ ] phase2.mjs drives Bash, Read, Edit and a failing command; cargo tests
      cover the summaries and bodies; snapshots of every state above saved
      under `tool-cards/review/`.
