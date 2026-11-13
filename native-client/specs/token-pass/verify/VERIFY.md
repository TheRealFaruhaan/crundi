# 2.T token pass: QA check of the fixes (F37-F41)

Fresh test server `tools/test-server.sh 4812`, seeded with 5 projects and 9
chats (the design review's seed: a long job, a question, an approval, a
background task…). Web UI at 1400×860 in a hidden browser; native release
build of HEAD on DISPLAY=:99, from a fresh config, driven by its test driver.

| Finding | Result | Picture |
|---|---|---|
| F37 AccentFill fills its button | **verified.** The question card's Submit is filled edge to edge (label padding even, the gap to Dismiss as on the web). Send is filled edge to edge at A− (9), default (13) and A+ (24), round corners, with the glow round it, not inside it. The folded active avatar is round. | `F37-submit-web-over-native-x4.png`, `F37-send-native-aminus-default-aplus-x3.png`, `F37-send-{aminus,default,aplus}-web-over-native-x2.png`, `F40-expand-web-left-native-right-x4.png` (avatar) |
| F38 tools/tokens.sh | **verified.** It passes on the tree (0 lines). In a scratch copy, each of the review's eight cases fails it (`x: -6px`, `20 * 1px`, `.5px`, `opacity: c ? 0.6 : 1`, `rgb(10,20,30)`, `Theme.sp-8 * 3`, `12phx`, `1rem`), and so do `Theme.sp-8 + 3px`, `1px * 13`, `sp-8 * 1.5`, `37 * 1phx`, `self.w * 3/4`, `sp-8 / 3`, `10px*2`, `(sp-4)*7`, `7 * sp-4`, `150ms`, `#fff`, `0.5 * 1px`. The numbers it missed before are gone from the code. It still lets through named colours (`white`, `Colors.red`): see the new nit. | - |
| F39 folded Add project | **verified.** The dashed "+" box is above Inbox in the folded rail. | `F39-F40-folded-web-left-native-right-x2.png`, `F39-F40-footer-web-left-native-right-x4.png` |
| F40 » and the folded badge | **verified.** The expand button is "»"; the Inbox count sits on the envelope's top-right corner. | `F40-expand-…`, `F39-F40-footer-…` |
| F41 sign-in and pairing centred | **verified.** The login fields and Sign in are at x570-830, the same as the web (570-830). The pairing card's link field and "Pair this computer" are at 570-830 too. | `F41-login-…`, `F41-pair-…` (web over native; the web's nearest block is "Invited to a project") |

New (nit): the sidebar footer's line is drawn across the middle of the
footer (y812, between Add project and Inbox), not at its top as on the web
(y765, above Add project). Wide and folded. `ui/shell.slint` footer: the 1px
`Rectangle { height: 1px; … }` has no `y`, so Slint centres it; it was the
same before 2.T.
