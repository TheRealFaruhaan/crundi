# Measuring on the owner's PC

`tools/measure-pc.mjs` runs the release client with the owner's own config
folder (their sign-in, their server and their open project, as they are), one
window, closed by its PID, and reports each scenario from `ZED_MEASUREMENTS=1`:
frames, p50, p95, max, the share ≤ 7 ms, and the frames over 7 ms and over 16 ms.

    export PATH=$HOME/.cargo/bin:$PATH
    CRUNDI_BUILD_JOBS=8 RUSTFLAGS='-C force-frame-pointers=yes' native-v2/tools/build.sh --release
    CRUNDI_TEST_PORT=4881 node native-v2/tools/measure-pc.mjs              # with a streaming chat
    MEASURE_STREAM=0 node native-v2/tools/measure-pc.mjs                   # without it

## A profile without perf

`perf record --call-graph fp` needs `kernel.perf_event_paranoid` ≤ 2; on
farhan-RP-15 it is 4 and there is no sudo. In that case gdb samples the UI
thread instead. With `ptrace_scope=1`, gdb may only attach to a process that
allows it, so start the client through `perf/ptracer-exec.py`, which calls
PR_SET_PTRACER and then execs the binary (the PID stays the same):

    printf '#!/bin/sh\nexec python3 -I native-v2/perf/ptracer-exec.py "$@"\n' > /tmp/ptr.sh; chmod +x /tmp/ptr.sh
    PROFILE=native-v2/perf/gdb-sample.py SAMPLES=700 MEASURE_STREAM=0 \
      node native-v2/tools/measure-pc.mjs /tmp/ptr.sh /tmp/v2-profile

`gdb-sample.py` stops the process every ~12 ms with SIGUSR2 (not passed on to
the client), records thread 1's full stack in `samples.txt` and goes on. The
pauses stretch frames, so the numbers come from the unprofiled runs before it.
