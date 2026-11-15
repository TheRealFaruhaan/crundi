# 2.9 Queued messages: design review, round 1

Native build at 3357757, on a fresh `tools/test-server.sh 4814`, 1400×860, DISPLAY=:99.
Chats: the seed chat (idle), "Third chat" (340 wide, asks permission), "Second chat"
(wide, asks permission) and "Narrow chat" (bypass, working). Real Claude turns
(`timeout 900 tail` behind a Bash prompt); "waiting" and "started" faked with the
`stub send` / `chat-event queued-state` hooks, the errors with `stub`.

Crops are web (top) over native (bottom); `full-native-*.png` are whole-window snapshots.

| State | Crop | Result |
|---|---|---|
| needs input, nothing queued (Queue) | full-native-start | matches |
| one held line | full-native-held-one | matches |
| two held lines | held-two.png, held-two-2x.png | matches (rows tighter: not a finding) |
| held, hover (accent edge, accent-dim fill) + tooltip | hover.png, native-hover-tip.png | matches (sampled: edge #6366f1 vs #36377c at rest; fill 29,30,57 vs 26,26,47) |
| long held batch, trimmed | trimmed-2x.png, native-very-narrow-long-unclamped.png | **differs: no 4-row clamp** |
| ↑ take-back with box text | taken-back.png | matches (held lines, then the box's text) |
| click take-back | click-taken.png | matches |
| narrow (340), fs 1 | narrow-fs1-2x.png | matches (note fits on one line) |
| very narrow, A+ ×2 (note wraps, label centred) | very-narrow-aplus-2x.png | matches; fs scales all three texts |
| handed over, recallable | handed-recallable-2x.png | matches |
| handed over, hover (no change) + tooltip | native-handed-hover-tip.png | matches; tooltip cut at the window edge (nit) |
| handed taken back (cancel-queued, server's pending empty) | handed-taken-back.png | matches |
| SENT waiting (stub send uuid) | sent-waiting-2x.png | matches; ↑ and click do nothing |
| SENT started (queued-state started) | sent-started-2x.png | matches; ↑ and click do nothing |
| take-back fails (stub) | takeback-error.png | matches: error row, drawer stays |
| refused send (stub) | send-failed.png | **differs: caret stays at 0, typing lands in front** |
| restart with handed + held | full-native-after-restart | handed back from history, held in the box (named difference) |
| both drawers | full-native-both | matches: slate over dashed over input |
| both taken back by a click on the dashed one | full-native-both-taken | matches: "handed\nheld" |
| prompt allowed: held flushed as ONE | flushed.png | matches; server pending "a\nb" as one |
| exit with handed + held | full-native-exit | handed in the log as a user message, held in the box (named difference) |
| turn end (interrupt) | full-native-turn-end | drawer empties, both lines in the log, answered |

## Findings

1. **Blocking: the body is not clamped at 4 rows.** Web: at most 4 rows, "…" at the cut,
   hint "Shown trimmed · …" (`web-held-long-trimmed`). Native: every row is shown
   (6 rows in trimmed-2x.png, 13 in native-very-narrow-long-unclamped.png); only the
   "Shown trimmed ·" prefix appears. In a short pane a long held batch pushes the input
   row out of the pane: the box can't be seen, typed into or focused (typed keys and ↑
   went nowhere), only a click on the drawer gets it back. `QueueDrawer`'s body `Text`
   has `max-height` + `overflow: elide`, but the drawer's height is
   `col.preferred-height`, which ignores it. Clip the body to `queue-rows × line` (and
   the "…" or the named gap the spec allows).
2. **Blocking: a give-back leaves the caret where it was.** Web: `giveBack` sets
   `input.value`, which puts the caret at the end. Native: the box's text is replaced
   but the caret stays (at 0 in an empty box), so the next typing goes in front of the
   given-back text: "still typingThis one will not go through" (send-failed.png). Same
   path for the held text put back on exit (app.rs ~1614). Caret to the end, without
   taking focus.
3. Nit: the handed-over drawer's tooltip in the right-hand pane runs past the window
   edge and is cut ("…to the message box" lost). Shared Tooltip; web tooltips are the
   system's.

F59 verified (os-drop with file-hover false: no panic, the focused chat got the path).
F60: the "Unit tests" step is in native-client.yml as asked; proven on the next CI run.
