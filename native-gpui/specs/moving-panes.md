# 2.3 Moving and arranging panes on the workbench

How a person rearranges the workbench (the "mosaic"):
- dragging a pane by its header onto another pane;
- the split controls that float over a pane on hover;
- the empty pane a split leaves, with its chips;
- the "+" menu's Fit to screen and Layout presets;
- the gutters' look.

Dragging a gutter to resize and the pin are already in (2.1, phase 1). This
spec covers only how they look and how they limit moving.

Source, `src/webapp-html.js`:
- CSS:
  - ~1300-1342: `.wb-add-menu`, `.wb-sub`, `.wb-subitems`;
  - ~1346-1372: `.term-grid.mosaic`, `.mosaic-split`, `.mosaic-leaf`, `.mosaic-gutter`;
  - ~1452-1505: locked gutters, pin, gutter line, `.mosaic-empty`, `.me-chip`, `.me-acts`, `.leaf-ctrls`, `body.mosaic-cell-drag`;
  - 3160-3166: `.drag-clone`, `body.dragging-active`;
  - 746-768: `.term-cell`, `.term-head`, `.term-drag`.
- JS:
  - the tree: `mosLeaf` / `mosSplit` / `mosaicSync` / `mosaicAddKey` / `mosaicCollapseLeaf` / `mosaicSplitLeaf` / `mosaicAssignToLeaf` / `mosaicMaximizeLeaf` / `mosaicEvenSplit` / `mosaicApplyPreset`, ~9460-9612;
  - `buildMosaicDom` / `arrangeMosaic` / `mosaicFixed` / `mosaicFitToScreen` / `leafCtrlsHtml` / `emptyLeafHtml`, ~8136-8275;
  - `termDragHandlers`, ~9754 (the mosaic drop);
  - `makeDraggable`, ~16976 (the drag controller and its clone);
  - `wbScrollTick`, ~16917 (auto-scroll while dragging);
  - `setupMosaicResize`, ~9830 (double-click and the gutter drag);
  - layout motion `playCellMotion` / `mosaicResizeInPlace`, ~7924-8010;
  - the click actions `wb-layout`, `wb-fit`, `leaf-split`, `leaf-remove`, `wb-add`, ~16361-16399.

Screenshots are in `moving-panes/`:
- `web-<state>.png` is the whole 1400×860 window;
- `web-<state>-2x.png` is a crop scaled ×2.

They come from an isolated dev server on :4810, with project Demo and chats:
- "Fruit list and directory", the seeded chat with a reply;
- "Second chat", "Third chat" and "Fourth chat", all empty.

Chats were closed one by one through the API to get 3, 2 and 1 chats.

Ignore these in the screenshots:
- the walking robot (the logo mascot);
- the "While you were away" recap card (2.17);
- the Inbox badge.

## States and screenshots

| State | Screenshot |
|---|---|
| Four chats in a row: 340px each, so the grid scrolls sideways | `web-four-row` |
| Hover controls over a narrow pane (340): over the title | `web-hover-narrow` (+2x) |
| Hover over Split right | `web-hover-split-right` (+2x) |
| Hover controls over a wide pane (570): over the CHAT tag | `web-hover-wide-chat-tag` (+2x), `web-hover-wide` (+2x) |
| "+" menu open | `web-plus-menu` (+2x) |
| Layout submenu open, "Layout" hovered | `web-layout-submenu` (+2x) |
| Presets, 4 chats | `web-four-row` (= 2 columns = 3 columns), `web-preset4-cross4`, `web-preset4-left-right2`, `web-preset4-top-bottom2` |
| Presets, 3 chats | `web-preset3-cross4` (one empty pane), `web-preset3-top-bottom2` |
| Presets, 2 chats | `web-preset2-cols3`, `web-preset2-top-bottom2` |
| Presets, 1 chat | `web-preset1-cross4` |
| A preset change, mid-motion (220ms) | `web-motion-midflight` |
| Empty pane at rest | `web-empty-rest` (+2x) |
| Empty pane, chip hovered | `web-empty-hover-chip` (+2x) |
| Empty pane, Split ▸ hovered | `web-empty-hover-split` (+2x) |
| Gutter hover | `web-gutter-hover` (+2x) |
| Gutter being dragged (all gutters light up) | `web-gutter-active` |
| Fit after a manual resize (back to even) | `web-fit-after-resize` |
| Fit that cannot fit (error toast) | `web-fit-does-not-fit` |
| Dragging a pane over another pane | `web-drag-over-chat` |
| Dragging a pane over an empty pane | `web-drag-over-empty` (+2x) |
| After a swap | `web-drop-swap-result` |
| After a move into an empty pane | `web-drop-move-result` |
| Dragging over a pinned pane (no highlight) | `web-drag-over-pinned` |
| Dragging a pinned pane | `web-drag-pinned` |
| Dropping a pinned pane: refused, toast | `web-pinned-refused` |
| Split right and split down, the result | `web-split-result` |
| Terminal chip: the launcher lands in the first empty pane | `web-chip-terminal-launcher` |
| UI Mode: the session chooser (2.22) | `web-chip-chooser` |
| The chat that starts, appended (see "Launching from an empty pane") | `web-chip-chat-appended` |
| Double-click a header: maximised in its split; the empty pane below is squashed and clipped | `web-maximized-empty-squashed` |

