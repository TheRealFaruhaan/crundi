# 2.10 Scheduled messages: design review, round 1

Test server `tools/test-server.sh 4814` (seeded chat + "Second chat" and "Narrow chat"),
the native client on testctl 4815 in the spec's layout (wide pane left, two stacked
narrow ones right), 1400×860. Real clicks for the clock, triggers, Schedule, ✕, ×, the
picker icon; real wheel for the long list. Each `<state>.png` here is the web's
screenshot over the native one, same region (`-2x` = ZOOM 2).

| State | Crop | Result |
|---|---|---|
| open-empty | open-empty-panel.png, open-empty-time-list-2x.png | parts and order match; **calendar glyph is a clock**; **empty line not italic** |
| trig-turnend | trig-turnend.png | matches (box keeps focus, web's loses it: nit) |
| err-empty | err-empty.png | matches |
| err-notime | err-notime.png | **empty field shows nothing; web "mm/dd/yyyy, --:-- --"** |
| err-past | err-past.png | matches (server's words) |
| with-items | with-items.png, with-items-list-2x.png | matches; next edge, mono when, 3-row cut; nits: " · " bold accent, tighter line pitch |
| hover-del | hover-del-2x.png | red ✕ + tooltip, matches |
| trig-limit | trig-limit.png | matches |
| long-scrolled | long-scrolled.png | 82% cap, fixed head, body scrolls with thumb: matches |
| after-delete | after-delete.png | no confirm, gone from list and server: matches |
| err-offline | err-offline.png | matches |
| err-oldserver | err-oldserver.png | named difference OK; Limit resets stays enabled (web dimmed): nit |
| narrow | narrow.png (+ native-narrow-short.png) | same parts; 3-row box kept, body scrolls (named) |
| fired-stale / reopened | fired-stale.png, fired-reopened.png | user bubble + reply; stale "next" kept; gone on reopen: matches |
| clock-hover | clock-hover-2x.png | matches |
| pickers | native-datepicker.png, native-timepicker.png | std popups (named gap); picked 11/18 15:00 → "11/18/2026, 03:00 PM" |
| A+ | native-aplus.png | everything scales |

Esc and a click on the log leave the panel open; × and the clock close it; two panels
open in two chats at once; Schedule cleared the composer holding the same text.
