# 2.1 design review, round 3 (after F11 and F12)

Release build of this branch on DISPLAY=:99 (1400×860), isolated dev server :4811.

## Rename blur (F11): fixed
Driven with the test driver and with real X pointer events (xdotool), title checked
in the native props and on the server after each:

| Press | Result |
|---|---|
| in the chat log (driver and real X) | saves (`native-after-log-click.png`) |
| inside the input (driver and real X) | stays open, not saved |
| empty sidebar area | saves |
| empty header gap | saves |
| right button in the chat log (real X) | saves (the web blurs on any button) |
| Esc / Enter | cancels / saves, as before |

## Renaming crops
`renaming-1150x32+245+89.png`, `renaming-300x32+245+89.png` (×4): web over native.
The frame, ring, border, font and selection match. The text's scroll does not:
the web's input, opened with `select()`, scrolls so the caret (the end) shows
("ruit list and directory", the F cut); native shows the start and clips the end
("Fruit list and director").

`renaming-long-typed-native-300x32+245+89.png`: native after typing
"A much longer title that runs past the edge END". The input never scrolls:
it still shows "A much longer title th", the caret and everything typed after
it are out of sight. A browser input keeps the caret in view ("…the edge END").
