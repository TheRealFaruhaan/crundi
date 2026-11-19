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
