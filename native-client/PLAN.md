# Native PC client: plan

Scope: the **PC client only**. It replaces the Electron client (`app/`) with a
native one on Windows, macOS and Linux. It is the PC client's installer from
then on, not an addition beside Electron. The Android app is out of scope, and
so is anything for invitees (collaborators), who use the web UI, not the
client. Electron is not fixed or changed on the way.

Decision (13 Nov 2026): the PC client's whole interface is native, in Rust +
Slint. It is not the web UI inside a browser panel. CEF is used only for real
browser panels and live web content (embeds, previews).

What it must do is what the PC client does today: the Electron shell
(`app/`), plus the Crundi interface it shows (`src/webapp-html.js`). Those are
the specification. Nothing changes on the server, the web UI or the Android app.

## Phases

0. **Shell and browser panels** (done: this prototype). CEF panels in the
   layout, annotations, device emulation, native host over `/ws`, and builds
   for Linux, Windows and macOS in CI.
1. **Foundation** (done, with chats as the worked example: README, Phase 1):
   - connecting to a server: the HTTP API and `/ws`, with the client's token;
   - pairing and sign-in, as the PC client does now;
   - saved settings;
   - the theme and the flat icon set (no emoji);
   - the window: projects and navigation.
2. **Chats** (the factory run's CR: everything a chat does in today's PC
   client, natively, with the same layout and experience). The screens, in
   build order; each one has a spec in `specs/`, passes the design review
   (same parts, order, states, behaviour and wording, theme tokens only;
   side-by-side snapshots under the spec) and has an end-to-end test before
   it is ticked. Since 14 Nov 2026 the owner's rule is layout and
   experience, not pixels (DESIGN.md); 2.1-2.3 were built to the earlier
   pixel rule:
   - [x] **2.1 Pane header, wide.** `.term-head` of a UI chat: title (click to
     rename, `term-rename`), CHAT tag, the secrets lock (`chat-secrets-btn`,
     hidden when the server lacks `chat-secret-preauth`; count badge; yellow
     `.on`), A− / reset (rotate-ccw, `term-font-reset`) / A+ (the chat's
     `--cc-fs`, kept per chat), pin (`pane-pin`: fixed size and position),
     full screen (`pane-full`, Esc leaves), the two-click ×, double-click the
     header to maximise. States: rest, hover of each button, pinned, full
     screen, armed ×, renaming.
   - [x] **2.2 Pane header, narrow (< 560px).** Tools folded behind "…"
     (`term-more`, `.tools-open`): text size, pin, the secrets button with
     its label. (Split right / down, `.head-split`, show only in the web's
     phone layout, so not here; narrow panes split through `.leaf-ctrls`,
     2.3.) States: closed, open, each item hovered.
   - [x] **2.3 Moving panes.** Drag a pane by its header to move or swap it
     (the drop targets and their highlight); split right / split down from the
     hover controls (`leaf-split`, `head-split`); the empty pane and its chips;
     the "+" menu's Layout presets and Fit (`wb-layout`, `wb-fit`).
   - [x] **2.4 Live state.** The pane's dot and badge (idle, working,
     needs input, error), the project's dot in the sidebar, the chat title
     updating from the server.
   - [x] **2.T Token pass** (the owner's change of approach, 14 Nov 2026;
     DESIGN.md, rule 2). The scales in `ui/theme.slint`
     (spacing, radius, font, weights, line heights, colours, recurring
     sizes), and every hard-coded number in shell, workbench, chat, signin,
     browser and widgets replaced by a token (0 and 1px hairlines
     excepted). One review round checks that phase 1 and 2.1-2.4 keep their
     layout, states and behaviour, and look consistent. From here on, every
     screen follows the rule.
   - [x] **2.C Theme consolidation** (the owner, 14 Nov 2026). Theme keeps
     only the scales (spacing 2/4/6/8/10/12/16/20/24/32/40/48, radius
     4/6/8/10/12/pill, font 10/11/12/13/14/15/16/20/26, at most two
     letter-spacings) and named sizes for structural parts (window, top bar,
     sidebar, tab bar, pane header, controls, icons); every other size snaps
     to a scale step; old pixel-measurement comments go. Layout, order,
     states and behaviour unchanged; one review round and CI. With it: the
     sign-in card's fields and button centred again (a token-pass
     regression), its shadow smooth or dropped (the same for every layered
     shadow), and a sweep of every screen for alignment slips.
   - [x] **2.5 Composer and Stop.** The composer at rest, typing (auto-grow),
     the Enter-mode switch (`cc-sw`), the meta row (mode button, model label,
     session id, clock), Send, Stop arming to "Sure?" (`cc-armed`) and
     disarming, and the stacked composer in a narrow pane (`cc-narrow`).
   - [x] **2.6 Permission-mode picker.** `cc-modal` with every mode, its
     colour and description; changing a running chat's mode; the mode
     button's colour per mode.
   - [x] **2.7 Attachments.** The attach button, the file picker, drag and
     drop (`cc-dropping`), paste, upload progress (`cc-up`), and the path
     put into the message (`/api/attachments/upload`). No chips: the web
     has none, it puts the path into the message.
   - [x] **2.8 Drafts.** Kept per chat across switching and a restart
     (`saveDraft` / `readDraft` / `pruneDrafts`).
   - [x] **2.9 Queued messages.** Sent while a turn runs: the queue block
     (`cc-queue`), cancel one, take it back to edit (`recallQueued`), sent when
     the turn ends.
   - [x] **2.10 Scheduled messages.** The clock (`cc-sched-trig`) and its
     panel (`cc-schedpanel`): empty, a list, the form (when, triggers, text),
     save, delete.
   - [x] **2.11 Slash menu.** `cc-slash` with its items, keyboard selection.
   - [x] **2.12 Chat secrets dialog.** What the lock opens
     (`app/vendor/crundi-chat-secrets.js`): list, add, remove, the PIN step.
   - [x] **2.12b Secret requests in a chat.** The secret-request dock
     (`sreq-dock`, src/webapp-html.js ~17420-17530: 'Run a command with X',
     Approve / Reject, the PIN or passphrase field) and its 'Allow for this
     chat' row with a duration (`CrundiChatSecrets.allowRow`, sent as
     `allowForChat` on approve).
   - [x] **2.13 Questions.** One and several questions, multi-select,
     "Other…" with typed text, Submit, Dismiss, the answered and orphaned
     states.
   - [x] **2.14 Tool cards.** Running spinner, open and closed, the result
     body with its scroll and size limits, diffs (add / del), the Task tool's
     card, the todo list (`cc-todo`).
   - [x] **2.15 Thinking.** Live shimmer, collapsing when done, opening again.
   - [x] **2.16 The rest of the log.** Links, code blocks with copy, images
     and media in replies, the long-history notice (`cc-notice`), errors,
     the copy hint on a selection (`cc-copyhint`), the jump-to-bottom button
     (`cc-jump`).
   - [x] **2.16m Media in replies** (split out of 2.16; spec in
     `specs/log-rest.md` section 4). Pictures under `show_image` calls with
     their sized boxes, the loading and gone states and the viewer; video and
     audio (poster, play button, the system player); the file card's
     Download; frames (caption and Open); JPEG, WebP and GIF decoding.
   - [ ] **2.16f Live frames** (split out of 2.16m, F95). A `show_embed`
     frame is live: a windowless CEF view inside each realised frame box
     (https src with its referrer policy, stored HTML as a data: URL, popups
     to the system browser, pointer, wheel and keys forwarded), on the phase
     0 panel code. Until then the frame shows its caption and Open.
   - [x] **2.17 Suggestions and recaps.** `cc-sug` (send, dismiss) and
     `cc-recap`.
   - [x] **2.18 Agent dock.** Sub-agent badges, their list and one agent's
     panel (`cc-agents`, `cc-aglist`, `cc-agpanel`).
   - [x] **2.19 Goal and plan.** The goal bar (`cc-goalbar`) and the plan
     pill and panel (`cc-plan`, `chat-plan`).
   - [x] **2.20 Cards.** A factory's result card (`cc-fr-*`) and the other
     cards a reply can carry.
   - [x] **2.21 Side questions.** `cc-aside` (asked while a turn runs).
   - [x] **2.22 Resume picker.** `#chat-resume-modal` ("Open a session"):
     loading, the list, empty, error; opening a new or an earlier
     conversation.
   - [x] **2.23 Parked panes.** The parked card (Resume, its other choices,
     the warning, the narrow variants) and Peek / leave Peek
     (`/api/panes/:id/resume`).
   - [ ] **2.24 Voice input.** The chat's mic button, push-to-talk and the
     recording strip (`cc-mic`, `cc-rec`, `/api/voice/listen`), and spoken
     replies where the chat has them. In phase 2 by the owner's decision
     (13 Nov 2026); Cru stays in phase 6.
   - [ ] **2.25 Chat known gaps.** Inline-code chip, paragraph line height,
     glows (Send): fixed where Slint allows, the rest kept below.

   Not in phase 2: the header's bubble button (`chat-bubble`) is Electron's
   chat bubble, phase 6.
