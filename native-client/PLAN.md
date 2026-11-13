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
2. **Chats**:
   - the list;
   - the conversation, with tool cards and approvals;
   - the composer, with drafts and attachments;
   - media.
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

Every screen keeps the PC client's layout and user experience, drawn with one
constant theme of tokens (`DESIGN.md`); a phase's review includes its
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
  - the pane header's lock, refresh, A−/A+, pin and maximise;
  - attachments;
  - the launcher's Empty Shell and Terminal (they need a terminal emulator);
  - "Other…" answers to questions;
  - dragging a pane by its header.
- **"Run a server on this machine"** (the Server step) needs a bundled server.
  Decide whether the native installer carries one, as the Electron installer
  does.
