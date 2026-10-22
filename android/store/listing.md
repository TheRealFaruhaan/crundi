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

## Release notes, 1.1.0-dev.1 (en-US)

Version code 16. The app is now native: every screen is drawn by the app
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

Needs a Crundi server, version 1.20 or newer.
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
