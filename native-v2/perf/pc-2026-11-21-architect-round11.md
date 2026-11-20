# Round 11 (the Architect): the owner's display, release, HEAD bac9b400

Release, the owner's display (Wayland, GNOME, 1920x1040 window, AMD Renoir),
`tools/measure-pc.mjs` with a fresh `tools/stub-server.sh 4884` and a fresh
config folder. The build is bac9b400 plus this round's work. None of the
changes touches the frame path except the focus fix, which adds one frame
per focus change.
p50 / p95 ms / share ≤ 7 ms.

The display check first (the Manager's question: does the release client
show on Wayland?). Yes. A plain launch with a fresh config drew frames
(21, 5.5, 2.2, 1.5 ms), and the driver's new `window` state read
`active: true` at 1.5 s and 4 s (the compositor's activated state: mapped,
in front, with the keys), not hidden and not locked. The tray-only icon
the owner saw came from the Xvfb suite runs on this PC. Their clients put
the tray on the login session's bus while their windows sat on Xvfb.
`test/lib.mjs` now drops that bus.

| scenario | this round |
|---|---|
| idle-open, idle-end, idle-planning | 0 frames |
| idle-terminal, idle-drawer | 1 frame |
| scroll-panes | 4.56 / 6.18 / 99% |
| scroll-chat | 4.03 / 6.05 / 99% |
| stream+scroll | 5.47 / 6.47 / 99% |
| stream | 5.20 / 6.25 / 98% |
| term-stream+scroll | 5.11 / 7.65 / 81% |
| scroll-kanban | 2.82 / 4.15 / 100% |
| pan-mindmap | 4.05 / 5.45 / 100% |
| planning+stream+scroll | 4.87 / 7.77 / 81% |
| cr-sweep | 4.97 / 7.50 / 85% |
| **cr-scroll** | 5.98 / **7.72 / 81%** |
| cr-drawer-sweep / cr-drawer-scroll | 7.22 / 93%, 7.62 / 88% |

So one pane, or the four chats, scroll and stream inside the budget. The
CR's mix is over it: five panes, a chat streaming and the wheel. Round 6
had cr-scroll at 5.55. Round 7, after the chat port, had it at 7.02-7.49.
Now it is 7.72.

## Where the frame goes (journal + pprof on cr-scroll)

These come from a `frame-journal` build with the sampler (perf/README.md).
The journal's own clock reads inflate request_layout, so read the shares
as rough. Main thread, cr-scroll: present ~20% (queue_submit, instance
writes, command encoding; mind-map path rasterising is 14% of present),
paint ~23%, prepaint ~14%, request_layout ~12% (the Workbench's and the
Shell's renders are 2.3% and 1.3% of all samples), taffy ~5%. Journal p50
→ p95 µs: layout 1071 → 1505, prepaint 1144 → 2217, paint 1456 → 3174.
The tail is in prepaint and paint, on frames where the streaming log and
the view under the wheel both re-render. The log's paint is mostly glyphs
(paint_glyphs half of it; should_use_subpixel_rendering, a per-glyph
platform check, ~20% of the log's paint).

Every scrolling frame re-renders the Workbench and the Shell (573 of 573
in cr-scroll). A leaf's notify dirties its ancestors in gpui, and the
cached siblings are replayed. No view stands out: the cost is spread over
the five panes' replays, the dirty chain's renders, and present.

Considered and not done:
- Skipping the empty overlays: only the lightbox and the "+" menu could
  skip safely (the file windows' layer measures the viewport in its
  render). That saves two replays, a few hundredths of a ms.
- An origin-tolerant cache (F130) means replaying a cached view's
  primitives, hitboxes and dispatch nodes at an offset. That is a gpui
  fork, not a patch.
- Fixing the text rendering mode: on Linux cosmic-text always answers
  Subpixel, so it saves only the virtual call.

F99 stays open. What is left needs a structural cut, not small ones. One
option is pane bodies that stay replayed while a sibling pane scrolls.
gpui re-renders everything under a re-rendering cached view (`refreshing`),
so a cached pane host would re-render the log and composer with any
change in that pane. Another is cutting what the streaming log re-lays
out each frame.

Raw: /tmp/v2-measure-r11/report.json and /tmp/v2-prof on farhan-RP-15 (not
kept in the tree).
