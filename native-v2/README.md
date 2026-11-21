# native-v2

Crundi's native PC client v2: the PC client's experience, built for speed
on GPUI the way Zed is. It replaces `native-gpui/` (which stays untouched
until it does).

- `ARCHITECTURE.md`: the rules every crate follows (read first).
- `AREAS.md`: who owns which crate, how an area plugs in, the shared
  controls, building and testing.
- `PLAN.md`: every screen and where it stands, with its frame numbers.
- `vendor/gpui`: GPUI 0.3.12 with two cache fixes (`PATCH.md`: nested
  cached views, moved views replayed), wired by `[patch.crates-io]` in
  `Cargo.toml`; delete both to go back to the published crate.

```
tools/build.sh                                          # the client: $HOME/.cache/crundi-native-target/debug/crundi-v2
tools/build.sh test --workspace                         # unit tests (headless; the browser host's
                                                        # need CEF: add --exclude crundi-browser-host without it)
tools/check.sh                                          # no window refresh anywhere (CI runs it)
node tools/frames.mjs <stderr log>                      # a ZED_MEASUREMENTS=1 log summed up
```

End-to-end suites (`test/*.mjs`) run on the Crundi server under Xvfb
against a fresh test server; see `test/lib.mjs`. On the owner's PC (when
the owner has said go), a suite that starts chats runs against
`tools/stub-server.sh <port>`, never `test-server.sh --bare`: there the
bare server's `claude` is the owner's own CLI, and its chats would run real
turns on the owner's account (F142). The same for `tools/measure-pc.mjs`.