## The grid and the tree

- `#term-grid.mosaic`: x240 y84, 1160×776 at 1400×860. It has 6px padding,
  `overflow-x: auto`, `overflow-y: hidden`. Panes start at x246 y90.
- The layout is a tree:
  - a split is `{t:'split', _id, dir:'row'|'col', sizes:[% …], kids:[…]}`;
  - a leaf is `{t:'leaf', _id, key, pin?}`;
  - `key` is `live:<id>` (a chat or terminal), `pend:<lid>` (a launcher),
    `panel:<id>`, `widget:<id>`, or null (an empty pane);
  - `pin` is `{w,h}` in px.
- A split's children are laid out with `flex: <size> 1 0`. Between siblings
  sits a 7px gutter.
- A pinned child, or a split that holds one, takes `0 0 <px>` along that
  axis (`mosaicFixed`, already in mosaic.rs).
- Every leaf is at least 340px wide, and a split is at least its
  min-content. When the floors don't fit, the grid scrolls sideways and a
  6px horizontal scrollbar takes the bottom: leaves are then 758 tall,
  not 764 (`web-four-row`).
  - Example: a split right inside the right-hand column needs 340 + 7 +
    340 = 687 there, so the left column shrinks to 454 (`web-split-result`).
- **Saved per project:** the browser keeps `localStorage.crundi_wb_mosaic`
  = `{ <project alias>: tree }`. It is written on every change: a split,
  a drop, a preset, Fit, the end of a gutter drag, a pin, a double-click.
  It is never sent to the server. Native keeps it in `store.rs`
  `layouts[project]` (already there), saved the same way, so it survives a
  restart.
- **Keeping the tree in step with the chats** (`mosaicSync`, on every redraw):
  - a key that is gone (a closed chat, a removed panel) collapses its leaf:
    the leaf is removed, its siblings' sizes become even, and a split left
    with one child is replaced by that child;
  - a new key fills the **first empty leaf in tree order** (depth-first, left
    to right); with none, it is appended to the root (a root leaf becomes a
    row of two), and the root's sizes become even;
  - with nothing at all, the project gets one launcher (`pend:`), the "New
    terminal" pane.
- Geometry checked, 3 chats, 2×2 cross, one empty: leaves 570.5×378.5 at
  (246,90), (246,475.5), (823.5,90) and (823.5,475.5). Gutters: v at
  816.5 (7×764), h at 468.5 (570.5×7) in each column.

## Layout motion

- A change of structure animates:
  - each pane that survives moves from where it was to where it is
    (translate plus scale from its top-left), 220ms,
    `cubic-bezier(.2,.7,.2,1)`;
  - a new pane fades in from opacity 0 at scale 0.985, 220ms, ease-out.
  - Structure changes are a drop, a split, a preset, a close, an add.
- A change of sizes only (double-click a header or a gutter; Fit when the
  structure is unchanged) eases the shares, 220ms, same curve.
- No motion while a gutter is being dragged.
- This runs always, whatever the system's reduce-motion setting says (the
  owner asked for that; see the comment at `LAYOUT_MS`).
- `web-motion-midflight` is a frame from 4 → 3 columns.
- Native: tween the leaf rectangles (x, y, w, h) over 220ms with that
  curve. A new pane goes from opacity 0 to 1.

## Dragging a pane by its header

### The handle and the cursor

- **The whole header (`.term-head`) is the handle**, not only the ⋮⋮.
  - A press on a button, input, select, textarea, link or label never
    starts a drag.
  - A press on the title span does. A click without moving still renames.
- The ⋮⋮ (`.term-drag`, "Drag to reorder"):
  - 26×12 at x249 y97.5 in a wide header (y96.5 in a narrow one); text box
    20×14;
  - 12px, letter-spacing −2px, line-height 1, padding 0 3px, colour
    `--text-muted` #5a5a78;
  - `cursor: grab`;
  - hidden in a full-screen pane, so a full-screen pane can't be dragged.
- The rest of the header shows the default arrow. The title shows its text
  cursor (2.1).
- **It starts** after the mouse moves more than 6px (either axis) with the
  left button held. Touch has a 320ms long-press; the native client is
  mouse only.
- **While dragging:**
  - `body.dragging-active`: the cursor is `grabbing` everywhere;
  - text selection is off;
  - the source pane stays where it is, at full opacity, nothing dimmed
    (`.term-cell.dragging` has no style).

### The clone under the pointer

