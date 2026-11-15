# 2.T token pass: design review (round 1)

Fresh test server `tools/test-server.sh 4814`, seeded with 5 projects and 9 chats
(the long job, a question, an approval, a background task…). The web UI was at
1400×860 in a hidden browser. Native was run from fresh configs on DISPLAY=:99:
- **old** = the tree at 53dd3e7 (before 2.T);
- **new** = HEAD (37e03a0).

Both ran the same scripted sequence. CEF allows one instance, so old ran first,
then new, against the same server.

- `old-new/<state>.png`: old (left) | new (right), at 50%. The states are:
  - sign-in: server, server error, login, button hover, login error, pairing;
  - main, sidebar hover, tab hover, folded, folded hover;
  - the "+" menu, the Layout submenu, a submenu item hovered;
  - the split controls on hover, the narrow header's "…" menu, a full (wide)
    header;
  - grip hover, dragging, the empty pane, a chip hovered;
  - the browser drawer.
- `web-native/<region>.png`: web over native (new), by `tools/compare.sh`.
- `accentfill-*.png`: the AccentFill sizing problem (below).

## Result: sent back

Kept from old to new: every part, its order and its state. There are no moved
or missing parts, and the only size changes are one-token steps (the chat text
is 13→14, and so on).

Findings:
1. **AccentFill has no size** (`ui/widgets.slint` AccentFill; used by
   chat.slint CardBtn :140 and Send :1002/:1047). It draws its SVG's natural
   60×28 centred in whatever holds it.
   - The question card's Submit: the box is 965-1037 (73px), the fill
     972-1031 (60px). Old filled all 73px. The label's padding drops to about
     5px and the gap to Dismiss grows. See `accentfill-submit-…`.
   - Send at A+: the box grows but the fill stays 60×28 in its middle, with
     the glow layers showing round it. At A− the fill is bigger than the box.
     See `accentfill-send-…`.
   - The fix: `width: parent.width; height: parent.height` (the round
     avatar's 38×38 happens to match).
2. **tools/tokens.sh is easy to fool.** Each of these passes unseen:
   - `x: -6px`;
   - `20 * 1px`;
   - `.5px`;
   - `opacity: c ? 0.6 : 1`;
   - `rgb(10,20,30)`;
   - `Theme.sp-8 * 3`;
   - `12phx` and `1rem`.

   Real typed numbers it misses today:
   - chat.slint:661 `* 0.86`;
   - chat.slint:680 and :694 `* 1.55` (the chat line height, typed);
   - signin.slint:236 `opacity: … 0.45`;
   - widgets.slint:59-60 `* 0.55`;
   - workbench.slint:469 `cubic-bezier(0.2, 0.7, 0.2, 1)`;
   - workbench.slint:538 `opacity: … 0.4` (`Theme.o-dim` exists).
3. **Folded rail, missing Add project.** The web's folded rail has a dashed
   "+" box above Inbox; native hides Add project when folded
   (shell.slint:278 `if !root.folded`). This was the same before 2.T.
4. Nits (all as they were before 2.T):
   - The folded expand icon: the web has "»" (chevrons-right), native ">"
     (chevron-right).
   - The folded Inbox badge: the web puts it on the icon's top-right corner;
     native puts it beside the icon.
   - On the login and pairing card, the fields and buttons sit at the card's
     left padding (521-780 in a 473-928 card). The web centres them (570-830
     in 469-931). Sign-in has the same fields in a VerticalLayout without
     centring.
5. Theme scales (rule 2): one-offs copied from the web, not on a scale:
   - `add-menu-x` 134.4px;
   - `track-code` 6.7px;
   - `track-code-sm` 2.59px.

   And duplicate tokens:
   - `radius` = `r-card` and `radius-sm` = `r-control`;
   - `font-size` / `-sm` / `-xs` = `text-14` / `12` / `11` (`-sm` and `-xs`
     unused).

   Also, `lh-chat` and `lh-ui` are Slint factors (1.33 and 1.12) while the
   code types 1.55 next to them.

F35 (a round gradient avatar) and F36 (DE / OT / TH / FO / FI) are verified
in `live-state/review/r2`.
