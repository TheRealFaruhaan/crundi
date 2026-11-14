# 2.7 Attachments and 2.8 Drafts

Two pieces of composer behaviour, specified together:
- **2.7 Attachments:** the attach button and its file picker, dropping
  files (or a path) on a chat, pasting a picture or a file, the upload to
  `/api/attachments/upload`, the upload bar, the path put into the message,
  and every way it fails.
- **2.8 Drafts:** what the message box keeps when it is not sent, where,
  for how long, and how it comes back after a switch, a closed chat or a
  restart.

The composer's layout, the attach button's place, size and hover are 2.5's
(`specs/composer.md`). This spec adds what the button does, the upload bar
(`.cc-up`, "hidden (2.7)" in 2.5's tree) and the drop outline.

**There are no attachment chips.** The web never shows a chip, a
thumbnail or a list of attachments: an attached file becomes a plain path
typed into the message, and that is all. The native client does the same.

Rule 2 of DESIGN.md applies: every size and colour is a theme token, named
below, never a pixel value.

Source, `app/vendor/claude-chat.js` (grep with `-a`):
- CSS 35 (`.cc-root.cc-dropping`), 183 (narrow), 250-261 (`.cc-attach`,
  `.busy`, `.cc-up` and its parts);
- 717-750: `CLAIMED`, `LAST_FOCUSED`, `DRAFT_TTL_MS`, `pruneDrafts()`;
- 969 (`apiUpload`), 992-999 (the button, its title, the hidden
  `<input type=file>`), 1052-1060 (the upload bar's parts);
- 3729-3810: `b64`, `insertPath()`, `showUpload()`, `uploadFile()`, the
  button → picker;
- 3812-3872: the drop target (`onDragOver`, `dragStillInside`,
  `onDragLeave`, `onDrop`); 3874-3918: `onPaste`; 3920-3949: the drop
  outline (`showDrop`, its 600 ms idle and window `drop`/`dragend` reset);
- 4053-4119: drafts (`DRAFT_KEY`, `DRAFT_PROJ_KEY`, `writeDraft`,
  `readDraft`, `saveDraft`, the restore at mount); 4120-4123 (`input` →
  `saveDraft`);
- `saveDraft()` callers: 2481 (`giveBack`, a queued message given back),
  2506 (`putBack`, cancelled queue), 2828 (`doSend`), 3752 (`insertPath`),
  4764 (scheduled: clears the box when it still holds the scheduled text);
- 5115 (`insertText` for the host), 5128-5138 (`destroy()`: releases the
  project claim, drops the listeners; never clears a draft).

Host, `src/webapp-html.js`: `uploadWithProgress()` 10277-10303 (XHR with
upload progress; the non-JSON error message); the workbench-wide drop
handler on `.terminal-wrap` 9905-9932; `toast()` 5473-5479.

Server, `src/webapp.js`: `POST /api/attachments/upload` 7591-7606;
`ensureAttachmentsDir()` 144-149 (`crundi_attachments`, gitignored);
unknown `/api/` paths 4694; uncaught errors → 500 at 9358-9361.

## Screenshots

`specs/attachments-drafts/`, from the web UI at 1400×860 (layout: one wide
chat on top, a wide and a narrow chat under it). `-2x`: the region ×2.

| File | State |
|---|---|
| `web-rest-wide` | the three chats at rest: attach button in each composer, no upload bar |
| `web-dropping` | a file dragged over the wide "Second chat": 2px dashed accent outline just inside the chat (log + composer; not the header). The caret sits after "and" |
| `web-dropping-narrow` | the same over the narrow chat |
| `web-drag-over-header` | a file over a chat pane's **header**: the web's whole-workbench solid outline (see "Drop on a header", a named difference) |
| `web-progress-wide` | uploading (42%): the bar between the input row and the meta row; name · bar · "42%"; attach dimmed (busy) |
| `web-saving` | the body sent, the server storing it: the bar full, "saving" in place of the number |
| `web-attached` | done: the path put in at the caret, spaced ("Please read and `<path>` summarise"); the box grew to two lines; toast "Attached: 1794629463746-notes.txt"; bar gone |
| `web-progress-narrow-long-name` | a narrow chat uploading a long file name (73%): the bar under the attach/clock/Send row; the name ellipsized |
| `web-paste-narrow` | a pasted screenshot (no file name) uploading as `image.png`, 55% |
| `web-multi-drop-progress` | two files dropped at once on the wide top chat: one bar, showing the file that reported last (photo.jpg 60%) |
| `web-multi-drop-done` | both paths in the box, space-separated, in the order the uploads finished |
| `web-error-network` | toast (error) "Upload failed: Network error" |
| `web-error-old-server` | toast (error) "Upload failed: Not found" (an `/api/` path the server doesn't know); the bar at 0% still showing for its last moment |
| `web-error-too-big-proxy` | toast (error) for a proxy's HTML 413: "Upload failed: The upload was answered with HTTP 413 (413 Request Entity Too Large 413 Request Entity Too Large nginx), not by Crundi. Something between this device and the server may have blocked it." (wraps; the toast is at most a pane wide) |
| `web-drafts-after-reload` | the page reloaded: every chat's unsent text back in its box, the boxes grown to fit |
| `web-draft-adopted` | the chat owning the project's draft was closed; a new chat opened in the project starts with that draft |

## 2.7 Attachments

### The parts

```
cc-root (the chat: log + composer; not the pane header)   ← drop target, dashed outline
  composer
    wrap
      vline        (voice, not this screen)
      inrow        box · actions (attach · [Stop] · Send)   — 2.5
      up           the upload bar, hidden unless uploading
        name       the file's name
        bar        track, with the fill inside
        pct        "42%" | "saving"
    meta           — 2.5
```

Narrow pane: `inrow` wraps (box, then attach · clock · [Stop] · Send, 2.5);
the upload bar still comes right after it, before meta.

### Upload bar

| Part | Look |
|---|---|
| row | full composer width; a row, items centred vertically, gap `sp-8`; padding `sp-4` top, `sp-2` left and right, 0 bottom. Shown only while uploading (and `menu-linger` after) |
| name | `text-11`, text-muted, one line, ellipsized; stretches with weight 1 |
| bar (track) | stretches with weight 2 (twice the name's share); `sp-4` high, `r-pill`, bg-tertiary, clips the fill |
| fill | accent, `r-pill`, full height; its width is the fraction sent; width changes animate over `motion-fast` |
| pct | `text-11`, text-muted, tabular figures, right-aligned, at least `sp-32 + sp-2` wide, never shrinks. "0%"…"99%" (rounded), then "saving" once the whole body is sent |

Name and pct sit on the text's line (`text-11` × 1.55 line); the row is
`sp-4` + that line high.

### The attach button

2.5 drew it (`control` square, paperclip, hover accent, title "Attach a
file (uploads to crundi_attachments)"). Now:
- **Click:** opens the system's file picker (a native dialog, never blocking
  the UI thread). **One file** (the web's `<input type=file>` has no
  `multiple`), any type, no filter. Cancel does nothing.
- The picked file is **always uploaded**, even when the server is on this
  machine (unlike a drop; that is the web's rule too).
- **Busy** while an upload runs: opacity `o-soft`, inert (no hover, no
  click). See "Several at once" for when it ends.

### Upload (one file)

`uploadFile(file)` for the picker, a paste, and a drop that must upload:
1. Name: the file's own name; a pasted picture with no name is
   `image.<subtype>` from its MIME type (`image/png` → `image.png`; no type →
   `image.png`).
2. Attach busy; the bar shows `<name>` at 0%.
3. Read the whole file, base64 it, `POST /api/attachments/upload`
   `{project, name, data}` (the chat's project alias). There is no size
   limit on either side; the whole file is sent in one JSON body.
4. While the body goes out: the fill and the number follow the bytes sent.
   When the last byte is sent: full, "saving".
5. The answer:
   - `{ok:true, path, name}` → the **path** goes into the box at the caret
     (`insertPath`, below); toast **"Attached: `<name>`"** where `<name>` is
     the server's stored name (`1794629463746-notes.txt`), falling back to the
     file's name;
   - `{ok:false, error}` (any status) → error toast **"Upload failed:
     `<error>`"** (`?` when there is none); nothing is inserted;
   - not JSON (a proxy or firewall answered) → error toast **"Upload failed:
     The upload was answered with HTTP `<status>` (`<said>`), not by Crundi.
     Something between this device and the server may have blocked it."**
     where `<said>` is the body with tags stripped, whitespace collapsed,
     the first 80 characters; with an empty body the bracket becomes
     ", and nothing else": "…HTTP 502, and nothing else, not by Crundi…";
   - no answer (connection failed) → **"Upload failed: Network error"**.
6. Always, success or not: attach no longer busy; the bar stays
   `menu-linger` more, then hides (so a fast upload doesn't flicker).

If the chat has no project (not possible for a live chat today), the web
toasts "No project for this chat" (error) and does nothing; keep it.

**An older server.** The endpoint is old, so a server without it is
unlikely; such a server answers `404 {"error":"Not found"}` and the web
says "Upload failed: Not found". Native follows the backwards-compatibility
rule for a missing endpoint: a 404 whose error is "Not found" →
**"Upload failed: not supported by this server"** (a named difference). A
404 "Project not found" is a real answer and is shown as is.

### Where the path goes (`insertPath`)

Into the message box, **at the caret, replacing any selection**; never on
the end by default:
- before = text before the selection start, after = text after its end;
- insert = (a space, if `before` is not empty and doesn't end in
  whitespace) + path + (a space, unless `after` starts with whitespace);
- the caret goes right after the inserted text (after its trailing space),
  the box grows to fit (up to `composer-max`), **gets focus**, and the draft
  is saved.

So "Please read and|summarise" (caret after "and", no space after it)
becomes "Please read and `<path>` summarise"; "Look at this: |" becomes
"Look at this: `<path>` " (trailing space); an empty box becomes
"`<path>` ". The caret is kept per chat while the upload runs: if the
person keeps typing, the path lands at the caret as it is when the upload
finishes.

The path is the server's **absolute** path,
`<project path>/crundi_attachments/<ms>-<safe name>`
(`/tmp/crundi-devtest-4810/proj/crundi_attachments/1794629463746-notes.txt`).
It is inserted raw: not quoted, not wrapped in a link or chip.

Slint: `TextInput` keeps the caret and the selection as byte offsets
(`cursor-position-byte-offset`, `anchor-position-byte-offset`); the
insertion works on those, and must land on character boundaries.

### Dropping on a chat

The target is the chat under the pointer: its log and composer (the
`cc-root`). The pane header is not part of it in the web (see "Drop on a
header").

While something droppable is dragged over the chat: a **dashed outline**
just inside the chat's edge: `DashedBox`, thick `sp-2`, dash `sp-6`, gap
`sp-4`, accent, square corners, no fill, drawn over the log and composer
(nothing moves). It goes when the drag leaves the chat (by pointer
position, not by which element), is dropped, ends anywhere, or is
cancelled (Esc), and if no drag movement arrives for a while (the web's
600 ms idle).

The drop:
- **Files** (from the OS):
  - when the **server runs on this machine** (the server address's host is
    `localhost`, `127.0.0.1`, `::1` or `[::1]`): nothing is uploaded,
    pictures included; each file's local path goes in. All the paths
    together, joined by single spaces, go in with one `insertPath`; toast
    "File added" (one) or "`N` files added" (several), not an error;
  - otherwise each file is **uploaded** as above (in parallel; see "Several
    at once"). Never both for one file.
- **Text** (a workbench panel's drag, e.g. the Files tab's
  `[File Path: C:\p\x.js]`; any `text/plain`): inserted as it is with
  `insertPath`; no toast. Native: the drag sources (Files, Git, Kanban,
  Mindmap, Media) don't exist yet; wire this when they do. Nothing else to
  build now.

**Drop on a header (named difference).** In the web a file dropped on a
chat pane's header is caught by the workbench (a solid accent outline round
the whole workbench, `web-drag-over-header`) and sent to the terminal input
bar, which a chat workbench hides, so the file is uploaded and its path
lost. Native: a drop anywhere on a chat pane, header included, goes to
that chat, with that pane's dashed outline. No whole-workbench outline.

Slint/winit (to check, then name in PLAN.md if it falls short): winit 0.30
gives `HoveredFile`, `HoveredFileCancelled` and `DroppedFile(path)` (one
event per file) through `slint::winit_030::WinitWindowAccessor`, but **no
pointer position** with them. The target is the chat pane under the last
pointer position known; where the platform sends no pointer movement
during an OS drag, the outline follows the pointer as far as it can be
known, and the files go to the pane last under the pointer, else the
focused chat. Collect the `DroppedFile` events that arrive together (same
event-loop turn) as one drop, so several files give one toast and one
insertion.

### Pasting

A paste of **files or a picture** uploads the first one (only the first,
as the web) and is consumed (no text is pasted); any other paste is the
ordinary text paste.
- Which chat: the one whose box has focus; the web also catches a paste
  outside the field when this chat was the last one focused (the last box
  the caret was in). Native: Ctrl+V (Cmd+V on macOS) and Shift+Insert while
  focus is in the chat pane (its box or its log).
- Clipboard: a picture (bitmap) → encode as PNG, name `image.png`; a copied
  file (from a file manager) → its contents under its own name (always
  uploaded, even on the same machine; the web's paste has no local path).
- Slint has no clipboard image API: read it with a clipboard crate (e.g.
  `arboard`) in a `FocusScope` `capture-key-pressed` before `TextInput`
  handles the key; if there's no picture or file, let the key through.

### Several at once

The web runs each upload on its own but shares one bar and one busy flag:
- the bar shows whichever upload reported progress last (name, fill,
  number);
- paths go in as each upload finishes, in finishing order, each with its own
  `insertPath` (so they come out space-separated) and its own toast;
- (web accident) the first to finish clears busy and starts the bar's
  hide while the others are still going; their next progress shows it again.

Native: the same bar and insertion, but attach stays busy and the bar stays
until the **last** upload of the chat finishes (then `menu-linger`, then
hidden). A named difference: the web's flicker and early un-busy are
accidents of a shared flag.

Uploads belong to their chat: switching away and back keeps the bar; a
chat closed mid-upload drops the insertion (the file still lands on the
server).

## 2.8 Drafts

### What is kept

Only the **message box's text**, per chat. Not the caret, not the
selection, not the box's scroll, not the upload in progress, not the
slash-menu. Answer drafts for questions (`crundi_chat_ans_*`) are the
question flows' (their own spec); the same pruning covers them.

### Where (web)

`localStorage`, as `{"v": "<text>", "t": <ms when written>}`:
- `crundi_chat_draft_<chat id>`: the chat's own draft;
- `crundi_chat_draft_p_<project alias>`: the project's copy, so a draft
  outlives the chat id (a server restart relaunches the chat under a new id;
  a closed chat's text can be picked up by the next chat in the project).

A draft written by an older web as bare text (no `{`) is still read as is.
Empty text **removes** the key; nothing stores an empty draft.

### When it is saved

On every change of the box: typing (each input event), `insertPath`
(attach, drop, paste), a queued message given back or a cancelled queue put
back (2.9), sending (clears), scheduling (clears, see below). Not while the
box shows a live voice transcript (that is not the draft).

Writing (`saveDraft`):
1. the chat's own key ← the box's text (removed when empty);
2. the project key, only if **no other open chat has claimed it** (claims
   live in memory, one owner per project):
   - text not empty → the project key ← the text, and this chat claims it;
   - text empty and this chat owns it → the project key is removed and the
     claim released;
   - owned by another chat → left alone.

### When it is cleared

- **Send:** the box empties and the draft is saved empty → own key removed,
  and the project key removed if this chat owns it. (Native's send already
  clears the box; a refused send puts the text back and saves it again.)
- **Schedule** (2.10): when the schedule is accepted and the box still holds
  exactly the scheduled text (trimmed), the box empties and the draft with
  it.
- **Closing a chat does NOT clear anything** (the web's `destroy()` keeps
  both keys, only releasing the claim). Its own key is stranded (pruned
  later); the project copy stays for the next chat in that project to adopt
  (`web-draft-adopted`). The native client today removes the draft on close
  (`src/app.rs` close_pane `a.drafts.remove`): change that.

### Restoring (when a chat's composer is built)

1. Prune (below).
2. The chat's own draft; if it has none, the project's copy, but only if no
   open chat has claimed it.
3. If one was found: it goes in the box, the box grows to fit, this chat
   claims the project key, and it is saved again (re-anchored under this
   chat's id, refreshing both timestamps).

So: switching between chats or projects, closing and reopening a pane, a
page reload or a client restart bring each chat's text back
(`web-drafts-after-reload`); after a server restart, or for a chat opened
after one with a draft was closed, the project copy is adopted by the first
chat built with no draft of its own.

Order matters when several chats with drafts are built at once: each one
with a draft claims the project key in turn, so the **last built** owns it
and its text is the project copy (web: the last mounted).

### Pruning

At every restore, every draft (and answer and dismissed-agent record:
`crundi_chat_draft_*`, `crundi_chat_ans_*`, `crundi_chat_dis_*`) whose
timestamp is older than **30 days** is removed. Entries without a timestamp
are left alone.

### Native

In the client's saved settings (`src/store.rs`, `Saved`), **per server**
(the server address) and per key:

```
drafts: { "<server>": { "chat:<id>": {v, t}, "project:<alias>": {v, t} } }
```

- Same rules as above: own key, project key with an in-memory claim per
  server and project, empty removes, 30-day prune at start (and when a
  composer is built), last-built owns.
- Every change updates memory at once (the pane rows already share
  `drafts` in `src/app.rs`); the file is written at most every
  `menu-linger` while typing and on quit, never once per keystroke.
- A different server signed in to has its own drafts; signing out keeps
  them.
- Nothing goes to the server: drafts have no endpoint.

## Data and endpoints

| What | Where |
|---|---|
| upload | `POST /api/attachments/upload`, JSON `{"project":"<alias>","name":"<file name>","data":"<base64>"}`, `Authorization: Bearer` |
| local drop | no request (the path is inserted) |
| drafts | none (client-only) |
| live events | none: nothing about attachments or drafts arrives over `/ws` |

The upload's answers:
- `200 {ok:true, path:"<abs path>", name:"<ms>-<safe>", size:<bytes>}`;
  `safe` = the name's basename with every run of characters other than
  `[A-Za-z0-9_.-]` turned into `_`, its last 80 characters, else `file`; the
  folder is `<project>/crundi_attachments` (created on demand, added to the
  project's `.gitignore`);
- `404 {ok:false, error:"Project not found"}`;
- `400 {ok:false, error:"Missing name or data"}`;
- `200 {ok:false, error:"<fs error>"}` (could not write);
- `500 {error:"Internal error"}` (a body that isn't JSON, a request that
  broke);
- an older server, an unknown path: `404 {error:"Not found"}`;
- something in between (proxy, firewall): any status with a non-JSON body.

Progress is reported while the request body is written (bytes sent / body
length), then "saving" until the answer. Native: stream the JSON body in
chunks through the HTTP client (`ureq`) and count the bytes, off the UI
thread.

## Tokens

Existing only: bg-tertiary, text-muted, accent, sp-2/4/6/8/32, r-pill,
text-11, o-soft, motion-fast (the fill), menu-linger (the bar's linger
and the draft write throttle), `DashedBox` (the drop outline), the toast.
No new tokens.

## How to bring each state about (for the end-to-end test)

- Server: `tools/test-server.sh <port>` (seeded project "Demo", alias
  `demo`, one chat; more with `POST /api/ui-sessions/create {project}`).
- A file: any temp file. The upload's result is on disk under
  `/tmp/crundi-devtest-<port>/proj/crundi_attachments/`.
- Same machine vs remote: the test server is on `localhost`, so a drop
  inserts the local path. To make a drop upload, connect to the server as
  `http://127.0.0.2:<port>` (loopback, but not one of the four names), or
  give the driver a switch.
- Errors: a project alias that doesn't exist (404 "Project not found"); for
  the network error, stop the server mid-test or point at a closed port; a
  non-JSON answer needs a stub (a tiny HTTP server answering HTML 413).

Driver commands the native test driver (`src/testctl.rs`) needs, as
proposals: `attach <path>` (as if picked in the dialog), `drop <pane>
<path>…` (as if dropped from the OS), `paste-image <pane> <png>` and
`paste-file <pane> <path>` (as if on the clipboard), `caret <pane>
<offset>` (to check the insertion point); `state` already reports each
pane's `draft`; add the upload bar (`upName`, `upPct`, `busy`).

End-to-end tests (test/phase2.mjs):
1. **Attach:** type "Please read and summarise", put the caret after "and",
   `attach` a temp file: the upload reaches the server (the file is in
   `crundi_attachments` with the same bytes), the box reads "Please read and
   `<abs path>` summarise", the toast says "Attached: `<ms>-<name>`",
   attach is not busy and the bar is gone after the linger.
2. **Drop, same machine:** `drop` two files: both local paths go in,
   space-separated, nothing uploaded, toast "2 files added".
3. **Drop, remote** (127.0.0.2): `drop` a file: it uploads; the path is the
   server's.
4. **Paste:** `paste-image` a PNG: uploads as `<ms>-image.png`, its path in
   the box.
5. **Errors:** a missing project → "Upload failed: Project not found"; a
   closed server → "Upload failed: Network error"; nothing inserted, attach
   back to normal.
6. **Draft across a restart:** type a draft in a chat, wait past the write
   throttle, quit the client, start it again with the same config dir: the
   box holds the draft (`state` → `draft`), and `session.json` holds it
   under the server and `chat:<id>`.
7. **Draft cleared on send:** send it: the box and the saved draft are
   empty (both keys).
8. **Switching:** drafts in two chats, switch the pane between them (or
   close and reopen a pane): each comes back.
9. **Adoption:** close the chat holding the project's draft, open a new
   chat in the project: it starts with that text.
10. **Prune:** a saved draft with a timestamp 31 days old is gone after a
    start; one 29 days old stays.

## Checklist

Attach button
- [ ] Click opens the system file picker (one file, no filter), without
      freezing the window; cancel does nothing.
- [ ] A picked file always uploads (also on the same machine).
- [ ] Busy (o-soft, inert) while any upload of the chat runs.

Upload bar
- [ ] Hidden at rest; shown under the input row (wide) or under the
      attach/clock/Send row (narrow), before the meta row.
- [ ] name (text-11 muted, ellipsized, weight 1) · track (sp-4 high, pill,
      bg-tertiary, weight 2) with an accent fill · pct (text-11 muted,
      tabular, right, ≥ sp-32 + sp-2).
- [ ] "N%" follows the bytes sent; "saving" once the body is sent.
- [ ] Stays menu-linger after the last upload ends, then hides.

Success
- [ ] Path inserted at the caret / replacing the selection, with the
      spacing rules; caret after it; box grows; box focused; draft saved.
- [ ] Toast "Attached: `<server name>`".
- [ ] The path is the server's absolute path, raw; no chips anywhere.

Drop
- [ ] Dashed accent outline (DashedBox, sp-2 / sp-6 / sp-4) just inside the
      chat while a drag is over it; goes on leave, drop, end, Esc.
- [ ] Same machine (localhost, 127.0.0.1, ::1, [::1]): local paths,
      space-joined, one insertion, "File added" / "N files added".
- [ ] Otherwise: each file uploads.
- [ ] A drop on a chat pane's header goes to that chat (named difference).
- [ ] Several files from one drop are one drop.

Paste
- [ ] Ctrl+V / Cmd+V / Shift+Insert with a picture: uploads `image.png`; a
      copied file uploads under its name; only the first.
- [ ] Text paste unchanged.

Errors
- [ ] `{ok:false, error}` → "Upload failed: `<error>`" (or "?").
- [ ] Non-JSON → "Upload failed: The upload was answered with HTTP …, not by
      Crundi. Something between this device and the server may have
      blocked it." (tags stripped, 80 chars; ", and nothing else" when empty).
- [ ] No connection → "Upload failed: Network error".
- [ ] 404 "Not found" → "Upload failed: not supported by this server".
- [ ] Error toasts are the red toast; nothing is inserted; attach and bar
      recover.

Drafts
- [ ] Text kept per server and chat in `Saved`, plus the project copy with
      the claim rules.
- [ ] Saved on every change (typing, insert, give-back, put-back), written
      to disk throttled and on quit.
- [ ] Cleared on send and on a matching schedule; NOT on close.
- [ ] Restored when a chat's composer is built: own, else unclaimed project
      copy; the box grows to fit; last built owns the project copy.
- [ ] 30-day prune at start.
- [ ] Survives switching, closing and reopening a pane, a client restart;
      adopted by a new chat after a close or a server restart.

Tests
- [ ] The ten end-to-end tests above pass in test/phase2.mjs; phase1 and
      drive still pass.