- A copy of the whole pane follows the pointer, keeping the offset of the
  grab point. Measured: 570.5×378.5 for a 570.5×378.5 pane.
- Its look:
  - `position: fixed`, opacity 0.92;
  - `transform: rotate(1.5deg)`, around its centre;
  - z above everything; it takes no pointer events.
- Shadow:
  - `.drag-clone` gives `0 12px 32px rgba(0,0,0,.55)`;
  - a **focused** pane's clone instead shows its focus ring, accent border
    plus `0 0 0 1px` accent (`.term-cell.focused` wins on specificity;
    `web-drag-over-chat`);
  - native draws the pane's own border (accent if focused) **and** the drop
    shadow. DESIGN rule 5: the CSS slip drops the shadow, and the shadow is
    what `.drag-clone` means.
- Slint:
  - draw a second instance of the pane card with the same data at the
    pointer;
  - if the renderer can't rotate it, draw it unrotated and name the gap
    in PLAN.md ("turning a dragged pane").

### The drop targets and how they light up

- **There are no edge zones.** The whole leaf under the pointer is the
  target (`elementFromPoint(x, y).closest('.mosaic-leaf')`).
  - Dropping on a chat swaps the two.
  - Dropping on an empty pane moves the chat there.
  - There is no "drop on an edge to split". Splitting is only through the
    split buttons.
