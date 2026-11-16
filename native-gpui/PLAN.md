# Native PC client on GPUI: plan

Scope: the **PC client only**. It replaces the Electron client (`app/`) with a
native one on Windows, macOS and Linux. The Android app is out of scope, and
so is anything for invitees (collaborators), who use the web UI. Electron is
not fixed or changed on the way.

Decision (16 Nov 2026, the owner): the native client is built on **GPUI**
(Zed's UI framework, `gpui-pre 0.3.12` with `gpui-component 0.10.0`), not
Slint. CEF stays for the browser. Everything is compiled Rust: no screens
loaded at run time. The Slint client in `native-client/` (phases 0 to 2 done
there) is the reference: its behaviour, its specs (`native-client/specs/`)
and its tests (`native-client/test/`), which run against this client through
the same test driver.

What it must do is what the PC client does today: the Electron shell
(`app/`), plus the Crundi interface it shows (`src/webapp-html.js`). Those are
the specification. Every screen keeps the PC client's layout and user
experience, drawn with one constant theme of tokens, every control from the
widgets crate (`DESIGN.md`). Who builds what: `AREAS.md`.

## Phases

- [x] **F. Foundation** (the Lead, 16 Nov 2026): the Cargo workspace (core,
  theme, widgets, webview, shell, workbench, chat-model, chats,
  chat-composer, chat-log, workspace, planning, operations, desktop, app),
  versions pinned; the Slint client's non-UI Rust ported into `core`; the
  tokens as a registered gpui-component theme with the bundled fonts and
  Lucide icons; every control in `widgets`; the CEF bridge in `webview`;
  the shell (frameless window, top bar, sidebar, project bar, tab host with
  per-part registration, the drawer's place and divider, sign-in and
  pairing); the pane host; a chat pane (log and composer); the test driver
  on the Slint client's protocol (`test/phase1.mjs`: sign-in, the project
  list, a new chat with Claude's reply, a tool run, sign-out and pairing
  pass); `tools/build.sh`, `tools/smoke.mjs`; CI on three systems
  (`.github/workflows/native-gpui.yml`).

0. **Shell and browser panels** (Desktop; Lead for the window): the drawer
   on the phase 0 panels, the native host over `/ws` (`test/drive.mjs`),
   builds for Linux, Windows and macOS in CI.
1. **Foundation** (Lead): connecting to a server (the HTTP API and `/ws`),
   pairing and sign-in as the PC client does (server step, password and
   code, Google / GitHub / Telegram through the browser), saved settings,
   the window (projects and navigation). Test: `test/phase1.mjs` in full.
   - [x] sign-in through the browser (`sign-in-browser`, app-login)
   - [x] a restart signs in from the saved login
   - [x] the top bar (17 Nov): weekly and 5-hour usage bars (used against
     time passed, over-pace colour, countdowns, "updated … ago", a click
     opens Usage; owner only), Cru's mark (the PC client's: bars and "cru",
     breathing while live, unread dot, hover / click / Ctrl+Space push to
     talk), Telegram's `send` icon and dot, the
     connection badge, the window buttons; the sidebar's dots, folding,
     Add project and Inbox
2. **Chats** (Workspace: 2.1-2.4; Planning: 2.5-2.12b, 2.24; Operations:
   2.13-2.23), each item as `native-client/PLAN.md` describes it, with its
   spec in `native-client/specs/`. Test: `test/phase2.mjs`.
   - [x] 2.1 Pane header, wide · [x] 2.2 narrow · [x] 2.3 moving panes · [x] 2.4 live state (Workspace)
   - [x] 2.5 Composer and Stop · [x] 2.6 permission-mode picker · [x] 2.7 attachments · [x] 2.8 drafts ·
     [x] 2.9 queued messages · [x] 2.10 scheduled messages · [x] 2.11 slash menu · [x] 2.12 chat secrets dialog ·
     [x] 2.12b secret requests · [x] 2.24 voice input (Planning)
   - [x] 2.13 questions · [x] 2.14 tool cards · [x] 2.15 thinking · [x] 2.16 the rest of the log (2.16m media,
     2.16f live frames) · [x] 2.17 suggestions and recaps · [x] 2.18 agent dock · [x] 2.19 goal and plan ·
     [x] 2.20 cards · [x] 2.21 side questions · [x] 2.22 resume picker · [x] 2.23 parked panes (Operations)
   - The gate: `test/phase1.mjs` and `test/phase2.mjs` pass against this
     client, and the UI reviewer passes the screens. Then phases 3 to 6.
