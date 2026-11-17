# 2.12b Secret requests (the request dock)

When Claude calls `secret_get` or `secret_run`, the server holds the call
and asks the owner. The PC client answers it in a **dock of request cards
at the bottom right of the window**, over the workbench, wherever the
person is: read what is asked (the command, for a run), type the secret's
PIN or passphrase, Approve or Reject. A run request a chat made also offers
**Allow for this chat**, which pre-authorises that secret for that chat
(2.12's grants) as part of the approval.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below, never a pixel value. Reuse 2.12's pieces (`ui/secrets.slint`: the
checkbox look, `SecBtn`/`Btn`, the lock field rules, the error line) where
the web shares them.

Source:
- `src/webapp-html.js` 17423-17544: `renderSecretPrompt()`,
  `sreqCanAllow()`, `confirmSecretPin()`, `onSecretPromptClick()`; state
  13157-13160 (`sreqArmed`, `sreqAuto`, `sreqAllow`, `sreqError`); CSS
  2797-2836 (`.sreq-*`), 2892 (`.sreq-pin input.pass`); the SSE handler
  11109-11117; `loadSecrets()` 17396; `secretsPost()` 17596.
- `app/vendor/crundi-chat-secrets.js` 122-147: `allowRow()`,
  `allowForChat()`; CSS 341-344 (`.ccs-allow`).
- Server `src/webapp.js`: `waitForSecretApproval()` 2552-2600 (the 3-minute
  timeout, the notification), `publicSecretRequests()` 2389,
  `broadcastSecretRequests()` 2406, `/api/secrets` actions `approve` and
  `deny` 5328-5356, `/api/mcp/call` `secret_get` 8742 / `secret_run` 8766
  (pre-authorised use skips the request: `preauthorisedAsk`,
  `src/chat-secrets.js` 215).

## Screenshots

`secret-requests/web-<state>.png`, 1400×860 unless said; `-2x` is the dock
cropped and doubled. Three passphrase secrets are seeded (GITHUB_TOKEN,
DATABASE_URL, DEPLOY_KEY; passphrase `open-sesame`), and three chats
(c1 "Fruit list…", c2 "Second chat", c3 "Third chat").

| State | What it shows |
|---|---|
| `web-one-run` | One run request from c1: the card at the bottom right, over the panes (it covers the third pane's composer). Head "Run a command with GITHUB_TOKEN", "project: demo", the reason, the command box, "the value is bound to $SECRET and never shown to Claude", the passphrase field (centred placeholder "Passphrase") with Approve and Reject, and the "Allow for this chat" row (unticked, "1 hour" greyed). Not focused (the composer keeps the keyboard). The top bar's approvals shield reads 1. |
| `web-allow-on` | "Allow for this chat" ticked: the duration enabled, and the hint under it "This chat may then run commands with this secret without asking, until then. Its value is never shown." The card grew upward. |
| `web-select-open` | The duration list open over the card: 15 minutes, 1 hour (current), 8 hours, Until this chat ends. |
| `web-err-empty` | Approve with nothing typed: "Enter the passphrase" (red) at the card's foot; the field takes the focus (accent border and ring). Nothing sent. |
| `web-approving` | Sent: the primary reads "Approving…", disabled and dimmed; the earlier error line is still there until the answer. |
| `web-err-wrong` | Wrong passphrase: "Incorrect passphrase"; the typed text kept and selected, focused; the allow row as it was (ticked, "Until this chat ends"). The request is still pending. |
| `web-several` | Three waiting (c1 run, c2 get, c3 run with a long command): one card each, oldest at the top, gap sp-10; only the first has the field. The c2 one is a **get**: "Release DATABASE_URL", "Claude will be able to read this value.", no command, no allow row. Cards without the field show Approve / Reject. The long command wraps and stops at `cmd-max-h`, scrolling. |
| `web-armed-get` | Approve on the c2 card: the field moves to that card and takes the focus; the first card falls back to Approve / Reject. (The web carries the text typed in the first card's field over; native does not, see "Named differences".) |
| `web-rejected` | Reject on it: the card goes, the toast "Rejected" (top right); the field returns to the first card (allow row reset to unticked, its duration kept). |
| `web-approved-allow` | The c1 request approved with the right passphrase and "Allow for this chat" / 1 hour: its card goes, the toast "Approved, and allowed for this chat" (green edge, success); the field moves to the next card. |
| `web-lock-count-wide` | c1's header (wide pane): the secrets lock yellow with "1", from the grant the approval made. A second `secret_run` from c1 with GITHUB_TOKEN then ran at once (no card). |
| `web-narrow` | Window 620 wide: the dock spans the window less sp-12 on each side, sp-12 from the bottom. |
| `web-real-permission-first` | A real Claude asked to use `secret_run` in default mode: first the chat's own permission card "Claude wants to use Secret Run" (2.13's), the pane NEEDS INPUT. |
| `web-real-waiting-working` | After Allow: the tool row spins and the pane reads **WORKING** while the request waits in the dock (no needs-input state for a secret request); the new card is at the bottom of the dock. |
| `web-timed-out` | The DEPLOY_KEY request reached 3 minutes: its card vanished without a toast; the remaining card is sized to its content (shorter command, narrower card). |
| `web-approved-ran` | The c2 request approved: the dock gone, the toast "Approved", the shield gone (zero), Claude's answer "len=14". |
| `web-err-offline` | Approve with the network down: "Failed to fetch". A card with no reason has no reason line. |
| `web-err-gone` | Approve after the request went (answered elsewhere or timed out between the draw and the click): "Request no longer pending (it may have timed out)". |

Not screenshotted: a 6-digit PIN field (only old secrets have one; a new
server cannot make it; rules below), hover (the buttons are 2.12's `Btn`).

## The parts

```
window
└── dock  (bottom-right; sp-16 from the right and bottom edges; cards stacked, gap sp-10)
    └── card × n   (oldest first, newest at the bottom; the dock grows upward)
        ├── head    [lock]  Run a command with GITHUB_TOKEN      | Release DATABASE_URL
        ├── why     project: demo                                (only with a project)
        ├── why     Check which GitHub account …                 (only with a reason)
        ├── run:  cmd box   curl -s -H "Authorization: …"        (wraps; at most cmd-max-h, scrolls)
        │         bind      the value is bound to $SECRET and never shown to Claude
        │   get:  why       Claude will be able to read this value.
        ├── armed card only:
        │     pin row  [ Passphrase            ] [Approve] [Reject]
        │     allow    [ ] Allow for this chat  [1 hour      v]   (run + a chat of the owner's)
        │              This chat may then run commands with this secret … (only when ticked)
        │     error    Incorrect passphrase                       (only with text)
        └── other cards:
              actions  [Approve] [Reject]
```

- **Dock:** a column at the window's bottom right, sp-16 from the right and
  bottom, gap sp-10, width at most `dialog-w-lg` (2.12's) and never more
  than the window less sp-16 each side. Over the workbench and the composer
  (it covers what is under it, as the web does), under modal dialogs (2.12's
  dialog, the mode picker) and the toast. Window 640 or narrower: it spans
  the window less sp-12 each side, sp-12 from the bottom.
  Native: a card is always the dock's full width (the web shrinks a short
  card to its content; not a finding). The web dock has no height limit; native caps it at the
  window height less the top bar and sp-16, and scrolls (RevealFlickable +
  ScrollThumb), so the oldest card can still be reached.
- **Card:** `bg-card`, 1px `accent` border, `r-card`, `SoftShadow`
  (`shadow-card`); padding sp-12 top/bottom, sp-16 sides (web 14/16); its
  parts in a column, gap sp-8. A new card
  slides up a little and fades in (web 0.18 s); none with reduced motion.
- **Head:** a row, gap sp-6 (web 7): the lock icon (`icon-md`, web 15,
  `accent-hover`); "Run a command with " or "Release " then the secret's
  name in `mono` `accent-hover`. All `text-13`, `w-semibold`,
  `text-primary`. Wraps if long.
- **Why lines** ("project: <project>", the reason, "Claude will be able to
  read this value."): `text-12`, `text-secondary`, wrap.
- **Command box** (run): `bg-primary`, 1px `border`, `r-control`, padding
  sp-8; `mono` `text-11` (web 11.5), chat line height (1.5), `text-primary`;
  wraps anywhere (pre-wrap, long words break), never truncated; at most
  `cmd-max-h` (new named size; web 140) tall, then it scrolls. The text is
  selectable (copy).
- **Bind line** (run): "the value is bound to $<envName> and never shown to
  Claude", `mono` `text-11`, `text-muted`.
- **Pin row** (armed card): a row, gap sp-6, sp-2 above. The field fills
  the row; then **Approve** (primary `Btn`), **Reject** (quiet `Btn`).
  Field: `bg-primary`, 1px `border`, `r-control`, height `control` (the
  buttons the same height), padding sp-6/sp-10, text centred.
  - Passphrase (`lock: 'passphrase'`): password, placeholder "Passphrase",
    UI font `text-15`, normal tracking.
  - PIN (`lock: 'pin'`, old secrets): password-masked digits, placeholder
    "PIN", digits only, at most 6, `mono` `text-15`, wide tracking.
  - Focused: `accent` border and the focus ring. No red border on an error
    (the error line says it).
  - Busy: Approve reads "Approving…", disabled, dimmed.
- **Allow row** (armed card, only when "Allow for this chat" is offered,
  see Behaviour): wraps, gap sp-8, sp-6 above; `text-13`.
  - A checkbox and "Allow for this chat" as one clickable label (gap sp-6);
    2.12's checkbox look.
  - The duration: a dropdown (web `<select>`, aria "For how long") showing
    the choice: "15 minutes", "1 hour", "8 hours", "Until this chat ends";
    `bg-primary`, 1px `border`, `r-control`, `text-12`, padding sp-4/sp-6.
    Disabled (dimmed, not clickable) while the box is unticked. Opens a
    list of the four with the current one marked; a click picks it; Esc or
    a click outside closes it. Native: the shared dropdown if one exists by
    now, else a `PopupWindow` list like `ui/browser.slint`'s.
  - The hint, its own line (full width), only while ticked: "This chat may
    then run commands with this secret without asking, until then. Its
    value is never shown." `text-12`, `text-secondary`, wraps.
- **Error line:** `text-12` (web 11.7), `red`; under the allow row; no room
  when empty.
- **Actions** (other cards): a row, gap sp-6, sp-2 above: **Approve**
  (primary), **Reject** (quiet).
- **Top bar and sidebar** (already elsewhere, not this screen): the
  approvals shield counts secret requests too (phase 5 per PLAN.md); the
  Secrets tab's red badge counts them (Secrets screen). Nothing to add here.

## Behaviour

### Where requests come from
- The server opens a request when `secret_get` or `secret_run` needs the
  owner: `kind` `'get'` or `'run'`; for a run, the exact `command` and
  `envName`; the `sessionId` of the chat that asked (empty when not from a
  chat); the secret's `lock`.
- A `secret_run` from a chat that holds a grant for that secret (2.12, or
  an earlier "Allow for this chat") **does not ask**: it runs at once, no
  card. `secret_get` always asks.
- While it waits, the asking chat's pane is **working** (its tool row
  spins, "working…"), not needs-input: nothing on the pane says a request
  waits. (In "Ask permissions" mode Claude's own permission card for the
  tool comes first; that card and its needs-input are 2.13's.)
- Answered nowhere within **3 minutes**, the server drops it and Claude's
  call fails ("Request timed out — the user did not approve within 3
  minutes.").

### The list (live)
- At start: `GET /api/secrets` → `requests`. Then the SSE event
  `secret-requests` `{requests}` replaces the whole list, after every
  change from any client: new, approved, rejected, timed out.
- One card per request, in the server's order (oldest first). No requests:
  no dock.
- A redraw (any list change) keeps what is typed in the field, its focus,
  the allow row's state and the error, as long as the armed request is
  still there.

### The armed card
- Exactly one card has the field: the armed one. When nothing is armed, or
  the armed request has gone, the **first** card is armed automatically,
  **without taking the focus** (the composer keeps the keyboard; the field
  shows unfocused).
- **Approve** on another card arms that one and focuses its field; the
  allow row is reset to unticked (its duration kept), the error cleared.
- **Allow for this chat** is offered only on a `run` request that came from
  a chat (`sessionId`) which is an owner's chat (not a collaborator's), and
  only with the server capability `chat-secret-preauth`. The duration
  starts at "1 hour" and keeps the last choice across cards for as long as
  the client runs; the tick is always cleared for a new card or after an
  answer. (The web draws the dock before it knows the capability and so
  leaves the row out until the next redraw; native shows it as soon as the
  handshake is known.)

### Approve (the button, or Enter in the field)
1. Local check: passphrase empty → "Enter the passphrase"; PIN not exactly
   6 digits → "Enter a 6-digit PIN". The field takes the focus, its text
   selected. Nothing sent.
2. Approve reads "Approving…" and is disabled; `POST /api/secrets`
   `{action:'approve', reqId, pin}` plus `allowForChat:{duration}` when the
   box is ticked (`duration` one of `15m`, `1h`, `8h`, `chat`).
3. **Refused** (`ok:false`): the error line shows the answer's `error`
   ("Incorrect passphrase", "Incorrect PIN", "Request no longer pending (it
   may have timed out)"), or "Wrong PIN" with none; a network failure shows
   its message ("Failed to fetch"). The button is back; the text stays,
   focused and selected; the request stays pending, so it can be retried.
4. **Approved:** the request goes (the server's `secret-requests` event
   removes the card); the toast **"Approved"**, or **"Approved, and allowed
   for this chat"** when allowForChat was sent and the answer has
   `preauthorised: true`; both success toasts (green edge). The allow
   tick is cleared; the next card is armed (not focused). With
   allow-for-chat, the server also sends `chat-secrets` for that chat, so
   its header lock counts one more (2.1, already live) and 2.12's dialog
   lists the grant.
5. `needsUpgrade` in the answer (an old lock): the web opens 2.12's
   "Update this secret's lock" prompt. Same as 2.12: native may leave it
   out and name it in PLAN.md "Known gaps".

### Reject
- **Reject** on any card (armed or not): `POST /api/secrets`
  `{action:'deny', reqId}` at once, no confirmation; the toast
  **"Rejected"** (info). Claude's call fails with "The user denied this
  secret access request." The card goes with the server's event; if it was
  armed, the next one is armed.
- Network failure: the web shows nothing (the card stays). Native: an error
  toast with the transport error (named difference).

### Keyboard
- **Enter** in the field: Approve.
- **Esc** in the field: clears the error and leaves the field (the field
  loses the focus; nothing else is focused). The typed text stays. (The
  web clears the error only at the next redraw; native clears it at once.)
- Tab moves field → Approve → Reject → the allow box → the duration →
  the next card's buttons, in reading order. The dock is not modal: a click
  anywhere else works as usual, and the composer can be typed in while
  cards wait.

### Leaving
- A card leaves only when the server says so (approved, rejected, timed
  out, or answered from another client/phone): no close button, no
  dismiss. A timed-out card vanishes without a toast.
- Native: clear the typed PIN or passphrase from memory as soon as the
  request is sent or the card goes (overwrite the string); never keep it
  in a model after the answer.

## Data and endpoints

| Call | Body | Answer |
|---|---|---|
| `GET /api/handshake` | | `server.capabilities` includes `chat-secret-preauth` (Allow for this chat) and `secret-upgrade` (the upgrade prompt) |
| `GET /api/secrets` | | `{ok, secrets:[…], requests:[request]}` |
| `POST /api/secrets` | `{action:'approve', reqId, pin, allowForChat?:{duration}}` | `{ok:true, preauthorised?:bool, needsUpgrade?:true}` or `{ok:false, error}` (wrong lock: the request stays) |
| `POST /api/secrets` | `{action:'deny', reqId}` | `{ok:true}` (also when it was already gone) |
| SSE `secret-requests` | | `{requests:[request]}`, the whole list, on every change |
| SSE `chat-secrets` | | `{sessionId, grants}` after an allow-for-chat (2.12) |
| SSE `approvals` | | the shield's list (not this screen) |

- **request:** `{id, secretId, secretName, project, reason, createdAt,
  kind:'get'|'run', command, envName, lock:'pin'|'passphrase',
  needsUpgrade, sessionId}`. Never a value.
- **allowForChat.duration:** `15m` | `1h` | `8h` | `chat` (the presets of
  2.12); the server grants it only for an owner's chat that is open.
- **404 / older server:** `requests` and `approve`/`deny` are old; only
  `allowForChat` is new. Without the capability the row is not drawn, so
  nothing new is sent.

## Named differences (native, not findings)
- The dock caps its height and scrolls (the web runs off the top).
- A card is always the dock's width.
- Moving the field to another card starts it **empty** (the web carries the
  text typed for one secret into another secret's field).
- Esc clears the error line at once.
- The allow row appears as soon as the capability is known.
- A Reject that fails says so in an error toast.
- The native toast needs a **success** kind (green edge, as the web's
  `.toast.success`); today `toast()` has only normal and error.

## End-to-end test (test/phase2.mjs)
Drive the request the way Claude's MCP bridge does: `POST /api/mcp/call`
with header `X-Api-Key` (the key in the test server's
`<data>/dev-mcp/<chatId>.json`, `mcpServers.crundi.env.CRUNDI_API_KEY`)
and `{tool:'secret_run', args:{sessionId:<chat>, name:'E2E_RUN', command:'echo len=${#SECRET}', reason:'e2e'}}`.
The call blocks until answered, so start it without awaiting.
1. Seed `E2E_RUN` (`POST /api/secrets` `{action:'add', name:'E2E_RUN',
   value:'v-e2e', pin:'open-sesame'}`).
2. **Reject:** start a run; the dock shows one card "Run a command with
   E2E_RUN" with the command, armed but not focused, and the allow row.
   Press Reject: the call answers `{ok:false, error:'The user denied this
   secret access request.'}`; the dock is gone; the toast "Rejected".
3. **Wrong, then right:** start a run; type `nope-nope`, Enter: "Incorrect
   passphrase", the card still there and the request still in
   `GET /api/secrets`. Type `open-sesame`, Approve: the call answers
   `{ok:true, code:0, stdout:'len=5\n'}`; the toast "Approved"; no dock.
4. **Allow for this chat:** start a run; tick the box, choose 1 hour, type
   `open-sesame`, Enter: the toast "Approved, and allowed for this chat";
   `GET /api/secrets/chat?sessionId=` lists E2E_RUN with `expiresAt` about
   an hour away; the pane's `secretsN` is 1. Start a second run: it
   answers `ok:true` at once and **no card appears**.
5. **Several:** start a `secret_get` and a run from two chats: two cards,
   the first armed; Approve on the second moves the field there and it is
   empty and focused. Reject both.
6. **Esc** in the field after a wrong passphrase clears the error line and
   keeps the text.
7. (Optional, slow) a timed-out request: skip; covered by the event test —
   push a `secret-requests` with an empty list and the dock goes.

## Checklist
- [x] Dock at the window's bottom right (sp-16), over the panes, under
      dialogs and the toast; full width less sp-12 at 640 or narrower;
      height capped with a scroll.
- [x] One card per request, oldest first, live from `secret-requests`;
      no requests, no dock.
- [x] Head wording per kind: "Run a command with <NAME>" / "Release
      <NAME>", the name mono in accent.
- [x] Project and reason lines only when set; run: the command box (wraps,
      `cmd-max-h`, scrolls, selectable) and the bind line with the env
      name; get: "Claude will be able to read this value."
- [x] The first card armed without focus; Approve on another arms and
      focuses it (empty field); others show Approve / Reject.
- [x] Passphrase field centred, "Passphrase"; PIN field digits only, 6
      max, "PIN".
- [x] Allow for this chat: only for a run from an owner's chat with the
      capability; unticked by default; duration dropdown disabled until
      ticked, 1 hour first, last choice kept; hint only when ticked; sent
      as `allowForChat`.
- [x] Approve: the two local messages; "Approving…" disabled; server
      errors in the line, text kept, focused and selected; success toasts
      "Approved" / "Approved, and allowed for this chat" (green).
- [x] After allow-for-chat the header lock counts it, and the next run
      from that chat asks nothing.
- [x] Reject: at once, toast "Rejected", the card goes; failure → error
      toast.
- [x] Timed out or answered elsewhere: the card goes silently; a stale
      Approve shows "Request no longer pending (it may have timed out)".
- [x] The pane stays "working" while a request waits (no needs-input).
- [x] Enter approves; Esc clears the error and leaves the field; the dock
      is not modal.
- [x] Typed locks cleared from memory after sending or when the card goes.
- [x] Upgrade prompt: built, or named in PLAN.md "Known gaps" (as 2.12).
- [ ] Tokens only (`cmd-max-h` the one new size; `dialog-w-lg` reused);
      side-by-side crops of each state under `secret-requests/review/`.
