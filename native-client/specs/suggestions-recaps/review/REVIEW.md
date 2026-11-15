# 2.17 review, round 1 (sent back)

Isolated server tools/test-server.sh 4814 (its seeded real chat), native on testctl 4815,
web in a hidden 1400x860 browser. The same recap/suggestion frames were injected on both
sides (web: the page's WebSocket as a `ui-event`; native: testctl chat-event). A real idle
recap also arrived from the server on both. Real X pointer for hover and the click.

Matches: dock place and width, order (recap, then suggestion last), caption chip, dashed
bubble, hover look (solid edge, accent-dim, text-primary), × hover box, tooltips, long text
wraps inside 320 keeping its line break, jump button above the dock (native-jump.png), log
padded so the result line clears the dock, narrow 370 pane (native-narrow-370.png), the
real click sends straight away: dock empty, chat WORKING, box still "keep me"
(native-sent.png), hidden while not idle.

Differences (web over native in each crop):
1. both-*.png, long-*.png, native-realrecap.png: the recap's text sits on the card's bottom
   edge with no padding under it, and the gap under the caption is about 10px (web: 2px under
   the caption, 8px padding under the text). Cause: `rl` (VerticalLayout in the recap card)
   has no height, so it takes the card's height (preferred + sp-16) and stretches.
2. xhover-*.png: × hovered — web: the bubble shows its hover look too (solid edge, accent-dim
   fill, text-primary); native: the bubble stays at rest (dashed, muted) while the × is hovered.
3. (nit) sug-corner-*.png at 4x: the dashed edge rounds the bottom-right corner; web's is the
   bubble's sharp corner (r-chip), as the fill already is.
