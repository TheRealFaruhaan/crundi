# 2.2 Pane header of a UI chat, narrow pane (< 560px)

The same `.term-head` as 2.1, when the header's **content box** is under
560px wide. The secondary tools fold into a menu behind "…" (`.term-more`).
Everything 2.1 says about each button (what it does, its tooltips, its
colours) still holds; this spec covers only what folding changes.

Out of scope here: dragging panes, and the split buttons that float over the
header on hover (`.leaf-ctrls`), which are 2.3. They still show over a narrow
header when the mouse is over the pane: see `web-hover-more.png`. The dot and
badge are 2.4, the dialog the lock opens is 2.12.

Source: `src/webapp-html.js`:
- CSS ~779-845: `.term-head { container-type: inline-size }`, `.term-head-tools`,
  `.term-head.tools-open`, `.term-more`, `.head-split`, then
  `@container (max-width: 559px)` and `@media (max-width: 768px)`;
- CSS ~877-883: `.chat-secrets-btn .csb-label`, shown under 560px;
- `headTools()` ~8892, `closeHeadMenus()` and the document click listener
  (capture phase) ~8940;
- the click actions `head-more`, `head-split`, `chat-secrets`, `pane-full`
  ~16373-16396, and `togglePanePin()` ~8208.

Screenshots are in `pane-header-narrow/`:
- `web-<state>.png` is the whole 1400×860 window;
- `web-<state>-head2x.png` is a crop scaled ×2. Most are the first pane's
  header and menu (380×72 at +245+89). The rest is 380×32 for the header
  alone, 570×32 for the 560px edge, 345×100 for the wrapped menu,
  380×72+630+89 for the second pane, and 1150×32 for the rest overview.

They come from an isolated dev server on :4810, with project Demo and three
chats side by side, each 378px wide:
- "Fruit list and directory", the seeded chat, with 2 secrets granted until
  the chat ends;
- "Second chat", no secrets; later renamed to a long typed title;
- "Third chat".

Ignore two things in them:
- the walking robot (the logo mascot);
- the split icons in the middle of the header in the hover shots. Those are
  `.leaf-ctrls` (2.3), shown while the mouse is over the pane.

## When it folds

- The header is a size container, so the rule reads its **content box**:
  header width − 6 (padding 2 left and 4 right). That is the pane's width
  − 8 (1px pane border on each side, then the padding).
- **Narrow when the pane is ≤ 567px wide; wide from 568px.** We checked it by
  setting the leaf to 566 / 567 / 568 / 569px:
  - 566 and 567: "…" shown, short title;
  - 568 and 569: tools in the header, long title.
  - Screenshots: `web-edge-567-narrow`, `web-edge-568-wide`.
- The long title follows the same edge. It takes over only from 560px of
  content box, which is the same test.
- A full-screen pane is 1398 wide, so it is always wide. Its tools are back
  in the header (2.1).
- A window under 769px switches the whole web UI to its phone layout (see the
  end of this spec). The native window's minimum width is 900px, so it never
  gets there.

## Layout

Narrow header, left to right:

```
[⋮⋮][●][Title…………]   (spacer)   [⛶][…][×]
```

- `.term-head-tools` (CHAT tag, lock, A-, reset, A+, pin) leaves the row and
  becomes an absolutely placed menu that is hidden until opened.
- Full screen, "…" and × stay in the header, in that order, 4px apart.
- **The header is 26px high, not 28.** The pin (21px), the tallest item, is
  gone from the row, so the tallest left is 19px (full and "…"): 3 + 19 + 3
  of content plus the 1px bottom border. Every item sits 1px higher than in
  2.1, and the pane body starts at y117 instead of y119.
- The title loses its 160px cap: `max-width: none; flex: 0 1 auto;
  min-width: 0`. It grows to its text, then shrinks and elides when space
  runs out. The spacer can shrink to 0, but **its two 4px gaps stay**, so a
  title that is too long ends 8px before the full button.
- The short form is always shown, never the long one.

The menu (`.term-head.tools-open .term-head-tools`):

```
                       ┌──────────────────────────────────────────────┐
                       │ (CHAT) [🔒 2 Secrets for this chat] [A-][⟲][A+][📌] │
                       └──────────────────────────────────────────────┘
```

