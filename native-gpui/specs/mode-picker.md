# 2.6 Permission-mode picker

The dialog the composer's mode button opens ("Permission mode"), the list
of modes in it, the mode button's look per mode, and how a chat's mode
changes: from the picker, on an idle or a running chat, live from the
server or another client, and when a change fails.

The button's place and size in the meta row (wide: icon + name; narrow:
icon only, finger-sized) are 2.5's (`specs/composer.md`); this spec adds
its colour per mode, its hover, and what it opens.

Rule 2 of DESIGN.md applies: every size and colour is a theme token
(`ui/theme.slint`), named below, never a pixel value. New tokens this
screen needs are listed under "Tokens".

Source, `app/vendor/claude-chat.js`:
- CSS 193-226: `.cc-mode` (the button, its colours per mode, narrow),
  `.cc-modal-back`, `.cc-modal`, `.cc-modal-h`, `.cc-modal-x`,
  `.cc-mode-opt` and its parts;
- 1064-1160: `MODES`, `MODE_INFO` (icons, names, descriptions),
  `syncModeBtn()`, `closeModeModal()`, `modeModalKey()`, `openModeModal()`,
  `buildModes(isBypass)`, `setModeIfKnown()`; the hidden `.cc-mode-store`
  `<select>` is the value everything reads;
- 4128-4135: the select's `change` → `POST …/permission-mode`;
- 4937-4939: the snapshot (`applyHistory`): `buildModes(session.skipPermissions)`,
  the mode from `session.permissionMode`;
- 5057 (`init`) and 5062-5067 (`meta`): live modes;
- 5117: `destroy()` closes an open picker.

