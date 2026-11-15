# 2.13 Questions: design review, round 1

Isolated test server (tools/test-server.sh 4814) with the spec's seed: Pick
(One question, Multi select), Many (Several questions, Previews), Demo (Long
question, Orphaned) plus "Previews again" and "Orphan two" for the text-size
and orphaned-Dismiss states. Every chat asked a real AskUserQuestion. Native
release build on DISPLAY=:99, driven with real xdotool pointer input; keys
through testctl. Each PNG is web (top) over native (bottom), except where
noted.

| State | Crop | Result |
|---|---|---|
| pending, nothing picked | pending-single-multi.png | matches |
| hover on a choice | hover-option.png | matches |
| single picked + Submit hover | picked.png, after-x.png | **focus in Other, not on the radio (F-a)** |
| hover on the note button | hover-note.png | button matches; row edge stays `border` (nit) |
| note open / follows the pick | note-open.png, note-follows.png | matches, except focus jumps to Other on the MySQL click (F-a) |
| multi ticked + Dismiss hover | multi-dismiss.png | matches |
| restored after a client restart | restored.png | matches (picks, ticks, note) |
| Other focused / typed (ticks cleared) | other-focused.png, other-typed.png | matches |
| answered (Other; note + tag) | answered-other.png, answered-both.png | matches; server got answers + annotations |
| several questions + previews | several-and-previews.png | matches |
| partly / all answered (Submit) | partly.png, all-answered.png | matches, except Deploy's Other takes the focus after "This server" (F-a) |
| answered several + dismissed | sev-dis.png | matches (tags in order; red dismissed; server denied) |
| narrow: long question, preview wrap | narrow-long-top.png, narrow-preview-wrap.png | matches (wraps, no sideways scroll) |
| text larger / smaller | larger.png, smaller.png | matches (fonts scale, radio/paddings do not) |
| orphaned card; answered elsewhere | orphaned-and-elsewhere.png | matches (line on top; API respond updates live) |
| orphaned answered, session gone | orphaned-answered-session-gone.png | **error in the pane strip, not in the conversation (F-b)** |
| orphaned Dismiss | orphan-dismissed-answered.png (native only) | neutral "answered", server `answered-late`, `answers {}` |
| keyboard | kbd-radio.png, backtab.png, shift-tab.png | ring on the radio; Backtab from Other into the group, Up picks, Tab to Other |
| note ✕ hover / Enter in note | x-hover.png, note2.png | matches; Enter does nothing; ✕ clears the note |

## Differences

- **F-a (blocking). A click on a choice row that is not the radio group's
  current Tab stop puts the keyboard focus in that question's Other field.**
  Web: the clicked radio takes the focus (the ring on the radio); arrows
  move on from it. Native: Other shows the accent edge and ring with a caret;
  typing goes into Other, which clears the pick (clicked SQLite, typed "x":
  picks `{chosen: [], other: "x"}`). Seen three times: SQLite and MySQL on
  One question, "This server" on Several questions. A click on the current
  Tab stop (or a checkbox row) is fine. ui/chat.slint QOptRow: `clicked =>
  { fs-key.focus(); … }` while `fs-key` is disabled for non-Tab-stop rows.
- **F-b (blocking). `answer-closed` failure shown in the wrong place.** Web
  (and the spec's Errors: "appends a local error entry in the chat"): a red
  error entry in the conversation right under the card, "Session is not
  running". Native: the pane's error strip above the composer.
- Nit: with the pointer on a choice's note button, the web row's edge is
  `accent-hover` (the pointer is in the row); native keeps `border`.
- Nit (chat log, not 2.13 code): the pending AskUserQuestion tool row shows
  no argument summary (web `questions=[{"header":…`) and a dot instead of
  the running spinner; chat.rs `tool_summary` only falls back to string
  values.

Tokens: QOptRow / QuestionCard use theme tokens only (checked in
ui/chat.slint 797-1000).
