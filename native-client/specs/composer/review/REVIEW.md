# 2.5 Composer and Stop: design review (approved)

Isolated server `tools/test-server.sh 4814` (seeded chat + Long job (Bypass all,
working), Question (needs input), Gone (exited)); native client on DISPLAY=:99,
testctl :4815, fresh release build of 016583a. Each image: the web (spec
screenshot, top) over the native client (bottom), ×2. Wide states are taken in
full screen (a 1400px pane); narrow ones in 340px panes.

| Image | State | Result |
|---|---|---|
| rest, focus, typing | idle; focused (accent border + ring); one line | match |
| long-max | ten lines: grown to composer-max, scrolled to the caret, buttons on the last line | match (8 vs 9 lines: named gap) |
| enter-newline, narrow-newline-allpanes | Ctrl+Enter mode: thumb right, hint changed, focus back in the box; every pane follows (named difference) | match |
| running, stop-hover, stop-armed, stop-interrupted | working…, Stop between attach and Queue; red hover; "Sure?" on red-dim; second click → idle in under 0.1 s, Send back | match |
| needs-input, narrow-wrap | "needs your input", Queue, no Stop; the cost wraps to a second line | match |
| exited | "Session ended.", dimmed, label exited, Send kept; a click gives no focus ring | match |
| send-error-window | refused send: "Session is not running" in the log, text back in the box, chat stays exited (named difference) | match |
| sid-copied-window | toast "Session id copied: <uuid>"; Ctrl+V into a box pasted the full uuid | match |
| a-plus, a-minus-9 | text scales; attach, Stop, clock, box min/max do not | match |
| hover-* | attach/clock accent, mode accent border, switch label, id, Send accent-hover; tooltips with the web's titles | match |
| narrow, narrow-armed | box alone; attach, clock, Stop, stretched Send; icon-only mode, no id | match |

Behaviour checked live: Enter sends / Shift+Enter newline / Ctrl+Enter in Enter
mode does nothing; blank box does nothing; optimistic working 50 ms after Enter;
Stop auto-disarms after 3 s and not on pointer leave; Enter mode persisted
(`chat_enter: newline` in session.json, kept after a restart); no box takes the
focus at start; `tools/tokens.sh`: 0 typed numbers.

Earlier nits verified: F45 launcher ">_" full size, F46 Add project dashes with
round corners (F46-add-project-corner-x6.png), F47 a narrow launcher shows Pin
and × (F45-F47-launcher-window.png), F48 the sign-in card's shadow corners have
no seam step (pixel run below the corner: 8,8,8,7,7, smooth).

New nit: at composer-max the web box shows its scrollbar; the native box scrolls
with no indicator.