- While a drag runs (`body.mosaic-cell-drag`), **every** leaf gets
  `outline: 1px dashed --border` (#2a2a3d), `outline-offset: -2px`.
  - Sampled (`web-drag-over-chat`): the line is the leaf's second pixel row
    and column (inset 1px), just inside the pane's own 1px border. Dashes
    3px, gaps 2px, colour #2a2a3d over the header's #12121a.
  - It shows on the source too, and on an empty pane inside its 3px margin,
    so the empty pane shows its own 2px dashed border plus this outline
    (`web-drag-over-empty-2x`).
- The leaf under the pointer, if it isn't pinned, gets
  `outline: 2px solid --accent` (#6366f1), offset −2px (`.drop-hover`):
  - the source leaf itself lights up when the pointer is back over it;
  - a **pinned** leaf never lights up (`web-drag-over-pinned`): it keeps the
    dashed outline;
  - the gutters, the grid padding and anything outside the grid have no
    target, so nothing lights up there.
- `.mosaic-empty.drop-hover` (accent border, accent-dim fill) exists in CSS
  but this path never sets it. The empty pane lights up through its leaf's
  outline like any other. Copy that.
- The 2px solid outline covers the leaf's outer two pixel rows and columns
  (inset 0-2, sampled: y90 and y91 are #6366f1, y92 is the header). It is
  over the pane's own border.
  - The leaf has no radius, so the outline has square corners over the
    pane's rounded ones.
- The hover controls (`.leaf-ctrls`) of the leaf under the pointer still
  fade in while dragging (CSS `:hover`; `web-drag-pinned`). Copy that.

### What drops where (`mosaicAssignToLeaf`)

When the mouse is released over leaf T, dragging pane S:

1. If S's leaf is pinned: nothing changes, and the toast "This pane is
   pinned. Unpin it to move it." (plain toast, accent left edge;
   `web-pinned-refused`). A pinned pane can still be picked up and dragged
   (`web-drag-pinned`); it is refused on release.
2. No target (pinned, a gutter, outside): nothing changes.
3. T is S's own leaf: nothing changes.
4. T holds a pane: the two keys swap. Each leaf keeps its size and pin. Only
   the panes trade places (`web-drop-swap-result`).
5. T is empty: S moves into T, then S's old leaf collapses (removed, its
   parent's sizes even, a one-child split hoisted; `web-drop-move-result`).
   - Example: `row[col[A, B], col[C, ∅]]`, drag B onto ∅, gives
     `row[A, col[C, B]]`.

After a drop:
- the tree is saved;
- the grid is redrawn with the layout motion;
- the pane that was pressed gets the focus (its mousedown focused it).

A drop never changes the server's pane order. `/api/ui-sessions/reorder` is
only used by the web's phone layout, which has no mosaic.

**Auto-scroll while dragging**, when the grid scrolls sideways:
- within 60px of the grid's left or right edge, it scrolls each frame by
  up to 22px, ramping with how deep into the 60px the pointer is;
- the same holds for top and bottom if it ever scrolls that way.

Esc: the web has none during a drag. Native: Esc cancels the drag with no
change (rule 5, like the narrow menu's Esc in 2.2). Release outside the
window: no change.

## Split right / split down: the hover controls (`.leaf-ctrls`)

- **When:**
  - every leaf that holds a pane (chat, launcher, panel) has them;
  - an empty leaf doesn't (it has its own buttons);
  - they fade in while the mouse is anywhere over the leaf, header or
    body: opacity 0 → 1, `transition: opacity .12s`;
  - they fade out when it leaves;
  - they show in narrow and wide panes alike; folding the header (2.2)
    doesn't change them.
- **Where:**
  - absolutely placed in the leaf: `top: 3px`, centred,
    `left: 50%; translateX(-50%)`;
  - z-index 6, so above the header, and they **cover** whatever is under
    them.
  - Measured: box 47×18 at x392.5 y93 for a 340px leaf at x246 (centre
    x416); at x739.5 for the next leaf.
  - In a narrow pane they sit over the title text (`web-hover-narrow`).
  - In a 570px pane, over the left part of the CHAT tag
    (`web-hover-wide-chat-tag`, x1087-1134 against the tag at 1128). Copy
    that; it's how the web looks.
  - Pane top is leaf.y + 1, so the buttons start 2px below the pane's top
    edge, and their 18px end 1px above a wide header's 28px (y93-111
    against the header y91-119).
- **Look:** a flex row, gap 3px, two buttons, each:
  - 22×18, padding 0, border 1px `--border` #2a2a3d, background
    `--bg-secondary` #12121a, radius 5px, colour `--text-secondary`
    #8888a8, `cursor: pointer`;
  - icon `.lay-ic`, 14×11, at +4,+3.5 inside the button;
  - Split right: viewBox 0 0 24 18, stroke-width 1.8, a rounded rect
    (1,1,22,16, rx 1.5) and a vertical line at x12;
  - Split down: the same rect and a horizontal line at y9;
  - stroke `currentColor`, no fill.
  - Hover: border `--accent` #6366f1, colour `--accent-hover` #818cf8
    (`web-hover-split-right-2x`). The background stays.
  - Tooltips: "Split right", "Split down".
- **What they do** (`mosaicSplitLeaf`), on the leaf L they sit in:
  - if L's parent split runs the same way (row for right, col for down), a
    new empty leaf goes in right after L, and that parent's sizes become
    even;
  - otherwise L becomes a new split `[L, ∅]` in that direction, 50/50,
    in L's place (or as the root);
  - the click doesn't reach the pane (no focus change);
  - saved, redrawn with motion (the new leaf fades in).
  - Example (`web-split-result`): split down on the first pane of
    `row[A, col[C, B]]` gives `row[col[A, ∅], col[C, B]]`. Then split
    right on C gives `row[col[A, ∅], col[row[C, ∅], B]]`.
- The header's own `.head-split` buttons show only in the web's phone
  layout (2.2), so they aren't here.

## The empty pane (`.mosaic-empty`)

What a split, a preset with too few panes, or a move leaves behind.
Measured on a 570.5×378.5 leaf at (823.5, 475.5) (`web-empty-rest-2x`):

- **Box:**
  - margin 3px inside the leaf, so 564.5×372.5 at (826.5, 478.5);
  - border **2px dashed** `--border` #2a2a3d, radius 10px (`--radius`);
  - no fill; the grid's background shows through;
  - text colour `--text-muted` #5a5a78, 13.12px (0.82rem);
  - a column, centred both ways, gap 10px.
  - Slint: the dashed edge needs the dash workaround (`Dashed` in
    `ui/shell.slint` draws 1px; this needs 2px). Dash length as Chromium
    draws a 2px dashed border: 6px dashes, 4px gaps (sampled from `web-empty-rest.png`).
- **Message:** "Drop a panel here or launch a new one", 13.12px, #5a5a78.
  Text box 255.6×15 at (981, 588.3), centred.
- **Chips** (`.me-chips`):
  - a wrapping row, centred, gap 6px, max-width 300px, margin-bottom 2px;
  - box 300×90 here, three rows of 26 + 6 + 26 + 6 + 26.
  - Each chip (`.me-chip`):
    - inline-flex, gap 5px, padding 5px 10px, height 26;
    - radius 999px, border 1px `--border` #2a2a3d;
    - background `--bg-primary` #0a0a0f;
    - text `--text-secondary` #8888a8, 11.5px (the button font; native uses
      the UI font, the known 2.1 gap);
    - icon 13×13 in `--accent-hover` #818cf8;
    - `transition: border-color, color, background .14s`.
  - Measured: Terminal 88.6×26 at (997.5, 613.3), icon at +11,+6.5, text
    at +29,+6; Files 65.3 wide.
  - Hover (`web-empty-hover-chip-2x`): border `--accent` #6366f1, text
    `--text-primary` #e8e8f0, background `--accent-dim`
    rgba(99,102,241,.15). The icon stays #818cf8.
  - Tooltip: "Add <label>".
  - **Which chips**, in order:
    - **Terminal** (icon `terminal`);
    - then each kind of `WB_KIND_META` **not already open in this
      project's workbench**: Files (`folder`), Git (`git-branch`), Kanban
      (`kanban`), Notes (`note`, always shown, since several are allowed),
      Mindmap (`mindmap`), Media (`images`), Factory (`factory`);
    - then **Panel** (`panel`) when the server has widgets (`widgetsOn()`).
  - With nothing open there are 9 chips in rows of 3 / 3 / 3 at this width.
    The rows come from the 300px max: Terminal + Files + Git = 88.6 + 65.3
    + 59 + gaps.
- **Buttons** (`.me-acts`), 10 + 2 = 12px below the chips: a row, gap 6px,
  189×26.
  - "+ Split ▸" (Split right) 72×26, "+ Split ▾" (Split down) 72×26, and ×
    (Remove pane) 33×26.
  - Each:
    - padding 5px 9px, gap 4px;
    - border 1px `--border`, background `--bg-tertiary` #1a1a28;
    - radius 6px (`--radius-sm`), not a pill;
    - colour `--text-secondary` #8888a8, 11.52px (0.72rem);
    - icons 13×13 (`plus`, `x`).
  - The arrow is a text character: ▸ U+25B8, ▾ U+25BE.
  - Hover (`web-empty-hover-split-2x`): border `--accent`, text and icon
    `--text-primary` #e8e8f0. The background stays.
  - Split ▸ / Split ▾ do what the hover controls do, on this empty leaf.
  - × removes the leaf (`mosaicCollapseLeaf`). The root leaf alone becomes
    an empty root leaf.
- **Double-click** on an empty pane does what it does on a header
  (maximise, below).
- **Small empty panes clip:**
  - the content stays centred and the leaf clips it top and bottom
    (`web-maximized-empty-squashed`, 60px tall);
  - narrower than 300 + margins, the chips wrap into more rows;
  - a 340px pane still fits 3 per row (`web-split-result`).

### Launching from an empty pane

- **Terminal:**
  - adds a launcher, the "New terminal" pane (`pend:`, phase 1's Launcher:
    Empty Shell, then CLAUDE with UI Mode and Terminal);
  - **UI Mode** opens the session chooser (2.22, `web-chip-chooser`);
  - **New session** starts a chat: `POST /api/ui-sessions/create`.
- **Where the web puts them (a slip):**
  - the launcher goes into the **first empty leaf in tree order**, not
    necessarily the clicked one (`web-chip-terminal-launcher`: clicked
    top-right, landed bottom-left);
  - when the chat starts, the launcher's key goes away, so its leaf
    **collapses**, and the chat's new key goes into the next empty leaf;
  - with no empty leaf left, the chat is **appended as a new column at the
    right of the root** (`web-chip-chat-appended`).
- **Native, by DESIGN rule 5:**
  - the code's comment says the new panel "lands in the empty leaf";
  - so the launcher goes into **the leaf whose chip was clicked**;
  - the chat that the launcher starts **takes the launcher's leaf** (the
    key is replaced in place, keeping the leaf, its size and its pin);
  - everywhere else, keep `mosaicSync`'s rules (a chat that appears from
    elsewhere fills the first empty leaf, or is appended);
  - name this in PLAN.md as a deliberate difference.
