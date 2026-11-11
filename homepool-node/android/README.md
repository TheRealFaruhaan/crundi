# Homepool Node (Android)

A small app that lends a phone's connection to a Crundi server, for a friend
who does not use Crundi. It holds one websocket to the server
(`/api/homepool/node`, signed in with a node key) and opens, stream by stream,
the TCP connections the server asks for. It only dials out, listens on
nothing, and refuses to open anything on the phone itself or its home network.

It is the same node the Crundi app runs under Settings › Homepool: this
project compiles the Crundi app's package `mv.obitech.crundi.homepool`
(`../../android/app/src/main/java/mv/obitech/crundi/homepool`) as it is, and
adds only its own screen (`app/src/main/java/mv/obitech/homepoolnode`). The
wire protocol is described in `../../app/homepool-node.js`.

What the screen has: status (Connected / Connecting… / Paused, with why /
Stopped / Refused), node, network, public IP, connected for, data today (and
the mobile part, of the cap), what the server has it carrying; Start / Stop;
Register / Unregister service (start at boot and after an update, restarted
if Android kills it); Scan QR / Paste code / `crundi-homepool://` links;
Wi-Fi only; a daily mobile-data cap (default 500 MB); the battery-exemption
and notification prompts and phone-specific help; the recent activity log.
A VPN on the phone pauses it, and a network change reconnects it at once.

## Building

Needs JDK 17 and the Android SDK (platform 36). `local.properties` (not
committed) says where the SDK is, e.g. `sdk.dir=/path/to/android/sdk`. Then:

```bash
./gradlew assembleDebug          # app/build/outputs/apk/debug/app-debug.apk
./gradlew testDebugUnitTest      # the node code, frames and private-target rule, and the protocol against a stub server
```

On the Crundi dev server the toolchain is in `~/android-toolchain`:

```bash
JAVA_HOME=~/android-toolchain/jdk ./gradlew assembleDebug
```

A debug build is signed with the SDK's debug key and installs as it is. A
release build is not signed by this project; no keys are kept here.

## Setting a phone up

1. The owner of the Crundi server makes a node for it: **Settings › Homepool
   › Nodes › Add node**. Crundi shows the node's code
   (`crundi-homepool://node?s=…&k=hpn_…`) and its QR.
2. The friend installs this app, opens it and taps **Scan QR** (or copies
   the code and taps **Paste code**, or opens the link on the phone).
3. **Register service** to keep it running (it starts at boot), or **Start**
   to run it only until stopped. Allow notifications and unrestricted battery
   when asked; on Xiaomi, Samsung, Oppo, Vivo and Huawei also allow Autostart
   (see **Phone-specific help**).

The key only lets this phone carry traffic for that server; it is not a
Crundi login. It is kept encrypted with a key from the phone's keystore. If
the owner switches the node off or removes it, the app shows **Refused** and
stops trying; a new code from the owner sets it going again.
