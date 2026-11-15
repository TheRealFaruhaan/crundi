# 2.11 Slash menu: design review (round 1, passed)

Isolated test server (tools/test-server.sh 4814) with the seeded chat (has had a
turn: names only) and "Narrow chat" (no turn: names + descriptions), web
(1400x860, server browser) and native (testctl, real xdotool clicks and wheel)
side by side. Crops are web over native.

| State | Crop | Result |
|---|---|---|
| `/` typed, descriptions | open-desc.png, open-desc-2x.png | same parts, order, selection, elided descriptions, thumb. Names never wrap (named difference). |
| down x2 | arrow-down2.png | third item selected, nothing else changes |
| down x8 more | arrow-offscreen.png | native keeps the 11th in view (named difference); web leaves it out of sight |
| wheel + hover | hover-scrolled.png | wheel scrolls; hover changes nothing (pixel-identical before/after); selection stays |
| click /motion-reels | chosen-click.png | box "/motion-reels " caret at end, focus kept, list gone; draft saved (named difference) |
| `/eng`, `/engineering:t` | filtered.png, filtered-few.png | prefix filter; list shrinks to two |
| down down Enter | (props) | "/engineering:testing-strategy ", nothing sent; clamps at last |
| `/b` | btw.png | CLI's b-commands then /btw last with its description |
| `/bzz` | (props) | no list; backspace back to `/b` reopens it |
| Esc | (props) | list closes, text kept |
| names only | names-only.png | names alone |
| narrow pane (340) | narrow.png | list spans the box only, above the stacked composer |

Also checked: Tab chooses; Shift+Enter inserts a newline and closes it; Up
does not recall while open; a click in the other pane's box closes it (blur).
Tokens: tools/tokens.sh finds 0 typed numbers.
