# 2.16f live frames: design review, round 1 (sent back)

Rig: tools/test-server.sh 4814, the seeded chat; frames stored with show_embed
through /api/mcp/call (a gist, "Scrolls inside" 40 lines h160, "Click test"
h200, YouTube "Me at the zoo", example.com h480, a probe page, a gone name),
injected as done tool entries into the native client (testctl chat-event) and
into the web client (ui-event over its WebSocket), 1400 x 860, one wide pane.
Real xdotool clicks and wheel on DISPLAY=:99; keys through testctl.

## Matches
- At rest (`idle-*`): box, transparent view over `frame-bg`, 1px edge, caption
  and Open link, the page's own scroll bar: the same as the web.
- Click test (`native-typing` / `web-typing`): 3 clicks -> "Click me 3",
  green, title "clicked 3"; typing goes to the frame's input, the composer
  stays empty; Esc stays with the frame; a click outside takes focus back.
- target=_blank link -> the opener (frameOpened https://example.com/).
- Drag-select inside a frame, Ctrl+A, right click (no menu, the page's
  contextmenu fires), confirm suppressed (named gap).
- Scrolled far and back: released (0 views) and a fresh page on return.
- Workbench tab left -> 0 views; back -> made again. Full screen draws them.
- Narrow (window 800): stored boxes 520 wide, fixed heights kept.
- Gone stored frame: dashed box, "This embed is no longer stored.".

## Differences (blocking)
1. **https frames never load** (`https-example-web-left-native-right.png`,
   `https-youtube-native-ring-forever.png`). Web: example.com draws its page,
   YouTube its player. Native: example.com box stays empty (title
   about:blank), YouTube keeps the ring forever. native.log shows
   "Terminating renderer for bad IPC message, reason 213" twice each time
   the two views are made: frames.rs open_view creates the view on
   about:blank and then calls main_frame().load_request() with a referrer;
   Chromium kills that renderer. Stored HTML (data: URL) works.
2. **Double click does not reach the page** (`dblclick-web-native-dragselect.png`).
   Web: double click selects the word and fires dblclick ("dbl:word").
   Native: nothing (frames.rs pointer() always sends click_count 1).
3. **Wheel over a frame never chains to the log.** Spec: chain when the
   page's root scroller is at that end or does not scroll; only an inner
   scroller may be named. Native: 5 notches over a non-scrolling frame
   (probe, click test, a player) leave the log where it is (dist 0 -> 0) and
   turn stick off. PLAN names it as a difference, but it is not a Slint gap.

## Nits
- test/.p16f-dev.mjs (a scratch copy of the suite) is committed.
- chat.slint ~1188: stale comment "a live frame in the box is not drawn yet".
- Alt / Meta are not passed to the frame (key() sends Ctrl and Shift only).
