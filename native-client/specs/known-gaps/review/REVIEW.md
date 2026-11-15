# 2.25 known gaps + F131-F137: design review, round 1

Web 1400x860 (hidden browser, test server :4814) over native :99 (debug build of a1da01a), the same injected reply (md.mjs in the reviewer's tools: heading, code spans, bold, `[1]` then a `[text](https://…)` link, a bare URL, a list, a quote) and user bubble.

Verified:
- K1/F138: wrapped reply and bubble lines 20px apart (web 20), heading → paragraph 21 both (`reply-*.png`).
- K2/F139: app logs `4 faces as "Crundi Chat", line 1.550; own line 1.164, lh-chat 1.332, lh-ui 1.117, lh-composer 1.246`; drive.mjs checks it.
- K3/F140: chip outline faint (`zoom-400x30+250+540.png`).
- K4/F141: Send glow (`send-glow-web-over-native-4x.png`), + menu shadow (`menu-shadow-web-left-native-right-2x.png`).
- F131: full screen, wheel up, an arrival: stick false, jump with the new dot, no snap (`native-full-screen-scrolled.png`).
- F132: example.com and YouTube load and draw (`native-https-frames.png`).
- F133: double click selects the word and fires dblclick (`frame-dblclick-native.png`).
- F134: the wheel over YouTube moves the log (dist 504 → 240 → 504); in "Scrolls inside" the page scrolls (0 → 720) and then the log takes over at its top (dist 0 → 384).
- F137: Alt and Ctrl reach the page (`frame-keys-native.png`).
- F135, F136, F142, F143, F144: checked in the tree and PLAN.

Not approved:
1. The text before a link disappears (`zoom-560x30+250+585.png`, `native-reply.png`). md::colorize pairs a `[` with the next `](` anywhere later in the block, so "Footnote [1] in the log says …; the details are in [the cargo docs](https://…)" closes the colour span at "[1]" and everything up to the link is drawn in chip-key (#0a0a0f, the background).
2. Links and paths in replies have lost their underline. Web: `a` is underlined in accent-hover and paths have a dotted underline. Before 2.25, native drew a solid underline (`specs/log-rest/native-reply.png`: "docs", "https://example.com/b"). Now a URL link has no underline, and a path (`src/server.js`, ./lib/util.js, /tmp/a/client.js:12) is plain text with no underline (`native-paths-links.png`). The user bubble's links are still underlined.
Nit: the recap uses lh-chat (1.55), but the web's .cc-recap is line-height 1.45.
