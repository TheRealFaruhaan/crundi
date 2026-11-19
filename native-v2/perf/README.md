# Measuring on the owner's PC

`tools/measure-pc.mjs` runs the release client in one window on the PC's
own display, closed by its PID, and reports each scenario from
`ZED_MEASUREMENTS=1`: frames, p50, p95, max, the share ≤ 7 ms, the frames
over 7 ms and over 16 ms, and which views rendered (the driver's `renders`).

**It never uses the owner's sign-in, config or server.** A used or rotated
refresh token can get the owner's login revoked, so the harness:

- refuses to run without `CRUNDI_CONFIG_DIR`, with the owner's own config
  folder (`~/.config/crundi-native`), or with a folder that already holds
  anything; it removes that folder when it is done;
- signs in to a **fresh test server** on the Crundi server
  (`native-gpui/tools/test-server.sh <port> --bare`, a port in 4880-4889),
  reached from the PC through a forward, with that server's own new account
  (it refuses a server already set up, or one with a Telegram bot);
- seeds what it measures there: a project "demo" and four chats with long
  replies (a 300-line list and long mixed markdown), and streams a fifth.

On the Crundi server, then on the PC:

    native-gpui/tools/test-server.sh 4884 --bare        # (forward 4884 to the PC)

    export PATH=$HOME/.cargo/bin:$PATH
    CRUNDI_BUILD_JOBS=8 RUSTFLAGS='-C force-frame-pointers=yes' native-v2/tools/build.sh --release
    export CRUNDI_CONFIG_DIR=$(mktemp -d /tmp/crundi-v2-measure-config-XXXXXX)
    MEASURE_SERVER=http://localhost:4884 CRUNDI_TEST_PORT=4881 node native-v2/tools/measure-pc.mjs
    MEASURE_STREAM=0 ...                                 # without the streaming chat

However the run ends (done, a failure, Ctrl+C or another signal, the
client dying, or `MEASURE_TIMEOUT`, 2400 s by default), the harness stops
the client by its PID, removes the config folder and writes what it has
to `report.json`; a seeded chat's reply is waited for by its `agentState`
(each seeded chat a new conversation, `sessionMode: "new"`). The owner's
config folders are refused by name on each system: `~/.config/crundi-native`,
`%APPDATA%\Crundi Native`, `~/Library/Application Support/Crundi Native`
(and the folders holding them).

The test server's chats run Claude on the Crundi server. Every run needs a
fresh server (the harness sets it up): restart it with the same command.

## A profile without perf

`perf record --call-graph fp` needs `kernel.perf_event_paranoid` ≤ 2; on
farhan-RP-15 it is 4 and there is no sudo. In that case gdb samples the UI
thread instead. With `ptrace_scope=1`, gdb may only attach to a process that
allows it, so start the client through `perf/ptracer-exec.py`, which calls
PR_SET_PTRACER and then execs the binary (the PID stays the same):

    printf '#!/bin/sh\nexec python3 -I native-v2/perf/ptracer-exec.py "$@"\n' > /tmp/ptr.sh; chmod +x /tmp/ptr.sh
    CRUNDI_CONFIG_DIR=$(mktemp -d) MEASURE_SERVER=http://localhost:4884 \
      PROFILE=native-v2/perf/gdb-sample.py SAMPLES=700 MEASURE_STREAM=0 \
      node native-v2/tools/measure-pc.mjs /tmp/ptr.sh /tmp/v2-profile

`gdb-sample.py` stops the process every ~12 ms with SIGUSR2 (not passed on to
the client), records thread 1's full stack in `samples.txt` and goes on. The
pauses stretch frames, so the numbers come from the unprofiled runs before it.

Summarise the samples (busy vs idle, stage, inside or outside the log's list)
with `python3 -I native-v2/perf/gdb-summary.py /tmp/v2-profile/samples.txt`.

A test server on the PC must **not** run the owner's Claude CLI (the seeded
chats would spend the owner's turns): use `native-v2/tools/stub-server.sh
<port>` there. It is a bare test server whose `claude` is
tools/claude-stub.mjs (stream-json, a fast streamed reply, titles), with
its own HOME, so nothing of the owner's ~/.claude is reachable from it.
Results: perf/pc-2026-11-20c.md. For a chat scene without any server,
use the floor's `chatlog` and `chatlog5` modes (below): their histories are
seeded in the client.

## The floor

`crundi-v2-floor` (crates/app/src/floor.rs, built with the app) opens
one window with no server and no sign-in. Its modes go from GPUI's emptiest
frame up to five seeded chat panes: `empty`, `list`, `uniform`, `workbench`
(five panes of plain text), `chatlog` and `chatlog5`. `tools/measure-floor.mjs`
sweeps each with the PC harness's wheel (tools/sweep.mjs) and reports
idle frames, p50, p95, the share at most 7 ms and the renders:

    CRUNDI_BUILD_JOBS=8 native-v2/tools/build.sh --release -p crundi-app
    node native-v2/tools/measure-floor.mjs                 # on the PC's display (the Measure step)
    FLOOR_HEADLESS=1 xvfb-run -a node native-v2/tools/measure-floor.mjs   # headless: idle check only

Under xvfb-run, `FLOOR_HEADLESS=1` is required: without it, GPUI follows
`WAYLAND_DISPLAY` to the desktop session and opens the window there.
`PROFILE=native-v2/perf/gdb-sample.py SAMPLES=…` samples the UI thread
after the sweep, as above. Results: `perf/floor-2026-11-20.md`.