3. [x] **Workbench** (Workspace; Desktop for the browser), test `test/workspace.mjs`:
   the workbench's panels (Files, Git, Kanban, Notes, Mindmap, Media,
   Factory, Panel), Claude terminal panels (their input inside each panel:
   key tools, attach, Send; no shared bar), Git, Files, Media; the browser
   drawer in full and the native host's full protocol (Desktop, `test/desktop.mjs`).
4. [x] **Planning** (Planning), test `test/planning.mjs`: Kanban (and the card
   dialog), Mind map, Notes, Schedules, webhooks, Factories (the editor, the
   live run graph, the factory panel).
5. [x] **Operations** (Operations), test `test/operations.mjs`: Services,
   forwards, Docker; Secrets, Skills, Usage; Settings, sign-in, backup; Add
   project; Homepool; Info; Panels (widgets); What's new; the approvals button.
6. [ ] **PC-client extras from `app/`** (Desktop; all but Cru's live screen, kid mode and wake word, below), test `test/desktop.mjs`:
   notifications and the tray, the Inbox, the chat bubble, saved passwords,
   the homepool node, Cru on the PC.
7. **Ship** (not this run): installers and auto-update, signing, the release
   workflow, moving PC users off Electron.

## Deliberate differences decided by the owner

- **The terminal input is inside each terminal panel** (15 Nov 2026): no
  shared bar under the workbench. Each terminal panel has, at its bottom, as
  a chat panel has its composer: the text box (Ctrl+Enter or Send sends to
  that terminal), the keyboard-tools toggle with its key grid, attach
  (uploads to `crundi_attachments`, with its progress strip) and Send.
- **The browser is a drawer** beside the whole project area, below the top
  bar, with a draggable divider; hidden, the project bar's Browser button
  returns with a dot.
- The Slint client's own deliberate differences (`native-client/PLAN.md`,
  Known gaps) carry over where they are behaviour, not Slint workarounds.

## Known gaps

- **Snapshots** come from `xwd` of the window: X11 only (Xvfb in tests). On
  Windows and macOS the driver's `snapshot` says so; CI's pictures are Linux's.
- **No compositor** (Xvfb): GPUI falls back to server-side decorations; the
  client still draws its own top bar and window buttons.
- **CPU on this server** is not representative: no GPU, so wgpu draws with
  Mesa's llvmpipe (lavapipe).
- **macOS** loads CEF's framework and makes NSApp CEF's (`webview/src/mac.rs`,
  from the Slint client), not yet tried by a person on a Mac; CI's macOS job builds it, runs the
  unit tests and starts it (green since 17 Nov, F83).
- **Cru on the PC** (`crates/desktop/src/cru.rs`) has the voice screen,
  listen / ask / speak, barge-in, keyboard mode, history, the Deepgram spend
  line, the top-bar mark and the per-device push-to-talk shortcut. Still to
  port from `app/vendor/crundi-voice.js` and `app/voice-wake` (Desktop,
  `shared/cru-plan.md`): the live screen (Cru's panel: blocks, pages, video),
  kid mode, and the wake word "Hey Cru" (sherpa-onnx keyword spotter,
  training from the owner's voice, the controller).
- **Buttons with a label** use gpui-component's Button with the theme's
  sizes (`widgets/src/control.rs`); its own hover tooltip for `.tooltip()`
  on such a Button is gpui-component's managed style, not `w::tooltip`.
- **Switches and checkboxes** keep gpui-component's drawing, sized from
  the tokens.
- **Toasts** are gpui-component's notifications in one slot: a new toast
  replaces the last (the web stacks a few).
- **Editor highlighting** is syntect's (gpui-component's lighter
  highlighter), not tree-sitter: no code folding or outline.