3. **Workbench**:
   - the pane layout and Claude panels;
   - the browser drawer on the phase 0 panels;
   - the native host's full browser protocol (all of what `app/` answers today).
4. **Planning**: kanban, mind map, notes, schedules, webhooks, and factories
   with the factory editor.
5. **Operations**:
   - services, forwards, Docker;
   - secrets, skills, usage;
   - settings, sign-in, backup;
   - homepool;
   - What's new.
6. **PC-client extras from `app/`**:
   - notifications and the tray;
   - the chat bubble;
   - saved passwords;
   - the homepool node;
   - Cru on the PC.
7. **Ship**:
   - installers and auto-update for Windows, macOS and Linux;
   - signing;
   - the release workflow;
   - moving PC users off Electron.

Every screen keeps the PC client's layout and user experience (the same parts,
arrangement, controls, behaviour, wording and states), drawn with one constant
theme of tokens, by the method in `DESIGN.md`; a phase's review includes its
side-by-side comparisons.

Each phase ends with the screens working against a real server, a
`test/drive`-style check where it fits, and screenshots from CI on all three
systems.

## Known gaps (to fix during the factory run)

Phase 1 left these, each with its workaround in place:

- **Telegram sign-in.** It needs Telegram's web widget: open it in a CEF panel.
- **Inline code.** Slint's StyledText gives the chip a faint outline; the
  web's is a plain fill (#1a1a28, radius 4).
