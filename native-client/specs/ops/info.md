# Info (ops `info`)

Web: renderInfo, copyServerLogs, renderServerLogs, appendServerLog,
infoRow (~14431-14548), renderSystemStats (~13084), loadMaintenance /
renderMaintenance / runMaintenance (~13624), loadCollaborators /
collabAction / collabCreate / showInviteModal / offerGitInit (~13190-13620).
Native: `ui/ops/info.slint`, `src/ops/info.rs`.

## Parts (in order)
1. PROJECT (alias, name, path) and STATUS (Claude Terminal Active / Not
   started; Services "N registered, M running"): only with a project open.
2. THIS MACHINE: CPU %, one bar per core (green / amber 60 / red 85), the CPU
   history sparkline (area + line), Memory, Swap (if any), Disk with bars;
   Load, Traffic (where the server has them), Uptime, Crundi, Machine,
   Processor; one line per network interface. Every 2 s while on show
   (GET /api/stats), stopped when another tab shows.
3. RECLAIM DISK SPACE: the total, one row per task (label, root /
   destructive tags, description, "Will remove: …", size, Clear/Remove with
   the two-click confirm "Clear it?" / "Prune it?" / "Remove N containers?",
   then "Clearing…"). GET/POST /api/maintenance.
4. OUTSIDE COLLABORATORS: the sandbox line (ready, or the problems with Set
   up sandbox / the apt line); "Nobody outside has access right now."; one
   card per person (name, @telegram or "passcode only", New passcode; tokens
   used with the meter, Set/Change limit, Reset; allowed sites with ×; each
   project: branch, time left, Extend, Merge, Revoke/Restore, Remove); the
   invite form (name, Telegram, projects to tick, Access lasts N days/hours
   with the end date, token limit, Create access). Dialogs: the new
   passcode (fields with Copy, Copy everything, Done) and "not a git
   repository yet" (Cancel, Set up git and share). GET/POST /api/collaborators.
5. TUNNEL: URL (opens), Status, Cloudflare service. GET /api/tunnel.
6. SERVER LOGS: Copy; the log box (300 high), lines coloured by level,
   newest at the bottom; live lines from /ws `server-log` after
   `subscribe-logs` (sent again on reconnect). GET /api/server-logs.

## Checklist
- [x] every section, the web's order and wording
- [x] live: stats every 2 s, log lines, the project's status from `state`
- [x] Copy (all lines; the web copies a selection when there is one)
- Gaps: Slint text can't be selected across lines, so Copy takes the whole
  log; the invite's project list is a list of ticks (the web's multi-select).

Screens: `screens/ops/info-*.png`.
