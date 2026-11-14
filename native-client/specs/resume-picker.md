# 2.22 Resume picker ("Open a session")

Every Claude launch from a launcher asks first **which conversation**: a new
one, or an earlier one in this project (Claude Code's own transcripts on the
server's disk). It is the dialog `#chat-resume-modal`, opened by the
launcher's **UI Mode** and **UI Mode — Skip Permissions** buttons (and, in the
web, Terminal and Terminal — Skip Permissions; terminals are not phase 2's,
but the dialog is the same one, so build it mode-agnostic). Empty Shell never
asks.

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.
The px in "Web" columns are only there to pick the token.

Source (`src/webapp-html.js`):
- CSS 1013-1083: `.btn-chat-resume` (unused now), `.cr-list`, `.cr-item`,
  `.cr-title`, `.cr-meta`, `.cr-empty`, `.cr-new`, `.cr-intro`, the `.cr-busy`
  spinner, `#chat-resume-modal`.
- Markup 3890-3899 (`#chat-resume-modal`, `#cr-heading`, `#cr-body`, Cancel).
- `beginLaunch` 7621 (busy and lock), `launchTerminal` 7645 (the create call),
  `openSessionChooser` 7718, `renderChatResume` 7746, `renderHeavyChoice`
  7766, `closeChatResume` 7779, `relTime` 7784.
- Clicks 16517-16566: `launch-terminal`, `cr-new`, `chat-resume-cancel`,
  `cr-back`, `chat-resume-pick`, `cr-resume`.
- Server: `GET /api/ui-sessions/resumable` (`src/webapp.js` 6405),
  `listResumable` / `readTranscriptTitle` / `estimateTranscriptTokens`
  (`src/claude-ui.js` 282-440), `HEAVY_TOKENS = 100_000` (359).

Native today: `launch_chat(leaf, skip)` (`src/app.rs` 1541) posts
`/api/ui-sessions/create` straight away. The picker goes in between.

## Screenshots

