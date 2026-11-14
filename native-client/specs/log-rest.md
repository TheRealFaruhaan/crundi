# 2.16 The rest of the log

Everything in a chat's conversation (the log, `.cc-log`) that the earlier
screens did not cover:

1. **Claude's reply text** (`.cc-assistant`): the markdown it is drawn from,
   paragraph spacing and line height, inline code, links and file paths.
2. **Code blocks** (`pre`) and **the copy button** (`.cc-copyhint`) that
   shows by a hovered code block, code span, link or path.
3. **Links**: what a click and a right-click do.
4. **Images and media** Claude shows with `show_image`, `show_video`,
   `show_audio`, `show_file` and `show_embed`, under that tool's card, and
   the picture viewer.
5. **Notices** (`.cc-notice`), including the long-history line.
6. **Errors** (`.cc-error`) and the end-of-turn line (`.cc-result`).
7. **Following the bottom** (stick-to-bottom) and **the jump button**
   (`.cc-jump`). This includes F87.
8. **Selection** ("Selection stays in the conversation").

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.

Source:
- `app/vendor/claude-chat.js`
  - CSS 31-41 (`.cc-root` 13px / 1.55, `.cc-log` padding, `.cc-entry` gap),
    60-85 (user bubble, `.cc-assistant` p / ul / ol / li / h1-h3 / a /
    `.cc-path` / code / pre / blockquote, `.cc-cursor`), 77-81
    (`.cc-copyhint`), 169-172 (`.cc-result`, `.cc-notice`, `.cc-error`),
    297-308 (`.cc-jump`).
  - `md()` 541, `inline()` 594, `linkify()` 614, `FILE_EXT` 643,
    `looksLikePath()` 646.
  - The jump button 1196-1202; path clicks 1224-1229; the copy hint
    1231-1288 (`copyTargetOf`, `copyTextOf`, `placeCopyHint`).
  - `atBottom()` 1402, `followingBottom()` 1409, `scrollDown()` 1410; the
    log's scroll handler 1427-1439; `jumpAway()` / `syncJump()` /
    `noteBelow()` / the jump click 1446-1460; "Selection stays in the
    conversation" 1461-1640; `restoreScroll()` and the resize observer
    1642-1680.
  - `paint()` 1744-1812 (`assistant-text`, `result`, `notice`, `error`,
    `gone`); `toolNode()`'s `toolExtra` 1937-1951 (the media under a call).
  - `addEntry()` 4589 and the `patch` / `delta` frames 4975-5008.
- `src/webapp-html.js`: `openPathFromChat()` 8414, `widgetToolExtra()` 8579,
  `webLinkOf()` / `openLinkMenu()` and the link click and context-menu
  handlers 12410-12440, `openCtxMenu()` 12162, `crundiDownload()` 12082.
- `app/vendor/crundi-widgets.js`: media CSS 133-200, `watchMedia()` 765,
  `zoomImage()` (the viewer) 821, `sizedBox()` 958, `imageItem()` 972,
  `playerItem()` 1016, `embedItem()` ~1200, `fileItem()` 1250, `mediaNode()`
  1270, `toolMedia()` 1690, `itemsMedia()` 1705.
- `app/main.js`: `setWindowOpenHandler` 197 (http(s) to the system
  browser), `will-download` 511 (a save dialog in Downloads).
- Server `src/webapp.js`: `GET /api/chat-media` 4749, `GET
  /api/chat-media/file` 4773, `GET /api/files/resolve` 7806, the `show_*`
  MCP tools 8385; `src/chat-media.js`.

## Screenshots

`log-rest/web-<state>.png` at 1400×860, one wide pane (the two `narrow`
shots: a 900 wide window, the pane 650). A `-2x` file is the named region
cropped and doubled. The reply text, notices and errors were injected
through the page's WebSocket (`entry` / `delta` frames); the media are real:
stored on the server by calling `show_image` / `show_video` / `show_audio` /
`show_file` / `show_embed` through `/api/mcp/call`, with their results
injected as tool entries.

