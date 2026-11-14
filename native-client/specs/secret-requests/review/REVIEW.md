# 2.12b Secret requests — design review, round 1

Test server: tools/test-server.sh 4814, seeded with GITHUB_TOKEN, DATABASE_URL and DEPLOY_KEY
(passphrase open-sesame) and three chats (Fruit list and directory, Second chat, Third chat).
Native: DISPLAY=:99, testctl 4815, three panes side by side, 1400×860. Requests were made the
way the MCP bridge does (POST /api/mcp/call with each chat's dev-mcp key, alias demo).
Web reference: the spec's own web-*.png.

Crops (web over native, or web | native):
one-run-dock, allow-on-dock, err-empty-dock, err-wrong-dock, esc-dock, approving-dock,
several-dock, armed-get-dock, rejected-dock (toast "Rejected"), approved-allow-dock (green
toast), timed-out-dock, err-offline-dock, narrow-window (620 wide),
native-tall-top-and-scrolled (dock capped, wheel scrolls), hover-leak-third-chat-header.

## Matches
- Card parts and order for run and get; project and reason lines only when set; command box
  wraps, caps at cmd-max-h, scrolls; bind line; "Claude will be able to read this value."
- First card armed without focus; Approve on another arms it, empty and focused; the
  others show Approve / Reject; oldest first.
- Allow row: offered on runs from owner chats; unticked by default; duration greyed until
  ticked; list of four with the current one ticked; Esc, a click outside, or a pick closes
  it; the last choice is kept across cards; the tick is cleared after an answer.
- Local errors "Enter the passphrase"; server "Incorrect passphrase" with the text kept and
  selected; Esc clears the error and lets go of the field, the text stays; "Approving…"
  disabled while the server is held.
- Toasts: "Rejected" (info), "Approved", "Approved, and allowed for this chat" (green edge).
- Allow for this chat: the grant shows in the pane (secretsN 1, tip "1 secret
  pre-authorised for this chat"); the next run from that chat asked nothing.
- Time-out at 3 minutes: the card goes silently and the next one is armed.
- 620-wide window: the dock spans the window less sp-12, sp-12 from the bottom.
- Tall dock: capped under the top bar and scrolls with the wheel (named gap: cut shadows).
- Dock under a modal dialog (the chat-secrets dialog's scrim covers it).
- Not modal: the composer takes a click and typing while cards wait.

## Differences
1. BLOCKING. Presses and hover on the dock reach the chat pane underneath it.
   A press anywhere on a card (its head, Approve, Reject, the duration select) makes the pane
   under that spot the focused pane: Fruit list was focused, a click on the card's head at
   (1050,600) focused Third chat; Approve on the get card at (979,511) focused Second chat.
   Hovering the dock shows that pane's split controls in its header (hover-leak crop).
   Web: the dock is in document.body, outside the panes; pane focus is set only by a
   mousedown inside the pane, so pressing or hovering the dock leaves the focused pane and
   the panes' hover as they were.
2. Nit. Network failure wording: web "Failed to fetch"; native "Cannot reach the server:
   Network error" (the client's own transport wording; spec quotes the web's).

Not shown live: "Request no longer pending" (needs a stale card; the text is the server's
error passed through), the PIN field (no PIN secret can be made), the pane's working state
with a real Claude (the dock does not touch pane state).

## F76 (chat secrets pick row) — verified
specs/chat-secrets/review/f76-r4-rows-1400x860-2x.png and f76-r4-short-860x480-2x.png:
checkbox and name centred on the field; the field and its focus ring end clear of the
scroll thumb, at 1400×860 and in an 860×480 window.