- Its box: `position: absolute; top: calc(100% + 2px); right: 4px`, against
  the header's padding box, which is 25px high. So the menu's top is the
  header's top + 27, **1px below the header's bottom border**. Its right edge
  is the header's right edge − 4.
- Flex row: `flex-wrap: wrap`, `justify-content: flex-end`, `align-items:
  center`, gap 6 (between items and between rows), padding 8.
- Max width: the header's width − 8 (`calc(100% − 8px)`). Otherwise as wide
  as its content.
- Fill `#12121a`, border 1px `#2a2a3d`, radius 6, shadow `0 6px 20px
  rgba(0,0,0,.5)` (`--shadow-md`).
- It sits above the pane body: the header gets `position: relative;
  z-index: 6` while open, and the menu has z-index 30.
- It never leaves its pane, because of the max width. The pane clips with
  `overflow: hidden`, but the menu is at most 65px high, so it always fits.
- Items in DOM order:
  1. the two `head-split` buttons, which are `display: none` here (see
     "Split right / split down" below);
  2. the plan pill, Factory tag or collaborator tag when they apply (2.19
     and later);
  3. the CHAT tag;
  4. the lock with its label;
  5. A-, reset, A+;
  6. pin.

## States

| State | Screenshot | What changes |
|---|---|---|
| Rest | `web-rest` (1150 crop: all three panes) | 26px header; full, "…" and × at the right; short title |
| Hover "…" (closed) | `web-hover-more` | "…" icon `#e8e8f0`, border `#6366f1`, as for every 2.1 button |
| Menu open | `web-menu-open` | the menu below the header; "…" icon `#818cf8`, border `#6366f1`, fill unchanged `#0a0a0f`. This holds **while hovered too**: `.term-head.tools-open .term-more` beats `:hover` |
| Hover lock, secrets on | `web-hover-secrets-on` | nothing changes: stays yellow (as in 2.1) |
| Hover A- / reset / A+ / pin | `web-hover-aminus`, `-reset`, `-aplus`, `-pin` | text or icon `#e8e8f0`, border `#6366f1` |
| Hover CHAT tag | (none) | no hover style; not clickable |
| Pinned, 2 secrets | `web-menu-pinned-secrets2` | pin in its pinned style (2.1: `#818cf8` on `rgba(99,102,241,.15)`, border `#6366f1`, icon turned −30°); lock reads "🔒 2 Secrets for this chat" in yellow `#f59e0b` with a yellow border. With the menu closed, nothing in the header shows the pin |
| No secrets granted | `web-menu-secrets-none` | lock reads "🔒 Secrets for this chat", grey `#8888a8`, border `#2a2a3d`; no count |
| Hover lock, none granted | `web-hover-secrets-none` | text and icon `#e8e8f0`, border `#6366f1` |
| Lock clicked | `web-secrets-clicked` | menu closed, chat secrets dialog open (2.12) |
| Menu wraps (340px pane) | `web-menu-wrapped-340` | max width 330 is less than the 355 needed, so pin goes to a second row, **right-aligned**; menu 330×65 |
| Long typed title | `web-long-title-typed` | title elides at "Investigate the flaky login test…", 8px before full |
| Armed × | `web-close-armed` | "Sure?" (44.8 wide) pushes full and "…" 17px left, as in 2.1; it closed the menu open in pane 3 |
| Edge: pane 567 | `web-edge-567-narrow` | folded, short title, 26px header |
| Edge: pane 568 | `web-edge-568-wide` | 2.1's wide header: 28px, long title, every tool inline |
| Window 760 (reference only) | `web-window-760-reference` | the phone layout's menu with large buttons; never reached in native |

## Measurements (web, 1400×860, three panes, first pane focused at x=246, 378 wide)

