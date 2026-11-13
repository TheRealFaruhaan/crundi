# Native PC client: plan

Scope: the **PC client only**. It replaces the Electron client (`app/`) with a
native one on Windows, macOS and Linux. The Android app is out of scope.

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
1. **Foundation**:
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

Each phase ends with the screens working against a real server, a
`test/drive`-style check where it fits, and screenshots from CI on all three
systems.
