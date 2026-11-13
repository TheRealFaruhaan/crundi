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
   client, natively and exactly). The screens, in build order; each one has
   a spec in `specs/`, passes the design review (web-over-native crops under
   the spec) and has an end-to-end test before it is ticked:
   - [ ] **2.1 Pane header, wide.** `.term-head` of a UI chat: title (click to
     rename, `term-rename`), CHAT tag, the secrets lock (`chat-secrets-btn`,
     hidden when the server lacks `chat-secret-preauth`; count badge; yellow
     `.on`), A− / reset (rotate-ccw, `term-font-reset`) / A+ (the chat's
     `--cc-fs`, kept per chat), pin (`pane-pin`: fixed size and position),
     full screen (`pane-full`, Esc leaves), the two-click ×, double-click the
     header to maximise. States: rest, hover of each button, pinned, full
     screen, armed ×, renaming.
   - [ ] **2.2 Pane header, narrow (< 560px).** Tools folded behind "…"
     (`term-more`, `.tools-open`): split right / down, text size, pin, the
     secrets button with its label. States: closed, open, each item hovered.
   - [ ] **2.3 Moving panes.** Drag a pane by its header to move or swap it
     (the drop targets and their highlight); split right / split down from the
     hover controls (`leaf-split`, `head-split`); the empty pane and its chips;
     the "+" menu's Layout presets and Fit (`wb-layout`, `wb-fit`).
   - [ ] **2.4 Live state.** The pane's dot and badge (idle, working,
     needs input, error), the project's dot in the sidebar, the chat title
     updating from the server.
   - [ ] **2.5 Composer and Stop.** The composer at rest, typing (auto-grow),
     the Enter-mode switch (`cc-sw`), the meta row (mode button, model label,
     session id, clock), Send, Stop arming to "Sure?" (`cc-armed`) and
     disarming, and the stacked composer in a narrow pane (`cc-narrow`).
   - [ ] **2.6 Permission-mode picker.** `cc-modal` with every mode, its
     colour and description; changing a running chat's mode; the mode
     button's colour per mode.
   - [ ] **2.7 Attachments.** The attach button, the file picker, drag and
     drop (`cc-dropping`), paste, upload progress (`cc-up`), the chips, and
     the path put into the message (`/api/attachments/upload`).
   - [ ] **2.8 Drafts.** Kept per chat across switching and a restart
     (`saveDraft` / `readDraft` / `pruneDrafts`).
   - [ ] **2.9 Queued messages.** Sent while a turn runs: the queue block
     (`cc-queue`), cancel one, take it back to edit (`recallQueued`), sent when
     the turn ends.
   - [ ] **2.10 Scheduled messages.** The clock (`cc-sched-trig`) and its
     panel (`cc-schedpanel`): empty, a list, the form (when, triggers, text),
     save, delete.
   - [ ] **2.11 Slash menu.** `cc-slash` with its items, keyboard selection.
   - [ ] **2.12 Chat secrets dialog.** What the lock opens
     (`app/vendor/crundi-chat-secrets.js`): list, add, remove, the PIN step.
   - [ ] **2.13 Questions.** One and several questions, multi-select,
     "Other…" with typed text, Submit, Dismiss, the answered and orphaned
     states.
   - [ ] **2.14 Tool cards.** Running spinner, open and closed, the result
     body with its scroll and size limits, diffs (add / del), the Task tool's
     card, the todo list (`cc-todo`).
   - [ ] **2.15 Thinking.** Live shimmer, collapsing when done, opening again.
   - [ ] **2.16 The rest of the log.** Links, code blocks with copy, images
     and media in replies, the long-history notice (`cc-notice`), errors,
     the copy hint on a selection (`cc-copyhint`), the jump-to-bottom button
     (`cc-jump`).
   - [ ] **2.17 Suggestions and recaps.** `cc-sug` (send, dismiss) and
     `cc-recap`.
   - [ ] **2.18 Agent dock.** Sub-agent badges, their list and one agent's
     panel (`cc-agents`, `cc-aglist`, `cc-agpanel`).
   - [ ] **2.19 Goal and plan.** The goal bar (`cc-goalbar`) and the plan
     pill and panel (`cc-plan`, `chat-plan`).
   - [ ] **2.20 Cards.** A factory's result card (`cc-fr-*`) and the other
     cards a reply can carry.
   - [ ] **2.21 Side questions.** `cc-aside` (asked while a turn runs).
   - [ ] **2.22 Resume picker.** `#chat-resume-modal` ("Open a session"):
     loading, the list, empty, error; opening a new or an earlier
     conversation.
   - [ ] **2.23 Parked panes.** The parked card (Resume, its other choices,
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

Every screen copies the PC client's design exactly, by the method and checklist
in `DESIGN.md`; a phase's review includes its side-by-side comparisons.

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
- **Shadows and glows** (Send, the login card, menus) are drawn as soft layers.
  The software renderer has no blur behind rounded corners. Try the GPU
  renderer, or images.
- **The connection dot's ring** doesn't pulse; it's drawn at rest.
- **The login step sits under the 32px title bar.** In Electron the web page
  fills the window. Keep the bar (the window needs it), or give the login a
  drag area and window buttons of its own.
- **Visual-only controls** until their phase:
  - Cru;
  - Add project and Inbox;
  - the schedule clock and the permission-mode picker;
  - the pane header's lock opens nothing yet (its dialog is 2.12);
  - attachments;
  - the launcher's Empty Shell and Terminal (they need a terminal emulator);
  - "Other…" answers to questions;
  - dragging a pane by its header.
- **Tooltips** (phase 2). The web's are the system's own (`title`); Slint
  draws its own (`Tooltip` with a `TipBox`: bg-tertiary, 1px border, 12px).
  They show after Slint's delay, under the pointer.
- **Turning an icon** (2.1, the pinned pin's −30°). The software renderer
  can't rotate an image, so `scripts/gen-icons.mjs` writes a turned copy
  (`pin-turned`).
- **Header text widths** (2.1). "A-" and "A+" come out up to 1px wider than
  the web's (the UI font's widths; the web draws buttons in Arial), so
  everything left of them sits 1px further left. The boxes' heights and the
  buttons' right edges match.
- **Small bold text** (2.1, the CHAT tag). Chromium's hinting draws DejaVu
  Sans Bold at 9.92px with 8px capitals; Slint draws 7px. The tag is drawn
  at 11px with −0.4px tracking instead of 9.92px with +0.6px: caps 8px, C 6px
  wide, the pill 47px wide (web 47.6).
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
- **"Run a server on this machine"** (the Server step) needs a bundled server.
  Decide whether the native installer carries one, as the Electron installer
  does.