- The other chips open the workbench panels (Files, Git…, phase 3).
  - Until those exist natively, show only the chips the native client can
    open: Terminal, and Panel if it has widgets. Hide the rest; don't draw
    dead chips.
  - The chip row then wraps differently; check it again in phase 3.

## The "+" menu: Fit to screen and Layout

The "+" by the Workbench tab (`#tab-add-term`), 24×24 at (374.4, 53.5):
- radius 6, background #1a1a28, border #2a2a3d, "+" 17px #8888a8;
- hover: accent fill, white.

It opens `#wb-add-menu` (`web-plus-menu`):

- **Placement:** `position: fixed`, top = button bottom + 4 = 81.5,
  left = clamp(button left, 6, window width − 166) = 374.4.
- **Menu:**
  - min-width 150 (150×384 here), padding 5px, gap 2px;
  - background `--bg-secondary` #12121a, border 1px #2a2a3d, radius 10px;
  - shadow `0 6px 22px rgba(0,0,0,.55)`.
- **Items:**
  - 138×32, padding 8px 11px, gap 9px, radius 6px;
  - text 13.76px (0.86rem) `--text-primary` #e8e8f0, left aligned;
  - icon 16×16 in `--accent-hover` #818cf8.
  - Hover: background `--accent` #6366f1, text and icon #fff.
  - Text starts at x416.4, 42px from the menu's left edge.
- **Order:** Terminal, Files, Git, Kanban, Notes, Mindmap, Media, Factory,
  Panel, **Fit to screen**, **Layout ▸**.
  - Fit's icon: polylines 7 8 3 12 7 16 and 17 8 21 12 17 16, plus a line
    3,12 → 21,12. It is not `maximize`; native's AddMenu uses
    `Icons.maximize` and must change to this icon (an arrows-horizontal).
  - Fit's tooltip: "Share the width out again so every pane fits the
    window (pinned panes keep their size)".
  - Layout's icon: a rect 3,3,18,18 rx 2 and a line 12,3 → 12,21 (the
    columns icon).
  - The caret ▸ (U+25B8) is pushed right (`margin-left: auto`) at opacity
    .55, 6.9×16 at x500.5.
