# 2.1 Pane header of a UI chat, wide pane (≥ 560px)

The strip at the top of a chat pane (`.term-head`) when the pane's header is
at least 560px wide. Below 560px the tools fold behind "…": that is 2.2.
Dragging the header and the split buttons that float over it on hover
(`.leaf-ctrls`) are 2.3. The dot's colours and the agent badge
(working / needs input / waiting) are 2.4. The dialog the lock opens is 2.12.

Source: `src/webapp-html.js`:
- `headHtmlLive()` (~9062), `headTools()` (~8891), `updateCellHead()` (~9084);
- `chatSecretsBtnHtml()` / `paintChatSecretBtns()` (~9040);
- `renameTerminal()` (~9708), `setTermFont` / `resetTermFont` / `applyCellFont` / `cellFont` (~9669);
- `togglePanePin()` (~8208), `setPaneFull()` (~8914), `tryCloseTerminal()` / `disarmClose()` (~7836), `closeTerminal()` (~7802);
- the header double-click in `setupMosaicResize()` (~9835) and `mosaicMaximizeLeaf()` (~9573);
- CSS ~761-905, `.term-kind-tag` ~1085, `.pane-pin` ~1456.

The chat log's sizes are in `app/vendor/claude-chat.js`. The file holds a
non-UTF-8 byte, so use `grep -a`.

Screenshots are in `pane-header-wide/`:
- `web-<state>.png` is the whole 1400×860 window;
- `web-<state>-head2x.png` is the header row (1150×32 at +245+89, or
  1400×31 at +0+0 for full screen), scaled ×2.

They were taken from an isolated dev server with project Demo and the seeded
chat ("Fruit list and directory listing"), at 1400×860. Ignore two things in
them:
- the walking robot is the logo mascot's idle animation; it isn't part of
  the header;
- "While you were away" is the recap card, which is 2.17.

## Layout (left to right)

```
[⋮⋮][●][Title……………]              (spacer, flex 1)              [CHAT][🔒n][A-][⟲][A+][📌][⛶][×]
 drag dot  title / rename input                                tag  secrets  font ×3  pin full close
```

Header: flex row, `align-items: center`, `gap: 4px`, padding `3px 4px 3px 2px`.
Height 28px: 27px of content plus a 1px bottom border.

The DOM order, from `headHtmlLive`:
1. `.term-drag`;
2. `.term-status-dot`;
3. `.term-title`;
4. the agent badge (2.4);
5. `.term-head-spacer`;
6. `.term-head-tools`, which is `display: contents` when wide, so its
   children sit in the row:
   - the head-split pair, `display: none` on desktop;
   - the plan pill, only when there's a plan (2.19);
   - the Factory or collaborator tag, only for those chats;
   - `.term-kind-tag` "chat";
   - `.chat-secrets-btn`;
   - A- / reset / A+;
   - the Bubble button, only inside the Android app; never in the PC client;
   - `.pane-pin`;
7. `.term-full`;
8. `.term-more`, `display: none` when wide;
9. `.term-close`.

The title never grows. Everything after the spacer is pushed right, with 4px
between each item and 4px from the header's right edge.

## States

