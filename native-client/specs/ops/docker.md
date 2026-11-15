# Docker (tab `docker`)

Web: `app/vendor/crundi-docker.js`, `loadDockerTab` / `checkDockerPresence`
(src/webapp-html.js). Native: `ui/ops/docker.slint` (+ `docker-parts.slint`),
`src/ops/docker.rs` (+ `docker/fmt.rs`, pure, unit-tested). Test:
`test/ops/docker.mjs`. Screens: `screens/ops/docker-*.png`.

## Parts

- **Gate** (every show, `GET /api/docker/status`): "Docker is not installed"
  / "Docker is installed, but Crundi cannot reach it" + its error + Try again
  (`?force=1`); the nav is hidden in both.
- **Nav**: segmented Containers, Stacks, Images, Volumes, Networks, Forwards,
  System (icon, label, count); Filter (per view; hidden in a detail and on
  System); Refresh.
- **Containers** (poll 3s): summary (running, paused, stopped, CPU of one
  core, memory), bay plan (a square per container by state, tooltip, click
  opens), rows (dot, name, image · status, chips: project, ≤3 published
  ports each with its forwards, health; CPU, MEM; Stop (two-click "Stop?") /
  Resume / Start). Empty: "No containers"; "Nothing matches the filter."
- **Detail** (head refreshed 4s): back, dot, name, image · status · project;
  Stop (arm "Stop <name>?"), Restart, Pause, Kill (arm) / Resume, Stop / Start;
  Rename, Remove. Rename and remove are inline lines under the head (Enter /
  Esc; remove: "Also its anonymous volumes", "Stop it first").
  Tabs Overview, Logs, Stats, Shell, Inspect.
  - Overview: State, Runs (two columns); Ports (each published port: its
    forwards and "Add forward"/"Another", the forward form inline under it),
    Networks; Mounts; Environment (secret-looking values hidden + note);
    Labels (n), folded.
  - Logs: `docker-logs-follow` over /ws (tail 500, 5000 lines kept; a REST
    read when the socket is down); Filter lines (marks matches), Follow,
    Times, Wrap, Copy, Clear; ANSI colours; stream ends and errors as a
    muted line.
  - Stats: every 2s; CPU, Memory, Network (in/out), Disk (read/write) charts
    with legends; "processes · a sample every 2 seconds…"; not running: says so.
  - Shell: `docker-shell-*` over /ws; bar "A shell inside <name>…" + New
    shell; on exit "The shell ended (exit n)." / "This image may not include
    a shell" + Open again; stopped: "Start the container to open a shell in it."
  - Inspect: the raw JSON (selectable) + Copy JSON.
- **Stacks** (poll 5s): per project: name, "n/m running", compose files (or
  "not found on this machine"); Up, Pull (disabled without the file),
  Restart + Stop (arm) or Start, Down (arm); its services (open the detail).
- **Images**: Pull field + Pull (job drawer), Clean untagged / Clean unused
  (arm); "n images, n in use · size"; rows with users (open), untagged/unused,
  remove (arm) or locked when in use.
- **Volumes**: count line, Clean unused (arm); rows; remove (arm).
- **Networks**: count line, Clean unused (arm); "built in", "internal";
  remove (arm), locked for Docker's own or in use.
- **Forwards** (poll 10s): grouped Nothing behind it / Container stopped /
  Running container / Another program (with help); public, watched; the
  container chip (opens), Open; bell (watch), remove (arm); Add forward
  (port, name, mode, Public, Warn if it goes down), Remove orphaned (arm);
  empty state with Add forward.
- **System**: Engine, Contents (+ warnings); Disk use table (measuring…;
  per row Clean (arm), disabled with nothing to reclaim); the shared note.
- **Job drawer**: title, spinner → done / exit n, hide; output polled 0.7s.

## Endpoints

`/api/docker/{status,containers[?stats=1],containers/detail,containers/logs,
containers/stats,containers/action,containers/rename,containers/remove,
stacks,stacks/action,images,images/pull,images/remove,volumes,volumes/remove,
networks,networks/remove,system,system/df,prune,job,forwards}`,
`/api/forwards` (POST, PATCH watch, DELETE), `/api/forwards/options`;
/ws `docker-logs-*`, `docker-shell-*`.

## Differences (Slint limits)

- The shell is a text screen (CR, backspace, erase, clear; colours dropped)
  with a command line under it (Enter runs it; Ctrl+C/D/L/Z, Tab, arrows are
  sent as keys): Slint has no terminal emulator (xterm). Full-screen
  programs (vi, top) do not draw properly.
- Wrap off cuts a log line at the pane's edge (StyledText always wraps; no
  sideways scroll). A match is marked underlined in the accent colour (no
  background in StyledText).
- The tab button stays when Docker is absent (the web hides it): it needs a
  hook in main.slint's tab bar (request in shared/requests.md); the tab says
  "Docker is not installed".
- The chart's dashed middle line is a solid hairline.

## Checklist

- [x] gate: ok, unreachable (+Try again), absent (DK_EXPECT runs)
- [x] every view loads, filters, empty / no-match states
- [x] detail: actions, rename, remove, overview, logs (live, filter, times,
      copy, clear), stats, shell, inspect (copy)
- [x] forwards: create from a port and from Forwards, watch, remove, orphans
- [x] volumes / networks remove (the test's own only), job drawer (a failed pull)
- [x] system + disk use
