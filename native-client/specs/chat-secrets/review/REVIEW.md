# 2.12 Chat secrets dialog: design review (round 1, sent back)

Isolated test server (4814), eight passphrase secrets as in the spec
(open-sesame), web and native side by side, real xdotool clicks on native.
Crops are web over native (sec-short-window.png: web left, native right).

Matches: open (sec-open.png), two ticked (sec-chosen2.png), both empty
(sec-err-missing.png), wrong passphrase all-or-nothing (sec-err-wrong.png,
server had no grants), granted until chat ends with toast and Revoke all
(sec-granted-two.png), a live 15-min grant from the API "until 14:33 (15 min
left)" (sec-live-timed.png), Revoke with "Revoked SENTRY_DSN"
(native-revoked-toast.png), Esc closes, lock yellow "2" (lock-on-2-2x.png).

Differences:
1. **Tab leaves the lock fields** (blocking). From a focused lock field, Tab
   does not go to the next ticked secret's field: the focus leaves both fields
   and what is typed next lands nowhere (native-tab-leaves-fields.png: focused
   DATABASE_URL, Tab, "Q": no field focused, GITHUB_TOKEN still empty). The
   dialog's `keys` FocusScope is a sibling of the card, not its ancestor, so
   its capture-key-pressed never sees keys from a field; Slint's own Tab
   navigation runs instead. Spec and PLAN gap both say Tab moves between the
   lock fields and stays in the dialog.
2. **Short window** (blocking). Web 860x480: the pick list gives up its height
   first, so the duration chips, Close and Pre-authorise stay on screen. Native:
   the pick list keeps up to pick-max-h and the card scrolls as one, so the
   chips and both buttons are below the card's fold (sec-short-window.png).
3. **Pick row alignment** (blocking). Web centres the checkbox and name on the
   row (on the field's middle when ticked); native puts them at the row's top,
   the field hanging below them; the field's right edge and focus ring run
   under the list's scroll thumb and are clipped (sec-chosen2-2x.png).
4. Nits: after Pre-authorise is clicked the focused lock field keeps its
   focus ring (web: the button takes focus); PLAN.md "Visual-only controls"
   still says the lock opens nothing yet.
