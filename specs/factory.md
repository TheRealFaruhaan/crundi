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
- From the project's Factory panel (or a kanban card): choose factory + task,
  optionally override merge mode.
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
  `rename_chat`) are `--allowedTools` on every factory chat: an MCP call needs
  permission otherwise, and a normal-mode Manager stalled on its first call.
- Employees always run in `acceptEdits` (never bypass). AskUserQuestion from
  an employee is declined with "decide yourself, or escalate".
- The Manager answers with `factory_permission` `{ decisions: [{ id, allow,
  reason, rule? }] }`. A rule is refused if too broad (`Bash`, `Bash(*)`), on
  the deny list, or an edit rule. A Bash prefix rule never covers a compound
  command (`;`, `&&`, pipes, substitution). A new rule also answers matching
  requests already waiting. Requests are batched for 1.5 s.
- Every decision (deny list, rule, Manager, dropped) is kept in the run's
  permission log.

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

### 4.7 Recovery
- Every change is written to disk at once, and effects are written ahead:
  saved as `meta.pending` before anything happens and ticked off one by one,
  so a crash part-way leaves exactly what is still to do.
- Crashed runs show in the Factory panel with **Recover**; never automatic.
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
message. **Everything stays local; nothing is ever pushed.** Worktree and branch
are cleaned up after a successful merge.

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

### 5.3 Cleanup
- On finish: Merge/Discard remove worktree, branch, sub-worktrees, services,
  sessions' processes; Keep branch keeps only the branch.
- On Crundi boot: reconcile orphaned worktrees, runs whose worktree vanished,
  services from dead runs, `git worktree prune`.
- Cleanup view lists leftovers with size and age; clear one or all. Uncommitted
  work flagged before deletion. Nothing silent except after Merge/Discard.
- Optional: auto-delete kept/crashed leftovers after N days.

### 5.4 Live view
- Same graph as the editor, read-only, from the run's snapshot.
- Node states: idle, working, waiting on you, done, retired, failed; loop-count
  badge. Active edge animates (CSS only).
- Tap a node → that employee's chat (existing renderer); user can type into it.
- Timeline/replay of handoffs.
- Updates pushed over the existing event stream; nodes update in place.

### 5.5 Implementation notes
- Frontend is vanilla JS. React + `@xyflow/react` ship as a self-contained
  island bundled with esbuild (like `app/vendor/codemirror-entry.mjs`), loaded
  only when the Factories tab or a live graph opens.
- Employees are the user's own `claude` over stream-json (no Agent SDK).
- `webapp-html` is one template literal: no bare `\n`/`\r` in browser JS.
- Icons from the `ic()`/`ICON_PATHS` set; no emoji.
- New feature → What's new tour step.

## 6. Storage (proposed)

```
<dataDir>/factories/<factoryId>.json        factory definition (roster, graph, settings, version)
<dataDir>/factory-runs/<runId>/run.json     snapshot, state machine, loop counters, caps used
<dataDir>/factory-runs/<runId>/handoffs.jsonl   write-ahead log of handoffs
<dataDir>/factory-runs/<runId>/permissions.jsonl  permission queue
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