Colours and fonts are as in 2.1: buttons are 11px "Arial", which means the
UI sans in native (2.1's font note); the CHAT tag is the system sans.

Header (narrow):

| Element | Box x,y,w,h | Text box | Notes |
|---|---|---|---|
| `.term-head` | 247,91,376,26 | | fill `#12121a`, bottom border 1px `#2a2a3d`, padding 3/4/3/2, gap 4 |
| `.term-drag` | 249,97.5,26,12 | 252,96.5,20,14 | as 2.1, 1px higher |
| `.term-status-dot` | 279,100,7,7 | | as 2.1, 1px higher |
| `.term-title` "Fruit list and directory" | 290,94.5,164.2,18 | 296,96.5,152.2,14 | 12px/600, padding 2/6, radius 6; **no max width** (164 here, over 2.1's 160 cap) |
| spacer | 458.2,·,71.1,0 | | flex 1; can reach 0 |
| `.term-full` | 533.3,94,25,19 | icon 540.3,98,11×11 | as 2.1 |
| `.term-more` "…" | 562.3,94,25,19 | icon 569.3,98,11×11 | `inline-flex`, padding 3/6, border 1px `#2a2a3d`, fill `#0a0a0f`, radius 6, colour `#8888a8`; icon `more` (three dots, r 1.4, at x 5/12/19 of 24), 11px; tooltip and aria-label "More", `aria-haspopup` |
| `.term-close` × | 591.3,94.5,27.7,18 | 599.3,95.5,11.7,16 | as 2.1; right edge 619 = header right 623 − 4 |
| × armed "Sure?" | 574.2,96,44.8,15 | | full at 516.2, "…" at 545.2 (worked out from 2.1's widths) |
| long typed title (pane 2 at x=631) | 675,94.5,235.3,18 | ellipsis | ends 910.3; spacer 0 at 914.3; full at 918.3 |

Menu, first pane, 2 secrets (`web-menu-open`):

| Element | Box x,y,w,h | Text box | Notes |
|---|---|---|---|
| `.term-head-tools` (open) | 263.7,118,355.3,39 | | right 619 = header right − 4; top 118 = header top + 27; 39 = 1 + 8 + 21 + 8 + 1 |
| CHAT tag | 272.7,130,47.6,15 | 280.7,132,31.6,11 | as 2.1 (9.92px/700, upper case, .6px tracking, `#818cf8`, border `#2a2a3d`, radius 99, padding 1/7) |
| lock, on, 2 | 326.3,127.5,151.7,20 | | `inline-flex`, gap 4, padding 3/6, yellow text and border |
| › lock icon | 333.3,131.5,12,12 | | |
| › count "2" | 349.3,132,7,11 | 349.3,131,7,13 | 11px |
| › label "Secrets for this chat" | 360.3,132,110.7,11 | 360.3,131,110.7,13 | `.csb-label`, 11px, same colour as the button |
| A- | 484,128,25.3,19 | 491,131,11.3,13 | as 2.1 |
| reset | 515.3,127.7,25,19.6 | icon 522.3,131.7,11×11 | as 2.1 |
| A+ | 546.3,128,30.8,19 | 553.3,131,16.8,13 | as 2.1 |
| pin | 583,127,27,21 | icon 590,131,13×13 | as 2.1; ends at 610 = menu right − 1 − 8 |

The steps between items are all 6: 272.7 + 47.6 + 6 = 326.3, and so on.
Every item is centred on y137.5, the middle of the 21px row.

Second pane, no secrets (`web-menu-secrets-none`): the menu is 659.7,118,344.3,39.
The lock is 722.3,127.5,140.7,20: lock icon at +7, then 4px, then the label
at 745.3. The count is `display: none` (`.csb-n:empty`), so it takes no gap.

Wrapped, pane 340 wide (`web-menu-wrapped-340`), header 247,91,338,26:

| Element | Box |
|---|---|
| menu | 251,118,330,65 (max width 338 − 8); 65 = 1 + 8 + 20 + 6 + 21 + 8 + 1 |
| row 1 | CHAT 267.7,129.5; lock 321.3,127,151.7×20; A- 479,127.5; reset 510.3,127.2; A+ 541.3,127.5, ending 572.1 |
| row 2 | pin 545,153,27×21: alone, right-aligned to 572 |

Each row is centred on its own tallest item: 20px for row 1, 21px for row 2.

## Behaviour

### Opening and closing
- **"…" toggles** this header's menu (`head-more`: `closeHeadMenus(head)`,
  then toggle `.tools-open`).
- **Only one menu is open at a time.** Opening one closes any other: we
  clicked pane 1's "…", then pane 2's, and only pane 2's stayed open.
- The click also focuses its pane, like any click in a pane.
- **A click anywhere outside** the menu and outside "…" closes it.
  - In the web this is a capture-phase document listener, so **the click
    still does its own job**. We clicked pane 2's title while pane 1's menu
    was open: the menu closed *and* the rename input opened.
  - Native: close on any pointer press outside the menu and its "…", and let
    the press go on to whatever is under it. Don't swallow it.
- **Clicks inside the menu don't close it**, whether on its padding, the
  CHAT tag, A-, reset, A+ or pin. Checked for each:
  - A- / reset / A+ change the text size (2.1) and the menu **stays open**,
    so a person can step the size several times;
  - pin toggles the pin (2.1) and the menu **stays open**, now drawing the
    pinned style. The web re-renders the grid, but the cell DOM is reused, so
    `.tools-open` survives.
- **The lock closes the menu** (`closeHeadMenus(null)`) and opens the chat
  secrets dialog (2.12).
- **Full screen** (the header's ⛶) closes the menu and goes full screen. The
  full-screen pane is wide, so its tools are inline.
- **×** in the header: its click is outside the menu, so the menu closes, and
  the first click arms × (2.1).
- **Esc:** the web has no handler. We dispatched Escape and the menu stayed
  open.
  - **Native: Esc closes the menu**, and only the menu. With the menu open,
    Esc must not also leave full screen or cancel anything else.
  - This matches 2.1's rule that menus close before Esc reaches the pane,
    and the Manager's brief.
  - It's a deliberate addition (DESIGN rule 5); note it in the code.
- Live updates leave the menu alone. A `state` or `chat-secrets` event
  updates the title, dot and lock count in place, with the menu still open.
- Native, for cases the web leaves undefined:
  - close the menu when its pane goes away;
  - close it when the pane becomes wide (≥ 568px). The web hides it there
    (`display: contents`), but leaves `.tools-open` set, so it would
    reappear open when the pane narrowed again. That's a slip.

### The items
Each item does exactly what its 2.1 header button does, with the same
tooltip and the same endpoint. Nothing new on the server.
- **CHAT tag:** static.
- **Lock:**
  - it carries a text label in the menu, "Secrets for this chat", after the
    icon and the count;
  - the count and the yellow `.on` style follow 2.1's rules (grants that
    haven't expired);
  - it isn't drawn at all without the `chat-secret-preauth` capability, nor
    for collaborators;
  - the tooltip stays 2.1's: "Pre-authorise secrets for this chat" or "n
    secret(s) pre-authorised for this chat";
  - aria-label "Secrets for this chat".
- **A- / reset / A+:** 2.1, unchanged.
- **Pin:** 2.1, unchanged. Saves `{w, h}` in the layout, e.g.
  `{"t":"leaf","key":"live:…","pin":{"w":378,"h":764}}`.

### Split right / split down: not in the PC client's menu
`headTools()` does put two `head-split` buttons first in the menu, but
`.head-split { display: none }` hides them everywhere except
`@media (max-width: 768px) … .term-grid.mosaic.mobile`. That is the phone
layout, which the web turns on for any window ≤ 768px (`isMobileTerm()`).
- At 1400×860 we measured them: box 0×0. At 760 wide they were still hidden
  until the grid re-rendered into the mobile layout. See
  `web-window-760-reference`.
- **The native window is at least 900px wide, so the PC client never shows
  them.**
  - Copy that: **no split items in the native menu.**
  - A narrow pane splits through `.leaf-ctrls`, the two buttons that float
    in the middle of the header on hover (visible in `web-hover-more`).
    That is 2.3 and works the same in narrow and wide panes.
- **This differs from the Manager's brief**, which lists split right and
  split down as menu items. If the Manager still wants them, here they are
  (phone layout):
  - `term-font-btn` frame, padding 3/6, with an SVG 14×11 (viewBox 24×18,
    stroke 1.8, `currentColor`);
  - split right: rect 1,1,22×16 rx 1.5 with a vertical line at x 12;
  - split down: the same rect with a horizontal line at y 9;
  - tooltips "Split right" and "Split down";
  - the action: `mosaicSplitLeaf(tree, leafId, 'row' | 'col')`, then
    re-render and close the menu. That is the same as `leaf-split`, which
    2.3 builds.

### The phone layout's menu (reference only)
The 768px media rule also enlarges the menu's buttons:
- min 38×34, padding 6/10, 13px;
- the lock becomes 181×34;
- the menu at 760 wide was 368×92 and wrapped.

It can't happen in the native client (window ≥ 900), so it isn't part of
this screen.

## Data and endpoints

Nothing beyond 2.1. The items call 2.1's endpoints:
- text size and pin are client-only;
- the lock opens 2.12's dialog;
- the lock's count comes from `GET /api/secrets/chat` and the SSE
  `chat-secrets` `{sessionId, grants}`;
- the title, the long title and the dot come from SSE `state`.

Opening and closing the menu is client-only and isn't saved.

## Native notes (what 2.1's code has today, against this spec)

These show up when the native narrow header is compared with the web's.
They are raised as findings.
- `ui/workbench.slint`: `narrow: p.w < 560px`. The web folds at pane width
  ≤ 567, which is header content < 560. If `p.w` is the pane width, use
  `p.w < 568px`, i.e. `p.w − 8 < 560`.
- `src/head.rs` `title_shown(…, head_w)` is called with `l.w − 2`, the
  header's border-box width, and tests `head_w >= 560`. The web tests the
  content box: header − 6, i.e. pane − 8.
- `title_shown` returns a 160 cap for the short form. In a narrow header
  there's no cap: the title takes what the row leaves (elide), so a narrow
  pane shows "Fruit list and directory" in full at 164px.
- `PaneHead` is a fixed `height: 28px`. Narrow is 26px, with every item 1px
  higher, and the pane body starts 2px higher.
- The "…" button exists (`chat.slint:335`, y 4 / 19 high, which is right for
  a 28px header but must be y 3 in 26). It needs the menu, its open style
  and the close rules above.

## Checklist for the reviewer

- [ ] Folds exactly at the web's edge: pane 567 is narrow, pane 568 is wide. The long title switches at the same edge.
- [ ] Narrow header is 26px high (25 + 1px border). Drag handle, dot and title are 1px higher than wide; the pane body starts 2px higher.
- [ ] Header row: drag, dot, title, spacer, then full (25×19), "…" (25×19, 11px `more` icon), × (27.7×18), 4px apart, × 4px from the right.
- [ ] Narrow title: short form only, no 160 cap, elides when out of room; 8px left before full when the spacer is 0.
- [ ] "…" at rest `#8888a8` / border `#2a2a3d`; hover `#e8e8f0` / `#6366f1`; open `#818cf8` / `#6366f1`, also while hovered; tooltip "More".
- [ ] Menu: 1px below the header's bottom line, right edge 4px in from the header's; padding 8, gap 6; fill `#12121a`, 1px `#2a2a3d`, radius 6, shadow 0 6 20 rgba(0,0,0,.5); drawn over the pane body.
- [ ] Menu order: (plan / Factory / collaborator tags when present) CHAT, lock, A-, reset, A+, pin. **No split items.**
- [ ] Menu is at most header − 8 wide; it wraps right-aligned. At 340px, pin is alone on row 2 at the right; menu 330×65.
- [ ] Lock in the menu:
  - icon 12, then the count if any, then "Secrets for this chat", gap 4, 11px;
  - 151.7×20 with a count, 140.7×20 without;
  - yellow when granted (hover doesn't change it), grey otherwise (hover goes white with an accent border);
  - absent without the capability.
- [ ] Each item's hover: text or icon `#e8e8f0`, border `#6366f1`. The CHAT tag has none.
- [ ] Pinned pin in the menu: accent style, icon turned −30°.
- [ ] "…" toggles; opening one pane's menu closes any other; it focuses the pane.
- [ ] A click outside closes the menu **and still acts** (e.g. it opens a rename on another title).
- [ ] A-, reset, A+ and pin act and leave the menu open. The lock closes it and opens the secrets dialog. Full screen closes it.
- [ ] Esc closes the menu only. It doesn't also leave full screen.
- [ ] The menu stays open through live `state` and `chat-secrets` updates (the count changes inside it). It closes when the pane goes wide or away.
- [ ] Armed × in a narrow header pushes full and "…" left; it closes an open menu.
- [ ] Web-over-native crops of every state in the table (`web-*-head2x.png` against native), including both sides of the edge and the wrapped menu.
