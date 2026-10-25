# Factory — spec

Status: agreed design, not started. Written 2026-10-21 from the design discussion.

## 1. What it is

A **factory** is a reusable workflow template: a graph of AI employees and
decisions that turns a task into a finished, reviewed change. Factories are
global (not project scoped) and can be run in any project. A **run** is one
factory executing one task in one project.

The engine knows no roles except the **Manager**. Everything else (planner,
devs, reviewers, security…) is whatever the user draws.

Core principle: **Claude never chooses the next step. It answers questions;
Crundi routes.**

## 2. Concepts

### 2.1 Roster
The factory's employees. Each entry:

| Field | Notes |
|---|---|
| name | "Frontend dev" |
| persona | free text the employee reads and acts as; starts from a template, user edits freely |
| model, effort | per employee |
| tools | allowlist; e.g. reviewers read-only (no Edit/Write). Roles are enforced by tools, not just persona |
| memory | `keep` (one session for the whole run, resumed on every visit) or `fresh` (new session each visit) |
| skills | optional pinned skills (see 2.5); `/skill-name` mentions in the persona work too |

The Manager is a roster entry that always exists and cannot be deleted.

### 2.2 Graph — node types

| Node | Rules |
|---|---|
| Start | exactly one; the task enters here |
| Manager | exactly one; entry and final decider. Has a built-in, undeletable **Finish** output wired to End |
| Employee | references a roster entry. Several nodes may reference the same employee (same session/memory) |
| Decision | a question: `yes/no`, `pick one`, `pick many`. Pick many fans out in parallel and must include a "none of these" option |
| Join | waits for the branches of its matching fan-out that actually started |
| Ask me | a human gate; notifies the user (push/Telegram) and waits. Each possible reply is an output |
| Wait | holds the flow: a length of time, a time of day, until a command succeeds (polled in the worktree, every N s), or until a signal (a secret POST URL, or the person). Command and signal waits have **Done** and **Timed out** outputs and a time limit, so every wait is bounded. What came in goes on to the next step with what happened. The plan is journaled and re-armed after a recovery; the person can end any wait early |
| End | always goes through the Manager's present-and-merge step |

**Who answers a decision:** the employee whose node feeds into it; overridable
per decision ("answered by: Manager" or another roster entry).

### 2.3 Graph — edge types

- **Flow edge** — passes control.
- **Note edge** — delivers a message only (e.g. "instructions for reviewers").
  The note waits in the target's inbox until flow reaches it. Note edges never
  count for reachability.
- A flow edge may **require a note** ("what to change"), filled in by the
  answering employee.

### 2.4 Handoff
Every factory chat (and only those) is offered a `factory_handoff` MCP tool:
`{ summary, answers: {<decisionId>: answer}, notes: {<id>: text}, finish?,
escalate? }`. The schema is generic (an MCP tool list is fixed when the session
starts, but one employee visits different nodes); each step message spells out
exactly which answers and notes apply, as a tree keyed by answer, with an
example. Crundi validates the call against the route actually taken and, if
anything is missing or wrong, the call fails saying what, and the employee
calls again.

**Back-edge into a pick many:** re-runs only the branches picked last time,
unless the decision is answered again.

### 2.5 Skills
Employees are full Claude sessions and can use skills like any chat.

- **Available to every employee:** global skills (`~/.claude/skills`),
  synced skills, and the run project's skills.
- **Project skills reach the worktree.** Project skills live in
  `<project>/.claude/skills`, and the worktree is outside the project, so
  Claude would not find them (it looks upward from cwd), and uncommitted or
  ignored skills would not be in the checkout anyway. The runner links the
  project's skills folder into the run and branch worktrees (the whole folder
  if the worktree has none; otherwise each missing skill), and those links are
  excluded from every commit by pathspec. Verified: Claude loads a skill
  through the link.
- **The Skill tool is always in an employee's allowlist** unless the user
  removes it on purpose.
- **Skill mentions in any prompt text** — persona, Manager instructions,
  decision questions, edge note templates, Ask me prompts, the task given at
  start, recovery notes, request-changes notes. Written `/skill-name`; the
  editor autocompletes from the skill list and renders them as chips.
- **At runtime** each mention is resolved and the prompt gets a line naming the
  skill and its description with "use it via the Skill tool". The skill body is
  not inlined; Claude loads it when it invokes it.
- **Roster field `skills`:** optional pinned skills per employee, treated the
  same as mentions in the persona.
