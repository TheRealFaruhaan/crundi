# Services (tab `services`)

Reference: `src/webapp-html.js` loadServices ~12937, renderServices ~13749,
exposeService / beginExpose / saveExpose / removeForward ~13880-14003,
svcAction / svcLogs / svcTunnelSetPort / svcDelete ~14005-14075, the register
form ~14077-14125, forwards ~12860-12935, the usage strip ~12955-13060,
pollStats / ensureStatsPolling ~13721. Native: `ui/ops/services.slint`,
`src/ops/services.rs`, test `test/ops/services.mjs`.

System stats (Info's machine panel), collaborators and maintenance are drawn
in Info by the web, not here (the info helper's).

## Parts, top to bottom

1. Toolbar: "Services" ("Services · all projects" with no project), "+ Register service".
2. Register form (when open): "Register a service", Service name, Command (e.g. npm run dev),
   Working directory (optional), Stop command (optional, e.g. docker compose down),
   Tunnel port (optional) — expose via Cloudflare; Register (primary), Cancel.
3. Empty: server icon, "No services registered[ for this project]".
4. One card per service of the project (all with none):
   - head: dot, name, status pill (RUNNING / STOPPED / ERROR);
   - mono command line, "· up 3m" while running;
   - usage strip while running: CPU value + sparkline (green/yellow/red by load),
     MEMORY value + sky sparkline, "pid N" / "N containers" / "no containers";
   - actions: Stop, Restart (running) or Start (primary); Logs; Tunnel (only with no port
     and no tunnel); Delete (danger);
   - exposure row (port, tunnel or forward): globe, "Port N" / "Set port", Off | Tunnel |
     Subdomain | Path, then the address (link, PUBLIC tag) or the word (off, idle,
     starting… yellow, error red), the forward's trash;
   - naming form (Subdomain/Path): "/tunnel/" prefix, name field, ".domain" or "/",
     Save (primary), Cancel; "Public — reachable without signing in to Crundi";
   - inline logs (mono, max 200, scrolled to the end; "(no logs)", "Error: …").
5. Other forwards (forwards on ports no service has, in this project or none): head,
   hint, rows: address link, PUBLIC, "port N", project select (No project, aliases), trash.

## Behaviour

| Control | Does |
|---|---|
| Register service | opens the form (a second click leaves it); Register checks name/command ("Service name is required", "Command is required"), POST /api/services, toast "Registered X", reload after 500ms |
| Start/Stop/Restart | POST /api/services/:key/:action, toast "<action> <key>", reload after 500ms |
| Logs | GET /api/services/:key/logs, toggles the inline log |
| Tunnel / Port N | prompt "Tunnel port" (blank or 0 clears), POST …/tunnel {port}, toast "Tunnel port set to :N" / "Tunnel port cleared" |
| Delete | confirm 'Delete service "key"?', POST …/delete, toast "Deleted key" |
| Off | delete the port's forward, tunnel off; "No longer exposed" |
| Tunnel | clear, then tunnel on: "Starting a tunnel" |
| Subdomain / Path | no port: "Set a port first"; subdomain without domain: "No domain is configured…"; opens the naming form seeded with the forward's name and public flag, or the service's name |
| Save | name rule (lowercase, digits, hyphens); public asks "Publish name?…" first; delete old forward, tunnel off, POST /api/forwards {name, port, mode, public, project}; "Reachable at URL" |
| trash | DELETE /api/forwards/:host, "Removed" |
| project select | PATCH /api/forwards/:host {project}; "host now serves p" / "host is not linked to a project" |
| address | opens in the browser (a path one on this server) |

Live: GET /api/stats every 2s while the tab is on show (the strips); the
`state` event's services reload the cards when a service's status, port or
tunnel changes (a service started or registered elsewhere), and a reconnect
reloads.

## States (screens/ops/)

services-empty, services-register, services-running, services-logs,
services-expose-form, services-forward, services-public, services-no-port,
services-other-forwards, services-stopped.

## Differences / gaps

- Owner only: the collaborator's limits (canEdit, no other forwards) are left out (DESIGN rule 4).
- The Off/Tunnel/Subdomain/Path buttons have no hover titles (kit Seg has none).
- The rows do not wrap on a narrow pane (Slint layouts don't wrap); the address elides.
- The live reload on `state` is an addition (the web refreshes on its own actions and tab opens).

## Checklist

- [x] toolbar, title per project, register form, empty
- [x] cards: head, command, uptime, strip with sparklines, actions
- [x] logs toggle; delete confirm; port prompt
- [x] exposure: off, tunnel, subdomain (refused without domain), path form, public confirm, remove
- [x] other forwards: list, project, remove
- [x] stats poll only on the tab; live state reload