| State | What it shows |
|---|---|
| `web-reply-markdown` | **A reply with everything**: a `##` heading ("The fix", bold), a paragraph with **bold**, *italic*, a `[text](url)` link and a bare URL that wraps (links break anywhere), a code span that is a path (`src/server.js`: chip plus dotted underline), an absolute path linked (`/tmp/…/client.js:12`), a plain code span, a code block, a numbered list, a quote, a rule (`---`), a last paragraph. A user bubble above with a link in it (underlined, the bubble's text colour). |
| `web-copy-code` (+`-2x`) | **Copy button on a code block**: hovering the block puts the button inside its top-right corner. |
| `web-copy-code-done` (+`-2x`) | **After a click**: the button shows a green tick; the toast "Copied" at the top right. |
| `web-copy-span` (+`-2x`) | **Copy button by a code span**: just after its end, centred on its line. |
| `web-copy-link-wrapped` (+`-2x`) | **By a link that wraps**: after the end of its last line. |
| `web-link-menu` | **Right-click on a link**: the menu with "Open in new tab" (the PC app says "Open in browser") and "Copy link", at the pointer. |
| `web-notices-errors` | **Notices and errors**: "Context compacted." (a notice), an error box ("Process error: …"), a failed turn's end ("error_max_turns" in the same box), the end-of-turn line "12.3s · $0.0421", a long error that wraps (breaking a long path), and a long notice that wraps, centred. |
| `web-jump-on` (+`-2x`) | **The jump button** after scrolling 600px up: a round button at the bottom centre of the log, a down arrow. |
| `web-jump-hover` (+`-2x`) | Hovered: accent edge, accent tint. |
| `web-jump-new` (+`-2x`) | **Something arrived below** while reading up: accent edge and an accent dot at its top right; the arrow nudges. |
| `web-media-single` | **One picture** (`show_image`, 1200×700): under the call's card, 640 wide, its caption under it. |
| `web-media-row` | **Several pictures** (three, one call): a row 220 tall, 8 apart, each with its caption, the call's caption under the row. |
| `web-viewer` | **The viewer**: the picture large on a dark backdrop; "1 / 4" top left (the chat's four pictures); ‹ › at the sides; −, 100%, +, × top right. |
| `web-viewer-zoomed` | After → and +: "2 / 4", "140%"; the side buttons hide while zoomed. |
| `web-media-video` | **A video** (`show_video`): its first frame (poster), the big play button, the bar (play, 0:00 / 0:06, seek line, sound, full screen); the caption "The test pattern · 0:06". Under it **an audio clip**: a pill with the accent play button, time, seek line, sound; "A440 tone · 0:08". |
| `web-media-audio-file-embed` | The audio clip; **a file** (`show_file`): a card with a file icon, "export.csv", "12 B · The export" and Download; **a frame** (`show_embed`, stored HTML) 640 × 200 with its title under it. |
| `web-download-tray` | **Download clicked**: the download row at the top right ("export.csv", "Saved · 12 B", a full bar, ×). |
| `web-media-gone` | **A picture no longer stored** (404): its box at its size, dashed edge, "This image is no longer stored." centred. |
| `web-media-loading` | **A picture loading** (a tall one, 600×900: 280 × 420): its box with a spinning ring; the caption already under it. |
| `web-selection` | **Selecting reply text** with the mouse: the system's selection colour over two paragraphs; the copy button still shows by the hovered code block. |
| `web-narrow` | **Narrow pane**: text and links wrap ("ws READ ME" breaks inside the link), the code block and error boxes keep the full width. |
| `web-narrow-media` | Narrow: the single picture shrinks to the pane; the row of three just fits (it scrolls sideways when it does not). |

## 1. Reply text

### The markdown (`md()`, `inline()`, `linkify()`)

Deliberately small: everything is escaped first, then these come back.

| Source | Drawn as |
|---|---|
| ```` ```lang ```` … ```` ``` ```` | a code block (see 2); the language is not shown; one trailing newline dropped |
| blank line | ends a paragraph, a list |
| lines with no blank between | **one paragraph, the line breaks kept** (`<br>`) |
| `# x` / `## x` / `### x`… | heading level +2: `#` → h3: 14 (`text-14`), weight 650 (`w-semibold`), 12 above, 6 below; `##` and deeper → h4-h6: **13 bold, no extra space** (the web styles h1-h3 only; `web-reply-markdown`'s "The fix") |
| `---`, `***`, `___` alone | a rule: full width, 2 tall, a light grey line (the browser's default `hr`; no gaps above or below) |
| `> x` | a quote line: 2px `border` bar at the left, 10 padding, `text-secondary`, 8 below. **Each `>` line is its own quote** |
| `- x`, `* x`, `+ x` | bullet list; `1. x` / `1) x`: numbered list (numbers from 1, "1."). 20 indent, items 2 above and below, the list 8 below; indented items are not nested |
| `` `x` `` | code span: mono 12, `bg-tertiary`, padding 1 × 5, radius 4 (`r-chip`). A span that **looks like a path** (see below) is also a path link |
| `[text](https://…)` | a link with that text, to that URL (http/https only) |
| bare `http(s)://…` | a link; trailing `.,;:!?)]'"` and `>` stay outside it |
| `**x**` | bold; `*x*` (after a space, a bracket or the start) italic |
| an absolute path `/a/b.js`, `/a/b.js:12`, `/a/b:12:3` | a path link |
| `./x/y`, `../x`, `a/b.ext` with a known file extension (`FILE_EXT`) | a path link ("and/or", "10/12" stay words) |
| `![alt](url)` | **not an image**: the text as it is (the web has no markdown images; pictures come only through `show_image`) |

`looksLikePath(c)` for a code span: no spaces, not a URL, no `<>&*|$\``,
at most 260 characters; either has a `/` between word parts, or is a bare
name with a known extension (`app.js`, not `obj.prop`).

### Look

| Part | Web | Token |
|---|---|---|
| Reply text | 13 × fs, line 1.55 (20.15), `text-primary`, breaks long words | `text-13`, `line-chat` |
| Paragraph gap | 8 (none after the last) | `sp-8` |
| List indent / item gap | 20 / 2 above and below | `sp-20` / `sp-2` |
| Heading (h3) | 14 / 650, 12 above, 6 below | `text-14`, `w-semibold`, `sp-12`, `sp-6` |
| Quote | bar 2 `border`, 10 in, `text-secondary` | `sp-2`, `sp-10` |
| Link | `accent-hover`, underlined (offset 2), breaks anywhere | `accent-hover` |
| Path link | `accent-hover`, **dotted** underline (offset 3); hover: solid | |
| Path code span | the chip, its text `text-primary`, dotted underline | |
| Code span | mono 12, `bg-tertiary`, 1 × 5 padding, radius 4, line 18.6 | `text-12`, `bg-tertiary`, `sp-2`×`sp-4`, `r-chip` |
| Link in a user bubble | the bubble's text colour, underlined | |
| Streaming reply | a 6 × 13 `accent` block blinking at the end (`.cc-cursor`) | |
| Entry gap | 10 below every entry | `sp-10` |
| Log padding | 12 sides and top, 4 bottom | `sp-12`, `sp-4` |

Text size (A- / A+) scales every text above (× fs); gaps and padding do not.

**PLAN "Known gaps" that belong here** (keep them named if Slint still
cannot do better, otherwise fix them):
- **Inline-code chip.** The web's chip is a plain `bg-tertiary` fill with
  radius 4 and 5px side padding; StyledText draws a faint outline and pads
  nothing (the native pads with thin spaces). Check what Slint 1.18's
  StyledText offers for the code style; if nothing, keep the gap named.
- **Paragraph line height.** The web's 1.55 applies inside a wrapped
  paragraph; StyledText has no line height, so wrapped lines sit tighter.
  Single lines, list items and paragraph gaps already match (each block is
  padded to the 1.55 box). If StyledText still has no line height, keep the
  gap named. Do **not** switch the paragraphs to plain text to get it: bold,
  links and code spans matter more.
- **Reply text size.** The web's reply text is 13 × fs; the native
  paragraphs are drawn at `text-14` (ui/chat.slint, `e.kind == "assistant"`).
  Use `text-13`, or name why not.

## 2. Code blocks and the copy button

### Code block

| Part | Web | Token |
|---|---|---|
| Box | `bg-secondary`, 1px `border-subtle`, radius 6, padding 9 × 11, 8 below | `bg-secondary`, `border-subtle`, `r-control`, `sp-10`×`sp-12`, `sp-8` |
| Text | mono 12, line 1.5 (18), `text-primary` | `text-12`, mono |
| Long lines | **do not wrap**: the block scrolls sideways (`overflow-x:auto`) | |

The native code block wraps today (`word-wrap`); it must keep lines whole
and scroll sideways (a horizontal `ScrollView`/`Flickable` with a thin
`ScrollThumb` when it overflows). Its text is selectable.

### The copy button (`.cc-copyhint`)

One button per chat, moved to whatever is hovered.

| Part | Web | Token |
|---|---|---|
| Button | 24 × 22, `bg-secondary`, 1px `border`, radius 6, shadow 0 4 12 35% black | `sp-24`×(`sp-20`+`sp-2`), `r-control`, `SoftShadow` |
| Icon | copy (two squares) 13, `text-secondary` | `icon-md` |
| Hover | icon `text-primary`, fill `bg-hover` | |
| Done | a tick, `green` | |

- **What shows it:** the pointer over a code block, a code span, a link or a
  path link **in Claude's reply**, or a link in a user bubble. Not over
  tool cards, thinking, notices.
- **Where:** a code block: inside its top-right corner, 6 in from the top and
  the right. Anything else: 3 after the end of its **last line**, centred on
  that line; if that would pass the window's right edge (less 4), it sits
  over the end of the span instead.
- **Goes:** 250 ms after the pointer leaves both the target and the button;
  at once when the log scrolls.
- **Tooltip:** "Copy code" (block or code span), "Copy path" (a path, code or
  link), "Copy link".
- **Click:** copies — a block: its whole text; a link: its **URL** (not its
  text: `[ws README](https://…)` copies the address); a path: the path as
  written (with its `:12`); a code span: its text. Then the tick and the
  toast "Copied". If the clipboard refuses: toast "Could not copy" (error).
  Pressing it does not take the focus or clear a selection.
- The tick stays until the button moves to another target.

## 3. Links and paths

- **Click a link** (left button, mouse): opens the URL in the **system
  browser** (Electron's `setWindowOpenHandler` → `shell.openExternal`), not
  in the drawer. Native: `open::that(url)` (the client already uses it).
- **Right-click a link**: a menu at the pointer (kept 6 inside the window):
  "Open in browser" (icon external-link) and "Copy link" (icon copy, toast
  "Link copied"). Menu: `bg-secondary`, `border`, radius 10, padding 5,
  shadow; items 14px, 12 padding, icon 14 `text-secondary`, gap 10, radius 6,
  hover `bg-hover`. Closes on a press outside, Esc, the window losing focus,
  a resize or any scroll. Use the client's menu widget (`HeadMenu` /
  `MenuItem` style).
- **Click a path** (`.cc-path`, text or code): opens the file in the Files
  panel: an absolute path inside a known project opens there; anything else
  is asked about with `GET /api/files/resolve?project=<chat's project>&path=`
  → `{ok, file, dir}`; not found: toast "File not found: <path>" (error);
  a folder: toast "That is a folder: <path>". **The native client has no
  Files panel until phase 3**: name the gap in PLAN (a path click does
  nothing yet; its copy button works).

## 4. Images and media

What Claude shows with `show_image`, `show_video`, `show_audio`, `show_file`
or `show_embed` (any MCP server prefix) is drawn **under that call's tool
card**, when the call is done and not an error. The call's result is JSON:

```
{ ok, shown, items: [ … ], caption?, note }
item: { kind:'image', name, filename, size, caption, w, h }
    | { kind:'video', name, filename, size, caption, w, h, duration, poster }
    | { kind:'audio', name, filename, size, caption, duration }
    | { kind:'file',  id, filename, size, ext, caption }
    | { kind:'embed', title, height, name }            (stored HTML)
    | { kind:'embed', title, provider, src, url, w, h, height, width } (https frame)
legacy (1.19.19): { images: [ { name, caption } ] }  → kind image
```

Only names matching `^[0-9a-f]{16}(.poster)?.(png|jpg|gif|webp|mp4|webm|mov|mp3|wav|ogg|m4a|flac|weba)$`
(embed: `^[0-9a-f]{16}.embed$`, file ids `^[0-9a-f]{16}$`) are used;
captions and names are text, never markup.

### Layout (`mediaNode`)

| Part | Web | Token |
|---|---|---|
| Under the card | 6 above, 2 below | `sp-6`, `sp-2` |
| Row | side by side, 8 apart, scrolls sideways when too wide, 4 below; files and audio stack instead | `sp-8`, `sp-4` |
| Item | the box, then its caption 4 below | `sp-4` |
| Box | 1px `border`, radius 8, `bg-secondary`, clipped | `border`, `r-field`, `bg-secondary` |
| Size, one item | width `min(w, 420·w/h, 640)`, height from `w/h` (never taller than 420, never wider than 640, never wider than the pane) | |
| Size, several | **220 tall**, width `220·w/h` | |
| Size unknown | 16:9, 420 wide (one) or 391 (several) | |
| Item caption | 11.5, `text-secondary`, line 1.35, at most 440 wide, wraps | `text-12`, `text-secondary` |
| Call caption (several items) | the same, under the row, 4 above | `sp-4` |
| Image caption | its `caption` | |
| Video / audio caption | `caption` (or the file name) · `m:ss` duration | |

The box has its final size **before** anything loads, so the log never
jumps when a picture arrives.

### Picture

- **Loading** (`web-media-loading`): a ring 34 wide, 3 thick (white 18%,
  its top `accent`), turning (0.8 s), centred in the box. Then the picture
  fades in (0.15 s), fitted inside the box (`contain`).
- **Gone** (the server answers 404): the box keeps its size, its edge dashed;
  "This image is no longer stored." centred, 11.5 `text-muted`.
- **Load when near**: a picture loads when it comes within about a screen
  (900) of the view and is let go when far (4500). Native: load when the
  row is realised (the ListView already does this), drop the decoded image
  when it is far, keep the size.
- A network failure is not "gone": it retries the next time it comes near.
- **Click a picture** (pointer: zoom-in): the viewer.

### The viewer (`zoomImage`)

| Part | Web | Token |
|---|---|---|
| Backdrop | the whole window, black 86% | a theme colour (darker than `scrim`) |
| Picture | fitted inside 16 padding, centred | `sp-16` |
| Bar (top right, 10 in) | pill, padding 3, gap 2, `bg-secondary`-like at 88%, `border`: **−** "Zoom out", **100%** "Back to fit" (`text-secondary`, at least 52 wide), **+** "Zoom in", **×** "Close" | `r-pill` |
| Bar buttons | 34 round, mono 12 / 600, hover white 10% | |
| Counter (top left, 14 in) | "2 / 4", pill, mono 12 / 600, `text-secondary` | |
| Side buttons | 44 round, 12 from the sides, centred vertically, ‹ "Previous picture" / › "Next picture"; hidden while zoomed | |

- **Gallery**: the chat's pictures in order (every picture in this pane's
  log); counter and side buttons only when there are two or more. ← / →
  and the side buttons move (wrapping round); each step resets the zoom.
- **Zoom**: + / = (×1.4), − (÷1.4), 0 (fit), the bar's buttons, Ctrl
  (or ⌘) + wheel at the pointer, a double click (2.5× at the point, or back
  to fit). From 1× to 8×; the 100% button shows the scale.
- **Move** when zoomed: drag, or the wheel; never further than the
  picture's edge plus 40.
- **Close**: Esc, ×, or a click on the backdrop (not on the picture).
- Native: a window-level overlay over everything (like the secret-request
  dock, specs/secret-requests.md), keys taken while open.

### Video and audio (`playerItem`)

- **Video**: the box sized like a picture; its poster frame
  (`GET /api/chat-media?…&name=<poster>`); a big round play button in the
  middle (56, black 55%, white 25% edge); a bar along the bottom over a
  dark gradient: play/pause (32 round), "0:00 / 0:06" (mono 11.5 / 500),
  the seek line (4 tall, 6 on hover, `accent` fill, buffered part lighter,
  a white 12 knob on hover), sound on/off, full screen. Nothing is fetched
  until play is first pressed. While playing and idle the bar hides.
  Buffering: a white ring 48 in the middle.
- **Audio**: a pill at most 440 wide, 50 tall, `bg-card`, `border`; the
  play button 38 round in `accent` with a white icon; time; seek line
  (`border` track); sound.
- Keys on the seek line: ← / → step. Click on the line: jump there.
- **Slint has no media playback.** Name it in PLAN (Known gaps), with the
  closest equivalent: draw the video box with its poster, the big play
  button and the caption (and the audio pill with its time and caption),
  and on play fetch the file (`GET /api/chat-media?session=&name=`, with
  the auth header) to a temporary file and open it in the system's player
  (`open::that`). The Manager decides if a player library is wanted later.

### File (`fileItem`)

| Part | Web | Token |
|---|---|---|
| Card | at most 460 wide, padding 9 × 10, gap 10, `bg-card`, 1px `border`, radius 10 | `sp-10`, `r-card` |
| Icon box | 34, `bg-primary`, `border`, radius 8, a file icon `text-secondary` | `r-field` |
| Name | 13 / 600 `text-primary`, one line, elided | `text-13`, `w-semibold` |
| Line under | "12 B · The export" (size, then the caption), 11.5 `text-secondary`, elided | `text-12` |
| Download | 28 tall, padding 4 × 10, gap 5, `bg-tertiary`, `border`, radius 6, 12px, a download icon + "Download" | the client's small button |

**Download**: Electron asks where to save (a save dialog, the system's
Downloads folder and the file's name filled in), then a download row shows
at the top right: the name, "Saved · 12 B" (or progress, "Starting…"), a
bar, × to cancel. Native: the system save dialog (`rfd` or the platform's),
then stream `GET /api/chat-media/file?session=&id=` (auth header) to that
path; while it runs and when done, a toast-sized row with the name and
"Saved · <size>"; failure: the server's error (410 "<name> is no longer
where it was.") as an error toast.

### Frame (`embedItem`, `show_embed`)

A sandboxed frame: stored HTML (`GET /api/chat-media?…&name=<x>.embed`
answers the HTML as text) or an https page (`src`); the box `width` × 
`height` (or from `w`/`h`); caption "title · caption" (or the provider, or
"Embedded HTML"), plus "Open" / "Open on <provider>" (`accent-hover`) when
it has a `url`. **Native: frames need CEF**, which DESIGN keeps for browser
panels: name the gap; draw the box with the caption and, inside, "Open"
(a stored HTML frame opens the HTML in the browser drawer or the system
browser; an https one its `url`/`src` in the system browser).

## 5. Notices

Centred, 11.5 italic `text-muted`, wraps. Server texts:
- "— earlier messages in this conversation —" (**the long-history line**:
  `restoreHistory` puts it between the restored transcript and the new
  session's messages);
- "Continuing your previous conversation. Claude has the full context; only
  recent messages are shown here." (after a resume);
- "This conversation stopped waiting for an answer. …" (restored while a
  question was open);
- "Conversation compacted" (from history), "Context compacted." (live);
- "Starting Claude…" (patched to an error "Claude failed to start: …" on
  failure).

Token: `text-12` (or `text-11`; the web's is 11.5) **italic**, `text-muted`,
centred. The native notice today is not italic.

(The queue drawer's "Shown trimmed ·" hint, `markTrimmed()`, belongs to 2.9
and is already specced there.)

## 6. Errors and the end of a turn

- **Error** (`kind:'error'`, and a `result` whose `subtype` is not
  `success`, showing its `text` or else the subtype, e.g. "error_max_turns"):
  a box, full width: `red` text 12, `red-dim` fill, **1px `red` edge**,
  radius 6, padding 7 × 10, long words break. Also the client's own errors
  (a failed send, a failed answer) as the same box.
  The native draws `error-text` on `error-edge` today, and a failed result
  as centred red text: use `red` / `red`, and the box for a failed result.
- **End of turn** (`result`, success): centred 11 `text-muted`, padding 3,
  a dashed `border-subtle` line above; "12.3s · $0.0421" (only the parts it
  has; empty when neither).
- `kind:'gone'`: drawn as nothing.

## 7. Following the bottom, and the jump button

### The web's rules

- `stick` (follow the newest message) starts true.
- **Only the reader's scrolling changes it**: on every scroll of the log
  (while the log is visible and has not just changed height),
  `stick = (distance to the bottom < 60)`.
- **When content changes** (a new entry, a patch, a streamed delta, the
  history): if `stick` (or the log is already within 60 of the bottom), the
  log goes to the very bottom, at once (no animation).
- The log getting shorter or taller (window resize, a panel opening) while
  `stick`: back to the bottom. Shorter while reading up: what was at the
  bottom of the view stays there.
- A pane shown again (moved, unhidden): back to the bottom if `stick`,
  else where the reader was.

Measured on the web, from the bottom: three wheel steps up (100 each) left
the log 100, 200, 300 from the bottom, and a delta arriving then **did not
move it**. A 40px step (a trackpad flick) stayed within 60, so the next delta
brought it back to the bottom.

### F87 (native)

From the bottom, single wheel steps up snap back. The native re-sticks in
`changed content-height` while `stick` is true, and the ListView's
**estimated** content height changes as rows come into view, so a step up
is undone. The fix, as the web does it:
- `stick` is set **only** from the reader's scroll (`scrolled`, i.e. wheel,
  drag, the scrollbar, keys), with the web's threshold: within
  `sp-48 + sp-12` (60) of the bottom.
- Scroll to the bottom **only when content arrives** (the app bumps a
  counter on every entry / patch / delta / history it applies) and
  `stick` is true; never on a content-height change alone.
- After the scroll to the bottom, re-check once on the next frame (the
  ListView's estimate settles as the last rows are realised), still only
  for that content change.

### The jump button (`.cc-jump`)

| Part | Web | Token |
|---|---|---|
| Button | 38 round, centred, 10 above the log's bottom (over the log, under the composer's top), `bg-secondary`, 1px `border`, shadow 0 6 18 40% | `sp-40` (one step), `sp-10`, `SoftShadow` |
| Icon | arrow down 18, stroke 2.2, `text-primary` | `icon-lg` |
| Hover / focus | `accent` edge, `accent-dim` fill | |
| New | `accent` edge; a 10 `accent` dot with a 2px `bg-primary` ring at the top right (−2, −2); the arrow nudges down every 1.6 s | `sp-10`, `sp-2` |
| Tooltip | "Go to the newest message" | |

- **Shows** when the reader is more than `max(240, a third of the log's
  height)` from the bottom; hides again inside that. Appears with a short
  rise and grow (0.22 s, a little overshoot), the arrow dropping in.
- **New**: when an entry arrives (not a `gone` one) while `stick` is false
  and the log is more than 60 from the bottom. Cleared within 60 of the
  bottom, or by a click.
- **Click**: `stick` = true, the button goes, the log scrolls smoothly to
  the bottom.
- It sits in the log's area, so it moves with the composer's height.

## 8. Selection

On the web all the conversation's text is selectable with the mouse (the
reply text, bubbles, code, tool bodies, thinking bodies) and copied with
Ctrl+C or the system menu. "Selection stays in the conversation"
(1461-1640): while a selection lives in a log, the rest of the page cannot
be selected; a drag past the log's edge keeps the far end in the log;
"select all" selects the log; dragging a handle near an edge scrolls the
log (touch). The point: a selection never runs into the page around the
chat.

Native: each text block is its own selectable field (TextInput read-only),
so a selection cannot leave the conversation (the rule holds by
construction), but it also cannot span blocks. Name that in PLAN. The reply
paragraphs (StyledText) **cannot be selected at all** today: make them
selectable if Slint 1.18's StyledText allows it, else name it in PLAN with
the closest equivalent (the code blocks, bubbles and tool bodies are
selectable; the copy button covers code, links and paths).

## Data and endpoints

From the chat's stream (specs/live-state.md): `ui-history` and `ui-event`
(`entry`, `patch`, `delta`) with entries `assistant-text` (`text`,
`streaming`), `user` (`text`, `by`), `notice` (`text`), `error` (`text`),
`result` (`subtype`, `text`, `durationMs`, `costUsd`), `gone`, and `tool`
(for the media: `name` ending in `show_image|video|audio|file|embed`,
`status:'done'`, `isError:false`, `result` = the JSON above).

| Method, path | Use | Answer |
|---|---|---|
| `GET /api/chat-media?session=<chat id>&name=<name>` | a picture, poster, video, audio, or stored embed HTML (auth header) | the bytes, `Content-Type` from the file, byte ranges honoured; 404 `{ok:false,error:'Not found'}` when gone; 403 for a confined principal |
| `GET /api/chat-media/file?session=<chat id>&id=<id>` | a `show_file` download | the file, `attachment`; 404; 410 `{ok:false,error:'<name> is no longer where it was.',gone:true}` |
| `GET /api/files/resolve?project=&path=` | a path click (phase 3) | `{ok:true,file,dir,size}` or 404 `{ok:false,error}` |
| `POST /api/mcp/call` `{tool:'show_image', args:{sessionId, path\|paths, caption}}` (`x-api-key` from `data/dev-mcp/<chat>.json`) | **tests only**: store real media in a chat, as Claude does | `{ok, shown, items, caption?, note}` |

`/api/chat-images` is the 1.19.19 name of `/api/chat-media` (same answer).

## End-to-end test (phase2.mjs)

Against `tools/test-server.sh`, one chat open in one wide pane:

1. **Reply**: inject an `assistant-text` entry with a paragraph holding
   `[docs](https://example.com/a)`, a bare URL, `` `src/app.js` ``, a code
   block of 3 lines (one longer than the pane), a list and a quote. Check:
   the link and the bare URL are links; the code block is one block whose
   long line does not wrap (its content is wider than the block); the list
   has its items.
2. **Copy**: hover the code block: the copy button shows in its corner;
   click it: the system clipboard holds exactly the block's text; the toast
   says "Copied". Hover the link, click its copy button: the clipboard
   holds `https://example.com/a`.
3. **Link click**: clicking the link calls the system opener with the URL
   (the test control records it instead of opening a browser). Right-click:
   the menu has "Open in browser" and "Copy link".
4. **Image**: store a PNG through `POST /api/mcp/call show_image` (the
   test-server chat's API key), inject the tool entry with that result:
   the card is followed by an image box of the computed size, which loads
   (no spinner after a moment). Click it: the viewer opens; Esc closes it.
   A second `show_image` with a name that does not exist: "This image is
   no longer stored."
5. **Scroll**: with the log at the bottom and long enough to scroll, send
   three wheel steps up, waiting 300 ms after each: the distance to the
   bottom grows each time and never returns to 0; inject a delta: it still
   does not return. The jump button shows once past `max(240, h/3)`.
6. **Jump**: inject an entry: the button gets its dot (new). Click it: the
   log ends at the bottom (distance < 2) and the button hides.
7. **Errors and notices**: inject a notice, an error and a failed result:
   the notice is italic and centred; both errors are boxes.

`cargo test`: the markdown split (paragraph line breaks kept; `#` → the
heading style, `##` → bold body size; each `>` line a quote; numbered and
bulleted lists; fences with their trailing newline dropped), `linkify`
(trailing punctuation outside, `and/or` and `10/12` stay words, `a/b.js`
and `./x` are paths, `obj.prop` is not), `looksLikePath`, the media item
filter (bad names dropped, legacy `images`), `sizedBox` (one: 1200×700 →
640×373; 600×900 → 280×420; several: 500×500 → 220×220; unknown → 16:9),
the token wording of 2.15, and the stick rule (a content change scrolls
only when `stick`; a height change alone does not).

## Checklist

- [ ] Reply text 13 × fs, line 1.55 where Slint allows (else the named gap);
      paragraphs 8 apart; line breaks inside a paragraph kept.
- [ ] Headings (`#` 14 semibold with its gaps, `##`+ bold body size), quotes
      (bar, `text-secondary`, one per line), lists (20 indent, numbers),
      rules, bold, italic.
- [ ] Code span chip (`bg-tertiary`, `r-chip`), or the named gap.
- [ ] Links `accent-hover` underlined; paths dotted; user-bubble links in
      the bubble's colour.
- [ ] Code block: `bg-secondary`, `border-subtle`, `r-control`, mono 12;
      long lines scroll sideways, not wrap; selectable.
- [ ] Copy button: on hover of a code block (corner), code span, link or
      path (after its last line); tooltip by kind; copies the block / URL /
      path / text; tick + "Copied"; hides 250 ms after leaving and on scroll.
- [ ] Link click → system browser; right-click → "Open in browser" /
      "Copy link" menu.
- [ ] Path click: named gap until the Files panel (phase 3).
- [ ] Media under `show_*` calls: boxes sized before loading (one / several
      / unknown), captions, the call's caption, row gap 8, horizontal scroll
      when too wide.
- [ ] Picture: loading ring, fade in, "gone" box; click → viewer with
      gallery counter, side buttons, zoom bar, keys (Esc, ← →, + − 0),
      Ctrl+wheel, drag, double click.
- [ ] Video / audio: poster, play button, bar and caption drawn; play per
      the named gap (system player).
- [ ] File card: icon, name, size · caption, Download → save dialog, the
      download row, errors as toasts.
- [ ] Frame: named gap; caption and "Open".
- [ ] Notices italic, centred, `text-muted`, wrap; the long-history line.
- [ ] Errors and failed results: the red box (`red` text and edge,
      `red-dim`); end-of-turn line dashed, centred, "12.3s · $0.0421".
- [ ] F87: wheel steps up from the bottom stay; `stick` only from the
      reader's scroll (60); scroll to bottom only when content arrives.
- [ ] Jump button: shows past max(240, h/3); hover; new dot on arrivals;
      click scrolls to the bottom smoothly and hides it.
- [ ] Selection: reply text selectable, or the named gap; nothing outside
      the conversation can be pulled into a selection.
- [ ] Text size scales the text; boxes, gaps and the jump button do not.
- [ ] Narrow pane: text, links and errors wrap; media shrink to the pane.
- [ ] phase2.mjs steps 1-7 pass; cargo tests above; snapshots of every
      state saved under `log-rest/review/`.
