# 2.14 Tool cards: design review, round 1

**Approved.** Reviewed against `../../tool-cards.md` on a fresh release build (dde9ac5) with an isolated test server (`tools/test-server.sh 4814`). The native client was given the web screenshots' own data: the chat stored by the spec writer (`/tmp/crundi-devtest-4810/.../demo.json`: the real Bash/Read/Edit/cat/Write/… turn with its groups and agent records), replayed as a `history` frame. The entries the spec writer injected (running Bash, TodoWrite, Grep, Glob, WebFetch, WebSearch, Task, Skill, NotebookEdit, a long Bash, the two MCP tools, the pending AskUserQuestion, the failed Edit, the two late groups and the wrapping output) were rebuilt from the screenshots. Each file here is the web (top) over the native client (bottom). Clicks, hovers and the scrollbar were driven with real X input; toggles and patches went through testctl.

| File | State | Result |
|---|---|---|
| groups-closed | groups closed, agent rows between them | match |
| groups-open | two groups open, first member sp-4 below | match |
| open-bash-edit, failed-open, diff-edit-2x | Bash open (command, muted description, hairline, output); Edit diff bands; failed cat red name, dot and result | match |
| read-scroll, read-body-scrolled | Read: JSON input, file with tab stops, body at its limit with thumb; wheel scrolls the body | match |
| write | Write: one green band, then the result | match (nit N4) |
| long-output-bottom | seq 1 600 scrolled to its end, card at its max height | match |
| mcp-unknown | ToolSearch, `mcp__crundi__list_services` (`{}`), raw names | match |
| summaries-per-kind | every summary rule incl. 140-cut, key=value list, no-arg MCP, pending AskUserQuestion with spinner | match |
| spinner-4x | ring + turning accent quarter | match (named: paths) |
| todo-grep-open, todo-marks-3x | done struck/muted, active accent-hover with activeForm, pending; Grep JSON | match (named: strike width; nit N2) |
| task-open, failed-edit-open | Task as a plain card; failed Edit diff + red tool_use_error | match |
| groups-running-failed, groups-open-states | group spinner while a member runs, red dot when one failed | match |
| live-done-and-wrap | patched running member → group dot turns green live; long line wraps | match; open card stayed open (named) |
| agent-row-hover-2x | real hover: edge accent | match (named: padding) |
| text-larger, text-smaller | ×1.31 / ×0.69: text scales, dots/spinner/padding/caret place don't | match (nit N1) |
| narrow | 340-wide pane: summaries and group line elide, counts/carets keep their place, to-dos and diff lines wrap | match |
| status-dot-centring-3x | see N1 | nit |

Tokens: `tools/tokens.sh` clean; new theme tokens `spin`, `spin-stroke`, `turn`.

## Fix checks (2.13), crops in `../../questions/review/r2/`
- **F81 verified.** Real click on MySQL (not the Tab stop): picked, ring on its radio; typing `x` leaves Other empty (picks `{chosen:[MySQL]}`); ↓ picks Postgres with the ring; click SQLite, `x`, ↑ → Postgres.
- **F82 verified.** Orphaned card (claude process killed while asking), real click SQLite + Submit: the tag shows and "Session is not running" is a red-edged entry under the card in the conversation, not the strip.
- **F83 verified.** Pointer on SQLite's note button: that row's edge is accent-hover (#818CF8), the others stay border.
- **F84 verified.** Pending AskUserQuestion row: spinner and `questions=[{"header":"Database",…`.

## Nits (follow-ups, not blocking)
- **N1** The status mark (dot / spinner) sits at the top of the head row, not centred: `StatusMark { y: (parent.height - self.height) / 2 }` inside a HorizontalLayout is ignored by the layout. 2–3 px high at the default size, ~5 px at A+ (web: centred at every size). chat.slint 824, 1271, 1304.
- **N2** The done to-do's strike line runs the whole width of the card for a short one-line item ("Read the server" struck to the right edge). Named in PLAN, but a one-line item's width is measurable (an unwrapped Text's preferred-width), so only the last line of a wrapped item needs the full-column fallback.
- **N3** (the log, 2.16; since phase 1) From the bottom of a chat, single wheel steps up move ~15 px and then snap back to the bottom (tracked: 582, 599, 568, 582, 596, 568…; the same in a plain chat). `changed content-height` re-sticks while `stick` is still true (within sp-48), and the ListView's height estimate changes as rows come into view. Seen intermittently; dragging the scrollbar or a fast flick escapes it.
- **N4** Write's band shows an extra empty line when the content ends with a newline (`"# Changelog\n\n- Port moved to 4000.\n"`: web 3 lines, native 4). The web's pre drops one trailing newline.
