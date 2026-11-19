# native-v2

Crundi's native PC client v2: the PC client's experience, built for speed
on GPUI the way Zed is. It replaces `native-gpui/` (which stays untouched
until it does).

- `ARCHITECTURE.md`: the rules every crate follows (read first).
- `AREAS.md`: who owns which crate, how an area plugs in, the shared
  controls, building and testing.
- `PLAN.md`: every screen and where it stands, with its frame numbers.

```
tools/build.sh                                          # the client: $HOME/.cache/crundi-native-target/debug/crundi-v2
tools/build.sh test --workspace --exclude crundi-browser-host   # unit tests (headless)
tools/check.sh                                          # no window refresh anywhere (CI runs it)
node tools/frames.mjs <stderr log>                      # a ZED_MEASUREMENTS=1 log summed up
```

End-to-end suites (`test/*.mjs`) run on the Crundi server under Xvfb
against a fresh test server; see `test/lib.mjs`.