- Closes on any click outside (capture phase), and after choosing an
  item.

**Layout submenu** (`.wb-subitems`, `web-layout-submenu`):
- **Placement:** to the right of "Layout": left = Layout's right + 4
  (x522.4), top = Layout's top − 5 (y422.5).
- **Box:** min-width 178 (178×196 here), padding 5, gap 2, the same
  background, border, radius and shadow as the menu.
- **Opening:**
  - it opens when the mouse is over "Layout" or over the submenu, with a
    0.1s fade;
  - it closes 0.35s after the mouse leaves both, fading from 0.25s to
    0.35s;
  - a 10px invisible bridge (`::before`) covers the 4px gap, so moving
    straight across never closes it.
- While it's open, "Layout" shows its hover fill (accent, white).
- **Items:**
  - 166×32, the same padding, radius and font;
  - icon `.lay-ic` 22×16 in #818cf8, white on hover;
  - text starts at x570.4.
  - The icons (viewBox 0 0 24 18, stroke-width 1.6, no fill):
    - **2 columns**: rects (1,1,10,16) and (13,1,10,16), rx 1.5;
    - **3 columns**: (1,1,6.3,16), (8.85,1,6.3,16), (16.7,1,6.3,16), rx 1.2;
    - **2×2 cross**: four rects 10×7.5 at (1,1), (13,1), (1,9.5), (13,9.5),
      rx 1.2;
    - **Left + right split**: (1,1,10,16), (13,1,10,7.5), (13,9.5,10,7.5),
      rx 1.2;
    - **Top + bottom split**: (1,1,22,7.5), (1,9.5,10,7.5), (13,9.5,10,7.5),
      rx 1.2.
  - The labels, exactly: "2 columns", "3 columns", "2×2 cross" (U+00D7),
    "Left + right split", "Top + bottom split".
  - Native today has "2 × 2", "Left + right stack" and "Top + bottom
    pair", which are wrong.
  - **"Top + bottom split" wraps to two lines** in the web: 166×48, text
    93.2×32. The box is shrink-to-fit inside the 150px menu and stops at
    its 178 minimum. That's a CSS slip (rule 5): native draws it on one
    line and lets the submenu grow to fit its widest item. Name it in
    PLAN.md.

### What each preset makes (`mosaicApplyPreset`)

Keys are the workbench's panes in display order. Slots fill in order and
missing ones are empty leaves:

| Preset | Tree (L(i) = i-th pane or empty) |
|---|---|
| cols2 | `row[L0, L1]` |
| cols3 | `row[L0, L1, L2]` |
| cross4 | `row[col[L0, L2], col[L1, L3]]` (0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right) |
| left-right2 | `row[L0, col[L1, L2]]` |
| top-bottom2 | `col[L0, row[L1, L2]]` |

- All sizes are even.
- Panes beyond the slots are then added by `mosaicAddKey`: the first empty
  leaf, or **appended to the root split**, and the root's sizes become
  even.
- A pinned pane keeps its pin wherever it lands.
- Saved; redrawn with motion.

Measured results (leaf boxes, x,y,w,h):

| Chats | Preset | Result |
|---|---|---|
| 4 | cols2, cols3 | `row[0,1,2,3]` 25% each: four 340×758 at x246/593/940/1287, grid scrolls (1393 > 1160) (`web-four-row`) |
| 4 | cross4 | 0 (246,90,570.5,378.5), 2 (246,475.5), 1 (823.5,90), 3 (823.5,475.5) (`web-preset4-cross4`) |
| 4 | left-right2 | `row[0, col[1,2], 3]` 33.3% each: 0 (246,90,378,764), 1 (631,90,378,378.5), 2 (631,475.5,378,378.5), 3 (1016,90,378,764) (`web-preset4-left-right2`) |
| 4 | top-bottom2 | `col[0, row[1,2], 3]` 33.3% each: 0 (246,90,1148,250), 1 (246,347,570.5,250), 2 (823.5,347,570.5,250), 3 (246,604,1148,250) (`web-preset4-top-bottom2`) |
| 3 | cols2, cols3 | `row[0,1,2]`: 378×764 at x246/631/1016 |
| 3 | cross4 | 0, 2 left column; 1 top-right; **empty** bottom-right (`web-preset3-cross4`) |
| 3 | left-right2 | 0 (246,90,570.5,764); 1, 2 stacked at x823.5 |
| 3 | top-bottom2 | 0 (246,90,1148,378.5); 1 (246,475.5,570.5,378.5); 2 (823.5,475.5) (`web-preset3-top-bottom2`) |
| 2 | cols2 | `row[0,1]` 570.5×764 |
| 2 | cols3 | `row[0,1,∅]` 378 each, empty on the right (`web-preset2-cols3`) |
| 2 | cross4 | `row[col[0,∅], col[1,∅]]` |
| 2 | left-right2 | `row[0, col[1,∅]]` |
| 2 | top-bottom2 | `col[0, row[1,∅]]` (`web-preset2-top-bottom2`) |
| 1 | cols2 | `row[0,∅]` |
| 1 | cols3 | `row[0,∅,∅]` |
| 1 | cross4 | `row[col[0,∅], col[∅,∅]]` (`web-preset1-cross4`) |
| 1 | left-right2 | `row[0, col[∅,∅]]` |
| 1 | top-bottom2 | `col[0, row[∅,∅]]` |

