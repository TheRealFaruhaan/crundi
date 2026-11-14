# 2.23 Parked panes

A **parked** pane is a chat whose process Crundi stopped while keeping its
place: the pane stays in the layout, same id, same slot, showing what it was
(blurred) under a card with **Resume**. It comes back in place, live, when
resumed. Chats are parked:
- **manually** (`POST /api/panes/:id/park`, also Cru's `parkChat`): "Closed";
- **when idle**: nothing going on in it and nobody at its project for the
  auto-park time (default 60 min, at least 10; 0 = off) — "Closed while idle";
- **on a server restart**: every live pane comes back parked — "Closed when
  Crundi restarted".

Phase 2 covers chat panes (`paneKind: "ui"`). Parked terminals and shells use
the same card (their wording is listed for completeness) but belong with
terminals.

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.

Source:
- `src/webapp-html.js`
  - CSS 1373-1451 (`.parked-cell`, `.pk-view`, `.pk-veil`, `.pk-card`,
    `.pk-go`, `.pk-resume`, `.pk-peek-btn`, `.pk-unpeek`, `.pk-why`,
    `.pk-what`, `.pk-warn*`, `.pk-more`, `.pk-alt`, the two container
    queries).
  - Grid: parked entries become cells of type `parked` 8052, a cell
    switching between parked and live is rebuilt 8080, `mountParked` 8105.
  - The cell 8315-8336; `headHtmlParked` 8787; `mountParked` (the preview,
    Peek's fetch) 8801-8844; `parkedChoicesHtml` 8853; `resumePane` 8871.
  - Clicks 16369-16371 (`pane-resume`, `pane-peek`, `pane-unpeek`);
    `closeTerminal` 7802 (a parked pane's × → discard).
- `src/panes.js`: `park` 133, `resume` 161 (modes), `listParked` 204,
  `weightOf` 218 (heavy), `preview` 234, `tick` 252 (idle parking), `boot`
  122 (restart).
- `src/webapp.js` 6307-6330 (`/api/panes/:id/(resume|discard|park|preview)`),
  1237 (`HEAVY_TOKENS` by size), the state's `terminals` 2131.

Native today: parked entries are skipped (`app.rs` 960, project dots). This
screen makes them panes.

## Screenshots

`parked-panes/web-<state>.png`, 1400×860; `-2x` is the card (or the named
region) cropped and doubled. Chats parked with the real endpoint and a real
server restart; the heavy one resumes a seeded 138.2k-token transcript. Busy
and failure states with a fetch stub.

| State | What it shows |
|---|---|
| `web-parked` (+`-2x`) | **A parked chat** (manual): the log blurred and darkened behind; the card centred: **Resume** (accent pill, play icon, glow) + **Peek** (pill, eye icon); "Closed · just now"; "Resume picks up this conversation."; the quiet underlined **New session** link. Header: grey dot, title, `CHAT` tag, pin, ×. |
| `web-resume-hover` (+`-2x`) | Resume hovered: `accent-hover` fill. |
| `web-quiet-hover` (+`-2x`) | New session (quiet) hovered: text goes `text-primary`. |
| `web-peek` (+`-2x`) | **Peek:** the conversation sharp and scrollable, the composer showing "Session ended." and the state "exited"; the floating **Parked view** pill (eye-off) top right. |
| `web-resuming` (+`-2x`) | **Resuming…:** the pressed button's label changes, every button on the card disabled (dimmed). |
| `web-resume-failed` | **Failed:** error toast with the server's message; the card back to normal. |
| `web-resumed-live` (+`-2x`) | **Resumed:** the same pane, same slot, now a live chat ("Starting Claude…"), green dot, full header. |
| `web-heavy-and-no-session` | **Heavy** (middle, tall pane) and **no conversation** (right: Resume only, "Resume starts a new conversation.", no Peek, no choices). |
| `web-heavy-card` (+`-2x`) | **The heavy card:** the warning box (shield icon, **138.2k tokens** in yellow, "— continuing loads all of it into every turn. Compacting loads it once, then summarises it so later turns stay small."), then **Compact and resume** and **New session** as two pills (not quiet). |
| `web-compacting` (+`-2x`) | Compact and resume pressed: "Compacting…". |
| `web-starting` (+`-2x`) | New session pressed: "Starting…". |
| `web-short` (+`-2x`) | **Short pane** (body ≤ 330): tighter card, no "what" line, the warning as a one-line pill "**138.2k tokens**, a long conversation". |
| `web-very-short` (+`-2x`) | **Very short** (≤ 210): one wrapping row: Resume (smaller, no glow), Peek, the warning pill with just "**138.2k tokens**"; no "why" line; the choices on the next row. |
| `web-sliver` (+`-2x`) | A sliver (body 76): the card is taller than the body and **scrolls**; its top stays reachable. |
| `web-after-restart-preview-loading` | Right after a restart: every pane parked, "Closed when Crundi restarted · just now"; the backgrounds still empty while the previews load. |
| `web-after-restart` | The same with the previews in. |

## Parts

### Header (`headHtmlParked`)

The pane header's frame, only what still applies: drag handle, status dot
(`text-muted` grey, the "exited" dot, tooltip "Closed"), title (tooltip the
full title), kind tag (`chat` / `terminal` / `shell`), spacer, **pin**, **×**
("Close this pane for good"). No text-size, mode, plan, fullscreen or "…"
buttons. The head-hover split buttons stay (moving-panes). Drag and pin work
as for any pane.

### Body

| Part | Web | Native |
|---|---|---|
| Background `.pk-view` | the chat view with the stored transcript (or the terminal's last screen), `blur(3px) saturate(.7) brightness(.55)`, no pointer, no selection | the same read-only chat log, under a `Theme.scrim` overlay at reduced opacity. **Slint gap:** no blur filter in the software renderer (PLAN "Known gaps" already lists blur) — darken, no blur |
| Veil `.pk-veil` | full body, radial gradient `rgba(10,10,15,.55)` centre → `.15` at 70%; scrolls when the card is taller | `@radial-gradient` from `scrim` to transparent-ish; a `Flickable` so a sliver still reaches the card |
| Card `.pk-card` | column, centred, gap 8, padding 16, max width min(100%, 330) | column, gap `sp-8`, padding `sp-16`, max width a named size (330 → **`park-card-w`**, new) |
| Top row `.pk-go` | Resume + Peek, gap 8, centred, wraps | row, gap `sp-8` |
| **Resume** | pill, padding 10/22, `accent` fill + border, white, 14.4 px w600, play icon 14, gap 8, glow `0 0 24px accent/.35` | `r-pill`, height `control-lg`, padding `sp-24`, `accent`, `white`, `text-14` `w-semibold`, `icon-sm` play, `accent-glow` `SoftShadow` |
| Resume hover / disabled / focus | `accent-hover` / opacity .7 / `--ring` | `accent-hover` / `o-dim` / `accent-ring` |
| **Peek** `.pk-alt.pk-peek-btn` | pill, min height 30, padding 5/12, `bg-tertiary`, 1px `border`, 12.2 px w600, eye icon 14, gap 6; tooltip "Read the conversation without resuming it" | `r-pill`, `control-sm`, `bg-tertiary`, `border`, `text-12` `w-semibold`, `icon-sm` eye |
| `.pk-alt` hover / disabled | border `accent` / opacity .6 | same / `o-dim` |
| Why `.pk-why` | 12.5 px `text-primary` | `text-12` `text-primary` |
| What `.pk-what` | 11.5 px `text-secondary` | `text-11` `text-secondary` |
| Warning `.pk-warn` | row, gap 7, padding 8/10, radius 6, `yellow-dim` fill, 1px `rgba(245,158,11,.35)` border, 11.5 px, line-height 1.4; icon `shield-alert` 13 `yellow`; tokens `<b>` yellow, tabular | `yellow-dim`, border a yellow edge (`yellow` at `o-soft`, or a new `yellow-edge` beside `sky-edge`), `r-control`, padding `sp-8`/`sp-10`, gap `sp-6`, `text-11` `lh-ui`; `Icons.shield-alert` `icon-sm` `yellow`; tokens `w-bold` `yellow` |
| Choices `.pk-more` | row, wrap, gap 6, centred | row, gap `sp-6` |
| Compact and resume / New session (heavy) | `.pk-alt` pills as Peek | as Peek |
| New session (light) `.pk-alt.quiet` | transparent, no border, `text-secondary`, w500, underlined (offset 3); hover `text-primary` | a link: `text-secondary` `w-medium`, underline; hover `text-primary` |
| Parked view `.pk-unpeek` (Peek only) | absolute top 10 / right 12 of the body, padding 7/12, radius 16, 1px `accent`, `bg-card`, 12.5 px w600, eye-off icon 14, gap 6, shadow `0 6px 18px rgba(0,0,0,.45)`; hover `accent-dim`; tooltip "Back to the parked view" | top `sp-10` right `sp-12`, `r-pill`, `accent` border, `bg-card`, `text-12` `w-semibold`, `Icons.eye-off`, `SoftShadow`; hover `accent-dim` |

### Texts

- Why: `Closed while idle` (idle) / `Closed` (manual) / `Closed when Crundi
  restarted` (restart), then ` · ` + `relTime(parkedAt)` (see resume-picker.md).
- What (chat): `Resume picks up this conversation.` (has a conversation) /
  `Resume starts a new conversation.` (none). Terminal: `Resume opens a new
  shell in the same folder.` (shell) / `Resume picks up this conversation.` /
  `Resume starts a new Claude session.`
- Warning (only when `heavy`): **`<tokens/1000 .toFixed(1)>k tokens`** then
  the long form ` — continuing loads all of it into every turn. Compacting
  loads it once, then summarises it so later turns stay small.` (terminal:
  ` — continuing loads all of it into every turn.`), or the short form
  `, a long conversation`. Tooltip: the long text.
- Choices (only when the pane has a conversation and is not a shell):
  heavy chat → `Compact and resume`, `New session`; otherwise the quiet
  `New session` (tooltip "Start a fresh conversation in this pane. The old
  one stays, and can be resumed from the session list.").
- Peek appears only for a chat with a conversation.
- Busy labels: `Resuming…` (resume), `Compacting…` (compact), `Starting…` (new).

### Size variants (the veil's height)

| Body height | Card |
|---|---|
| > 330 | as above: column, everything, long warning |
| ≤ 330 | gap `sp-6`, padding `sp-10`/`sp-12`; **no What**; the warning a pill (`r-pill`, padding `sp-4`/`sp-10`, centred) with the **short** form |
| ≤ 210 | one wrapping **row**, centred, gap `sp-6`, padding `sp-8`, full width; **no Why**; Resume smaller (`control`, `text-13`, no glow); the warning pill with the tokens only (neither form); choices follow on the wrap |
| card taller than body | the veil scrolls; the card centres when it fits and starts at the top when not (`margin: auto`) |

Width: the card shrinks to the pane (max `park-card-w`); the rows wrap.

## Behaviour

1. **Where parked panes come from:** the `state` push's `terminals` list has
   `{ id, project, title, order, kind: "parked", paneKind: "ui"|"terminal",
   shellOnly, status: "parked", parkedAt (ms), reason: "idle"|"manual"|"restart",
   hasSession, tokens?, heavy? }`. A parked chat keeps its id, so the layout
   key (`chat:<id>` natively, `live:<id>` in the web) keeps its leaf. A
   parked chat not in the layout is placed like any new chat.
2. **Background:** on mount, `GET /api/panes/:id/preview` → `{ ok, kind: "ui",
   messages }` (up to 200 messages) → the chat log, read-only, never
   connected; its draft (2.8, keyed by the same id) shows in the composer.
   Failure → the background stays empty, the card works. Fetched once per
   mount.
3. **Resume** → `POST /api/panes/:id/resume { mode: "resume" }`. **Compact and
   resume** → `{ mode: "compact" }`. **New session** → `{ mode: "new" }`.
   While it runs: every button on the card disabled, the pressed one shows
   its busy label (Resume keeps its icon).
   - `{ ok: true, id, sameSlot }` → nothing more here: the server's `state`
     push lists the chat live (`kind: "ui"`) under the same id, and the pane
     **swaps in place** to the live chat (same leaf, size, pin, focus); its
     history arrives as for any chat ("Starting Claude…", then the log).
     If `id` differs (`sameSlot: false`), the new chat takes the parked
     pane's leaf.
   - `{ ok: false, error }` or a network error → error toast with the
     message ("Could not resume" if none); the buttons enabled again, the
     label back. Errors seen: "This pane is not parked", `Project "x" no
     longer exists`.
4. **Peek** → the parked view gives way to the conversation: background
   sharp, scrollable, selectable; the card hidden; **Parked view** shown top
   right. Purely local, no request.
   - Reading works (scroll, open tool cards, copy). Anything that would
     change the chat (sending, answering, a mode change: any non-GET call
     the chat makes) **resumes first** (`POST /api/panes/:id/resume { mode:
     "resume" }`) and then goes through; a failed resume → that action fails
     with its error. GET reads while parked come back empty.
   - **Parked view** → back to the card. The web has no Esc for it.
     **Native:** Esc in a peeking pane = Parked view (named addition).
   - The composer reads "Session ended." with the state "exited" while
     peeking (the chat view's status `parked`).
5. **×** on a parked pane → `POST /api/panes/:id/discard` → the pane closes
   for good (its scheduled messages discarded server-side); failure → toast
   "Failed to close: <message>".
6. **Live:** a pane being parked while open (idle timeout, Cru, another
   client) swaps from live to parked in place on the `state` push, and the
   other way on resume. `parkedAt`'s relative time is computed when the card
   is built (it does not tick).
7. Focus: clicking the parked pane focuses it as any pane. The web then
   shows its global terminal input bar under the grid ("Type here… (Ctrl+Enter
   or Send button)") — a web accident for a parked chat; **native: leave it
   out**.
8. The project's dot ignores parked panes (already so natively).

## Data and endpoints

- `state` push → `terminals[]` entries as in Behaviour 1.
- `GET /api/panes/:id/preview` → `{ ok, kind: "ui", messages }` or
  `{ ok, kind: "terminal", screen }` or `{ ok: false, error: "This pane is not parked" }`.
- `POST /api/panes/:id/resume` body `{ mode: "resume"|"compact"|"new" }` →
  `{ ok: true, id, sameSlot }` / `{ ok: false, error }`.
- `POST /api/panes/:id/park` → `{ ok }` / `{ ok: false, error: "This chat cannot be parked" | "No such pane" }`
  (tests use it; no button in the web's chat header).
- `POST /api/panes/:id/discard` → `{ ok: true }`.
- `403 { error: "Not available" }` for confined (invitee) sessions — not the
  native client's case.
- A `404` from any of these (old server) → toast "not supported by this server".
- Ids are 16 hex characters.

## End-to-end (for test/phase2.mjs)

- Resume picker: create an earlier conversation (a chat that ran one turn,
  then closed), open a launcher, UI Mode, pick that row → the create body is
  `{ sessionMode: "resume", resumeId }` and a chat with that `sessionId`
  lands in the launcher's pane.
- Park: `POST /api/panes/<chat>/park` (or restart the test server: every
  pane comes back `reason: "restart"`) → the pane shows the card ("Closed",
  Resume, Peek) in the same leaf; Peek → the log readable, Parked view →
  the card; Resume → the same id live in the same leaf.

## Checklist

- [ ] Parked chats from the `state` push appear in their own leaf, header with grey dot, title, `CHAT` tag, pin, × only.
- [ ] Background: the stored conversation, darkened (no blur: named gap), read-only.
- [ ] Card: Resume (accent pill, glow) + Peek; Why · time; What; quiet New session.
- [ ] Wording for idle / manual / restart and with / without a conversation.
- [ ] Heavy: warning box (long form), Compact and resume + New session as pills.
- [ ] ≤ 330: no What, warning pill short form. ≤ 210: one row, no Why, smaller Resume, tokens-only pill. Sliver: card scrolls.
- [ ] Busy labels Resuming… / Compacting… / Starting…, all buttons disabled; failure → toast, restored.
- [ ] Resume bodies `{mode}` exactly; success swaps in place (same leaf, focus); different id takes the leaf.
- [ ] Peek: readable log, Parked view pill top right; Parked view (and Esc, native) returns; a send from Peek resumes first then sends.
- [ ] × → discard; the pane closes.
- [ ] No global input bar under a focused parked pane.
- [ ] Tokens only (`park-card-w` and, if used, `yellow-edge` added to theme.slint).
