# Workspace (phase 3: 3.1 to 3.5)

The web's code each screen follows (src/webapp-html.js) and what the native
client does. Screens of each state: `screens/workspace/` (test/workspace.mjs).

## 3.1 The workbench's panels

- The "+" menu's Files, Git, Kanban, Notes, Mindmap, Media, Factory and
  Panel open their area's view in a pane (`panel:<kind>`; Notes and Panel
  open many: `panel:notes:<n>`, `panel:widget:<n>`). A second one of the
  others is refused with the web's toast "<Kind> is already in the
  workbench" (addWbPanel).
- The empty pane's chips (emptyLeafHtml): Terminal, the panels not in the
  workbench yet (Notes always), Panel; they wrap onto rows. A chip puts its
  panel in that pane.
- A panel's header (headHtmlPanel): the drag handle, the kind's icon, its
  label (no rename), Refresh, Pin, ×. Refresh loads Git, Files and Media
  again; another area's panel hears it as its tab showing.

## 3.2 Claude terminal panels

- The launcher's Empty Shell (`/api/terminals/create`, shell) and Terminal
  and its Skip Permissions (the conversation picker first, "Open Terminal",
  as UI Mode; a heavy conversation resumes directly: the CLI asks).
- A terminal pane (`term:<id>`): subscribed on /ws, its scrollback then its
  output drawn by a VT/xterm emulator (src/workspace/vt.rs: cursor moves,
  erase, scroll regions, insert/delete, SGR colours (16, 256, true colour),
  bold-bright, inverse, the alternate screen, bracketed paste, cursor-key
  mode, status reports, titles, wide characters); keys typed into it
  (xterm's bytes; Shift+Enter is ESC CR, the web's default), Ctrl+V pastes,
  the wheel scrolls the scrollback (the arrow keys on the alternate screen),
  the jump to the bottom; a drag selects whole cells and copies them; the
  pane's size goes to the server (`resize`). Header: dot, title (rename:
  `/api/terminals/:id/rename`), A− ↺ A+, Pin, full screen, × (closes the
  terminal: `/api/terminals/:id/close`).
- Its own input (owner's decision, AREAS.md): the keyboard tools toggle and
  the key grid (Esc, Tab, ⇧Tab; Ctrl+C/Z/A/E/D/L, Space; Home End and the
  arrows round Enter), the text box ("Type here... (Ctrl+Enter or Send
  button)"; Ctrl+Enter or Send: the text, then Enter as its own write 60ms
  later), attach (to crundi_attachments, with the upload strip; its path
  into the box), Send.

## 3.3 Git

The web's Git panel (gitPanelHtml): the branch and ↑ahead ↓behind; Staged
and Changes with their counts, Unstage all / Stage all; each file's status
letter, folder, name, +/− counts, View diff, Stage / Unstage, Discard (asks
first); the commit box and ↓ Pull, ↑ Push (Pulling…, Pushing…), Commit
("Enter a commit message" when empty). Refreshed every 8 s while on show.
The diff (feOpenDiff) opens in a file window: ORIGINAL | MODIFIED, the line
numbers, the removed and added lines, the changed words of a replaced line,
a bar per chunk with Stage (then Staged), ↑ n/m ↓ between chunks;
`/api/git/stageHunk`.

## 3.4 Files

The web's Files panel: the search box ("Search files & folders…", 220 ms,
Esc clears; "N matches · gitignored files excluded", "No matches"), the
path bar (absolute crumbs) with Paste (when something is cut or copied),
Up and Upload; the list with "..", folders, files with their per-type mark
and size, the hover "…"; click selects, double click opens, right click or
"…" opens the menu: Open, Edit the source (SVG), Open externally / Reveal
in / Open in <file manager> (when the server runs here), Cut, Copy, Paste
inside / here, Copy path, Download, Delete (asks first). A file opens in a
file window (the editor: highlighting, line numbers, Save and Ctrl+S,
"Saving…", "Saved ✓"), a picture in the viewer; the windows drag, snap to
the edges and corners, resize, expand, and Tile / Grid / Cascade from the
Arrange bar. A path in a chat's reply opens in the editor (openPathFromChat:
inside a project, or resolved by the server).

## 3.5 Media

The web's media browser: Upload, the search, All / Kanban / Mindmap /
Unlinked, This project / All projects; the cards (thumbnail or kind glyph
and extension, kind badge, name, project and link line, View or Get, Copy
path, Jump to source, Delete); the empty lines; the lightbox (title,
Download, Close; Esc or the backdrop close it).

## Deliberate differences and gaps

- **Git history and branches:** the web's Git panel has none (only the
  branch name and ahead/behind); the native one matches it rather than
  inventing screens. `/api/git/log` exists on the server for a later one.
- **Video, audio and PDF** (Media's lightbox, a PDF in Files): opened in the
  system's player or viewer (CEF's build has no H.264/AAC, Slint no
  playback; as 2.16m). The lightbox shows the kind and Play / Open.
- **Changed words** in the diff are drawn in white, not on a stronger
  background (StyledText has no background spans).
- **The editor** colours files up to 4000 lines (it draws each line;
  CodeMirror only those in view); longer files edit in one colour. No
  folding, search or multiple carets (CodeMirror's basicSetup).
- **Terminal selection** is by whole cells and copies on release (no
  double-click word select, no selection on the alternate screen kept
  across output).
- **Drag a file row onto a terminal** (wireDragRefs) and dropping rows onto
  folders: not yet; Cut / Copy / Paste do the same moves.
- **Media modal** for a Kanban card or a Mindmap idea (openMediaModal) is
  the planning area's to open; the browser here takes no link filter yet.