Native's `Mosaic::preset` names the presets grid / stack / rows. They must
match the web's ids (`cols2`, `cols3`, `cross4`, `left-right2`,
`top-bottom2`), because they share the saved tree.

### Fit to screen (`mosaicFitToScreen`)

- Sets **every** split's sizes to even, recursively. Pinned panes keep
  their px size, since a pin overrides the share.
- Scrolls the grid back to the left.
- Saves and redraws.
- After 60ms, if the grid still scrolls sideways (scrollWidth >
  clientWidth + 2), an **error** toast (red left edge):
  - with a pin anywhere: "Does not fit: the pinned panes plus the others at
    their minimum width are wider than the window";
  - otherwise: "Does not fit: too many panes side by side for this window,
    even at their minimum width".
  - See `web-fit-does-not-fit`: `row[A, col[row[C,D], B], E]` needs
    340 + 687 + 340 + 14 > 1148.
- After a manual resize:
  - `web-gutter-active` drags the column gutter so the sizes go from 50/50
    to 61.4/38.6;
  - Fit brings back 50/50, leaves 570.5 wide (`web-fit-after-resize`);
  - the inner splits' sizes are also reset.

## Gutters: the look (resize is already in)

- `.mosaic-gutter`, 7px across (`flex: 0 0 7px`), z-index 3.
- A 1px line in the middle (`::after` at 3px):
  - vertical in a row split, horizontal in a col split;
  - full length; colour `--border` #2a2a3d;
  - `transition: background .12s`.
- **Hover:** the line turns `--accent` #6366f1 (`web-gutter-hover-2x`).
  The 7px strip has no fill.
- **While any gutter is being dragged** (`body.mosaic-resizing`): **every**
  gutter's line on the workbench is accent, not only the dragged one
  (`web-gutter-active`).
- **Cursor:** `col-resize` in a row split, `row-resize` in a col split.
- **Locked** (next to a pinned pane, or a split fixed by one):
  - `cursor: not-allowed`, line at opacity .4;
  - tooltip "A pinned pane is next to this divider";
  - a press does nothing;
  - the hover colour still applies (at .4).
  - Example: pinning a pane in the right column locks the column gutter
    too, because that column's width is then fixed (`mosaicFixed` across).
- The drag clamps each of the pair to 10%-90% of their combined share.
  Sizes are saved on release.
- Double-click a gutter: that split's sizes become even (eased).

## Double-click to maximise (`mosaicMaximizeLeaf`)

- A double-click on a pane's **header** or on an **empty pane**, never
  inside a chat or terminal body.
- It gives that leaf 100 − (n−1)·8 % of its **parent split**, and 8% to
  each sibling.