- **Line height inside a paragraph.** StyledText has no line height, so a
  wrapped paragraph's lines sit tighter than the web's 1.55. (Single lines,
  list items and paragraph gaps match.)
- **Shadows and glows** (Send, the login card, menus, the toast, the dragged
  pane, the folded rail's avatars). The software renderer has no blur, and
  stacked layers left visible steps (dark blocks at the edges). Each is now
  one smooth fade drawn with gradients (`SoftShadow` in `ui/widgets.slint`: a
  core, four linear edges, four radial corners), its strength a Theme
  `Shadow`. The fade is linear, not a blur's curve.
- **The connection dot's ring** doesn't pulse; it's drawn at rest.
- **The login step sits under the 32px title bar.** In Electron the web page
  fills the window. Keep the bar (the window needs it), or give the login a
  drag area and window buttons of its own.
- **Visual-only controls** until their phase:
  - Cru;
  - Add project and Inbox;
  - the launcher's Empty Shell and Terminal (they need a terminal emulator);
- **Tooltips** (phase 2). The web's are the system's own (`title`); the
  native client draws its own: `Tip` (ui/chat.slint) in what it describes,
  one `TipBox` drawn by main.slint after `tip-delay`, under the pointer,
  shifted left at the right edge and flipped above it at the bottom so it
  stays inside the window (Slint's own Tooltip ran past the edge). It goes
  on a press or when the pointer moves well away.
- **Turning an icon** (2.1, the pinned pin's −30°). The software renderer
  can't rotate an image, so `scripts/gen-icons.mjs` writes a turned copy
  (`pin-turned`).
- **Header text widths** (2.1). "A-" and "A+" come out up to 1px wider than
  the web's (the UI font's widths; the web draws buttons in Arial), so
  everything left of them sits 1px further left. The boxes' heights and the
  buttons' right edges match.
- **Small bold text** (2.1, the CHAT tag; 2.4, the agent badge). Slint draws
  small bold capitals smaller than Chromium does, so both are drawn a size up
  (text-11) with the tight tracking.
- **The secrets lock with no capability** (2.1) is drawn by the same rule
  (no lock, A- after CHAT), but the e2e test can only reach it on a server
  without `chat-secret-preauth`.
- **A refused close (409)** (2.1): how the answer is read (409 keeps the
  pane with the server's words; other answers close it; no answer says
  "Failed to close terminal") is unit-tested (`head::close_outcome`); the e2e
  test can't make the server refuse (it needs a factory Manager's chat while
  its run goes).
- **Blur on a click elsewhere** (2.1). Slint keeps the focus when a click
  lands on something that can't take it; a browser blurs the field. Every
  press is counted (`Pointer` in `ui/widgets.slint`, from the system's events
  in `src/main.rs`), and the rename input ends and saves on a press outside it.
- **Esc closes the narrow header's menu** (2.2). The web has no Esc there;
  added on purpose (DESIGN rule 5), so Esc closes menus before it reaches the
  pane.
- **No split items in the narrow header's menu** (2.2). The web hides them
  (`.head-split`) outside its phone layout, which the native window (900px or
  wider) never reaches; a narrow pane splits from the hover controls (2.3).
- **Selection height in the rename input** (2.1): Slint draws the selection
  at the font's line height, 1px shorter than Chromium's (y98–110 against
  98–111); its width and the scroll to the end match.
- **Turning a dragged pane** (2.3). The web turns the clone 1.5°; the
  software renderer can't rotate, so it is drawn straight (a little
  see-through, the pane's edge and a soft shadow).
- **A new pane's entrance** (2.3): it fades in over 220ms; the web also
  scales it from .985, which the software renderer can't.
- **Deliberate differences in moving panes** (2.3, DESIGN rule 5):
  - the Terminal chip puts its launcher in the empty pane that was clicked,
    and the chat it starts keeps that pane (the web uses the first empty
    pane, then appends the chat);
  - "Top + bottom split" fits on one line (the web wraps it in a 178px box);
  - the dragged clone keeps its drop shadow when focused (the web's focus
    ring CSS drops it);
  - Esc puts a dragged pane back (the web has no Esc there).
- **The empty pane's chips** (2.3): only Terminal, the one the native client
  can open; Files, Git, Kanban, Notes, Mindmap, Media, Factory and Panel come
  with their panels (phase 3). The chip row wraps differently until then.
- **Dashed rounded borders** (2.3, the empty pane): each corner carries one
  dash on its arc and the straight dashes are spaced to fit each edge
  (`DashedBox`). Chromium runs one pattern round the corners as well, so
  near the far corners (bottom, right) the dashes sit up to 5px from the
  web's; the top-left corner and the dash and gap lengths match.
- **Live state left to other screens** (2.4): the composer's foot label
  ("idle", "working…", "needs your input"; a waiting chat reads "idle") is
  2.5's; the parked pane's grey dot is 2.23's; the top bar's approvals button
  (the shield with a count, for chat cards, secrets requests and
  collaborators) is its own screen, for a later phase (5, operations); Cru's
  cells belong to Cru on the PC (phase 6).
- **Round gradients** (2.T): the software renderer fills a gradient as a
  square and ignores a radius on it or on a clip, so the accent gradient's
  round shapes are images (`AccentFill` in `ui/widgets.slint`, `ui/shapes/`),
  sized to fill what holds them: the folded rail's active avatar (an SVG
  circle) and Send and the cards' primary buttons (a nine-slice PNG, radius
  6: its corners stay round at any size, its middle stretches, so the 135°
  gradient flattens a little on a wide button).
  The same holds for the launcher's UI Mode button (2.C:
  `shapes/launcher-control.png`, nine-slice) and the sign-in card's fill
  (2.C: its square gradient showed dark corners outside the round border;
  now a round top cap, the gradient band, and a round bottom half).