- **Validation:** factories are global and skills can be project scoped, so an
  unknown skill is a **warning** in the editor ("not installed globally; will
  need to exist in the project"). At run start every mention is checked against
  what that project can see; missing ones are listed in the start dialog with
  Install / Start anyway / Cancel.
- **Freshness:** a running session keeps the skills it started with. Fresh-
  memory employees, resumed employees (after retirement or recovery) and newly
  spawned sessions pick up skills installed mid-run.
- **Live view** shows which skills a node invoked.

## 3. Validation (no run without a guaranteed end)

Runs live in the editor (badges on the offending node/edge, with a suggested
fix), on save, on import, and at run start. Invalid factories save as
**drafts** but cannot be started.

1. Exactly one Start and one Manager; Manager's Finish → End present.
2. Every node reachable from Start (flow edges only).
3. End reachable from every node (reverse walk from End).
4. Every decision answer wired (yes/no, every option, pick-many "none").
   Every Ask me reply wired.
5. Every cycle has a way out (covered by 3) **and** a round limit. The limit
   lives on an edge (`maxRounds`, usually the one going back round); the rule
   is "remove every limited edge and the graph is acyclic".
6. Every pick-many fan-out has exactly one matching Join; branches cannot leave
   the region except via escalation.
7. A note edge cannot be a node's only way in.
8. Every output takes exactly one flow edge; to branch, add a Decision.
9. Only the Manager's Finish reaches End; Start leads to the Manager.
10. A Join names its fan-out (`join.forkId`).
11. A decision that follows Start, a Join or an Ask me must set `answeredBy`;
    a chain of decisions is answered by the agent in front of the chain.

**Dry run:** click through the graph without Claude, choosing answers by hand;
shows path taken and loop counters.

### Runtime guarantees
- **Loop limit hit** → control goes to the Manager with the reason. It may only
  pick an exit edge from that loop, Finish, or Ask me. It cannot loop again.
- **No-progress detector** → same node, same answers, unchanged worktree diff →
  escalate early.
- **Run caps** (per factory, sensible defaults): max handoffs, max wall time,
  token budget. Hit → run goes to *Waiting on you* with the Manager's summary.
- **Stall** → an employee ending a turn without `factory_handoff` is nudged
  once, then escalated to the Manager. Idle timeout for hung sessions.

Termination: every node reaches End; every cycle has a finite limit whose
exhaustion only allows exits; run caps bound everything.

## 4. Running

### 4.1 Starting
- From the project's Factory panel (or a kanban card): choose factory, fill
  in a **change request**, optionally override merge mode.
- **Change request (CR)** (src/factory-cr.js): a title plus the factory's CR
  template sections. Default: Goal*, Current situation, Expected result*,
  Done when* (list), Out of scope (list), Constraints and references (*
  required). The template is edited per factory (Factory settings → Change
  request form: label, hint, required, list; reorder; back to default).
  Attachments use the chat's upload (project `crundi_attachments`), only from
  there; at start they are copied into the run's `shared/change-request/` and
  the CR is written to `shared/CHANGE-REQUEST.md` (lists as checklists).
  The Manager's first step (and a recovery) carries the whole CR; every other
  step carries a brief (title, goal, done checks, the file's path). The
  Manager is told to ask the person when the CR is unclear and to finish only
  when every done check is met; reviewers hold work to the done checks. The
  draft is kept per factory in the browser. The panel shows the CR folded.
- **One run per project.** A second start is refused with the reason.
- The run **snapshots** the factory; later edits never affect it. Library
  cards show "N runs on an older version".

### 4.2 The Manager is a chat
- The run opens a normal Crundi UI chat with a factory badge; the live graph
  sits beside it as a workbench pane. Works on the compact `/chat/<id>` page.
- The user talks to the Manager directly.
- **The Manager chat cannot be closed while the run is active** — "Stop the
  factory first". The Factory panel and live-graph pane close freely without
  affecting the run.

### 4.3 Worktrees
- One worktree per run on branch `factory/<run-slug>`, cut from the project's
  current branch (the **parent branch**). Starts from the last commit; the start
  dialog warns if the checkout is dirty.
- Location outside the repo and outside PROJECTS_DIR:
  `~/crundi-factory-worktrees/<project>/<run>/main` (override with
  `FACTORY_WORKTREES_DIR`), branch worktrees beside `main`.
- Crundi commits every step as "Crundi Factory"; its own files (`.mcp.json`,
  `.claude/skills`, `.factory`) are excluded by pathspec, because a linked
  worktree's own `info/exclude` is ignored by git. Employees get the Crundi
  tools via `--mcp-config` (a new folder's project MCP servers would need
  approval), and `--add-dir` for the run's other worktrees and notes folder.
- Parallel branches get **sub-worktrees** cut from the run branch, merged back
  automatically at the Join; on conflict the Manager sends a dev to resolve.
- Per-factory **setup command** (e.g. `npm ci`) runs in each new worktree.
- Edits outside the worktree are denied.
- Dev servers from a worktree must take a free port (see machine port rules).

### 4.4 Permissions
- Employee permission requests go to the Manager. **The Manager's decision is
  final** in every mode.
- Requests go into a per-run **FIFO queue persisted on disk**, tagged with
  employee, node and request id. If the Manager is mid-turn they are delivered
  when it is free (batched). Each answer returns to the exact session that asked.
  A waiting employee blocks only itself.
- Manager may grant **run-wide rules** ("allow `npm test`").
- File edits inside the worktree are auto-allowed for employees with edit tools.
- **Manager mode** is set on the factory and changeable in the chat:
  - *Bypass*: Manager launches with `--dangerously-skip-permissions`. Its
    prompt says the user trusts it: approve reasonable requests promptly,
    grant rules, do not escalate. Switching bypass **on** mid-run relaunches the
    Manager with `--resume` (CLI limitation); switching off is live.
  - *Normal*: the Manager's own actions ask the user.
- **Hard deny list, all modes:** `sudo`/`su`/`doas`, restarting or stopping
  Crundi or pm2, killing node/claude, binding 80/443/8888/22/53, `git push`
  and remote changes, and writes outside the run's folders. Checked by the
  runner on every employee request before the Manager sees it, and passed to
  every factory chat as `--disallowedTools` prefix rules (all that binds a
  Manager in bypass).
- The factory's own MCP tools (`factory_handoff`, `factory_permission`,
  `factory_delegate`, `factory_result`, `rename_chat`, `Skill`) are `--allowedTools` on every factory chat: an MCP call needs
  permission otherwise, and a normal-mode Manager stalled on its first call.
- **Team mode** (factory setting, changeable mid-run): *Ask* (default) or
  *Bypass*. With Bypass the employees launch with
  `--dangerously-skip-permissions` too; the deny list still rides along. When
  the Manager is bypassed and the team is not, the editor recommends giving the
  team bypass as well (the person already trusts the Manager to approve them),
  with a one-click button, and the Manager's prompt says the same. A mode
  change takes effect at each employee's next step: the CLI is relaunched on
  its conversation.
- Otherwise employees run in `acceptEdits`. AskUserQuestion from an employee
  is declined with "decide yourself, or escalate".
- **Rules from the start**, each scoped to a person or to everyone (`{rule,
  who}`, who `*` = everyone):
  - factory *presets* for everyone: `git-read`, `tests` (tests, builds,
    linters, plus the project's own `package.json` test/build/lint/check
    scripts and Makefile targets, never deploy/start/release), `packages`,
    `web`. Default: git-read and tests.
  - hand-written rules for everyone (factory settings), and per person
    (Team panel: their presets and their always-allowed rules).
  - Too-broad, edit and deny-listed rules are dropped. Each person's CLI gets
    `--allowedTools` with everyone's rules plus their own, and its prompt lists
    them.
- **Approval rules are per employee.** A rule the Manager grants with an
  answer applies to the person who asked, unless it adds `ruleFor:
  "everyone"` (or a name). It may also grant rules up front (`rules: [{rule,
  employee?}]`) and set a written `policy`. The factory setting *The Manager
  may set standing rules* (default on) turns all of that off: it then answers
  every request on its own.
- **Gatekeeper** (factory setting, off by default; model chosen there, Haiku
  by default): a small background chat with no edit or shell tools that
  answers requests first, following the Manager's policy. Unsure requests are
  passed on (`escalate: true` with a reason) and reach the Manager marked as
  such. The Gatekeeper cannot grant rules. The Manager's decision is final.
- The Manager answers with `factory_permission` `{ decisions: [{ id, allow,
  reason, rule? }] }`. A rule is refused if too broad (`Bash`, `Bash(*)`), on
  the deny list, or an edit rule. A Bash prefix rule never covers a compound
  command (`;`, `&&`, pipes, substitution). A new rule also answers matching
  requests already waiting. Requests are batched for 1.5 s.
- Every decision (deny list, rule, Manager, dropped) is kept in the run's
  permission log.

### 4.4b Working inside a run (from the role-play review, specs/factory-review.md)
- **Run context in every step message:** the base commit and parent branch,
  `git diff --stat <base>..HEAD`, what changed since this person's last step,
  the shared folder's files, the open review findings, where to run commands
  (a branch step's `cd` line) and the workspace's own ten ports (4300-4899,
  hashed from run and workspace).
- **Branch confinement:** a branch step's handoff is refused while the run's
  main worktree has stray changes, naming the files and how to move them.
- **Review ledger:** reviewers file `findings` ({text, where, blocking,
  reason}); they get ids F1, F2… kept in the run, so a fresh-eyes reviewer
  checks last round's requests first. Workers mark `done`, reviewers
  `verified` or `reopen`, the Manager may `dismiss`. After a person's first
  review, a new blocking finding needs a reason (stops moving goalposts); nits
  never block and come out as follow-ups on the result card.
- **Review gate** (a yes/no decision flag): "no" needs an open blocking
  finding, "yes" needs none open and closes what was claimed fixed. Loop-limit
  and no-progress escalations list the findings and how often each was
  reopened.
- **Delegation:** `factory_delegate {employee, task}` hands a one-off job to
  anyone (resolve a conflict, a quick check); it always returns to the Manager,
  counts as a handoff and cannot break termination.
- **`factory_result {action}`:** the Manager carries out the person's decision
  given in the chat (merge, keep, discard, changes, undo).

### 4.4c Usage limits (the person's choice)
`settings.limits` per factory, and changeable on a running run (Details
drawer, `limits` action, or the Manager's `factory_control` on the person's
word), journaled as a `limits` event that overrides the factory's:
- **pauseAt5h / pauseAtWeek** (50-100, default 100): pause when the window
  reaches it; resume by itself once the window has reset. Pausing holds new
  steps; whoever is mid-turn finishes.
- **maxParallel** (default 0, no limit): at most this many people working at
  once; the rest wait for a seat (`waitingForSeat`, shown as "Waiting its
  turn" in the working strip) and start as seats free.
- **oneAtATimeAbove** (default 0, off): above this share of the 5-hour
  window, one at a time. Usage is read every minute and when a run starts.
- `factory_control {action: pause|resume|limits, reason}`: the Manager
  pauses or resumes with a reason shown to the person, or changes the limits.
- Models stay a per-person choice in the Team panel.

### 4.4d The Manager's ticker
- `settings.tickMinutes` (5-1440; 0 = the Manager chooses with
  `factory_tick` at the start; 15 until it does). Fixed by the person, the
  Manager cannot change it.
- Every N minutes, only if the Manager is idle and the run is running or
  waiting, it gets one "[Factory] Tick" message: a wake-up, not a task and not
  a guaranteed timer. Skipped entirely (nothing sent) while it is busy, or the
  run is paused, crashed or finishing. Rides on the run clock: armed on start,
  resume and recovery, cleared on pause and stop.
- `factory_message {employee, text, interrupt?, hold?}`: the Manager tells
  someone mid-step; interrupt stops their turn first, hold makes them wait
  (not nudged) until its next message.

### 4.5 Sessions: no sleep, reachability GC
- The pane parker (`src/panes.js`) **never parks** a session of an unfinished
  run (running, waiting, paused). Employee sessions are owned by the runner, not
  the workbench.
- **Reachability collector**, run after every batch of effects (handoff,
  escalation, Join, recovery):
  - Roots: every token (active nodes, waiting Joins, Ask me).
  - Reachable: flow-edge closure from the roots, without loops closed for good
    (used up and reopened twice), and **not onward through the Manager unless
    the Manager is acting**. Start only leads to the Manager, so everything in
    a valid factory is reachable from it; counting the Manager as a root would
    collect nobody, ever. Employees in the loop being worked stay alive;
    someone only the Manager could send work to again is stopped until it does.
  - Conservative within the loop: follows what the graph allows, not past choices.
  - An employee with no reachable node is **Retired**: process stopped,
    session id + transcript kept, still viewable. If flow reaches it again it is
    resumed with `--resume` (lossless).
- Same rule for sub-worktrees (deleted after their Join merges) and preview
  services (stopped with their node).
- Panel shows live session count per run.

### 4.6 Run states

| State | Meaning |
|---|---|
| Running | employees working |
| Waiting on you | permission (Normal mode Manager), Ask me, merge card, cap hit |
| Paused | by user (finish current turns, hold handoffs), or usage limit (shows reset time; auto-resumes at reset) |
| Crashed | Crundi restart or session died mid-step |
| Finished | merged / kept / discarded / stopped |

**Stop** interrupts immediately, then asks keep or delete the worktree.

### 4.7 Durability and recovery
- **The journal is the source of truth.** Every input that changes a run is an
  event appended to `events.jsonl` and flushed (fsync) before it takes effect:
  start, setup, begin, handoff, ask-reply, escalate, effect-done (with the
  effect's result), notice, session, retire, rule, manager-mode, perm-asked /
  perm-decided, cap, pause, resume, stop, crash, recover. The run is the fold
  of its events (`factory-journal.js`, pure; the engine is deterministic and
  time comes only from each event's `at`). `run.json` is a cache: if missing,
  damaged or behind the journal it is rebuilt by replay; a torn last line is
  ignored.
- **Effects are written ahead and safe to repeat.** They are pending from the
  event that causes them until an `effect-done` carrying their result. A
  branch worktree's folder and branch are fixed by its workspace id and reused
  if they exist (or the branch is checked out again if only the folder went);
  a merge already made is a no-op; activations are rebuilt from the tokens.
- What dies with the process is kept in memory only and rebuilt on recovery:
  chat ids, who is busy, queued steps, permission requests in flight.
- **Durable time limit.** maxMinutes counts active time from event timestamps:
  paused time and the time the process was dead (from its last event to the
  restart) do not count; it carries across restarts.
- **Auto-recover (per factory, off by default):** a crashed run recovers
  itself after 30 s, retrying at 2 and 10 minutes; if those fail it stays
  crashed and the person is told to press Recover.
- Crashed runs show in the Factory panel with **Recover**, unless the factory
  is set to auto-recover.
- Recover has a **reply box**; the user's note joins the continue prompt.
- Each agent that was mid-step gets a **continue prompt**: why it stopped,
  which node and last completed action, current worktree `git status` + diff
  summary, the user's note, "continue from where you stopped".
- The Manager is relaunched on its old chat id and conversation and gets a
  whole-run overview first (reason, where everyone was, last handoffs, the
  worktree, the note). Unfinished effects other than activations are
  completed; activations are rebuilt from the tokens. Permission requests
  that were waiting died with their sessions: listed to the Manager, and the
  employees ask again if they still need them.
- Usage-limit resume uses the same continue prompt.

### 4.8 Ending
The Manager decides the work is done (Finish). Per-factory **merge mode**
(overridable at start):

- **Ask me** — Manager presents: summary, diff stats + full diff, a preview
  (worktree app as a Crundi service on a free port + link, screenshots in chat),
  then a card: **Merge / Request changes / Keep branch / Discard**.
  Request changes feeds the user's notes back into the flow.
- **Auto-merge** — Manager merges, then presents what it merged with **Undo**
  (reverts the merge). If the parent moved and conflicts, the Manager resolves
  in the worktree; if it cannot, falls back to Ask me.

Merge is always a **squash** into the parent branch with a Manager-written
message (subject from the first sentence of its summary, the summary as body).
**Everything stays local; nothing is ever pushed.** Worktree and branch are
cleaned up after a successful merge, with any preview service registered from
inside the run's folder.

How the squash is done, without touching anyone's working files:
`git merge-tree --write-tree` computes the result in the object store
(conflicts reported, nothing written), `git commit-tree` makes the one commit
(the repository's own identity), and the parent moves by `merge --ff-only`
where it is checked out (refuses rather than overwrite uncommitted changes) or
`update-ref` with the old tip where it is not.

- **Conflict** (the parent moved on): the Manager is sent back to merge the
  parent into its branch in the worktree and resolve it, then finish again.
  Auto mode does this once; if it still conflicts it falls back to Ask me.
- **Blocked** (the person's uncommitted changes are in the way): the card says
  so and Merge can be pressed again.
- **Undo**: the parent goes back (`reset --keep` where checked out), and the
  work is restored on the run's branch. Refused once something else has been
  committed on top (it says how to `git revert`).
- **Request changes**: the person's note goes to the Manager and the run goes
  on; when the Manager finishes again, the same card asks again.

The result card is an entry Crundi puts in the Manager's chat
(`factory-result`), drawn by `claude-chat.js`, so it works on the compact
`/chat/<id>` page too; it is updated in place. Closing any chat of an
unfinished run is refused (409) with "Stop the factory first"; the Manager's
chat header carries a Factory badge.

### 4.9 Triggers
- A trigger (src/factory-triggers.js, `<dataDir>/factory-triggers.json`)
  starts a factory in a project by itself: **schedule** (HH:MM server time,
  chosen weekdays), **kanban** (a card enters a column), **webhook** (a POST to
  `/api/factory-hook/trigger/<id>`, outside the auth gate, at most 1 MB).
- Webhook security, per trigger: **key** (`?key=`, `X-Crundi-Key` or
  `Authorization: Bearer`) or **signature** (HMAC-SHA256 of the raw body with
  the trigger's secret, hex / `sha256=<hex>` / base64, in a header the person
  names; with a timestamp header the signed text is `<timestamp>:<body>` and
  the timestamp must be within 5 minutes, which is Grafana's form). Key and
  secret are replaced separately. Comparisons are constant-time.
- Each carries a CR with placeholders filled when it fires: `{{date}}`,
  `{{time}}`, `{{card.title|description|todos|id}}`; for webhooks
  `{{payload.path}}` (alias `{{body.path}}`; arrays by `.0` or `[0]`; own
  properties only), `{{payload}}`, `{{headers.name}}`, `{{query.name}}`.
  Credentials (authorization, cookie, the key, any signature header) are never
  readable or passed on. A placeholder the call does not have comes out empty
  and is listed for the Manager; a title that comes out empty becomes
  "<trigger>: webhook at <date time>". Nothing a caller sends can stop a
  firing (lenient CR check).
- The Manager always gets the whole call: `webhook-request.json` (method,
  headers and query with credentials removed, body as it came) in the CR's
  folder, and a "How this started" section in the CR naming the trigger, the
  idempotency key, what was missing, and the body inline (clipped at 12k).
- Idempotency: a webhook may name its key with a placeholder
  (`{{payload.groupKey}}`). Keys are remembered 30 days (1000 per trigger); a
  call with a seen key is answered 200 `{duplicate: true}` and recorded, not
  processed. Check-and-claim is synchronous, so a burst lets one through; a
  start that fails releases the key so the sender's retry is tried. A call
  with no key is processed and the Manager is told repeats cannot be told
  apart.
- One run per project still holds. A busy project makes the firing **tell
  the Manager** (webhooks' default; only when the running run is of the same
  factory), **wait its turn** (queued, started on the next tick once the
  project is free) or **skip**. Told: the Manager gets a message marked
  "Mid-run" (queued behind its turn) with the filled CR, the key and the full
  request in `shared/triggers/NNN-<key>.json`, and decides whether it is the
  same issue; journal event `trigger-message`, shown on the timeline. A
  Manager that cannot be reached, or a run of another factory, queues it.
  Every outcome (started, told the Manager, queued, duplicate, skipped,
  failed) goes in the trigger's history; Run now fires it by hand.
- **needsCards** (a column, or none): the trigger starts a run only while that
  column holds a card. Empty when it fires: recorded "skipped" with why, no
  run. While a run is going it still queues; when a queued firing comes up and
  the column is empty, it and the rest of the queue are dropped, recorded
  "skipped". For "next card" chains (a kanban trigger on Done working the top
  of To do), so the chain stops when To do runs out instead of waking a
  Manager with nothing to do. A save that leaves the field out keeps it (older
  clients); servers list it as the capability `trigger-needs-cards`.
- A run started from a kanban card moves the card to In progress, and to Done
  when the result is merged.
- UI: the project Factory panel, under "Triggers".

### 4.10 Building a factory from a chat or terminal
- Every Crundi chat and terminal (not a factory run's own chats, which get
  the run tools instead) has `factory_guide` (the JSON format, the
  validator's rules, shapes that work; src/factory-guide.js),
  `factory_list`, `factory_get`, `factory_validate`, `factory_save` (create,
  or update with baseVersion), `factory_start` (with a CR) and
  `factory_triggers` (list, save, delete). The shipped skill
  `crundi-factories` tells Claude when and how to use them.
- A definition saved without positions (or with nodes piled on one spot) is
  laid out top to bottom (`layoutFactory`; loops are the edges that go back
  up the path from Start). The editor's Tidy uses the same edges.
- Checked end to end: Haiku, asked for "planner, parallel frontend and
  backend, reviewer who approves or sends back, Manager finishes", built and
  saved a valid factory in 6 turns.

### 4.11 Provided templates and personas
- `src/factory-templates.js`: Pair and review, Product squad, New project
  kickoff, Design studio, Platform team. Read-only; "Use this" in the Library
  (or factory_get on a `tpl_` id from a chat) copies one into the library.
  Every one validates with no warnings and walks to Finish
  (scripts/test-factory-templates.mjs).
- `src/factory-personas.js` (shared with the editor): planner, architect,
  frontend, backend, database engineer, developer, designer, DevOps, reviewer,
  UI reviewer, security reviewer, tester, tech writer, researcher. Each has
  the skills that fit (crundi, frontend-design, logo-design, deep-research);
  a template or a new person keeps only the installed ones.

### 4.12 Measured: one chat against a factory (2026-10-21)
Same change request (notes app: node:http JSON API with search, validation,
atomic JSON storage, a phone page, tests, README), fresh identical repos,
everyone on Haiku, one run each.

| | One chat | Product squad factory |
|---|---|---|
| Wall time | 204 s | 462 s |
| Turns | 24 | 166 (11 chats, 20 handoffs) |
| Cost (CLI estimate) | $0.036 | $0.179 |
| Tests (all passing, rerun by hand) | 26 | 40 |
| Edge-case probes (bad %-encoding, 1 MB body, long title, bad JSON) | all handled | all handled |

**Completeness, driven through each page at 390 px** (add, edit, search
hit, no-match message, delete with confirm, empty-title message, no
horizontal scroll, notes kept across a server restart): both passed every
check. Server stopped while saving: the chat's page showed the raw
"Failed to fetch" at the top of the form, out of view when scrolled; the
factory's showed "Could not reach the server. Check your connection and try
again." under the form.

**Features beyond the brief.** Both: confirm before delete, search delay,
aria-live messages, sticky header, dates. Chat only: a 0/120 title counter;
the form always on screen. Factory only: a full-screen editor on phones with
Cancel and Save in a top bar, focus moved to the title, a floating New note
button, two-line previews with an ellipsis, reduced-motion support. Against
it: the factory's server listened on every interface (the chat's on
127.0.0.1 only).

**Design, two blind reviews** (Claude agents shown the same eight
screenshots, labels A/B swapped between them, not told who made which):

| out of 10 | Chat | Factory |
|---|---|---|
| Visual polish | 5, 5 | 7, 7 |
| Typography and hierarchy | 5, 5 | 8, 8 |
| Layout and spacing | 4, 4 | 7, 7 |
| Phone usability | 3, 3 | 8, 9 |
| Empty and error states | 3, 3 | 6, 6 |
| Overall | 4, 4 | 7, 7.5 |

Both reviewers would ship the factory's app ("puts the notes first on a
phone", "error written for people"); the chat's was "a form with a list
attached". Screenshots: /tmp/fx-dev/eval/ at the time.

What's new shows: the chat was quicker and cheaper; the factory's app was
better made; factories are intended for large work carried from start to
finish (new projects, features across interface, server and data). An
earlier factory attempt was discarded: the chat test's
`pkill -f "node src/index.js"` killed the dev Crundi it ran on, which led to
pkill/killall joining the factory deny rules.

## 5. UI

### 5.1 Global Factories tab
- **Library** — factory cards with graph thumbnail; new, duplicate, import/
  export JSON, delete. Shipped examples: *Solo* (Manager + Dev), *Standard*
  (planner, frontend/backend devs, security, reviewers, approval loop),
  *Bugfix* (Reproducer → Fixer → Verifier).
- **Editor** — React Flow canvas; roster and persona editor in a side panel;
  persona template library. Autosave, undo/redo, snap to grid, auto-layout
  (dagre/elk), minimap, keyboard shortcuts, inline validation, dry run.
- **Runs** — every run in every project; filter by state/project; same row
  actions as the panel.
- **Cleanup** — see 5.3.

### 5.2 Project Factory panel
- One per project (opening again focuses it). Shows the project's one run
  (active, waiting, paused, crashed) plus collapsed history.
- Row: factory, task, current step, elapsed, tokens, live sessions.
  Actions: Open (Manager chat + live graph), Pause/Resume, Stop, Recover
  (with reply box).
- Start button, disabled with reason while a run exists.
- Badge on the project and the Factories tab when a run is active or needs you.

### 5.2a The panel during a run: the god's-eye view
While a run is going, the project's Factory panel is its live graph, filling
the panel (the same viewer as the Factories tab, `mode: 'panel'`):
- **Working now** strip under the top bar: everyone at work, with what they
  are doing this second (from their chat: the tool in flight, thinking,
  writing), a pulse while their turn runs, amber when waiting on a
  permission. Each opens that person's transcript.
- **On the graph** each active step shows who is there and what they are
  doing; the camera **follows the work** (zooms to the active steps and moves
  with them; panning by hand stops it, the Follow button resumes).
- **Tapping** someone at work opens their live transcript inside the panel
  (the chat page with `?in=graph`, its own back arrow returns to the graph);
  tapping a finished step opens its history in the Details drawer.
- **Needs you** banner: crashed (Recover), done (Open the result), a limit
  (Let it carry on), a question (Answer), the Manager talking to you (Open its
  chat), paused (Resume).
- Top bar: progress (handoffs, time, turns and spend, open findings), Follow,
  Details (the run and timeline drawer), Pause and Stop.
The summary carries each person's `activity`; the view refreshes every 3 s
while visible. Without a run the panel is as before: Start, triggers,
earlier runs.

### 5.2b Phone
- The Manager's chat is an ordinary chat: it has its bubble, and its result
  card renders on the compact `/chat/<id>` page, which also shows a "Part of a
  factory · Open the live graph" bar (opens `/#factory-run=<id>` in the main
  app, straight into that run's live graph). Employees are background chats
  and stay off the phone.
- The live card follows the whole run, carried by the Manager's chat
  (`factoryLiveCard`): working while anyone on the team works, titled with who
  ("Std: Frontend dev working"); "needs you" for a question, a limit, a crash,
  the result or the Manager talking to you; waiting while paused; gone when
  the run ends. The Manager going idle while its team works does not drop it.
- Factory notifications (needs you, done, crashed, cap, merged) are filed
  under the Manager's chat (its bubble) and open it.

### 5.3 Cleanup
- On finish: Merge/Discard remove worktree, branch, sub-worktrees, services,
  sessions' processes; Keep branch keeps only the branch.
- On Crundi boot (30 s after) and every six hours: `git worktree prune` in
  every repo a run used, then auto-delete if turned on
  (`<dataDir>/factory-settings.json`). The Cleanup view scans the worktrees
  root: worktrees of stopped runs, folders no run knows about, kept branches;
  each with size, age and uncommitted changes. Removal unregisters worktrees
  from their repository and deletes factory/* branches, and never leaves the
  worktrees root. After a merge the run's empty folder goes too.
- Cleanup view lists leftovers with size and age; clear one or all. Uncommitted
  work flagged before deletion. Nothing silent except after Merge/Discard.
- Optional: auto-delete kept/crashed leftovers after N days.

### 5.4 Live view
- `CrundiFactoryFlow.mountViewer(el, { apiFetch, runId, subscribe, openChat, onClose })`.
  It fetches the run's journal (`/api/factory-runs/events?id&from=`) and
  folds it with the bundled reducer, so the graph at any point of the run is
  exact; live updates fetch only the new events (pushed via `subscribe`, the
  host forwarding its `factoryRun` SSE event; polls every 4 s without it).
- Replay: a slider and the timeline jump to any event; "Back to live".
- Steps: working / deciding / waiting on you glow; visited steps show their
  last summary; a person stopped by the collector is faded; the connection
  just taken flows (CSS, still under reduced motion); loops show their round.
- Skills: each handoff records the skills that person loaded during the step
  (Skill tool calls in their chat since the last handoff), journaled, shown
  on the step and in its details.
- Step details: who, what they are doing, every visit with its summary,
  answers and skills, and Open chat (the host decides how).
- Run panel: task, branch, live sessions, Ask me replies, Recover with a
  note, "let it carry on" after a cap; Pause / Resume / Stop in the header.
- Phone: graph on top, panel underneath.
- Node states: idle, working, waiting on you, done, retired, failed; loop-count
  badge. Active edge animates (CSS only).
- Tap a node → that employee's chat (existing renderer); user can type into it.
- Timeline/replay of handoffs.
- Updates pushed over the existing event stream; nodes update in place.

### 5.5 Implementation notes
- Frontend is vanilla JS. React + `@xyflow/react` (+ dagre for auto-layout)
  ship as a self-contained island, `window.CrundiFactoryFlow`, built from
  `tools/factory-flow` (esbuild, like `tools/codemirror`) into
  `app/vendor/factory-flow.js` + `.css`, loaded only when an editor or live
  graph opens. `mountEditor(el, { apiFetch, factoryId, onClose })`.
- The editor bundles the real `factory-model.js`, `factory-engine.js` and
  `factory-skills.js`: validation while editing and the dry run use exactly
  the server's rules. The build stamps a hash of those sources;
  `scripts/test-factory-flow-build.mjs` fails `npm run check` if they changed
  without a rebuild. Building needs `npm install --include=dev` on this
  machine (NODE_ENV=production skips dev dependencies).
- Editor details: the factory is the source of truth, nodes/edges derived;
  autosave 1.2 s after a change with baseVersion (conflict → Reload); undo/redo
  (Ctrl+Z / Ctrl+Shift+Z), Ctrl+S; snap 16 px; a connection that closes a loop
  gets a round limit of 3 at once; dropping "Pick any" brings its Join; loops
  are drawn in lanes past the right edge of the flow; phone: canvas full
  screen, panels as a bottom sheet, tab bar.
- Employees are the user's own `claude` over stream-json (no Agent SDK).
- `webapp-html` is one template literal: no bare `\n`/`\r` in browser JS.
- Icons from the `ic()`/`ICON_PATHS` set; no emoji.
- New feature → What's new tour step.

## 6. Storage (proposed)

```
<dataDir>/factories/<factoryId>.json        factory definition (roster, graph, settings, version)
<dataDir>/factory-runs/<runId>/events.jsonl  the journal: source of truth
<dataDir>/factory-runs/<runId>/run.json      cache of the fold of the journal
<dataDir>/factory-runs/<runId>/notes/<employee>.md  private notes
<dataDir>/factory-runs/<runId>/shared/plan.md      shared board
```

## 7. Build order

1. Data model + storage + validator (pure, unit tested).
2. Runner: worktrees, sessions, handoff tool, routing, joins, loop limits, caps.
3. Permissions queue + Manager gatekeeping + deny list.
4. Persistence, crash recovery, continue prompts, no-sleep, reachability GC.
5. Manager chat integration: close guard, present/merge (squash), auto-merge + undo.
6. React Flow island + editor + roster/persona editor + dry run.
7. Live view.
8. Project Factory panel + global tab (library, runs) + cleanup.
9. Example factories, persona templates, What's new step.