- Double-click again (when it's already ≥ that, ±0.5) to make the split
  even.
- A lone root leaf does nothing.
- Eased, and saved.
- Example: `col[A, ∅]` goes to 92/8, A 696 tall, the empty one 60.6
  (`web-maximized-empty-squashed`).

## Data, endpoints and events

The layout is **client-side only**. No endpoint stores it, and no event
changes it directly. Nothing in moving, splitting, presets, Fit or the
gutters talks to the server.

| What | Method, path | Body → response |
|---|---|---|
| The chats (and terminals) the tree must hold | SSE `GET /api/events?token=…`, event `state` | `{ terminals: [{ id, project, kind: 'ui'\|'terminal'\|'parked', title, order, … }], … }`: already read by live.rs |
| Starting a chat from the launcher (after the chooser) | `POST /api/ui-sessions/create` | `{ project, title?, skipPermissions?, sessionMode?: 'new'\|'resume'\|'compact', resumeId? }` → `{ ok, id, title, order }` (phase 1) |
| The chooser's list (2.22) | `GET /api/ui-sessions/resumable?project=` | (2.22) |
| Closing a chat collapses its leaf | `POST /api/ui-sessions/:id/close` | `{}` → `{ ok }`; the `state` event drops it |
| Not used by the desktop mosaic | `POST /api/ui-sessions/reorder`, `POST /api/terminals/reorder` | only the web's phone layout's flat reorder |

Saved locally:
- web: `localStorage.crundi_wb_mosaic` = `{ <alias>: tree }` as above;
- native: `store.rs` `layouts`, the same JSON per project, so it can be
  compared one-for-one.

## Native today, against this spec (to fix in this step)

- EmptyLeaf: no message colour or size check, no chips, buttons drawn as
  pills (must be radius 6, `--bg-tertiary`, with the plus and x icons),
  solid 2px border (must be dashed).
- AddMenu: preset labels and ids wrong (above); Fit's icon is `maximize`;
  the caret's font and opacity (13.76px ▸ at .55, not 10px muted).
- Hover split controls (`.leaf-ctrls`), header drag with the clone, the
  drop outlines and the pinned refusal: check each against this spec.
- **F24** (from 2.2's review, the Manager asks for it here): in a narrow
  pane, a wrapped paragraph that holds inline code gets a row shorter than
  its text, so the next row draws over it. The row's height must follow
  the real wrapped height.
  - Web: 20px line pitch, lines at y323/343 in a 378px pane
    (`specs/pane-header-narrow/review/r2/chatlog-overlap-native-390x640+240+110.png`).
  - The 1.55 line-height itself can stay a named gap for 2.25.

## Checklist for the reviewer

Drag:
- [ ] The whole header starts a drag after more than 6px; buttons don't;
      the title still renames on a plain click.
- [ ] ⋮⋮ 26×12, 12px #5a5a78, −2px tracking, grab cursor; hidden in full
      screen.
- [ ] Grabbing cursor everywhere while dragging; the source pane unchanged.
- [ ] The clone: the whole pane, opacity .92, rotated 1.5° (or a named
      gap), shadow 0 12 32 .55, following the grab offset.
- [ ] Every leaf shows a 1px dashed #2a2a3d outline (3/2 dashes) at inset
      1px. The one under the pointer shows 2px solid #6366f1 over its
      outer 2px, with square corners. Pinned ones never light up.
- [ ] Drop on a pane swaps (sizes and pins stay with the leaves); drop on
      an empty pane moves and collapses the old leaf.
- [ ] A pinned pane can be dragged but is refused on release with the
      toast "This pane is pinned. Unpin it to move it."
- [ ] Auto-scroll near the grid's left and right edges (60px, ≤ 22px a
      frame); Esc cancels (native addition).
- [ ] The result is saved per project and survives a restart.

Hover split controls:
- [ ] On every non-empty leaf, fading in over .12s on hover, top 3px,
      centred on the leaf, above the header (over the title in narrow
      panes, over CHAT at 570).
- [ ] Buttons 22×18, gap 3, radius 5, #12121a on #2a2a3d, icon 14×11
      #8888a8; hover accent border, #818cf8 icon; tooltips.
- [ ] Split right / down: same-direction parent gets a sibling after the
      leaf, sizes even; otherwise a new 50/50 split; new leaf fades in.

Empty pane:
- [ ] 3px margin, 2px dashed #2a2a3d, radius 10, no fill; centred column,
      gap 10.
- [ ] Message 13.12px #5a5a78.
- [ ] Chips: pill, 26 high, padding 5/10, #0a0a0f, #8888a8 text, 13px
      #818cf8 icon, gap 6, max 300 wide, wrapping; hover accent border,
      #e8e8f0, accent-dim fill.
- [ ] Only chips native can open (Terminal, Panel if widgets).
- [ ] Buttons 26 high, radius 6, #1a1a28, plus / x icons 13px, "Split ▸",
      "Split ▾", ×; hover accent border and #e8e8f0.
- [ ] Terminal chip puts the launcher in **that** leaf; the chat it starts
      keeps that leaf (rule 5, named in PLAN.md).
- [ ] × removes the leaf and collapses its parent.
- [ ] A short empty pane clips its centred content.

"+" menu:
- [ ] Menu at button bottom + 4, 150 wide, padding 5, gap 2, radius 10,
      shadow 0 6 22 .55; items 32 high, 13.76px, 16px #818cf8 icons;
      hover accent fill, white.
- [ ] Fit to screen with its own icon and tooltip; Layout with its icon
      and the ▸ at .55.
- [ ] Submenu at Layout's right + 4, top − 5; opens on hover, closes after
      .35s; 178 minimum, one line per item (rule 5); the five icons and
      exact labels.
- [ ] Each preset's tree for 1, 2, 3 and 4 chats as in the table (extra
      panes appended to the root, even sizes, pins kept).
- [ ] Fit makes every split even, scrolls to the left, and shows the right
      error toast when it still doesn't fit.

Gutters and the rest:
- [ ] Gutter 7px with a 1px #2a2a3d line; hover accent; every line accent
      while one is dragged; col/row-resize cursors; locked: not-allowed,
      .4, tooltip, no drag.
- [ ] Double-click a header or an empty pane: 100 − (n−1)·8 / 8, again:
      even; double-click a gutter: even.
- [ ] Layout motion: 220ms, cubic-bezier(.2,.7,.2,1) for moves, fade and
      scale .985 for new panes; none during a gutter drag.
- [ ] F24: no overlapping rows in a 378px pane with wrapped inline code.
