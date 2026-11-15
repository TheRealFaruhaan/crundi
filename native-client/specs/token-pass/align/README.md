# Alignment sweep (2.C)

Before (left: the build before the consolidation) and after (right), on a
fresh `tools/test-server.sh` server, 1400×860. `signin-login-web.png` is the
web's login card on the same server.

| File | What it shows |
|---|---|
| signin-server | the Server step (left-aligned card, as Electron's) |
| signin-login | password and code fields and Sign in, centred under the prompt |
| signin-error | the error line, centred |
| signin-pairing | the link field and Pair button, centred |
| signin-browser-code | the browser code and Cancel, centred |
| send-glow | Send's glow, close up: layers before, one fade after |
| card-shadow (and -contrast) | the card's shadow: stacked layers before, one smooth fade after; and its fill's corners |
| chat, addmenu, layoutmenu | the workbench, the "+" menu and its Layout submenu |
| launcher | the new pane: Empty Shell and the Claude card, centred |
| panel, drawer | a panel placeholder; the drawer beside narrow panes |
| folded | the folded rail (avatar, "+", Inbox) |
| noproject | the empty workbench (empty pane, chips and Split buttons centred) |

Fixed: the sign-in card's fill drew square dark corners outside its round
border (a gradient ignores the radius); the card's, menus', toast's,
clone's, Send's and rail avatars' shadows were stacked layers (now
`SoftShadow`); the launcher's UI Mode gradient had square corners; the
launcher's column now stays centred when it is wider than its pane.
