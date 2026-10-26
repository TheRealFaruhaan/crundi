# Play Store listing

Text and pictures for the store page. The pictures in this folder were
rendered from the real pages (the compact chat, and Connect a phone in
Settings), with a sample conversation and a sample device name.

## App name

Crundi

## Short description (80 characters at most)

Your self-hosted Crundi server on your phone: chats, projects and notifications.

## Full description

Crundi for Android connects to your own Crundi server.

Crundi is a self-hosted workbench for your projects: Claude chats, terminals,
files, git, a kanban board, a mind map and schedules, reachable from anywhere.
This app puts that server on your phone.

Pairing takes a few seconds. On your server, open Settings, then Connect a
phone, and point your phone at the picture it shows. No password is typed on
the phone.

What you get:

• Everything your server's web page does, in an app
• Pairing by scanning a picture, with no password on the phone
• Notifications when Claude finishes or needs you, with a reply box, a
  suggested reply, and Approve and Deny for a request
• Chats that float as bubbles over whatever else you are doing
• A live card while Claude works, showing which chats are at it
• Files, photos and the camera, straight into a chat
• Your projects' own icons on bubbles and notifications
• A check that the app and your server are compatible, with a clear message
  when one of them needs updating

This app does nothing by itself. You need a Crundi server of your own, version
1.20 or newer. Crundi is open source: https://github.com/TheRealFaruhaan/crundi

## Pictures

| File | Use | Size |
|---|---|---|
| `play-icon-512.png` | App icon | 512 x 512 |
| `feature-graphic-1024x500.png` | Feature graphic | 1024 x 500 |
| `screenshot-1-chat.png` | Phone screenshot | 1080 x 1920 |
| `screenshot-2-connect.png` | Phone screenshot | 1080 x 1920 |
| `screenshot-3-connected.png` | Phone screenshot | 1080 x 1920 |

Screenshots 2 and 3 show the server's Settings in a phone-sized browser, not
the app's own screens. Replace them with captures from a real phone once the
app has been run on one.

## Category and contact

- Category: Tools (or Productivity)
- Privacy policy: https://crundi.faruhaan.com/privacy.html (the page is
  `docs/privacy.html`; it is live once that file is on `main`).
- The steps to a public release are in `PLAY_RELEASE.md`, beside this file.

## Release notes, 1.1.2 (en-US)

Version code 31. Production, since 1.1.1 (code 28): everything from
1.1.2-dev.1 and 1.1.2-dev.2.

```
<en-US>
Claude Code's own summaries in your chats.

- A recap when you come back to a chat that sat idle, or one you pick up again.
- Ask on the side: type /btw and a question, or use Ask on the side in the chat menu. The answer is not added to the conversation.
- Runs of tool calls fold into one line saying what they did; tap to open.
- Subagents show a line on where they have got to.

Needs a Crundi server, version 1.21.3 or newer, for recaps and asking on the side.
</en-US>
```

## Release notes, 1.1.2-dev.2 (en-US)

