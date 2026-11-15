# 2.15 Thinking — design review, round 1

Setup: tools/test-server.sh 4814, one wide pane (1400×860). The web client
(1.21.7) and the native client were fed the same `ui-event` frames (web:
the page's WebSocket; native: testctl `chat-event`). `web-*.png` and
`native-*.png` are full windows; the other images show web over native.

| State | Crop | Verdict |
|---|---|---|
| From history (folded), Thought lines (~1.2k, ~640, plain) | ../../log-rest/review/reply-thoughts.png | matches |
| Live, open (shimmer band, caret) | live.png, native-live-blink-2x.png | matches (shimmer band and glyph caret are named gaps) |
| Live, long (max height, scrolls, kept at the newest line, thumb) | long-live.png | matches |
| A newer block folds the older one | newer-folds-older.png, newer-blank-band.png | **block folds correctly, but the log is left ~175px past its content (a blank band under the newest entry)**; the web stays at the bottom |
| Done, lingering | done-open.png | matches |
| Folded 5 s later | collapsed-6s.png | folds; **the log keeps a blank band below** (the web goes back to the bottom) |
| Opened again by a click | reopened.png | opens; **the log does not stay at the bottom**: the block opens below the fold and the jump button shows (the web keeps the bottom) |
| A+ twice | native-larger.png | text scales, bar and gaps do not: matches |

Rules (open on streaming, the others fold, 5 s linger, folded from history,
the reader's click wins) behave as the web's.

Open: the log does not follow the bottom on height changes that are not
arrivals (finding raised in the run). F87 reopened (log-rest).
