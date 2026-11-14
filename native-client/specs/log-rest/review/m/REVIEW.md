# 2.16m media + F87, F90–F94 check (design review, round 4)

Test server `tools/test-server.sh 4814`, native on testctl 4815, 1400×860, one wide pane.
The same media as the spec's shots, stored through `/api/mcp/call` (`show_image`
single 1200×700 / row of three with captions / formats JPEG+WebP+GIF / gone,
`show_video` 6 s, `show_audio` 8 s, `show_file` export.csv, `show_embed` stored
HTML 200, YouTube, Counter 160, example.com 480, a stored frame deleted (404)),
injected as done tool entries. Crops: web over native (`stack.sh`, labelled).

## Matches
- single.png, row.png: one picture 640 wide; a row 220 tall, 8 apart, item captions, the call's caption.
- formats.png: JPEG / WebP / GIF decoded, 377 / 220 / 220 (GIF first frame: named gap).
- video.png: poster + play button, "The test pattern · 0:06" (bar: named gap).
- audio-file-embed.png: audio pill "0:00 / 0:08", "A440 tone · 0:08"; file card "12 B · The export" + Download.
- loading-web-left-native-right.png: the 34 ring in a 280 × 420 box, caption under it.
- gone.png: dashed box, "This image is no longer stored.".
- download-tray-2x.png: "Saved · 12 B", green edge, full bar, ×; the saved bytes equal the CSV.
- viewer.png: counter, side buttons, bar, the picture fitted; + gives 140% and hides the side buttons; Esc and a backdrop click close it.
- md-image.png: "!" + link "Weekly sales"; the local one stays text with its path linked.
- narrow-*.png (900 window): a single picture and frames shrink to the pane, a row too wide scrolls sideways.
- F90: a thinking block folding (newer starts / 5 s after done): dist 0 over 0 (native-f90a-after.png); opening one at the bottom: dist 0 (f90b-open); 900 window: dist 0, stick on (f90c-narrow).
- F91: after the jump click, testctl wheel (120, 240) and a real thumb drag move the log (dist 489).
- F92: bubble URL underlined in the bubble colour; a real click opens it (lastOpened). Reply links open too.
- F93: paths drawn as links (accent-hover, solid underline: named gap).

## Differences
1. **F95 (open)**: frames are a box with a centred Open (frames-yt-counter.png, frames-page-gone.png).
2. **Stored frame gone (404)**: web dashed box "This embed is no longer stored."; native the same box with Open (frames-page-gone.png). Needs no CEF: the stored HTML's status says gone.
3. **Viewer gallery counts gone pictures**: web `galleryOf()` takes only pictures that loaded (a gone box has no img), so this chat is "n / 7"; native "n / 8", and ← from the JPEG lands on an empty viewer "5 / 8" (viewer-gone-step.png, native-viewer-gone.png).
4. **Viewer controls are small text glyphs**: web 18px icons (chevron-left/right, minus, plus, x); native ‹ › − + × text, the side chevron about 5px tall in a 44 button (viewer-side-3x.png, viewer-bar-2x.png).
5. **F87, real wheel**: at the bottom of a settled log (nothing live), each real notch (`xdotool click 4` over plain reply text) ends at dist 1–2 with stick still on: the log cannot be wheeled up from the bottom. Away from the bottom one real notch moves 120px (measured by snapshot shift). testctl wheel steps of 40 / 60 / 120 from the bottom do stay, so phase2 passes while real input fails.
6. Nit: the play glyph is an outlined, smaller triangle; web a filled white triangle (play-buttons-3x.png, audio-play-3x.png).
7. Nit: frame caption link "Open on YouTube" / "Open on example.com" is not underlined; web accent-hover underlined (frame-caption-yt-3x.png).
8. Nit: picture boxes have square corners drawn over the round border, and the gone box's dashes have square corners; web clips to radius 8 (row-corner-3x.png, gone-corner-3x.png). Name it with the renderer's clip-radius gap, or round them.
9. Nit (F94 reopened): no 8 gap between a numbered list and a bulleted list that follows (f94-lists-native-2x.png); the rule is #5a5a78, web the light grey hr (#9a9a9a / #eeeeee).