Version code 30. Since 1.1.2-dev.1 (code 29): the chat screen's actions
read the live chat (the recap's close button did nothing).

```
<en-US>
Fixes.

- The close button on a chat's recap now closes it.
- Sending a message hides the recap and the suggested reply at once.
- A message sent while Claude is busy leaves the Sending card once Claude reads it, even if you sent the same words before.
</en-US>
```

## Release notes, 1.1.2-dev.1 (en-US)

Version code 29. Since 1.1.1 (code 28): recaps (capability chat-recap),
side questions (chat-aside), folded tool batches and agent progress lines.

```
<en-US>
Claude Code's own summaries in your chats.

- A recap when you come back to a chat that sat idle, or one you pick up again.
- Ask on the side: type /btw and a question, or use Ask on the side in the chat menu. The answer is not added to the conversation.
- Runs of tool calls fold into one line saying what they did; tap to open.
- Subagents show a line on where they have got to.

Needs a Crundi server, version 1.21.3-dev.1 or newer, for recaps and asking on the side.
</en-US>
```

## Release notes, 1.1.1 (en-US)

Version code 28. Production, since 1.1.0 (code 18): everything from
1.1.1-dev.1 to 1.1.1-dev.9.

```
<en-US>
- Urgent alerts can take the screen and ring like an alarm: slide to attend or dismiss, snooze 5 to 60 minutes, ring 1 to 15 minutes.
- Approve secrets in one step, with a passphrase or your fingerprint.
- Allow chosen secrets for one chat, and revoke them.
- Edit a secret; old 6-digit PINs offer a passphrase on use.
- Secret requests in chat bubbles.
- What's new tour on the phone.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.9 (en-US)

Version code 27. Since 1.1.1-dev.8 (code 26): passphrases (capability
secret-passphrase), editing a secret (secret-edit), the upgrade prompt for
old locks (secret-upgrade), and refused saved locks forgotten everywhere.

```
<en-US>
Stronger secrets.

- Secrets are locked with a passphrase of 8+ characters, not only digits.
- A secret on an old 6-digit PIN offers to switch to a passphrase each time it is used.
- Edit a secret: change its passphrase, or its value, with the current one.
- A saved fingerprint passphrase the server refuses is forgotten, so the new one can be saved.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.8 (en-US)

Version code 26. Since 1.1.1-dev.7 (code 25): one-step secret approval
(PIN on the card, Approve and Reject, one height), save-with-fingerprint as
a tick box, and Secrets for this chat in a workbench pane's menu.

```
<en-US>
Approving a secret is one step now.

- The PIN box is on the request card straight away: Approve sends it, Reject refuses.
- With a saved PIN, the fingerprint sits in Approve's place until you type.
- Save PIN for fingerprint is a tick box on the card.
- Secrets for this chat is in a workbench chat's menu.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.7 (en-US)

Version code 25. Since 1.1.1-dev.6 (code 24): ring time and snooze for
urgent alerts (answers reported to alert_user), per-chat secret
pre-authorisation (server capability chat-secret-preauth), secret PINs
behind biometrics, and secret requests in chat bubbles.

```
<en-US>
Urgent alerts and secrets, made easier.

- Urgent alerts ring for 5 minutes (1 to 15 in App settings), with snooze for 5, 15, 30 min or 1 hour.
- Allow chosen secrets for one chat, for a time or until the chat ends; see and revoke them.
- Secret PINs behind your fingerprint: tap and the PIN goes in.
- Secret requests now show in chat bubbles.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.6 (en-US)

Version code 24. Since 1.1.1-dev.5 (code 23): the slider runs Attend or
Dismiss before it locks itself; each step of Attend goes in the
notification log.

```
<en-US>
Sliding a full-screen urgent alert to attend or dismiss now always acts.

- The slider no longer stops at the end without opening the chat.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.5 (en-US)

Version code 23. Since 1.1.1-dev.4 (code 22): Attend on the full-screen
alert no longer waits forever for the lock screen's answer.

```
<en-US>
Attending a full-screen urgent alert no longer gets stuck.

- After sliding to attend, the chat opens once you unlock, or waits behind the lock screen if you cancel the unlock.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.4 (en-US)

Version code 22. Since 1.1.1-dev.3 (code 21): each urgent alert is a
notification of its own, so the full-screen alert opens even when an
earlier alert is still in the shade.

```
<en-US>
Full-screen urgent alerts now open every time.

- A new alert no longer just updates an earlier one still in the notification shade, which kept it from taking the screen.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.3 (en-US)

Version code 21. Since 1.1.1-dev.2 (code 20): full-screen urgent alerts,
opt-in, with an attend/dismiss slider, and the one-time urgent alert setup.
Needs the Play Console's full-screen intent declaration (App content):
core function neither calling nor alarms.

```
<en-US>
Urgent alerts can now take the whole screen, like an alarm.

- Turn on Full-screen urgent alerts in App settings: a locked phone lights up with the alert.
- Slide right to attend, left to dismiss.
- A one-time setup after install offers full screen and ringing through Do Not Disturb.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.2 (en-US)

Version code 20. Since 1.1.1-dev.1 (code 19): urgent alerts keep ringing.
Server 1.21.2-dev.2 also stops sending a sounding "finished" right after a
chat's urgent alert.

```
<en-US>
Urgent alerts now keep ringing.

- An urgent alert is no longer cut short by the next notification: others arrive silently while it rings.
- A Dismiss button on the alert stops it without opening the app.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.1-dev.1 (en-US)

Version code 19. Everything new on the phone since 1.1.0 (code 18). Features
the server must also have (secrets, pause choices, skills from a link, urgent
alerts, the needs-cards option) appear only when the server lists them, so
the minimum server stays at 1.21.1-dev.12.

```
<en-US>
Build and watch factories from your phone, and much more.

- Create and edit factories, and follow a run live in a workbench pane.
- Skills in Settings, including install from a GitHub link.
- Webhooks for schedules and factory triggers.
- Urgent alerts that ring until you see them.
- Unsent text stays in the chat box; a sleeping chat wakes when you send.
- What's new after each update.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.1.0 (en-US)

Version code 18, the first native release. Same notes as 1.1.0-dev.2 (code 17),
which it replaces; it adds a note on each choice of a question from Claude.

Version code 17. Needs server 1.21.1-dev.12 or newer (the app's MIN_SERVER):
Docker, Factory and git history use routes older servers do not have. Code 16
(1.1.0-dev.1) was built with a 1.20 minimum and should not be released. The app is now native: every screen is drawn by the app
itself instead of the server's web page.

```
<en-US>
Crundi is now a native app.

- Every screen rebuilt for the phone: chats, workbench panes, files, git, board, mind map, schedules.
- Smoother scrolling, faster opening, full screen for any pane.
- Pictures and videos open with pinch to zoom; audio plays in the chat.
- Embeds, tables and code in replies, each with copy.
- Downloads show progress and where they were saved.
- Tap a path or link to open or copy it.

Needs a Crundi server, version 1.21.1 or newer.
</en-US>
```

## Release notes, 1.0.2-dev.3 (en-US)

Version code 15. The app's code is the same as 1.0.2-dev.2 (code 14); the
notes describe what is new since 1.0.1, the last release with notes.

```
<en-US>
Steadier connection and quieter notifications.

- The app stays connected while a bubble or live card is up, so a tap shows the chat at once instead of "Reconnecting".
- A renamed chat's live card follows the new name.
- No more being signed out when two windows renewed their login at the same moment.
- No bubble or buzz for the chat you already have open.

Needs a Crundi server, version 1.20 or newer.
</en-US>
```

## Release notes, 1.0.1 (en-US)

```
<en-US>
Fixes and improvements.

- Pair again when a phone's pairing has stopped working, replacing the old one.
- When a chat cannot open as a bubble, the app now says why.
- Chats that are already busy appear on the live card as soon as you open the app.

Needs a Crundi server, version 1.20 or newer.
</en-US>
```

The live-card catch-up needs server 1.20.1 or newer; against 1.20.0 the
app skips it without a word, so the requirement stays at 1.20.

## Release notes, 1.0.0 (en-US)

```
The first release of Crundi for Android.

- Pair with your own Crundi server by scanning a picture. No password on the phone.
- Notifications when Claude finishes or needs you, with reply, a suggested reply, and Approve or Deny.
- Chats that float as bubbles.
- A live card while Claude is working.
- Needs a Crundi server, version 1.20 or newer.
```
