# 2.12 Chat secrets dialog

What the pane header's lock opens: the secrets pre-authorised for this one
chat (their names and how long each lasts, never a value or a lock), revoke
one or all, and pre-authorise more: tick secrets, type each one's PIN or
passphrase once, pick a duration. A pre-authorised secret lets that chat's
`secret_run` use it without asking; `secret_get` still asks every time.

The lock itself (where it sits, its count, its yellow `.on` state, its
tooltip, hidden without the capability) is 2.1's
(`specs/pane-header-wide.md`, "Secrets lock") and is built. This spec adds
the dialog, and checks that the lock's count follows what the dialog does.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below, never a pixel value.

Source:
- `app/vendor/crundi-chat-secrets.js` (whole file, 356 lines): `open()`,
  `drawList()`, `drawPick()`, `submit()`, `close()`, `describe()`,
  `supported()`, `load()`, `handleEvent()`, CSS in `injectCss()`;
- `app/vendor/crundi-secret-lock.js`: `field()`, `lockOk()`, `kindOf()`
  (how a PIN or passphrase field behaves), `upgrade()` (see "After a grant");
- `src/webapp-html.js` 9022-9058 (`chatSecretsInit`, `chatSecretsFor`,
  `chatSecretsBtnHtml`, `paintChatSecretBtns`), 11105-11108 (the
  `chat-secrets` SSE event), 16379-16383 (the lock's click).

Server: `src/webapp.js` 5240-5275 (`/api/secrets/chat`), 5279
(`GET /api/secrets`), 2349-2381 (`broadcastChatSecrets`,
`chatForSecrets`); `src/chat-secrets.js` (presets, `parseDuration`,
`grantChatSecrets`, the limits).

## Screenshots

`chat-secrets/web-<state>.png` (1400×860 unless said). Eight secrets are
seeded, all with passphrases (a new secret can only have one; 6-digit PINs
exist only on old secrets).

| State | What it shows |
|---|---|
| `web-loading` | Just opened: head, the hint, "PRE-AUTHORISED NOW" / "Nothing is pre-authorised for this chat.", "PRE-AUTHORISE" / "Loading the secrets…", the four duration chips with **1 hour** on, Close and Pre-authorise. The card is centred and as tall as its content. |
| `web-open-none-granted` | Loaded: eight rows, each a checkbox and the name (mono); the pick list stops at `pick-max-h` and scrolls; a long name wraps. |
| `web-chosen-two` | GITHUB_TOKEN and DATABASE_URL ticked: each row grows a password field on the right ("Passphrase"); the last ticked has the focus (accent border and ring). |
| `web-err-missing` | Pre-authorise with both fields empty: the first chosen one's field has a red border; the error line (red) "Enter the PIN or passphrase of each secret chosen." |
| `web-checking` | Sent: the primary button reads "Checking…" and is dimmed and disabled. |
| `web-err-wrong` | One passphrase wrong: "GITHUB_TOKEN: Incorrect passphrase"; that field emptied and red; the other keeps its text; **nothing was granted** (all or nothing). |
| `web-granted-two` | Right passphrases, "Until this chat ends" chosen: two grant rows (name, "until this chat ends", Revoke), "Revoke all" (red text) under them, the ticks and fields cleared, the chip choice kept; the toast "Pre-authorised for this chat" (top right). |
| `web-live-timed` | A third grant made elsewhere (the API) for 15 minutes appears live: "until 12:24 (15 min left)". |
| `web-revoking` | Revoke pressed on it: that button disabled until the answer. Then the row goes and the toast reads "Revoked SENTRY_DSN". |
| `web-lock-on-2` | The dialog closed: the header's lock is yellow with "2". |
| `web-err-revoke-offline` | Reopened (duration back to 1 hour), Revoke all with the network down: the error toast "Failed to fetch", the button enabled again. |
| `web-no-secrets-choose` | No secrets on the server: "There are no secrets yet. Add them under Secrets."; Pre-authorise then says "Choose at least one secret." |
| `web-err-offline` | Pre-authorise with the network down: "Failed to fetch" (no name in front); the typed passphrase is kept. |
| `web-err-unsupported` | Pre-authorise against a server answering 404: "This server does not support this yet. Update the server to use it." |
| `web-short-window` | 860×480: the card is the window's height less sp-16 each side; the pick list gives up height first. |

Also, from 2.1: `pane-header-wide/web-no-secrets-capability.png` (no lock
at all on an older server), `web-secrets-on-1.png`,
`web-secrets-on-2-hover.png`.

Not screenshotted: the "Update this secret's lock" prompt (needs a secret
with an old 6-digit PIN, which a new server cannot make; see "After a
grant").

## The parts

```
scrim (whole window)
└── card  (centred; dialog-w-lg wide at most; window height − 2×sp-16 at most; scrolls as one)
    ├── head     [lock]  Secrets for this chat ........................ [×]
    ├── hint     A pre-authorised secret lets this chat run commands …
    ├── label    PRE-AUTHORISED NOW
    ├── list     grant rows | "Nothing is pre-authorised for this chat."
    │            ┌──────────────────────────────────────────────┐
    │            │ GITHUB_TOKEN                        [Revoke] │
    │            │ until this chat ends                         │
    │            └──────────────────────────────────────────────┘
    │                                               [Revoke all]  (2 or more)
    ├── label    PRE-AUTHORISE
    ├── pick     (at most pick-max-h, scrolls)
    │            [x] GITHUB_TOKEN                   [Passphrase ]
    │            [ ] OPENAI_API_KEY
    │            … | "Loading the secrets…" | "There are no secrets yet. Add them under Secrets."
    ├── chips    (15 minutes) (1 hour) (8 hours) (Until this chat ends)
    ├── error    (red; only when it has text)
    └── foot                                   [Close] [Pre-authorise]
```

- **Modal.** Reuse the mode picker's frame (`ui/modes.slint`): the scrim
  (`Theme.scrim`), `Pointer.modal` set through `modal-open`, the card with
  `SoftShadow` `shadow-card`, `RevealFlickable` + `ScrollThumb` for the
  card's scroll and the pick list's.
- **Card:** `bg-secondary`, 1px `border`, `r-card`; padding sp-16; the
  parts in a column, gap sp-10. Width: `dialog-w-lg` (new named size; the
  web's 460) or the window less sp-16 each side, whichever is smaller.
- **Head:** a row, gap sp-8: the lock icon (`icon-lg`, `accent-hover`);
  "Secrets for this chat" (`text-15`, `w-semibold`, `text-primary`)
  filling the row; the close × (`text-secondary`; tooltip/label "Close").
- **Hint:** "A pre-authorised secret lets this chat run commands with it
  without asking for the PIN or passphrase. Only to run: its value is never
  shown, and Claude reading a value still asks you every time. They are kept
  in the server’s memory only, for this chat only." `text-12`,
  `text-secondary`, wraps, paragraph line height.
- **Section labels:** "Pre-authorised now", "Pre-authorise", drawn upper
  case; `text-11`, `w-semibold`, `track-wide`, `text-secondary`; sp-4 more
  space above each.
- **Empty and status lines** ("Nothing is pre-authorised for this chat.",
  "Loading the secrets…", "There are no secrets yet. Add them under
  Secrets."): `text-13`, `text-muted`. Upright (not italic).
- **Grant row:** `bg-primary`, 1px `border`, `r-control`; padding sp-8
  top/bottom, sp-10 left/right; gap sp-10. Left, filling: the name
  (`mono`, `text-13`, `text-primary`, wraps anywhere) over the "when" line
  (`text-12`, `text-secondary`). Right: **Revoke** (quiet `Btn`). Rows gap
  sp-6.
- **Revoke all:** only with 2 or more grants; under the rows, right
  aligned; a quiet button whose text is `red` (not the red-filled danger
  `Btn`; web `.ccs-btn.ccs-danger`).
- **Pick row:** padding sp-4 top/bottom, sp-2 left/right; gap sp-8. A
  checkbox and the name (`mono`, `text-13`, wraps) as one clickable label
  filling the row; when ticked, the lock field at the right.
  Rows gap sp-6. The list scrolls past `pick-max-h` (new named size; the
  web's 240).
- **Lock field** (only on a ticked row), `Field`-like: `bg-primary`, 1px
  `border`, `r-control`, height `control`; focused: `accent` border
  (+ the focus ring other fields use); bad: `red` border.
  - **Passphrase** (`lock: 'passphrase'`): a password field, placeholder
    "Passphrase", UI font `text-15`, left aligned; width `lock-field-w`
    (new named size; web 150).
  - **PIN** (old secrets, `lock: 'pin'`): a password field, placeholder
    "PIN", digits only (anything else is dropped as typed), at most 6,
    `mono` `text-15` with wide tracking, centred; width a little narrower
    (web 110; `lock-field-w` is fine).
- **Duration chips:** a wrapping row, gap sp-6: "15 minutes", "1 hour",
  "8 hours", "Until this chat ends". Pill (`r-pill`), padding sp-6 / sp-12,
  1px `border`, `bg-tertiary`, `text-12`, `text-primary`. On: `accent-dim`
  background, `accent` border, `accent-hover` text. Exactly one is on.
- **Error line:** `text-12`, `red`; takes no room when empty.
- **Foot:** right aligned, gap sp-8: **Close** (quiet `Btn`), then
  **Pre-authorise** (primary `Btn`). Busy: "Checking…", disabled (dimmed).
- Buttons have no hover change on the web (`.ccs-btn` has no `:hover`);
  native's `Btn` hover is fine (not a finding).

## Behaviour

### Opening
- The lock in a chat's header (2.1) opens it for that chat, after closing
  any open header menu. One dialog at a time; opening it again replaces it.
- At once: the list from what the client already knows of this chat's
  grants (loaded at start for every chat, kept live, see "Live"); the pick
  list reads "Loading the secrets…"; duration **1 hour**; nothing ticked;
  no error.
- Then, in parallel:
  - `GET /api/secrets/chat?sessionId=<id>` replaces this chat's grants. If
    it answers `{ok:false, error}`, the error line shows `error` (e.g.
    "That chat is not open on this server.").
  - `GET /api/secrets` fills the pick list with every secret, in the
    server's order, each with its lock kind. Granted ones are listed too
    (ticking one again replaces its duration). On a failure the list reads "There are
    no secrets yet. Add them under Secrets." (the web makes no difference).
- **Without the capability** (`chat-secret-preauth` missing from the
  handshake) the lock is not drawn, so the dialog cannot open. If it is
  opened anyway, the error line reads "This server does not support this
  yet. Update the server to use it." and Pre-authorise is disabled.

### Choosing
- Ticking a secret (a click anywhere on its label): its lock field
  appears and takes the focus. Unticking: the field goes and what was typed
  in it is forgotten.
- Typing in a lock field: kept per secret while the dialog is open, across
  redraws (a live list update, the scroll).
- A duration chip: that one on, the others off. It stays as chosen after a
  grant; it goes back to 1 hour when the dialog reopens.

### Pre-authorise (the button, or Enter in any lock field)
1. Nothing ticked → "Choose at least one secret." Nothing sent.
2. A ticked secret whose lock is not acceptable (PIN: exactly 6 digits;
   passphrase: 1 to 1024 characters) → the first such field turns red, the
   error line reads "Enter the PIN or passphrase of each secret chosen."
   Nothing sent.
3. Otherwise the error line clears, the button reads "Checking…" and is
   disabled, and `POST /api/secrets/chat`
   `{action:'grant', sessionId, duration, secrets:[{id, pin}]}` goes.
   The PINs are dropped from memory as soon as the request is made.
4. **Refused** (`ok:false`): the button is back; if the answer names a
   `secretId`, that secret's field is emptied and red and the error reads
   "<name>: <error>" (e.g. "GITHUB_TOKEN: Incorrect passphrase"); else just
   `<error>` (e.g. "Failed to fetch", or the 404 wording above). The
   other fields keep what was typed. **All or nothing:** the server grants
   none when any one fails.
5. **Granted:** this chat's grants become the answer's `grants` (the list
   and the header lock redraw); the toast "Pre-authorised for this chat";
   every tick and typed lock is cleared; the dialog stays open.

### After a grant: an old lock
If the answer lists `upgradable` ids (secrets still on an old 6-digit PIN
or older parameters) and the server has the `secret-upgrade` capability,
the web then opens "Update this secret’s lock" for each
(`crundi-secret-lock.js` `upgrade()` / `drawUpgrade()`): the name, "This
secret uses an old 6-digit PIN. Set a new passphrase (8+ characters).",
two new-passphrase fields, Not now / Update, `POST /api/secrets`
`{action:'edit', id, pin, newPin}`. That prompt is shared with the Secrets
screen and is not part of phase 2's list: native may leave it out for now
and name it in PLAN.md "Known gaps" (a new server cannot make a PIN secret,
so only an upgraded installation meets it).

### Revoke
- **Revoke** on a row: that button disabled; `POST /api/secrets/chat`
  `{action:'revoke', sessionId, secretId}`. OK: grants replaced from the
  answer, toast "Revoked <name>". Not OK: the button is enabled again, an
  error toast with the answer's `error`, or "Could not revoke it".
- **Revoke all:** the same with `{action:'revokeAll', sessionId}`; toasts
  "Revoked all for this chat" / "Could not revoke them".
- No confirmation for either.

### Closing
- ×, Close, a click on the scrim, or Esc (anywhere while it is open; it
  does nothing else). Everything typed in lock fields is cleared (native:
  overwrite the strings, don't keep them in any model after close).
- Tab: the web lets Tab leave the card for the page behind; native keeps
  Tab and Shift+Tab inside the dialog, as the mode picker does.

### Live
- The SSE event `chat-secrets` `{sessionId, grants}` replaces that chat's
  grants; an open dialog for that chat redraws its list (`web-live-timed`)
  and the header lock redraws its count. Already handled in `live.rs`
  for the lock (2.1); the dialog listens to the same state.
- Every 30 s the "when" lines are recomputed and grants whose `expiresAt`
  has passed are dropped (the lock's count with them).
- "When" (`describe`): no `expiresAt` → "until this chat ends"; else
  "until HH:MM (<left> left)", HH:MM local 24-hour with leading zeros,
  `<left>` from the minutes rounded: "less than a minute" under 1,
  "N min" under 60, else "H h" or "H h M min".

### Owner only
- The native client signs in as the owner. The web hides the lock for a
  collaborator and on a collaborator's chat; native hides it on a chat
  with `collaborator` set (already so in `app.rs`). Nothing else here is
  for collaborators.

## Data and endpoints

| Call | Body / query | Answer |
|---|---|---|
| `GET /api/handshake` | | `{server:{capabilities:[…]}}`; `chat-secret-preauth` for the lock and dialog, `secret-upgrade` for the upgrade prompt |
| `GET /api/secrets/chat` | | `{ok, chats:{<sessionId>:[grant]}, presets}` (at start, every chat) |
| `GET /api/secrets/chat?sessionId=<id>` | | `{ok, sessionId, grants:[grant], presets}` or `{ok:false, error}` |
| `GET /api/secrets` | | `{ok, secrets:[{id, name, description, lock:'pin'\|'passphrase', needsUpgrade, …}], requests}` |
| `POST /api/secrets/chat` | `{action:'grant', sessionId, duration:'15m'\|'1h'\|'8h'\|'chat', secrets:[{id, pin}]}` | `{ok:true, granted:[id], upgradable?:[id], grants}` or `{ok:false, error, secretId?, grants}` |
| `POST /api/secrets/chat` | `{action:'revoke', sessionId, secretId}` | `{ok:true, revoked:0\|1, grants}` |
| `POST /api/secrets/chat` | `{action:'revokeAll', sessionId}` | `{ok:true, revoked:n, grants}` |
| SSE `chat-secrets` | | `{sessionId, grants}` after every change, from any client |

- **grant:** `{secretId, name, expiresAt: ISO|null, untilChatEnds, grantedAt}`.
- **presets** (`src/chat-secrets.js`): `15m` 15 min "15 minutes", `1h`
  60 "1 hour", `8h` 480 "8 hours", `chat` 0 "Until this chat ends".
- **Server errors** shown as they come: "Name a chat.", "That chat is not
  open on this server.", "Secrets cannot be pre-authorised for a
  collaborator's chat.", "Choose at least one secret.", "At most 20
  secrets per chat.", "Enter the PIN or passphrase for each secret.",
  "Incorrect PIN", "Incorrect passphrase", "Unknown duration.",
  "Nothing could be pre-authorised for this chat."; 403 `Not allowed`.
- **404** on any of these → "This server does not support this yet.
  Update the server to use it." (the web's own wording for this dialog;
  use it rather than the generic "not supported by this server").
- **Network failure** → the error's message ("Failed to fetch" on the web;
  native: its own transport error text, one line).
- A grant is lost when its time runs out, it is revoked, the chat's Claude
  exits or the chat is closed, or the server restarts.

## Native notes
- New named sizes: `dialog-w-lg` (card), `pick-max-h` (pick list),
  `lock-field-w` (lock field). Everything else from the scales above.
- The checkbox: use one shared look (the web shows the browser's own
  checkbox); an accent-filled box with a white check is fine. Not std's
  `CheckBox` if it clashes with the theme.
- Keep the dialog's state in Rust (`chosen`, `pins`, `bad`, `duration`,
  `busy`, `err`, the secrets list); the HTTP calls off the UI thread like
  the scheduled-messages panel. Unit-test `describe()` and the lock rule.

## End-to-end test (test/phase2.mjs)
- Seed a secret on the test server: `POST /api/secrets`
  `{action:'add', name:'E2E_TOKEN', description:'e2e', value:'v',
  pin:'open-sesame'}` (8+ characters, not only digits).
- Open the lock of the seeded chat (test control): the dialog lists
  E2E_TOKEN, "Nothing is pre-authorised for this chat.", 1 hour on.
- Tick it, type a wrong passphrase, Pre-authorise: the error reads
  "E2E_TOKEN: Incorrect passphrase"; `GET /api/secrets/chat?sessionId=`
  has no grants.
- Type `open-sesame`, Enter: the server's grants for the chat contain
  E2E_TOKEN with `expiresAt` about an hour away; the pane's `secretsN` is 1
  and its tooltip "1 secret pre-authorised for this chat"; the dialog's
  list shows it with "until HH:MM (60 min left)" (or 59).
- Revoke it: the server has no grants; `secretsN` 0; the list reads
  "Nothing is pre-authorised for this chat."
- Esc closes the dialog.

## Checklist
- [x] Opens from the lock for that chat; closes any header menu first.
- [x] Modal over a scrim; ×, Close, scrim click and Esc close it; Tab
      stays inside; typed locks are cleared on close.
- [x] Head (lock icon, title, ×), the hint text word for word, both
      section labels upper case.
- [x] List: grant rows (mono name, "when" line, Revoke), "Revoke all" in
      red text only with 2+, the empty line otherwise.
- [x] Pick list: loading line, then every secret with a checkbox; the
      no-secrets line; scrolls past `pick-max-h`; long names wrap.
- [x] Ticking shows the lock field and focuses it; PIN fields digits only,
      max 6; passphrase fields any text; unticking forgets the text.
- [x] Duration chips, one on, 1 hour by default, kept after a grant, reset
      on reopen.
- [x] Pre-authorise: the two local checks with their wording and the red
      field; "Checking…" while busy; the named-secret error and emptied
      field on a refusal; nothing granted when any one fails.
- [x] Success: list and header lock update, toast, ticks and typed locks
      cleared, the dialog stays open.
- [x] Revoke / Revoke all: disabled while pending, toasts on success and
      failure, no confirm.
- [x] Live: `chat-secrets` redraws an open dialog and the lock; "when"
      lines recomputed every 30 s; expired grants dropped.
- [x] Older server: no lock; a 404 says "This server does not support this
      yet. Update the server to use it."
- [x] Upgrade prompt: built, or named in PLAN.md "Known gaps".
- [ ] Tokens only (`dialog-w-lg`, `pick-max-h`, `lock-field-w` the only
      new ones); side-by-side crops of each state saved under
      `chat-secrets/review/`.