| State | Screenshot | What changes |
|---|---|---|
| Rest (focused pane) | `web-rest` | as measured below |
| Default text size | `web-font-default` | same as rest (13px chat text, `--cc-fs: 1`) |
| Minimum text size (9) | `web-font-min` | the chat log and composer shrink; **the header doesn't change** |
| Maximum text size (24) | `web-font-max` | the log and composer grow; the header doesn't change |
| Hover title | `web-hover-title` | title background `#1a1a28`, radius 6. (Hovering a pane also shows `.leaf-ctrls` in the middle of the header: 2.3) |
| Hover lock / A- / reset / A+ / pin / full | `web-hover-secrets`, `-aminus`, `-reset`, `-aplus`, `-pin`, `-full` | text/icon `#e8e8f0`, border `#6366f1` (accent). Nothing else |
| Hover × | `web-hover-close` | text and border `#ef4444` (red); background stays `#0a0a0f` |
| Armed × ("Sure?") | `web-close-armed` | label "Sure?", 11px, padding `1px 6px`, red text and border, background `rgba(239,68,68,.15)`; the box grows to 44.8×15, so every button to its left moves 17px left; tooltip "Click again to close" |
| Pinned | `web-pinned` | pin button: icon and text `#818cf8`, border `#6366f1`, background `rgba(99,102,241,.15)`; the pin icon is turned −30°; tooltip "Unpin (size and position are locked)" |
| Full screen | `web-full` | the pane covers the whole window (0,0,1400×860, radius 0; the focus border stays); **drag handle and × are hidden**; the full button is accent-coloured (icon `#818cf8`, border `#6366f1`) |
| Renaming | `web-renaming` | the title becomes an input (see Rename) |
| Long title, typed by the person | `web-long-title-typed` | 160px max, ellipsis ("Investigate the flak…") |
| Long title, automatic (long form) | `web-long-title-auto` | the long form shows, up to `min(440px, 62%` of header width`)`, then an ellipsis |
| Secrets, 1 granted (`.on`) | `web-secrets-on-1` | lock and count "1" `#f59e0b` (yellow), border `#f59e0b`; button 37×20; tooltip "1 secret pre-authorised for this chat" |
| Secrets, 2 granted, hovered | `web-secrets-on-2-hover` | count "2"; **hover doesn't change `.on`**: it stays yellow, because `.chat-secrets-btn.on` comes after `:hover` at equal specificity; tooltip "2 secrets pre-authorised for this chat" |
| No secrets capability | `web-no-secrets-capability` | no lock at all; A- sits 4px after CHAT |
| Two panes, 570px each | `web-two-panes` | both wide; the unfocused pane's border is `#2a2a3d` with no ring |
| Double-click maximised | `web-dblclick-maximised` | the left pane is 92% (801px); its neighbour is at its 340px floor and so narrow (2.2's "…") |

## Measurements (web, 1400×860, focused pane at x=246)

Every colour comes from computed styles.

Theme:

| Token | Value |
|---|---|
| bg-primary | `#0a0a0f` |
| bg-secondary | `#12121a` |
| bg-tertiary | `#1a1a28` |
| border | `#2a2a3d` |
| text-primary | `#e8e8f0` |
| text-secondary | `#8888a8` |
| text-muted | `#5a5a78` |
| accent | `#6366f1` |
| accent-hover | `#818cf8` |
| accent-dim | `rgba(99,102,241,.15)` |
| red | `#ef4444` |
| red-dim | `rgba(239,68,68,.15)` |
| yellow | `#f59e0b` |
| radius-sm | 6px |

| Element | Box x,y,w,h | Text box | Style |
|---|---|---|---|
| pane `.term-cell` (focused) | 246,90,1148,764 | | border 1px `#6366f1` + ring `0 0 0 1px #6366f1`; radius 10; background `#0a0a0f`; overflow hidden, so the header's top corners are clipped to radius 9 inside the border |
| pane (unfocused) | | | border 1px `#2a2a3d`, no ring |
| `.term-head` | 247,91,1146,28 | | background `#12121a`; border-bottom 1px `#2a2a3d`; padding 3/4/3/2; gap 4; `user-select: none` |
| `.term-drag` "⋮⋮" (two U+22EE) | 249,98.5,26,12 | 252,97.5,20,14 | 12px system sans, line-height 1, letter-spacing −2px, colour `#5a5a78`, padding 0 3px, cursor grab |
| `.term-status-dot` | 279,101,7,7 | | circle, `#10b981` when idle (other states: 2.4) |
| `.term-title` (short "Fruit list") | 290,95.5,69,18 | 296,97.5,57,14 | 12px/600 system sans, `#e8e8f0`, padding 2px 6px, radius 6, max-width 160, nowrap + ellipsis, cursor text |
| `.term-title[data-long]` | 290,95.5,210.7,18 | | wide pane: the long form is drawn instead (`::after`, 12px/600), max-width `min(440px, 62cqw)`; ellipsis |
| `.term-title` hover | same | | background `#1a1a28` |
| `.term-title-input` (renaming) | 290,94.5,160,20 | | 12px/600, padding 2px 6px, radius 6, border 1px `#6366f1`, background `#0a0a0f`, ring `0 0 0 3px rgba(99,102,241,.28)`, width 160 |
| spacer | 504.7,104.5,617.9,0 | | flex 1 |
| `.term-kind-tag` "CHAT" | 1126.6,97,47.6,15 | 1134.6,99,31.6,11 | 9.92px (.62rem)/700, uppercase, letter-spacing .5952px (.06em), `#818cf8`, border 1px `#2a2a3d`, radius 99, padding 1px 7px, no fill |
| `.chat-secrets-btn` (empty) | 1178.3,94.5,26,20 | | `inline-flex`, gap 4, padding 3px 6px, border 1px `#2a2a3d`, background `#0a0a0f`, radius 6, `#8888a8`; lock icon 12×12 at 1185.3,98.5 |
| `.chat-secrets-btn.on` "n" | 1167.3,94.5,37,20 | count 1190.3,98,7,13 | 11px Arial (the button's own font, see note), `#f59e0b`; border `#f59e0b`; lock 12×12 at x+7, 4px gap, then the count |
| A- | 1208.3,95,25.3,19 | 1215.3,98,11.3,13 | "A-" with an ASCII hyphen, 11px, line-height 1, padding 3px 6px, border 1px `#2a2a3d`, background `#0a0a0f`, radius 6, `#8888a8` |
| reset (rotate-ccw) | 1237.5,94.7,25,19.6 | | same frame; icon 11×11 at 1244.5,98.7 (inline-block, `vertical-align: -.125em`, hence the 0.6px extra height) |
| A+ | 1266.5,95,30.8,19 | 1273.5,98,16.8,13 | same as A- |
| pin `.pane-pin` | 1301.3,94,27,21 | | `inline-flex`, centred; icon 13×13 at 1308.3,98 |
| full `.term-full` | 1332.3,95,25,19 | | `inline-flex`, padding 3px 6px, font 13px declared, but the icon is 1em of 11px, so 11×11 at 1339.3,99 |
| close `.term-close` "×" (U+00D7) | 1361.3,95.5,27.7,18 | 1369.3,96.5,11.7,16 | 14px, padding 1px 7px, otherwise the same frame; right edge 1389 = header right 1393 − 4 |
| close armed "Sure?" | 1344.2,97,44.8,15 | 1351.2,98,30.8,13 | 11px, padding 1px 6px, red text and border, background `rgba(239,68,68,.15)` |
| pinned pin | 1301.3,94,27,21 | | `#818cf8`, border `#6366f1`, background `rgba(99,102,241,.15)`; icon turned −30° (bounding 17.8×17.8) |
| full-screen header | 1,1,1398,28 | title 14,5.5 | no drag handle, so the title starts at 1+2+7+4; full button at 1370,5 (`#818cf8` / `#6366f1`); no × |

Font notes:
- **Buttons don't inherit the page font.** Chromium computes `Arial` for
  them (Liberation Sans or Arimo on Linux). The title, tag and drag handle
  use the UI system sans (`-apple-system` stack). Draw the buttons' "A-",
  "A+", "×", "Sure?" and the count in the **UI sans at 11px or 14px**. Arial
  versus the system sans is a browser default, not a design choice: DESIGN
  rule 5.
- Icons come from `ICON_PATHS`:
  - `lock` 12px;
  - `rotate-ccw` 11px;
  - `pin` 13px;
  - `maximize` 11px (the same icon in full screen, only recoloured);
  - stroke `currentColor`, width 2.
- The buttons' heights differ (19, 19.6, 20, 21, 18) because each centres
  its own content. Copy the boxes as measured: they all centre on y≈104.5.

## Behaviour

### Title: click to rename
- One click on `.term-title` (`data-action="term-rename"`) swaps it for an
  `<input class="term-title-input">`.
- The input holds the **short** title, `t.title`, not the long form.
  Example: "Fruit list and directory", not "…listing".
- It is focused with all its text selected.
- **Enter** saves. **Escape** cancels and puts the old title back. **Blur**
  saves.
- The value is trimmed; an empty value becomes "Terminal". (The server
  turns an empty title into "Chat".)
- Nothing is sent if the value is unchanged.
- On save, the title becomes the new value at once and the long form is
  cleared (`longTitle = ''`). Then:
  `POST /api/ui-sessions/:id/rename {title}` → `{ok, title}`.
- The server marks the chat `titleByUser`. Claude's automatic rename
  (`rename_chat`) is refused from then on, and a typed title has no long
  form. The server then broadcasts `state`. Errors are ignored.
- There's no length limit on a typed title. The header elides it at
  160px.
- While the input is open, a state push doesn't touch it (`updateCellHead`
  skips `INPUT`). A full re-render would drop it.
- A title that changes on the server comes in on the `state` event
  (`terminals[].title` / `.longTitle`) and is updated in place. We
  verified this with a rename through the API: the header changed with no
  reload.

### Short and long title
- `t.title` holds up to about 20 characters (25 accepted). `t.longTitle`
  holds up to about 50 (55 accepted); it is set only by Claude's automatic
  rename.
- On a desktop where the header is at least 560px wide, the long form is
  shown when it exists. It is capped at `min(440px, 62%` of header
  width`)` and elided.
- Otherwise the short form is shown, capped at 160px and elided.
- The tooltip is "Click to rename".

### CHAT tag
- Static text, "chat" drawn upper case. No action.
- The plan pill, the Factory tag and the collaborator tag are put just
  before it when they apply. They belong to 2.19 and later screens.

### Secrets lock
- **Shown only when** the server's `GET /api/handshake` lists
  `server.capabilities` ∋ `'chat-secret-preauth'`, the user isn't a
  collaborator, the chat is `kind: 'ui'`, and the chat isn't a
  collaborator's.
- Otherwise it isn't drawn (`display: none`). It takes no room and A-
  follows CHAT directly.
- The badge shows the number of grants for this chat that haven't expired.
  A grant has expired when its `expiresAt` is set and is in the past.
  Grants with `untilChatEnds` never expire on the client.
  - **0**: no number, grey; tooltip "Pre-authorise secrets for this chat".
  - **n > 0**: `.on` (yellow) with the number; tooltip "n secret(s)
    pre-authorised for this chat".
- **Data:**
  - At start: `GET /api/secrets/chat` →
    `{ok, chats: {sessionId: [grant…]}, presets}`.
  - Live: the SSE event `chat-secrets` `{sessionId, grants}`, which
    replaces that chat's list. We verified both 0→1 and 1→2 live.
  - Every 30s the client re-counts, so timed grants drop off when they
    expire.
  - Grant shape:
    `{secretId, name, expiresAt: ISO|null, untilChatEnds, grantedAt}`.
- **Click:** opens the chat-secrets dialog for this chat
  (`CrundiChatSecrets.open(tid)`; that dialog is 2.12) and closes any open
  header menu.

### Text size: A- / reset / A+
- Each chat keeps its own size in px.
  - Web storage: `localStorage.crundi_term_font` = `{chatId: px}`, plus a
    per-kind default in `localStorage.crundi_font_default` = `{term, chat}`.
  - **Native:** keep both in `store::Saved`, beside `layouts`:
    `font: {chatId: px}` and `font_default_chat: px`.
- A chat with no stored size starts at the chat default (13 unless changed)
  and stores it the first time it's drawn. A later change to the default
  doesn't move it.
- A- / A+: step −1 / +1, clamped to **9…24**. Repeated clicks at a limit
  change nothing.
- Reset: set it back to **13**.
- Any change, reset included, also becomes the **default for new chats**
  (`crundi_font_default.chat`). We checked: after A+ to 24 the default
  read `{"term":14,"chat":24}`.
- It's applied as `--cc-fs = px / 13` on the pane. The chat's stylesheet
  multiplies **83 font sizes** by it, plus a few box sizes:
  - `.cc-mode`: min-width 26, height 22, icon 13 (the narrow and phone
    forms: 34×28);
  - `.cc-agbadge svg`: 15.
- **Padding, gaps, radii and the header don't scale.** The header stays the
  same at every size.
- Line heights follow, because they are factors (1.55).
- Measured at 9 / 13 / 24:

| Part | 9 | 13 | 24 |
|---|---|---|---|
| root, user bubble, paragraph, list item | 9px / lh 13.95 | 13 / 20.15 | 24 / 37.2 |
| tool summary, thinking | 8.31 | 12 | 22.15 |
| result line, meta row, mode button | 7.62 | 11 | 20.31 |
| composer textarea (lh 1.45) | 9 / 13.05 | 13 / 18.85 | 24 / 34.8 |

  The composer's Send and Stop don't scale (13.33px: a button's default),
  and the attach and clock buttons keep their box size.

  Native: one scale factor `px/13` for every chat text size in the pane, and
  for the mode button's size; nothing else.
- Closing a chat deletes its entry (`delete termFont[id]`).

### Pin (`pane-pin`)
- Desktop mosaic only. The phone layout hides it.
- Click toggles `leaf.pin` on this pane's mosaic leaf.
  - **Pinning** saves the pane's size **now**, in px:
    `pin = {w: round(width), h: round(height)}`. The value is stored in the
    project's layout tree (`localStorage.crundi_wb_mosaic[project]`; native:
    `Saved.layouts[project]`). Example: `{"t":"leaf",…,"pin":{"w":1148,"h":764}}`.
  - **Unpinning** deletes `pin`.
  - Either way: re-render and refit.
- What pinning does to the layout:
  - the leaf, or any split that holds it, takes that px size along its
    parent split's axis (`flex: 0 0 Npx`) instead of a share;
  - the dividers next to it are locked (`cursor: not-allowed`, tooltip "A
    pinned pane is next to this divider");
  - it can't be dragged ("This pane is pinned. Unpin it to move it.");
  - it isn't a drop target;
  - layout presets keep its pin.
  (The move and drop details are 2.3.)
- Tooltip: "Pin size and position" / "Unpin (size and position are
  locked)".

### Full screen (`pane-full`)
- Click toggles. Only one pane can be full at a time; turning one on turns
  off any other. Any open header menu is closed.
- **On:**
  - the pane gets `.pane-full`: `position: fixed; inset: 0; z-index: 390;
    radius 0`, background `#0a0a0f`;
  - `body.has-pane-full` is set;
  - the drag handle and × are hidden;
  - the full button is accent-coloured;
  - the pane survives re-renders, because its key is remembered
    (`paneFull.key`).
- The web in a plain browser also puts the **page** in browser full screen
  (`requestFullscreen` on `<html>`).
  - Leaving browser full screen any other way (Escape, the system gesture)
    fires `fullscreenchange` → `setPaneFull(null, false)`. That is how
    **Esc leaves** in a browser; verified with `exitFullscreen()`.
- In the Electron PC client (`window.api` is set) the window is **not**
  made full screen. The pane only covers the app window, and Esc has no
  handler there: only the button leaves.
- **Native:**
  - Cover the client's window content with the pane: title bar kept, no
    OS full screen, like Electron.
  - **Esc leaves.** The Manager asked for it, and it is what the design
    means everywhere it can.
  - Esc must not leave while a dialog or menu above the pane has the key;
    they close first.
  - Not saved across restarts: the web doesn't keep it on reload either.

### Close: the two-click ×
- **First click** arms it:
  - label "Sure?", tooltip "Click again to close", the armed style;
  - only one pane is armed at a time; arming another disarms the first;
  - it disarms by itself after **3000 ms**, back to "×" and "Close chat";
  - any full re-render of the grid disarms it too, so a click after a
    redraw can never close without the warning.
- **Second click** while armed closes:
  `POST /api/ui-sessions/:id/close`.
  - **409** → a toast with `j.error`, or "This chat cannot be closed right
    now." (a factory Manager's chat while its run is going). The pane stays.
  - **ok** → the chat leaves the list. If it was focused, focus is cleared.
    Its font entry is deleted; the grid and sidebar redraw; the mosaic leaf
    collapses.
  - **Network error** → toast "Failed to close terminal: <message>".
- Hidden in full screen.
- Tooltip "Close chat".

### Double-click the header
- In the mosaic, a double-click whose target is inside `.term-head`
  maximises this pane within its **parent split**:
  - this pane gets `100 − (n−1)·8`%; each sibling gets 8% and is held at
    its 340px floor;
  - double-click again (already ≥ max − 0.5) → even split, `100/n` each;
  - a lone root pane: nothing happens.
- Saved in the layout tree (`sizes`). Verified: [50,50] → [92,8] → [50,50].
- The web counts a double-click anywhere in the header, buttons included.
  That's incidental: double-clicking A+ would step the size twice *and*
  maximise. **Native:** take the double-click on the header's background,
  drag handle, dot, title (when not editing) and tag only. Not on the
  buttons, nor in the rename input. (DESIGN rule 5.)

## Data and endpoints

| What | Method and path | Body | Response |
|---|---|---|---|
| Capability for the lock | `GET /api/handshake` | | `{server: {capabilities: [...]}}`; the lock needs `'chat-secret-preauth'` |
| Rename | `POST /api/ui-sessions/:id/rename` | `{title}` | `{ok, title}` or `{ok: false, error: 'No session "…"'}`; then the SSE `state` |
| Close | `POST /api/ui-sessions/:id/close` | | 200 `{ok…}`; **409** `{error}` when refused |
| All grants | `GET /api/secrets/chat` | | `{ok, chats: {sid: [grant]}, presets}` |
| One chat's grants | `GET /api/secrets/chat?sessionId=…` | | `{ok, sessionId, grants, presets}` |

Live events: SSE `GET /api/events`; native already listens in `live.rs`.
- `state`: `terminals[]`. For a chat:
  - `{id, project, title, longTitle, kind: 'ui', status, agentState,
    model, permissionMode, planStatus, sessionId, collaborator, factory, …}`;
  - the title, long title, tags and dot update in place from this event.
- `chat-secrets`: `{sessionId, grants}`. Add it to `live.rs`'s handled
  events; today it handles `state`, `usage` and `inbox` only.

Text size, pin, the layout and full screen are **client-only**: no server
call.

## Checklist for the reviewer

- [ ] Header 28px high (27 + 1px bottom border `#2a2a3d`), fill `#12121a`, padding 3/4/3/2, gap 4; the top corners clipped by the pane's radius 10.
- [ ] Drag handle "⋮⋮": 12px `#5a5a78`, tight spacing (−2px tracking), 26×12 at x+2.
- [ ] Dot 7px at x+32, idle green `#10b981`.
- [ ] Title 12px/600 `#e8e8f0`, padding 2/6, radius 6:
  - short form ≤ 160px with an ellipsis;
  - long form ≤ min(440, 62% of header width);
  - hover fill `#1a1a28`.
- [ ] The title doesn't stretch: the spacer takes the slack, and the tools hug the right edge (4px).
- [ ] CHAT tag: 15px high pill, 9.92px/700 upper case, .6px tracking, `#818cf8`, 1px `#2a2a3d` border, padding 1/7, no fill.
- [ ] Lock 26×20 when empty, 37×20 with a count:
  - 12px icon, 4px gap, 11px count;
  - grey at rest, accent border on hover;
  - `.on`: yellow `#f59e0b` text and border, and it stays yellow on hover.
- [ ] Lock absent (no room taken) when the server lacks `chat-secret-preauth`.
- [ ] A- (ASCII hyphen) 25.3×19, reset 25×19.6 with an 11px rotate-ccw, A+ 30.8×19: 11px, padding 3/6, border `#2a2a3d`, fill `#0a0a0f`, radius 6, text `#8888a8`.
- [ ] Pin 27×21 with a 13px icon. Pinned: `#818cf8` on `rgba(99,102,241,.15)`, border `#6366f1`, icon turned −30°.
- [ ] Full 25×19 with an 11px maximize. In full screen it is accent-coloured.
- [ ] × 27.7×18, 14px, padding 1/7. Hover: red text and border.
- [ ] Armed: "Sure?" 11px, padding 1/6, 44.8×15, red on `rgba(239,68,68,.15)`. Neighbours shift left.
- [ ] Every button hover: text `#e8e8f0`, border `#6366f1`. Nothing else changes.
- [ ] Tooltips:
  - "Click to rename", "Smaller text", "Reset text size", "Larger text";
  - "Pin size and position" / "Unpin (size and position are locked)";
  - "Full screen", "Close chat" / "Click again to close";
  - the lock's three texts.
- [ ] Rename:
  - opens with the short title, all selected;
  - the input is 160 wide, accent border and 3px focus ring;
  - Enter or blur saves, Esc cancels;
  - empty → "Terminal";
  - `POST …/rename`; the long form is dropped.
- [ ] A title changed on the server (`state`) updates live, and doesn't clobber an open rename.
- [ ] A-/A+ clamp at 9…24; reset → 13; each change becomes the new-chat default; saved per chat across a restart.
- [ ] Text size scales the log and composer text (and the mode button) by px/13; padding and the header don't scale.
- [ ] Pin saves `{w, h}` in the layout and survives a restart; it locks the neighbouring dividers; a pinned pane can't be dragged.
- [ ] Full screen:
  - covers the window, with no drag handle or ×;
  - the button leaves it and **Esc leaves it**;
  - only one pane at a time;
  - it survives a re-render.
- [ ] Close:
  - the first click arms, the second closes;
  - it disarms after 3 s and on any re-render;
  - arming one pane disarms the others;
  - a 409 shows the server's error in a toast and keeps the pane.
- [ ] Double-click on the header (not on a button) → 92/8; again → even; a lone pane: nothing happens.
- [ ] Lock count updates live from `chat-secrets`; timed grants drop off when they expire, with a re-count at least every 30 s.
- [ ] Web-over-native crops of the header row for every state in the table above (`web-*-head2x.png` against the native ones).
