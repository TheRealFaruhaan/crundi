# 2.11 Slash menu

The list of commands that opens above the message box while the whole
message is one `/word`. It filters as you type; ↑ ↓ pick, Enter or Tab (or
a click) puts the command in the box. It is the CLI's own commands (skills,
`/clear`, `/code-review` …) plus Crundi's `/btw`.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below, never a pixel value.

Source, `app/vendor/claude-chat.js` (grep with `-a`):
- CSS 280-284: `.cc-slash`, `.cc-slash-item` (`.on`), `.cc-slash-item b`,
  `.cc-slash-item span`;
- 976 `wrap` (`.cc-wrap`, the row the list hangs over); 1294-1296 the
  state (`slashCommands`, `slashBox`, `slashIdx`);
- 3957-3992 `hideSlash()` / `showSlash()`; 4022-4034 the keys in
  `onKeyDown`; 4121-4125 the input and blur handlers; 2834 `doSend` hides
  it; 5145 closing the chat hides it;
- data: 4936 `applyHistory` (`session.slashCommands`), 5054-5055 the
  `init` event.

Server: `src/claude-ui.js` 1708 (`system/init` → `slash_commands`, names
only) and 2555 (the initialize control reply → `commands`, objects with a
description), both emitted as the `init` ui-event and kept on the session.

## Screenshots

`slash-menu/web-<state>.png` (1400×860; a wide pane on the left, a second
pane on the right) and `-2x.png` crops of the list.

| State | What it shows |
|---|---|
| `web-open-desc` | `/` typed in a chat whose CLI has not had a turn yet: the list over the composer, the first item selected (accent-dim), each item the name (mono, semibold, accent-hover) then its description (muted, one line, elided "…"). A scrollbar: 40 items, about 6½ visible. |
| `web-arrow-down2` | ↓ twice: the third item (`/crundi`) selected. |
| `web-arrow-offscreen` | ↓ eight more times: the 11th item is selected but **the list does not scroll to it** (web bug, see Behaviour). |
| `web-scrolled` | The list scrolled with the wheel: items 11+ in view; the selected one is the 11th (`/marketing`) at the top. |
| `web-hover` | The pointer over `/motion-reels`: **nothing changes** (no hover colour; the selection stays put). |
| `web-chosen-click` | `/motion-reels` clicked: the box reads `/motion-reels ` (a trailing space), caret at the end, the box keeps focus, the list is gone. |
| `web-filtered` | `/eng` typed: only names starting with `eng` (the five `engineering:*` visible, more under the scroll). |
| `web-filtered-few` | `/engineering:t`: two matches; the list shrinks to fit them (no empty space). |
| `web-chosen-enter` | From two matches, ↓ ↓ (clamps at the last) then Enter: the box reads `/engineering:testing-strategy `. Nothing was sent. |
| `web-btw` | `/b`: the CLI's `b…` commands, then Crundi's `/btw` last ("Ask on the side: answered from the conversation, not added to it"). |
| `web-no-match` | `/bzz`: no list at all (there is no "no match" line). |
| `web-names-only` | `/` in a chat that has had a turn: the CLI's `system/init` gave names only, so the items are the names alone. |
| `web-narrow` | A pane ~340 wide (the composer stacked): the list spans just the message box (attach and send are on the row below). |

Not screenshotted: the list going away when the box loses focus, Esc
(it only removes the list, see `web-no-match` for the look).

## The parts

```
composer
├── list (only while open; hangs over the composer, above the box's row)
│   ├── item  [/name   description …………………………………… elided]   ← selected: accent-dim
│   ├── item  [/name   description …]
│   └── …     (scrolls; at most slash-max-h tall)
└── the box's row  (wide: box + attach + send; narrow: the box alone)
```

