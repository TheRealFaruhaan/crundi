# 2.18 agent dock: design review, round 1

Fresh build of b5e861d on an isolated test server (tools/test-server.sh 4814).
The same `agent` / `agent-entry` / `entry` frames went into the web (through
its WebSocket) and the native client (testctl `chat-event`); real mouse input
on both. Each `*-WxH+X+Y.png` is web over native, same region.

## Matches
- Badges: order, pill, icons, mono count, spinner / green dot, hover/open
  accent edge (`badges-running`, `badge-hover-done`); tooltips' wording.
- Narrow pane: badges, list (titles cut, › and ✕ kept, foot wraps), panel
  (back, ✕, title and sub cut) (`narrow-*`, `native-narrow-panel.png`).
- List head (title, counts, Dismiss all with red hover), row texts and order,
  marks, row hover (`native-row-hover.png`), ✕ red hover with "Dismiss" tip.
- Panel: back link + hover, title, prompt box, summary, transcript as the
  log draws it, the command's own tool card open, the waiting line, live
  entries and the sub updating (`panel-*`).
- The log's agent row opens the panel without a back link; Dismiss all
  (this kind only, list closes) (`log-agentrow`, `after-dismiss-all`).

## Differences (sent back)
1. **List foot position** (`list-mixed`, `list-commands`): web pins the foot
   to the bottom of the sheet (the body fills, flex 1); native draws it
   straight under the last row, leaving the empty sheet below it.
2. **Panel head sub** (`panel-from-list`, `panel-live`, `panel-command`):
   web: title fills, sub sits at the right against ✕ (flex 0 1 auto);
   native: the sub sits in the middle of the head (both texts stretch).
3. **Panel does not follow its end** (`panel-live-long`,
   `panel-from-row-long`): web opens a panel scrolled to its end and stays
   at the bottom as entries arrive; native opens at the top and stays there
   (Flickable never moved).
4. **Prompt box does not scroll** (`prompt-long`): web caps it at 150 and
   scrolls it (scrollbar); native caps and clips: lines past the cap cannot
   be read.
5. **› in rows sits high** (`list-mixed`, `row-x-hover`): web centres it on
   the row; native draws it at the row's top third.

Nits: hovering a row's ✕ does not keep the row's bg-hover (web does); the
head ✕ is drawn smaller than the web's 15px ✕; panel entries are spaced
wider than the log's (sp-10 plus the rows' own gap).

Known gaps accepted: no arrival/finish animations; no list keyboard (as
other native buttons); "1 tool".
