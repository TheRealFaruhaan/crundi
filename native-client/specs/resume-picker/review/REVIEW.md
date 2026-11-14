# 2.22 Resume picker — design review, round 1

Crops (web over native, the spec's web shots vs the native snapshots of the same seeded
transcripts): list, list-end, heavy, error, launching.

- List: heading, New session (accent) first, rows newest first, title wraps, mono meta,
  heavy (yellow), Cancel bottom right, the list scrolls inside: same.
- Heavy choice: intro (bold title, cost line), Compact first (recommended), Load the full
  conversation, ← Back to the list: same.
- Error: New session then the muted line; native wording "Cannot reach the server: Failed to
  fetch" (the client's own network-error prefix) — nit.
- Launching: UI Mode spins, siblings dimmed (spec's named native choice).

Live run (real xdotool input): UI Mode click opens the list with New session focused; row hover
(accent border + accent-dim, live-row-hover.png); a backdrop click does nothing and no hover
leaks to the pane header beneath (live-after-backdrop-click.png); Tab walks rows; Enter on a
row resumes it (create body {sessionMode:"resume", resumeId}) into the launcher's leaf, focused.
2.22 passes.
