# 2.20 Cards: design review, round 2

Built from HEAD (4858eac). Fresh `tools/test-server.sh 4814`, native on testctl 4815,
DISPLAY=:99, real xdotool clicks; keys through testctl (no WM on :99).
Four real runs of "Site builder" in a git project, finished through `factory_handoff`.
Live snapshots in `live/`; the 25 web-over-native pairs in this folder.

## Driven live
| Action | Result | Shot |
|---|---|---|
| Card posted | Manager pane with Factory tag, title, summary, files line, message, usage "$0.17 · 1 turn · 1 handoff · 1 chat", follow-ups, 4 buttons | live/ask0 |
| Files line, follow-ups | open; rows, "+? −?" for the binary, long name elided, counts whole | live/files, live/fu |
| Show the full diff | plain mono diff, both scrollbars | live/diff |
| Request changes | form opens, note focused (accent ring); empty Send posts nothing | live/rc |
| Note + Send | card → changes, no buttons; second finish → same card back to ask | live/typed, live/sent, live/ask2b |
| Discard | "Sure? Discard" (red), back to "Discard" after 3 s; second run: armed then confirmed → discarded, branch gone | live/armed, live/disarmed, live/d1, live/d2 |
| Merge with a dirty checkout | refused, error shown **twice** | live/mergeerr |
| Merge | "Merged into dev as one commit c66fceb", Undo; git log has it | live/merged |
| Undo | "Undone…", dev back at First | — |
| Keep branch | "Kept on the branch …", branch kept | live/kept |
| Narrow (3 and 4 panes) | buttons wrap to a second row, lines wrap | live/keep0, live/kept |
| Refusal then another client's patch | stale error stays on the kept card | live/stuberr, live/stalerr |
| Widget shots (real PNGs on the server) | 150-tall pictures, "inline", "mobile · dark"; click opens the viewer | live/shots |
| Question card buttons | Dismiss red hover, Submit accent-hover, plain regular weight | live/q0, qh, qh2 |

## Findings
1. **Action error survives patches** (blocking). `act_err` is cleared only by the next press. The web rebuilds the card on every patch, so its error line goes. Native: stale "The run is running; there is no result waiting for a decision." under merged, undone, kept, discarded, conflict, changes and ask cards (pairs long-summary, merged, undone, kept-files-binary, discarded, conflict, merged-auto, changes, ask-empty, ask-merge-error; live/stalerr).
2. **Merge refusal shows the error twice** (blocking). Web (web-ask-merge-error): the server's `error` once, above the buttons. Native: above the buttons and again as the action error line at the bottom (live/mergeerr).
3. **A sent note comes back** (blocking). After Send to the Manager succeeds and the Manager finishes again, the card returns to ask with the note form open and the sent note in it. Web: form closed and empty. The named difference keeps a half-typed note, not a sent one (live/ask2b).

F121 (pairs, same data), F122 (bubble wraps its text), F123 (usage line) verified.