- **Anchor.** The list's bottom edge sits sp-4 above the top of the row
  that holds the message box (`.cc-wrap`): in a wide pane that row is the
  box, attach and Send, so the list is that wide; in a narrow pane
  (2.5's stacked composer) it is the box alone. Left and right edges match
  that row. It is drawn above the log and over anything under it (the
  queue drawer, a recap card), but under modal dialogs.
- **List:** background `bg-secondary`; 1px `border`; radius `r-control`;
  `SoftShadow` with `shadow-menu`; no padding. Height = its items, up to
  `slash-max-h` (new named size; the web's is 190, about 6½ one-line
  items), then it scrolls (`RevealFlickable` + `ScrollThumb`).
- **Item:** a row, padding sp-4 top and bottom (web 5) and sp-10 left and
  right, gap sp-8 between name and description; font `text-12`, line
  height `lh-ui`.
  - **Name:** `/` + the name; `mono`, `w-semibold`, `accent-hover`. Never
    wraps or shrinks (the web wraps long names at a hyphen, e.g.
    `/baby-` / `games`, because the flex item shrinks; that is an accident
    of the CSS, not copied).
  - **Description:** `text-muted`, one line, elided at the right edge with
    "…". Absent when the command is a bare string (names only).
  - **Selected** (`.on`): background `accent-dim`. Nothing else changes.
  - **Hover:** nothing changes but the pointer (hand cursor). The
    selection does not follow the mouse.
- Matching the number of the web's items: at most **40** shown.

## Behaviour

### When it opens and what it lists
- On every edit of the message box (typing, paste, delete; not on a
  programmatic change such as a draft restored or a recalled message):
  1. the selection goes back to the first item;
  2. if the **whole** text matches `^/([a-z0-9_:-]*)$` (case-insensitive:
     a slash at the very start, then only letters, digits, `_`, `:`, `-`,
     no space, no newline) **and** the chat's command list is not empty,
     the list shows; otherwise it closes.
- **Filter:** the commands whose name starts with the typed part,
  case-insensitively (prefix, not substring). In the order the server
  gave them. The first 40.
- **/btw:** if the CLI's list has no `btw`, Crundi's is added at the end:
  name `btw`, description "Ask on the side: answered from the
  conversation, not added to it". (What `/btw …` does when sent is 2.21.)
- **No match:** the list closes. There is no empty state.
- A command is a string (its name) or `{name, description, argumentHint}`;
  `argumentHint` is not shown.

### Keys (only while the list is open; they go to the list first)
- **↓ / ↑:** move the selection one item, stopping at the last / first (no
  wrap-around). They do not move the caret, and ↑ does **not** recall a
  queued message (2.9's ↑ is pre-empted while the list is open).
- **Enter** (without Shift) **or Tab:** choose the selected item. Enter
  does not send, whatever the chat's Enter mode (2.5) is.
- **Shift+Enter:** not handled by the list; it inserts a newline as usual,
  which then closes the list (the text no longer matches).
- **Esc:** closes the list; the text stays. It comes back at the next
  edit. Esc does nothing else while the list is open (native: the key is
  consumed).
- Any other key edits the text as usual, which re-filters.

### Choosing (keyboard or mouse)
- The box's whole text becomes `/` + name + one space, the caret at the
  end; the list closes; the box keeps (or gets back) the focus.
- **Mouse:** a press on an item chooses it at once (the web acts on
  mousedown and keeps the focus in the box).
- The draft: the web does not save the draft after a choice (no input
  event), so a restart before the next keystroke restores the partial
  `/eng…`. **Native saves the draft on a choice** (named difference: drafts
  survive restarts, 2.8).

### Closing
- No match, Esc, a choice, a send (the box empties), the chat closing.
- **The box losing focus** closes it after a short delay (`motion-fast`;
  the web's is 120 ms) so that a click on an item still lands.

### Scrolling
- The wheel and the thumb scroll the list. Its scroll position is kept
  while it is re-filtered by typing or moved with ↑ ↓ (the web rebuilds the
  rows in the same box) and starts at the top each time it opens.
- **The web never scrolls to the selection** (`web-arrow-offscreen`: the
  11th item is chosen out of sight). **Native keeps the selected item in
  view** when ↑ ↓ move it (named difference; the list is otherwise
  unusable past the 7th item by keyboard).

### Live
- The command list is the chat's, from two places:
  - the history snapshot (`ui-history` → `session.slashCommands`) when the
    chat is opened or reconnects;
  - the `init` ui-event (`slashCommands`), which **replaces** it: once when
    the CLI starts (objects with descriptions) and again on its first turn
    (`system/init`, names only). So the descriptions disappear after a
    chat's first turn (`web-open-desc` vs `web-names-only`). Native shows
    whatever the server sends; it does not merge the two.
- An open list is not redrawn when the commands change; the new list is
  used at the next edit. (Native may match this; it is not a finding
  either way.)
- No endpoints: the menu calls nothing.

## Data

| Source | Shape |
|---|---|
| `ui-history` (WebSocket, after `subscribe-ui`) | `{session: {slashCommands: [string \| {name, description?, argumentHint?}], …}}` |
| `ui-event` `init` | `{type: 'init', slashCommands: [same], sessionId?, model?, permissionMode?, …}` |

## Native notes
- The list belongs to the composer (2.5's `InputBox`), above the box's
  row; it is not a modal (`Pointer.modal` stays false) and other panes stay
  live.
- One list per chat; each pane has its own (two panes can show theirs at
  once only if both boxes had focus, which they cannot: the blur closes the
  first).
- `slash-max-h` is the one new token.
- The filter and the choice are logic: put them in Rust with unit tests
  (match rule, prefix filter, /btw added once, 40 cap, clamp ↑ ↓, chosen
  text `"/name "`).

## End-to-end test (test/phase2.mjs)
- Type `/` in a seeded chat's box (via the test control): the list is
  open with items; type more (e.g. `/cl`) and check every item starts with
  `/cl`.
- ↓ once, then Enter: the box reads `/` + the second item's name + a
  space; nothing was sent (no new user entry; the chat's state unchanged);
  the list is closed.
- `/zzzz`: no list. Esc on an open list closes it and keeps the text.
- The draft after a choice equals the chosen text.

## Checklist
- [ ] Opens only while the whole text is `/` + `[a-z0-9_:-]*`; closes on a
      space, a newline, or no match; no empty state.
- [ ] Prefix filter, case-insensitive, server order, at most 40.
- [ ] `/btw` added last when the CLI has none, with its description.
- [ ] Item: mono semibold accent-hover name (never wrapped), muted
      description elided on one line; names-only items show the name alone.
- [ ] Selected item accent-dim; hover changes nothing; the first item is
      selected after every edit.
- [ ] ↑ ↓ clamp; the selection stays in view (named difference).
- [ ] Enter (not Shift+Enter) and Tab choose; Enter never sends; ↑ never
      recalls while open.
- [ ] Esc closes the list only and keeps the text.
- [ ] A press on an item chooses it; focus stays in the box.
- [ ] Choice: text `/name ` with the caret at the end; the draft is saved.
- [ ] Losing focus closes it (after motion-fast); sending closes it.
- [ ] List over the box's row (wide: box + attach + send; narrow: box
      only), sp-4 above, bg-secondary, border, r-control, shadow-menu, max
      `slash-max-h`, scrolls with a thumb.
- [ ] Commands from `ui-history` and replaced by `init`.
- [ ] Tokens only; side-by-side crops of each state saved under
      `slash-menu/review/`.
