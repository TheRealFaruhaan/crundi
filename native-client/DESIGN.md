# Matching the PC client: for UI workers and reviewers

The native client keeps the PC client's **layout and user experience**:
- the same screens, panels and parts, arranged the same way;
- the same controls in the same places, doing the same things;
- the same states, flows and keyboard behaviour.

It does **not** copy the web UI pixel by pixel. The look comes from one
constant theme (`ui/theme.slint`) that keeps Crundi's character: dark
surfaces, indigo accent, flat icons, rounded cards.

The PC client is the web UI (`src/webapp-html.js`, `app/vendor/*.js`) as the
Electron client (`app/`) shows it. Where the web has no screen for something
(the server address step, say), the Electron client's is the reference.

## Rules

1. **Same layout and experience; never invent or rearrange.** Every screen,
   panel, menu, dialog and control the PC client has for a feature is there,
   in the same place and order, with the same wording and behaviour. Nothing
   is moved, merged, simplified or "improved" without the owner asking.
2. **One constant theme, not the web's numbers.** All sizes and colours are
   tokens in `ui/theme.slint`. Screens never type a number, apart from 0 and
   1px hairlines.
   - **Spacing:** 2, 4, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48.
   - **Radius:** 4 (chip), 6 (control), 8 (field), 10 (card or pane),
     12 (bubble), pill.
   - **Font size:** 10, 11, 12, 13, 14, 15, 16, 20, 26.
   - **Weights:** 400, 500, 600, 700.
   - **Line heights:** chat 1.55, UI 1.3.
   - **Colours:** the web's palette.
   - **Sizes that recur:** icons, control heights, bars, the sidebar.

   Pick the token that fits by eye or a quick measurement. Don't copy
   one-offs (13.12px, a gap of 7 beside a gap of 8) or the web's accidents
   (Electron's stretched radio buttons). Deliberate shapes, such as the chat
   bubble's one sharp corner, become named tokens.
3. **Every state and behaviour counts:** empty, loading, with data, long
   content, hover, focus, open menus, narrow, error, badges and unread. That
   includes what the keyboard does, what updates live and what each click
   does.
4. **Leave out what isn't the PC client's:** invitee-only parts (invitees use
   the web) and Electron itself (the native client replaces it).
5. **Slint limits** get the closest equivalent, plus a comment and a line in
   PLAN.md "Known gaps".

## The method

```
native-client/tools/test-server.sh         # an isolated dev Crundi on :8889, seeded
node native-client/tools/web-session.mjs   # a web session for the browser
```

1. **The web screen.** Open `http://127.0.0.1:8889/` in a hidden browser at
   1400×860 (Crundi's `browser_open`, on: server), with the session in
   localStorage. Skip the What's-new tour. Bring the screen into each of its
   states and take a screenshot of each.
2. **Its behaviour.** From the code (src/webapp-html.js, app/vendor/*.js):
   what each control does, keyboard handling, live updates and the events
   behind them, and the endpoints it calls. `tools/measure.js` is there for a
   quick check of a size or an order, not for a spec sheet of every element.
3. **The spec** (`native-client/specs/<screen>.md`):
   - the parts and their arrangement;
   - every control and what it does;
   - the states, with web screenshots;
   - the data and endpoints;
   - a checklist.
4. **The native screen,** built from the theme's tokens and the shared
   widgets. Snapshot it in the same states (`test/look.mjs`) and put the two
   side by side (`tools/compare.sh`).

## Reviewing a screen

A reviewer passes a screen when:
- **Layout:** every part and control of the web screen is there, in the same
  arrangement and order, in every state;
- **Behaviour:** it acts the same, for every control, keyboard path, live
  update and error;
- **Theme:** it uses tokens only (no typed numbers), and looks consistent
  with the rest of the client;
- **Proof:** side-by-side snapshots of each state are saved under the spec.

Not findings: pixel differences, a padding one token step away from the web,
a font a size off when it fits the scale. Findings: a missing or moved part,
a different order, a missing state, different behaviour or wording, a typed
number, or something that looks out of place in the theme.
