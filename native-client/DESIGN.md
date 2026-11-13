# Matching the PC client's design: for UI workers and reviewers

The native client **copies the PC client's design exactly**. The PC client is
the web UI (`src/webapp-html.js`, `app/vendor/*.js`) as the Electron client
(`app/`) shows it. The spec is that design, not this repo's earlier screens
and not anyone's taste.

## Rules

1. **Never invent a layout or a look.** Every screen, panel, button and label
   copies an existing one. If the web UI has no such screen (the server
   address step, say), copy the Electron client's (`app/index.html`).
2. **Measure, don't eyeball.** Sizes, paddings, gaps, fonts, colours, radii,
   borders and shadows come from the running page's computed styles
   (`tools/measure.js`). A screenshot shows what differs; the numbers say by
   how much.
3. **Compare at the same window size, with the same data**: 1400×860, the
   same server, project and chat.
4. **Every visible element counts**, down to a badge's padding or a dot's
   ring. If it can be seen, it's in scope.
5. **Copy what the design means, not a CSS slip.** If the old screen shows an
   obvious accident (Electron's Server card stretches its radio buttons), draw
   what was meant and note it. Electron is being replaced, not fixed.
6. **Leave out invitee-only parts.** Invitees (collaborators) use the web UI,
   not the client, so their screens and links stay out.
7. **What Slint can't do** gets the closest equivalent, plus a comment in the
   code naming the gap. Examples: dashes drawn as rectangles; a glow drawn in
   layers; padding inside inline code as thin spaces. List these gaps in the
   report too.

## One constant theme (tokens)

The owner wants one constant theme, not a copy of every one-off number. All
sizes and colours live in `ui/theme.slint` as tokens. Screens use the tokens
and never type a number, apart from 0 and the 1px hairlines.

- **Scales.** Starting points, from the web UI's CSS (its values are mostly
  rem leftovers such as 0.82rem = 13.12px):
  - **Spacing:** 2, 4, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48.
  - **Radius:** 4 (chip), 6 (control), 8 (field), 10 (card or pane),
    12 (bubble), pill.
  - **Font size:** 10, 11, 12, 13, 14, 15, 16, 20, 26.
  - **Weights:** 400, 500, 600, 700.
  - **Line heights:** chat 1.55, UI 1.3.
  - **Colours:** the web's variables (already in Theme).
  - **Sizes that recur:** icons (12, 14, 16, 18), control heights (19, 22,
    28, 34, 36), bars (32, 36, 48), the sidebar (240).

  Add a token only when a value recurs, or when it is a deliberate part of
  the design.
- **Measure to choose a token, not to copy a number.** A measured value goes
  to the nearest token on its scale: 13.12px is the 13 font, 14.08px the 14,
  7px is the 8 gap.
- **One-off asymmetries in the web are accidents. Don't copy them:** a gap of
  7 in one place and 8 in another, padding of 3px 4px 3px 2px. Use the token.
- **Keep only deliberate shapes,** each as a named token: the chat bubble's
  one 3px corner, the sharp underside of a tab.
- **Review:** a difference that comes only from snapping to a token is not a
  finding. Using the wrong token, or a hard-coded number, is one.

## The method

```
native-client/tools/test-server.sh                 # an isolated dev Crundi on :8889, seeded
node native-client/tools/web-session.mjs           # a web session for the browser
```

1. **The web screen.** Open `http://127.0.0.1:8889/` in a hidden browser at
   1400×860 (Crundi's `browser_open`, on: server). Put the session in
   localStorage (`crundi_token`, `crundi_refresh`) and reload.
   - Skip the What's-new tour and wait for the splash to fade before taking a
     screenshot.
   - Bring the screen into each of its states: hover, open menus, empty,
     narrow, with data, without data, unread, error.
2. **The numbers (to pick tokens).** Run `tools/measure.js` through `browser_eval`, with
   selectors for every element on the screen. Read the boxes, the text boxes
   (where text actually sits), and `::before` / `::after`. Dots, bars, rings
   and underlines are often pseudo-elements.
3. **The native screen.** Start the client with `CRUNDI_TEST_PORT`, sign in
   and snapshot with `test/look.mjs` (its `ui:` actions reach the same
   states).
4. **Compare.** Run `tools/compare.sh web.png native.png out <regions…>` for
   each region: top bar, sidebar header, rows, footer, tab bar, pane header,
   the chat log, the composer, menus, dialogs. Use `ZOOM=2` for small parts.
5. **Fix and repeat** until the regions match or only listed Slint gaps are
   left.

## What to check (each of these has gone wrong before)

- **Tokens only:** no number typed in a screen; every size and colour comes
  from `Theme`.
- **Padding and text position inside small things:** badges, pills, chips,
  buttons. Check where the text box starts (the measured text box), not just
  the outer box.
- **Fonts:**
  - **Family:** the UI's system sans, or the mono stack. Mono lands on
    Consolas on Windows, Menlo on macOS and DejaVu Sans Mono on Linux.
  - **Size, weight, letter-spacing, upper case:** `PROJECTS` is 12px/600,
    .6px tracking.
  - **Line height:** the chat is 1.55, which is `line-height-factor` ≈ 1.33
    in Slint. Markdown text can't set it, so pad each block 2.5px above and
    below.
- **Resting state versus hover:** measure each element without the mouse over
  it. The tab bar's "+" is grey at rest and indigo only on hover.
- **Pseudo-elements:**
  - the active project's 3px gradient bar;
  - a tab's 2px underline, inset 12px;
  - the connection dot and its ring;
  - the Browser tab's dot.
- **Borders:** solid, dashed (Add project, the result line) or dotted (the
  session id). Check the width and the colour, and that a focused pane's edge
  is a 2px accent (a border plus a ring).
- **Shadows and glows:** the top bar's shadow, Send's glow, the menus'
  shadows.
- **Icons:** the right icon from `ICON_PATHS` (`scripts/gen-icons.mjs`), at
  the measured size (tabs 15px, inbox 16px, meta 13px), tinted as the web
  tints it.
- **Spacing in content:** paragraph gaps (8px), list items (bullets in a 20px
  gutter, 22px apart), and the 10px between entries.
- **Number formats:** "2.6s · $0.1158", "1m ago", "wk 19%", "5d 11h".
- **Behaviour that shapes the look:**
  - the tab strip scrolls rather than widening the window;
  - panes are at least 340px, then the grid scrolls;
  - headers fold their tools into "…" under 560px;
  - the composer stacks in a narrow pane;
  - long titles elide.
- **Data-driven variants:** usage bars only with usage data; the Inbox border
  and badge only when unread; dots by state, with the ring for needs-input
  and working. Check with data and without.
- **Empty states and edge cases:** no project, an empty pane, the launcher,
  a long chat, errors.

## Reviewing a screen

A reviewer passes a screen only with:

1. side-by-side crops (web over native) of **every** region of the screen, in
   each of its states;
2. a measurement sheet (`tools/measure.js` output) for the elements on it,
   with the native values matching;
3. a list of every remaining difference, each one either fixed or a named
   Slint gap with its workaround.

Any difference that isn't explained means the screen goes back. "Looks about
right" isn't a pass.
