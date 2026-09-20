# Working on a Crundi host

The loop for doing work on a machine where Crundi is the workbench: run things,
show them to the user, test them, keep them running, hand them over. Commands
differ per platform — take them from `platform-linux.md` or
`platform-windows.md`, not from memory.

Nothing here assumes a language, framework, or kind of project. Find the
project's own commands — README, build file, task runner, CI config — and run
those. What follows is only the Crundi side: where a process should live, how the
user sees it, and what not to break on a shared machine.

The user is often on a phone, reading a narrow column, and cannot see your
terminal. That shapes everything below: prefer things they can *look at* — a URL
that works, a screenshot — over descriptions of what should be happening.

## Long-running things belong in a service

Anything that outlives a single command — a server, a watcher, a queue worker, a
long build — should be registered, not backgrounded:

```
register_service { name, command, cwd, stopCommand? }   → key "alias::name"
start_service    { key }
get_service_logs { key, lines: 50 }
```

A backgrounded command in a Bash call dies with the tool call or lingers with
nothing able to manage it. A service survives the chat, appears in the UI, and the
user can stop it without you.

- `command` runs through the host's own shell — write it for the platform you are
  actually on.
- `cwd` must be absolute.
- Add a `stopCommand` for anything that does not stop cleanly on a signal.
- Watch the first lines of `get_service_logs` before declaring it up. A process
  that crashes on boot still "started" as far as the service is concerned.

Example `command` values — illustrations only; use whatever the project in front
of you actually uses:

| Kind of thing | Example command |
|---|---|
| Node dev server | `npm run dev -- --port 4300 --host 127.0.0.1` |
| Python web app | `.venv/bin/uvicorn app:app --host 127.0.0.1 --port 4310` |
| Go binary | `./bin/server --addr 127.0.0.1:4320` |
| Static preview | `python3 -m http.server 4330 --bind 127.0.0.1` |
| Watcher / worker | `npm run watch`, `.venv/bin/celery -A app worker` |

Use `Bash` for one-off commands, and the project's own test or check command
before claiming anything works.

## Ports, if the work listens on one

Never assume a port is free. Another project, another Claude session, or Crundi
itself may hold it, and the loser of that race fails silently.

1. Pick a high port: 4000–9999, avoiding Crundi's own (443, 80, 8888) and 8889.
2. Check it is free — the command is in your platform file.
3. Bind `127.0.0.1`, not `0.0.0.0`. On a box with a public IP, `0.0.0.0` puts it
   on the internet with nothing in front of it.
4. Expose it deliberately with `add_forward` — never by opening a firewall port.

If the port is taken, pick another and say which. Do not kill whatever holds it;
it probably belongs to someone else's work.

## Letting the user reach it

A local port is invisible from a phone. Publish it:

```
add_forward { name, port, mode: "subdomain" | "path", public?: false }
```

- `subdomain` → `name.<domain>`, on the server's own certificate.
- `path` → `<domain>/tunnel/name/`, when DNS for a new subdomain is not set up.
- **Private by default** — reaching it requires a Crundi sign-in, which is what
  you want for anything unfinished. Set `public: true` only when the thing has
  its own authentication (an OAuth callback that must be reachable
  unauthenticated, say), and say so when you do.

Give the user the URL on its own line. `enable_tunnel` is the fallback when a
forward will not do; its hostname changes, so it is worse for anything you want
to send someone.

Remove the forward when the work is done.

### A worked example

A dev server on a Linux host, from nothing to a URL the user can open:

```
# 1. is the port free?  (Bash)
ss -ltn | awk '{print $4}' | grep -E ':4300$'        → no output, so yes

# 2. run it as a service
register_service { name: "app-dev", command: "npm run dev -- --port 4300 --host 127.0.0.1",
                   cwd: "/home/<user>/projects/<project>" }
start_service    { key: "<alias>::app-dev" }
get_service_logs { key: "<alias>::app-dev", lines: 30 }   → "ready on 127.0.0.1:4300"

# 3. give the user a way in
add_forward { name: "app-dev", port: 4300, mode: "subdomain" }
             → https://app-dev.<domain>   (private: needs a Crundi sign-in)

# 4. check it yourself before sending it
browser_open    { url: "http://127.0.0.1:4300", width: 420, height: 880 }
browser_console { key }                                   → no errors
browser_screenshot { key }

# 5. when the work is done
remove_forward { host: "app-dev" }
stop_service   { key: "<alias>::app-dev" }
delete_service { key: "<alias>::app-dev" }
browser_close  { key }
```

## Checking a web interface

When the thing has a browser UI, drive the real one rather than reasoning about
it:

```
browser_open    { url, width, height }
browser_console { key }                 → did it throw?
browser_snapshot / browser_elements     → is the DOM what you think?
browser_screenshot { key }              → what it actually looks like
```

- Check `browser_console` after every UI change. A silent JS error looks exactly
  like "my styling did not apply".
- Resize to a phone viewport before claiming a mobile fix works.
- `browser_network` (`action: "start"`, then `"log"`) when a request is the
  suspect — better than adding logging to the code.
- Close instances when finished; there is a cap of 5 per project.

For a visual change worth confirming, send the screenshot with
`send_photo_to_user`. A picture settles "is this what you meant?" in one round
trip instead of three.

## Keeping the user informed without spamming

- `send_message_to_user` for what is worth knowing before they next look: a long
  job finished, something failed, a question that blocks you. Not routine
  progress.
- `kanban_*` and `mindmap_*` are the shared record. If the user works from the
  board, move the card when you start and when you finish — they read state from
  the UI, not from the transcript.
- `schedule_*` for anything recurring they asked for, rather than promising to
  remember.
- `get_usage` before starting something long, if limits are a concern.

## Credentials

Whatever the project deploys to, the rule is the same:

- **Use `secret_run`.** It binds a stored secret to an environment variable for
  exactly one command; the value never reaches the transcript and is redacted
  from the output. `secret_search` first, to find the right one by name.

  ```
  secret_search { query: "registry" }        → pick the right id
  secret_run {
    id: "<id>",
    envName: "TOKEN",
    command: "curl -sS -H \"Authorization: Bearer $TOKEN\" https://api.example.com/v1/deploy",
    reason: "Trigger the deploy the user asked for"
  }
  ```

  The user reads that exact command before approving, so keep it plain and
  minimal — one command that does the thing, not a script they have to audit.
- `secret_get` only when the value itself must genuinely be known.
- Never paste a token into a file, a command you echo, a commit, or a service
  `command` string — service definitions are stored and visible in the UI.
- Never put a value, or anything you would not show a collaborator, in a secret's
  name or description: those are stored in the clear and are searchable without
  authentication.

## Handing over

Before you finish:

- Stop and delete services you created for the work.
- Remove forwards you added.
- Close browser instances.
- Say what is still running and why, if anything is meant to stay up.

Leaving them behind holds ports and leaves URLs live.

## Shared-machine etiquette

Other projects and other Claude sessions run here at the same time.

- Do not restart the Crundi server to fix your own problem. It ends every chat
  and terminal on the machine, including other people's. Ask first.
- Do not kill processes you did not start, or take a port you found occupied.
- Keep work inside the project directory unless the task genuinely reaches wider.
- `sudo` (where it exists) is for tasks that actually need it — a system package,
  a system service — not a default.
