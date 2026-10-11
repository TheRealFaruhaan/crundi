# Crundi for Android

The server's own page in a web view, with the phone's features round it. The
plan, and what is and is not built, is in [`PLAN.md`](PLAN.md). The push relay
it will receive notifications through is in [`../relay`](../relay), and what
the two say to each other is in [`RELAY.md`](RELAY.md).

## Building

Needs JDK 17 and the Android SDK (platform 36). From this folder:

```bash
./gradlew testGithubDebugUnitTest      # the unit tests
./gradlew assembleGithubDebug          # app/build/outputs/apk/github/debug/
./gradlew assemblePlayDebug            # app/build/outputs/apk/play/debug/
```

`local.properties` (not committed) must say where the SDK is:
`sdk.dir=/path/to/android/sdk`.

## Two builds

- **`github`** updates itself from GitHub releases. CI builds this one.
- **`play`** is for the Play Store and is built by hand. It contains no code
  that knows about GitHub: `src/github/` is not compiled into it.

Which one an installed app is was decided when it was built. The app does not
work it out at run time. Each tells the server which it is
(`CrundiAndroid/<version> (github)` in the user agent).

## What is here

| File | What it does |
|---|---|
| `MainActivity` | Asks the server who it is, checks neither side is too old, then shows the server's page. File picking, downloads, camera and microphone for the page, back. |
| `PairActivity` | Pairs with a server from a scanned or pasted link. |
| `ScanActivity` + `assets/scan.*` | The camera, and the reader for the pairing picture. The reader is `../src/crundi-code.js`, copied in at build time: one implementation, drawn by the server and read here. |
| `Gate`, `Versions` | The version check, in both directions. |
| `Store` | Paired servers, sealed with a key held in the phone's keystore. |
| `Api` | The few server calls the app makes itself. |
| `src/github`, `src/play` | Where each build looks for updates. |

## Versions

- `versionName` in `app/build.gradle.kts` is the app's version. The server
  holds the oldest one it accepts (`MIN_CLIENT.android` in
  `../src/client-version.js`).
- `MIN_SERVER` in the same file is the oldest server this app accepts.
- Raise either only for a real break.