`resume-picker/web-<state>.png`, 1400×860; `-2x` is the dialog (or the named
region) cropped and doubled. The list is the test server's real transcripts
(20 of them) plus one seeded heavy transcript (138.2k tokens, "Refactor the
billing exports into one module"). Loading, error, empty and failed launches
were made with a fetch stub.

| State | What it shows |
|---|---|
| `web-resume-loading` | **Loading:** UI Mode pressed, the list is being fetched. The web shows **nothing** (no spinner, the dialog is not open yet); a second click is ignored. |
| `web-list` (+`-2x`) | **The list.** Heading "Open UI Mode"; the accent **New session** row first; then one row per earlier conversation, newest first: title (bold, up to 2-3 lines), meta line in mono: id (8) · age · tokens, plus **heavy** (yellow) or **open elsewhere — opens a copy** (accent). Cancel bottom right. |
| `web-row-hover` (+`-2x`) | A row hovered: accent border, `accent-dim` fill. |
| `web-row-focus` (+`-2x`) | A row focused by Tab: the browser's focus ring (white 1px). |
| `web-list-scrolled-end` (+`-2x`) | **Long list** scrolled to the end: the list scrolls inside the dialog; heading and Cancel stay put. |
| `web-heavy-choice` (+`-2x`) | **A heavy conversation picked** (UI Mode only): the list is replaced by the intro (bold title, then "138.2k tokens — continuing loads all of it into every turn. Compacting loads it once, then summarises it so later turns stay small."), **Compact first (recommended)** (accent row), **Load the full conversation**, **← Back to the list** (mono, muted). |
| `web-error` (+`-2x`) | **The list could not load:** New session, then "Could not load earlier conversations: Failed to fetch" (muted). |
| `web-empty-launches-new` (+`-2x`) | **Empty** (no earlier conversation): no dialog at all; the launch goes straight on as a new session, the pressed UI Mode button spins and the launcher's other buttons are disabled. |
| `web-launching` | **Chosen** (here Compact first): the dialog closes at once; the launcher stays as it was (the web only locks it: a click on it does nothing) until the chat arrives. |
| `web-launch-failed` | **The create call failed:** an error toast with the server's message; the launcher is usable again. |
| `web-resumed` | **An earlier conversation resumed:** the new chat takes the launcher's pane, focused, its log loaded from the transcript ("— earlier messages in this conversation —"), and (server side) a recap "Where this chat stands". |

## Parts

A modal over the whole window:

| Part | Web | Native |
|---|---|---|
| Backdrop | fixed, `rgba(0,0,0,.7)`, centres the dialog | `Theme.backdrop`, centred |
| Dialog | `bg-secondary`, 1px `border`, radius 10, padding 22, width 92% up to 560 | `bg-secondary`, `border`, `r-card`, padding `sp-24`; width a named size **`dialog-w-xl`** (new, the web's 560; `dialog-w-lg` 460 wraps every title) capped at the window less `sp-32`; height at most `dialog-max-h` |
| Heading `#cr-heading` | 16.8 px bold, 14 below | `text-16` `w-bold`, `sp-12`/`sp-16` below |
| List `#cr-body` | column, gap 6, `max-height: 46vh`, scrolls | column, gap `sp-6`; takes what the dialog has left between heading and buttons and scrolls (a `ScrollThumb` as elsewhere) |
| Row `.cr-item` | full width, column gap 2, padding 9/12, 1px `border`, radius 6, `bg-primary` | full width, gap `sp-2`, padding `sp-8`/`sp-12`, `border`, `r-control`, `bg-primary` |
| Row hover | border `accent`, bg `accent-dim` | same |
| Row focus | browser outline | the client's focus ring (`accent-ring`) |
| Title `.cr-title` | 13.8 px w600 `text-primary`, wraps | `text-14` `w-semibold`, wraps (no line limit) |
| Meta `.cr-meta` | 11.5 px mono `text-muted` | `text-11` `Theme.mono` `text-muted` |
| "heavy" | bold `yellow` | `w-bold` `yellow` |
| "open elsewhere — opens a copy" | bold `accent` | `w-bold` `accent` |
| New session row `.cr-new` | border `accent`; title `accent-hover` | same |
| Error line `.cr-empty` | 13.6 px `text-muted`, padding 10/2 | `text-13` `text-muted`, padding `sp-10`/`sp-2` |
| Intro `.cr-intro` (heavy) | 12.8 px `text-secondary`, line-height 1.5; title bold on its own line | `text-13` `text-secondary`, `lh-chat`; title `w-bold` then a line break |
| Buttons row | right-aligned, gap 8, 14 above | right, gap `sp-8`, `sp-16` above |
| Cancel | padding 8/16, radius 6, 1px `border`, `bg-tertiary`, 13.8 px | `Btn` (secondary), `control` height |

### Row text

- **New session** / "Start a fresh conversation".
- Earlier conversation: title = the transcript's `ai-title`, else its last
  prompt, else its first user message, else "(untitled session)" (server,
  up to 120 characters). Meta: `id.slice(0,8)` · `relTime(updatedAt)` ·
  (`tokens` ? `(tokens/1000).toFixed(1)k tokens` : `max(1, round(sizeBytes/1024)) KB`)
  · **heavy** when `tokens >= 100000` · **open elsewhere — opens a copy** when `inUse`.
- `relTime`: < 60 s "just now", < 1 h "Nm ago", < 1 d "Nh ago",
  < 30 d "Nd ago", else the local date.

### Heading

"Open " + the launch mode's label: **UI Mode**, **UI Mode — Skip
Permissions**, **Terminal**, **Terminal — Skip Permissions**. ("Open a
session" is only the markup's placeholder, never seen.)

## Behaviour

1. **UI Mode** (or Skip Permissions) in a launcher: if a launch is already in
   flight in that launcher, or the list is already loading, nothing.
   Otherwise `GET /api/ui-sessions/resumable?project=<alias>`.
2. **Loading:** no dialog yet, no busy look (web). The launcher just ignores
   further clicks.
3. **Loaded, empty list** (and no error): no dialog. Launch a **new** session
   at once, exactly as "New session" below, with the pressed button busy.
4. **Loaded with rows, or failed:** open the dialog with the heading, New
   session first, then the error line (failed) or the rows.
5. **New session** → close the dialog, launch with `sessionMode: "new"`.
6. **An earlier conversation:**
   - UI Mode and `tokens >= 100000` → the **heavy choice** replaces the list
     (the dialog stays open, its height shrinks to fit):
     - **Compact first (recommended)** → close, launch `sessionMode: "compact"`, `resumeId`.
     - **Load the full conversation** → close, launch `sessionMode: "resume"`, `resumeId`.
     - **← Back to the list** → the list again (rows only, no error line).
   - Otherwise → close, launch `sessionMode: "resume"`, `resumeId`.
   - An `inUse` conversation still opens: the server forks it (a copy).
7. **Launch:** the launcher's pressed button gets the busy spinner and its
   siblings go disabled (`beginLaunch`) — in the web only when no dialog was
   shown (from the dialog, the dialog simply closes and the launcher stays
   locked but looks idle). On success the new chat takes the launcher's pane
   and is focused (native: the existing `launch_chat` placement). On failure
   (`ok:false` or a network error): an error toast with the message
   ("Failed to launch" if none), the launcher unlocked.
   - **Native:** show the launcher busy in both cases (the pressed button
     spinning, the rest disabled), since the native client has the busy
     look already; this is the web's own `beginLaunch` look, applied
     consistently.
8. **Cancel** → close, nothing launched, the launcher unchanged.
9. **Esc and the backdrop:** the web does **nothing** on either (no handler).
   - **Native:** Esc = Cancel, as the client's other dialogs (named
     difference, a keyboard path the web lacks); a click on the backdrop
     does nothing (as the web).
10. **Keyboard:** every row and Cancel is a button. Tab / Shift+Tab move
    through New session, the rows, Cancel; Enter or Space presses the
    focused one. Focus is not moved into the dialog on open in the web.
    - **Native:** focus New session on open, so Enter starts a new session
      and Tab walks the rows; ↑/↓ also move between rows (named addition,
      same as the slash menu's). The list scrolls to keep the focused row
      in view.
11. **Long list:** at most 30 rows (server limit), newest first; the list
    scrolls, the heading and Cancel do not.
12. Nothing in the dialog updates live: the list is read once per opening.

## Data and endpoints

- `GET /api/ui-sessions/resumable?project=<alias>` →
  `{ ok: true, sessions: [{ id, title, updatedAt (ISO), sizeBytes, tokens, ageHours, inUse }] }`,
  newest first, at most 30. `400 { ok:false, error:"Unknown project" }`.
  A `404` (an old server) → treat as an empty list (launch new), per the
  backwards-compatibility rule.
- `POST /api/ui-sessions/create`, body:
  - new: `{ project, sessionMode: "new", title: "Chat (new)" }`
  - resume: `{ project, sessionMode: "resume", resumeId }` (no title; the server names it)
  - compact: `{ project, sessionMode: "compact", resumeId, title: "Chat (compacted)" }`
  - Skip Permissions adds `skipPermissions: true` and, unless one of the
    titles above is set, `title: "Chat (skip perms)"` (the web sets it
    first, then `sessionMode`'s title replaces it for new/compact).
  - → `{ ok, id, project, title, order, kind: "ui" }` or `{ ok:false, error }`.
  Then the chat arrives as usual (`ui-history` with the transcript's
  messages, "— earlier messages in this conversation —").

## Checklist

- [ ] UI Mode and UI Mode — Skip Permissions open the picker; Empty Shell does not.
- [ ] Loading: no dialog; repeated clicks ignored.
- [ ] Empty list: no dialog, a new session starts, the button busy.
- [ ] Heading "Open UI Mode" / "Open UI Mode — Skip Permissions".
- [ ] New session row first, accent; then rows newest first.
- [ ] Row: title (wraps), mono meta "id8 · age · N.Nk tokens" (or KB), **heavy**, **open elsewhere — opens a copy**.
- [ ] Hover (accent border, accent-dim) and focus ring on rows.
- [ ] Long list scrolls inside the dialog; heading and Cancel fixed.
- [ ] Error: New session + "Could not load earlier conversations: <message>".
- [ ] Heavy (≥100k, UI Mode): intro, Compact first (recommended), Load the full conversation, ← Back to the list; Back restores the list.
- [ ] Create bodies exactly as listed (new / resume / compact, skip).
- [ ] The chat lands in the launcher's pane, focused; launch failure → error toast, launcher usable.
- [ ] Cancel closes; Esc closes (native); backdrop click does nothing.
- [ ] Keyboard: Tab through rows, Enter/Space picks; native ↑/↓ and initial focus on New session.
- [ ] 404 from `resumable` → new session (no breakage).
- [ ] Tokens only (`dialog-w-xl` added to theme.slint as a named size).