- **The sign-in fields "left-aligned"** (2.C): not reproduced on a fresh
  build at scale 1 or 1.25, in any step (server, password, error, pairing,
  browser code: `specs/token-pass/align/`). Each centred group now also
  has a fixed width, so it can't stretch and lose its centring.
- **The composer** (2.5), deliberate differences (DESIGN rule 5):
  - a refused send (`{ok:false}`, or no answer) puts the server's words in
    the log as an error, gives the text back to the box and returns the chat
    to its real state; the web stays on "working…" with Stop and loses the
    text;
  - the Enter mode is one saved setting that every open pane follows at
    once; the web's other open panes keep their old mode until rebuilt;
  - Queue sends to the server, which hands it to the running turn, until
    the client's own queue (2.9);
  - the cost is the snapshot's, as the web's (it refreshes on the next
    snapshot, not after each turn).
- **The permission-mode picker** (2.6), deliberate differences:
  - a refused or failed change puts the button back to the mode in effect
    and toasts "Could not switch permission mode: <error>" ("Not supported
    by this server" on a 404); the web ignores the answer and keeps showing
    the mode it asked for;
  - no check mark on the selected mode (the web's has no size and is never
    seen); the keyboard's focus is an accent border on the option, and Tab
    stays in the dialog (the web's can leave it, behind the backdrop).
- **Attachments and drafts** (2.7, 2.8), deliberate differences:
  - a file dropped on a chat pane's header goes to that chat (the web's
    workbench takes it and the path is lost);
  - attach stays busy, and the upload bar stays, until the chat's last
    upload ends (the web's shared flag lets the first one end it);
  - an older server's 404 "Not found" reads "Upload failed: not supported by
    this server";
  - drafts live in the client's settings per server (`Saved.drafts`),
    written at most every menu-linger and at quit.
- **Dropping files from the system** (2.7): winit sends no pointer position
  with a file drag, so the chat is the one under the last pointer position
  Slint saw (on X11 the drag may not move it), else the focused chat; full
  screen, its chat. Text drags from panels (Files, Git…) come with those
  panels.
- **Pasting** (2.7): a picture or a copied file is read from the system's
  clipboard (`arboard`) on Ctrl+V / Cmd+V / Shift+Insert in the message box;
  a paste with the focus in the log isn't caught.
- **The file picker** (2.7) is the system's (`rfd`); on Linux it needs the
  desktop's xdg-desktop-portal.
- **Upload errors** (2.7): a proxy's non-JSON answer and no answer at all
  are unit-tested (`attach::upload_answer`); the end-to-end test drives a
  refused upload (an empty file).
- **Queued messages** (2.9), deliberate differences (web bugs):
  - held lines (sent while a chat needs input) belong to the chat, not the
    pane: they survive pane moves and project switches, and go into the
    chat's draft when its pane closes or the app quits (the web loses them);
  - a chat that exits keeps its held lines in the box (the web flushes them
    into the dying session on the idle that comes before the exit and drops
    them);
  - Queue and Send go through the same path: a message sent while working
    is handed over at once (the server holds it), not kept in the client.
- **Scheduled messages** (2.10):
  - the time is picked with Slint's std `DatePickerPopup`, then its
    `TimePickerPopup`: their own (std) style, not our theme, and two steps
    where the web's datetime-local popup has one; the field can be typed;
  - the time field and the list's times use en-US formats (the web's are
    the browser's locale);
  - the panel's message box has no resize grip, and keeps its three rows in
    a short pane (the body scrolls instead; the web's shrinks to nothing);
  - an older server's 404 reads "Scheduled messages are not supported by
    this server." / "Not supported by this server." (named difference);
  - the open panel doesn't update when a message fires, as the web's (no
    event for it): it reloads on open, Schedule and ✕.
- **Italic text** (2.10, and every italic line: _emphasis_, "Thought", the
  schedule list's lines): Slint's software renderer draws no slanted face
  of its own, and a Linux system may have only DejaVu Sans's upright faces.
  The client bundles DejaVu Sans (four faces, ui/fonts/, with its licence)
  and uses it as the UI font on Linux (`Theme.ui-font`); Windows and macOS
  keep the system's UI font, which has its italics.
- **The slash menu** (2.11), deliberate differences: the selected item is
  kept in view as ↑ ↓ move it (the web's scrolls out of sight); a choice
  saves the draft; command names never wrap (the web's wrap at a hyphen).
- **The chat secrets dialog** (2.12):
  - the "Update this secret's lock" prompt after a grant (old 6-digit PIN
    secrets, `upgradable`) is not built: it belongs with the Secrets screen,
    and a new server cannot make a PIN secret;
  - Tab and Shift+Tab move between the ticked secrets' lock fields only
    (the web's Tab also reaches the buttons, and can leave the dialog);
  - the checkbox is the client's own (an accent box with a check), not the
    system's.
- **The secret-request dock** (2.12b):
  - the "Update this secret's lock" prompt after approving with an old
    6-digit PIN (`needsUpgrade`) is not built, as in 2.12;
  - Tab: the dock's buttons, the allow box and the duration take no
    keyboard focus (no native button does yet); Tab from the field goes to
    the next focusable thing, Enter in the field approves;
  - a run's command wraps where Slint's line breaker allows, which also
    breaks after a `/` in a long URL (the web keeps such a word whole and
    wraps at the space before it);
  - while the dock scrolls (taller than the window), the cards' shadows
    are cut at its top and bottom (a scrolling box clips).
- **Question cards** (2.13), deliberate differences and gaps:
  - Enter in Other submits the card when every question has an answer (the
    web does nothing on Enter);
  - ticking a multi-select choice clears Other, as a single choice does
    (the web keeps Other's text and then ignores the ticks);
  - a `respond` the server refuses (or a network error) puts the card back
    to pending with its picks and shows the chat's error line (the web
    leaves a card that looks answered);
  - the keyboard: a radio group is one Tab stop (the picked choice, or the
    first) whose arrows move and pick, Space picks or toggles, the fields
    are Tab stops; Submit, Dismiss, the note buttons and the note's ✕ take
    no keyboard focus (no native button does yet).
- **Tool cards** (2.14), deliberate differences and gaps:
  - an open card stays open when its result arrives (the web rebuilds the
    card on every patch, which closes it);
  - the agent row is padded as a card's head (the web's has no padding);
  - a done to-do is struck with a line across each of its lines, the
    whole width of the text column (Slint's text has no strike-through,
    and a wrapped line's own width is not known);
  - the spinner is two paths (a ring, a turning quarter): the software
    renderer cannot turn an image;
  - tabs in a body are expanded to stops of 8 in Rust (Slint's text has no
    tab stops);
  - the wheel over an open body scrolls the body; it does not pass on to
    the log at the body's end.
- **Thinking** (2.15):
  - the live label's shimmer is a brighter band sliding over it (the web's
    is a gradient fading in and out: the software renderer fills text in
    one colour);
  - the live caret is a glyph after the text (the end of a wrapped text has
    no position to draw a box at), blinking with Theme.blink;
  - the system's reduced-motion setting is not read: it always animates;
  - when a block finishes, its body keeps its scroll (the web's rebuild
    puts it back at the top).
- **The rest of the log** (2.16):
  - the copy button shows on code blocks only: StyledText tells nothing of
    where its links and code spans are, nor that the pointer is over one,
    so spans, links and paths get no copy button, and a right-click on a
    link has no "Open in browser / Copy link" menu (a left click opens it in
    the system browser);
  - reply paragraphs cannot be selected (StyledText has no selection); code
    blocks, bubbles, tool and thinking bodies can, each on its own (a
    selection cannot span blocks);
  - a path in a reply (in running text or as a code span) is drawn as a
    link in accent-hover with a solid underline (the web's is dotted:
    StyledText has one underline); its click does nothing until the Files
    panel (phase 3);
  - a user bubble is StyledText (its URLs links in the bubble's colour)
    under a transparent copy of its text that keeps the selection; a click
    that lands on a URL opens it (a drag selects);
  - a code block scrolls sideways with the wheel (Shift) and has no
    sideways thumb;
  - the jump button's click scrolls down in a few eased frames;
- **Media under show_* calls** (2.16m):
  - a frame (show_embed) is its box with the caption and an "Open" button
    (the page, or the stored HTML through a temp file, in the system
    browser): the live inline frame (a windowless CEF view per box) is not
    built yet;
  - a GIF shows its first frame (not animated);
  - video and audio play in the system's player: the file is fetched (with
    the sign-in, never a URL with the token) to the temp folder, then
    opened; the video box shows its poster and the play button, the audio
    pill its play button and "0:00 / m:ss" (no seek line or sound button:
    they would do nothing; CEF's build has no H.264/AAC and Slint no
    playback);
  - the file card shows the plain file icon for every type (the Files
    list's per-type icons come with phase 3);
  - Download asks where to save first, then streams (the web fetches, then
    asks, and says Saved even when the dialog was cancelled);
  - a decoded picture is kept while the client runs (not let go when far).
  - a media box's round corners: the software renderer's clip has no
    radius, so the box's four corners are covered with corner masks in the
    log's colour (ui/shapes/corner-*.svg) and the rounded edge drawn on
    top; it relies on the log's background being bg-primary.
- **Suggestions and recaps** (2.17), deliberate differences:
  - the log makes room under its last entry for the recap too (the web pads
    only for the suggestion, so a recap covers the end of the last reply);
  - Enter or Space on a focused suggestion sends it (the web's role=button
    has no key handler);
  - the suggestion's dashed edge is DashedBox's dashes (round corners as
    the bubble's), the hover look a solid sug-edge-on edge.
- **Side questions** (2.21), deliberate differences:
  - a card survives a reconnect, anchored after the entry that was last
    when it was asked (the web loses it when it rebuilds the log);
  - the answer is one block of markdown (StyledText), not the reply's
    split blocks: its lists are drawn as StyledText draws them.
- **The agent dock** (2.18):
  - the badges' arrival (scale and accent ring) and finish (green ring)
    animations are not played;
  - the list has no keyboard of its own (Tab, Enter, Esc back to the
    badge); the mouse does everything;
  - "1 tools" is "1 tool" (the web's plural slip).
- **The goal and the plan** (2.19):
  - the plan's headings are bold body text in the panel (StyledText has no
    heading sizes); its lists hang as the reply's.
- **Cards** (2.20):
  - the inline widget under a `widget_open` call (`slot: 'inline'`) is a
    live page: it waits for the in-log CEF view (2.16f) and the Panels tab
    (phase 3); until then nothing shows under the call. Cru's reply cards
    are phase 6's, the factory's own screens phase 4's;
  - deliberate differences on the factory result card: the summary's lists
    and headings are drawn as in replies (the web's hang outside the card);
    the usage line is pluralised ("1 turn"); open lists and a half-typed
    note survive the server's patches; a network failure and a 404 say so
    in the error line; Ctrl/Cmd+Enter in the note sends; the files and
    follow-ups lines open with the chevron icon (the web's ▸ marker);
  - the note grows to six lines and scrolls (no resize grip);
  - a widget check's screenshot opens in the picture viewer without the
    web's "Render: <frame>" title (the viewer has none).
- **The resume picker** (2.22), deliberate differences: Esc cancels, the
  keyboard starts on New session, ↑/↓ move between rows (the web has none
  of these); the launcher shows its busy look from the picker too (the web
  only when no dialog was shown).
- **Parked panes** (2.23):
  - no blur on the conversation behind the card (the software renderer
    has none): it is darkened (`park-dim`) under the veil;
  - the very short card's wrapping row is two rows (Resume, Peek, the
    tokens; then the choices): Slint has no wrapping row;
  - Esc in Peek goes back to the parked view (the web has no Esc there);
  - "Closed · 5m ago" is worked out again on each redraw (the web's stays
    as it was when the card was built);
  - the web's terminal input bar under a focused parked chat is left out;
  - the card's buttons take no Tab focus (the mouse does everything).
- **The session id's dotted underline** (2.5): Slint has no dotted border;
  hairline dashes (sp-2 every sp-4) under the id.
- **Scroll thumbs** (2.5 the composer, 2.6 the picker in a short window):
  `ScrollThumb` shows where the view is; the wheel, the caret and the
  keyboard's focus scroll it, but the thumb can't be dragged (the web's are
  the system's own scrollbars).
- **The composer's tallest box** (2.5): composer-max holds eight lines at
  the default size (the web's nine: its line box is 1.45, the native box
  pads its lines to line-chat).
- **"Run a server on this machine"** (the Server step) needs a bundled server.
  Decide whether the native installer carries one, as the Electron installer
  does.