Host, `src/webapp-html.js`: `planTagFor()` / `planTagHtml()` 8768-8783
(the header's "planning" tag), `.term-plan-tag` CSS 1270-1280,
`applyChatMeta()` 9175.

Server: `src/webapp.js` 6558-6562 (the route, then `broadcastState()`);
`src/claude-ui.js` `setPermissionMode()` 3042-3059, the CLI's own mode
changes 1722-1733 (`status`), a rejected change rolled back 2526-2538.

## Screenshots

`specs/mode-picker/`, from the web UI at 1400×860 (`-2x`: the region ×2).

| File | State |
|---|---|
| `web-buttons-rest` | the buttons at rest, wide: Ask permissions (top), Bypass all (a chat launched with skip permissions, red), Ask permissions |
| `web-buttons-modes` | Accept edits (accent-hover), Plan mode (sky, tinted border) with the header's PLANNING tag, Don't ask (yellow) |
| `web-hover-button-auto` | the button hovered (Auto): accent border |
| `web-open-default` | the picker open on a normal chat: five modes, Ask permissions selected |
| `web-open-bypass` | the picker open on a chat launched with skip permissions: Bypass all first and selected, then the five |
| `web-hover-option` | an option hovered (Auto): bg-tertiary |
| `web-hover-close` | × hovered |
| `web-narrow-buttons` | three narrow panes: the button as an icon only (Auto, Plan mode, Don't ask) |
| `web-narrow-open` | the picker opened from a narrow pane: the same dialog, centred in the window |
| `web-narrow-head-planning` | a narrow pane's header with "…" open: PLANNING sits in the tray before CHAT |
| `web-header-planning` | a wide header with PLANNING before CHAT |
| `web-short-window` | a window 420 high: the dialog stops `sp-16` short of the edges and scrolls, its header with it |
| `web-running-open` | the picker open on a running chat (working…, Stop shown) |
| `web-running-changed` | that running chat after choosing Bypass all: the button red at once, the turn carries on |
| `web-live-change` | the right pane's mode changed by another client (POST from the API): its button turned to Plan mode with no click |
| `web-post-fails` | the POST failed (stubbed 404): the web still shows Accept edits while the server kept Don't ask (a web accident, see Errors) |

The native client's: `native-<state>.png` (and `-2x` crops):
open-default, open-bypass, narrow-open, header-planning,
narrow-head-planning, button-accept, refused; the suite's own under
`DRIVE_OUT` (`phase2-2.6-*`).

## The parts and their order

```
backdrop        the whole window, scrim, the dialog centred, sp-16 clear on every side
  dialog        "Permission mode" (role dialog), dialog-w wide (never wider than the
                window less 2×sp-16), as tall as its content up to
                min(dialog-max-h, window height − 2×sp-16), then it scrolls (the
                header scrolls with the list); bg-secondary, 1px border, r-card,
                shadow-card, padding sp-6
    header      one row, padding sp-8 sp-8 sp-6, gap sp-8
      label     "PERMISSION MODE": text-11, w-bold, uppercase, track-wide, text-secondary; stretches
      ×         close: text-secondary, padding sp-4 × sp-8, r-control
    option ×N   one per mode, full width, stacked with no gap
      tile      sp-32 square, r-field, bg-primary, 1px border; the mode's icon,
                icon-lg, in the mode's colour
      text      stretches; name over description, sp-2 apart
        name    text-13, w-semibold, text-primary
        desc    text-12, text-secondary, line height lh-chat, wraps (two lines at dialog-w)
```

Option box: padding `sp-10`, gap `sp-12` between tile and text, at least
`sp-48 + sp-4` high (two-line descriptions make it taller), `r-field`,
a 1px border that is transparent unless selected; tile and text centred
vertically.

The web also puts a check mark at the right of the selected option
(`.ck`), but its SVG has no size and collapses to nothing: nobody sees it.
The native picker has **no check mark**; the selection is the highlighted
row.

### The modes, in order

A chat launched with skip permissions (`skipPermissions: true`) gets Bypass
all first; every other chat starts at Ask permissions.

| Key | Icon (`ui/icons/`) | Name | Description | Colour |
|---|---|---|---|---|
| `bypassPermissions` (skip-permissions chats only) | `mode-bypass-permissions` | Bypass all | Runs everything without asking. Only for a chat launched with skip permissions. | red |
| `default` | `mode-default` | Ask permissions | Asks you before editing files or running commands. | text-secondary |
| `acceptEdits` | `mode-accept-edits` | Accept edits | Edits files without asking. Still asks before running commands. | accent-hover |
| `auto` | `mode-auto` | Auto | Decides for itself what is safe to do, and asks about the rest. | accent-hover |
| `plan` | `mode-plan` | Plan mode | Reads and plans only. Changes nothing until you approve the plan. | sky |
| `dontAsk` | `mode-dont-ask` | Don't ask | Never asks. Anything not already allowed is refused. | yellow |

Wording is exact, including the full stops.

### Option looks

| State | Background | Border |
|---|---|---|
| rest | none | transparent |
| hover | bg-tertiary | transparent |
| selected (the chat's mode) | accent-dim | accent-line |
| selected and hovered | accent-dim (no change) | accent-line |

The tile's icon keeps the mode's colour in every state.

### The mode button's look per mode (meta row, 2.5's control)

Icon and name in the mode's colour (table above); background bg-primary,
1px border, r-control.

| Mode | Icon + name | Border | Hover |
|---|---|---|---|
| Ask permissions | text-secondary | border | border accent, ink text-primary |
| Accept edits, Auto | accent-hover | border | border accent |
| Don't ask | yellow | border | border accent |
| Plan mode | sky | sky-edge | none (keeps its sky border) |
| Bypass all | red | error-edge | none (keeps its red border) |

Narrow pane (2.5): the icon alone, in the same colour, the same border.
Tooltip on the button: "Permission mode: <name>. Click to change."

The native ModeBtn today colours only Bypass all; every row above must
hold.

### The header's PLANNING tag

While a chat's mode is Plan mode (and it has no plan of its own yet), its
pane header shows a pill "PLANNING" just before the CHAT tag: text-10,
w-bold, uppercase, track-wide, sky text and a 1px sky border, r-pill,
padding sp-2 × sp-6; not clickable; tooltip "Plan mode — Claude is
working one out". In a narrow header it sits in the "…" tray, before
CHAT (`web-narrow-head-planning`). It appears and goes with the mode, live.
The tag's other states (`plan` proposed / executing / rejected, which open
the plan) belong to 2.19; they take precedence over "planning".

## Behaviour

### Opening
- A click on the mode button opens the picker, built from the chat's list
  of modes (with or without Bypass all) with the current mode selected.
- The dialog is centred in the **window**, not the pane, over a backdrop
  that covers everything (sidebar, tabs, other panes). It looks the same
  from a wide or a narrow pane.
- The selected option has the keyboard focus when it opens.
- One picker at a time. Closing the chat's pane (or the chat going away)
  closes its picker.
- The button works in every state: idle, working, needs input, exited.

### Closing
- ×, a click on the backdrop (outside the dialog; a click inside it does
  nothing), or Esc. Esc is swallowed: it closes the picker and nothing
  else (a full-screen pane behind it stays full screen).
- Closing changes nothing. Focus isn't put back anywhere in particular.

### Keyboard
- Esc closes.
- Tab / Shift+Tab move between × and the options; Enter or Space chooses
  the focused option. (In the web focus can Tab out behind the backdrop;
  the native picker keeps it inside the dialog.)

### Choosing
- A click on an option closes the picker.
- The same mode as now: nothing else happens (no request).
- Another mode: the button shows it at once (icon, name, colour, tooltip),
  and `POST /api/ui-sessions/:id/permission-mode {mode}` goes to the
  server. No message appears in the log on success.
- On a **running** chat (working or needing input) it is allowed and
  acts the same: the turn carries on, the CLI gets the new mode for what it
  does next, and an open permission card stays open
  (`web-running-open`, `web-running-changed`).
- Plan mode chosen: the header's PLANNING tag appears; leaving Plan mode
  removes it.
- The server answers at once and also tells every client (below), so a
  second window or the web on the same chat follows.

### Live from the server
The mode can change without a click here; the button (and the PLANNING
tag) follow, with no reload:
- another client chose a mode (`web-live-change`);
- the CLI changed it itself, e.g. approving a plan drops out of Plan mode;
- the CLI rejected a change: the server puts the previous mode back and
  adds an error line to the log, "Could not switch permission mode:
  <error>" (2.16 draws error lines).

Events, on the chat's own subscription (`subscribe-ui`, see Data):
- `{type:"meta", permissionMode}`: the mode now;
- `{type:"init", permissionMode, …}`: the mode the CLI started in.

A mode not in this chat's list (e.g. `bypassPermissions` reported for a
chat not launched with it) is ignored by events: the button keeps what it
showed. The snapshot is taken as it comes: a mode outside the list shows
as Ask permissions.

### Errors
The web ignores the answer completely (`.catch(function(){})`, and it never
reads `ok`): a failed or refused change leaves the button showing a mode
that is not in effect (`web-post-fails`). That is an accident. Native,
a **deliberate difference** to name in PLAN.md:
- the request fails, or the server answers `{ok:false, error}`: the button
  goes back to the mode it had before, and an error toast says
  "Could not switch permission mode: <error>" (the server's text, or the
  failure's);
- the server has no such endpoint (404, an older server): the button goes
  back and the error toast says "Not supported by this server"
  (`ApiError::Status(404)` already says this).
Examples of `error`: `No session "<id>"` (the chat has exited),
`Invalid permission mode "<m>"`, and the Bypass gate below.

## Data and endpoints

| What | Where |
|---|---|
| the chat's mode and list | the snapshot (`GET /api/ui-sessions/:id/history`, or the `ui-history` reply to `subscribe-ui`): `permissionMode` (`default` when empty) and `skipPermissions` (true: Bypass all is in the list, first) |
| live mode | `/ws` events for the chat: `{type:"meta", permissionMode}`, `{type:"init", permissionMode}` |
| header tag | `/ws` `state` broadcast: `terminals[].permissionMode` (with `planStatus`); the server broadcasts after every change |
| change | `POST /api/ui-sessions/:id/permission-mode`, body `{"mode": "<key>"}` |

The POST's answers (always HTTP 200 from a current server):
- `{ok:true, permissionMode:"<key>"}`: done; a `meta` event follows to
  every subscriber (this client too);
- `{ok:false, error:"No session \"<id>\""}`: the chat isn't running;
- `{ok:false, error:"Bypass permissions can only be chosen when the chat is launched — start a new \"UI Mode — Skip Permissions\" chat."}`:
  `bypassPermissions` on a chat not launched with it (the picker never
  offers it there; only an API call can get this);
- `{ok:false, error:"Invalid permission mode \"<m>\""}`.
Valid keys: `default`, `acceptEdits`, `auto`, `plan`, `dontAsk`, and
`bypassPermissions` for skip-permissions chats.

The server sends `set_permission_mode` to the CLI and sets the mode at
once; if the CLI later answers with an error, the server rolls back
(`meta` with the old mode) and adds the error line to the log.

## Tokens

New in `ui/theme.slint`:
- `dialog-w` (380): the width of a small modal dialog (named size;
  2.12 and 2.22 can share it);
- `dialog-max-h` (560): a small dialog's tallest;
- `scrim` (#0000008c, black at 55%): a modal's backdrop;
- `sky-edge` (#38bdf873, sky at 45%): Plan mode's button border.

Existing: bg-primary, bg-secondary, bg-tertiary, border, text-primary,
text-secondary, accent, accent-hover, accent-dim, accent-line, sky, yellow,
red, error-edge, sp-2/4/6/8/10/12/16/32/48, r-control, r-field, r-card,
r-pill, text-10/11/12/13, w-semibold, w-bold, track-wide, lh-chat,
icon-lg, shadow-card.

## How to bring each state about (for the end-to-end test)

- Server: `tools/test-server.sh <port>` (`--bare` for the suites). A chat
  with Bypass all: `POST /api/ui-sessions/create {project, skipPermissions:true}`.
- Running: send it a long shell wait ("Run exactly this shell command in
  the foreground…: timeout 300 tail -f /dev/null"); stop it after with
  `POST …/interrupt`.
- Live change: `POST /api/ui-sessions/:id/permission-mode {mode}` from the
  test with the server's own token.
- Refused: an exited chat (`POST …/close`, or end its process) answers
  `{ok:false, error:"No session …"}`.

End-to-end test (test/phase2.mjs):
1. On a running chat (working), open the picker from the mode button,
   check the list (five modes; six with Bypass all first on a
   skip-permissions chat) and that the current one is selected.
2. Choose another mode: the picker closes, the button shows it at once,
   and `GET /api/ui-sessions` reports that `permissionMode` for the chat.
3. Choose Plan mode: the pane's header shows PLANNING.
4. Change the mode from the test (`POST …/permission-mode`): the button
   follows without a click (and PLANNING goes when leaving plan).
5. Esc, ×, and a backdrop click each close it without a request.
6. On an exited chat: the change is refused, the button returns to the old
   mode and the toast says "Could not switch permission mode: No session …".

## Checklist

Button
- [x] Each mode's icon and name, in its colour (Ask permissions
      text-secondary, Accept edits and Auto accent-hover, Plan mode sky with
      a sky-edge border, Don't ask yellow, Bypass all red with an
      error-edge border); narrow: the icon alone in the same colour.
- [x] Hover: accent border (Ask permissions also text-primary); Plan mode
      and Bypass all keep their own border.
- [x] Tooltip "Permission mode: <name>. Click to change."

Picker
- [x] Opens from the button in a wide and a narrow pane; centred in the
      window over a scrim covering everything; dialog-w wide; scrolls
      (header included) in a short window, sp-16 from the edges.
- [x] Header "PERMISSION MODE" and ×; × hover text-primary on bg-tertiary.
- [x] Options in order, each with tile (mode-coloured icon), name and the
      exact description; Bypass all first and only on skip-permissions
      chats.
- [x] Selected: accent-dim with accent-line border; hover bg-tertiary;
      hovering the selected changes nothing; no check mark.
- [x] The selected option has focus on open; Tab stays in the dialog;
      Enter/Space choose.
- [x] Closes on ×, backdrop, Esc (Esc does nothing else); a click inside
      the dialog doesn't close it.

Change
- [x] The same mode: closes, no request.
- [x] Another mode: closes, the button changes at once, POST with
      `{mode}`; the server has it.
- [x] Works the same on a running chat; the turn and any permission card
      carry on.
- [x] Plan mode shows the header's PLANNING tag (wide: before CHAT;
      narrow: in the "…" tray before CHAT); leaving it removes the tag.

Live
- [x] `meta` / `init` with `permissionMode` update the button and the tag
      with no click, from another client or the CLI; a mode outside the
      chat's list is ignored.
- [x] The snapshot sets the list (skipPermissions) and the mode on load.

Errors (named difference in PLAN.md)
- [x] `{ok:false}` or a failed request: the button returns to the previous
      mode; error toast "Could not switch permission mode: <error>".
- [x] 404: the button returns; error toast "Not supported by this server".

Test
- [x] The end-to-end steps above in test/phase2.mjs; phase1, drive and
      cargo test still pass.
