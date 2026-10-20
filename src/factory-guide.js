/**
 * factory-guide.js — how to write a factory as JSON, for a Claude chat or
 * terminal asked to build one. Returned by the factory_guide MCP tool and
 * shipped as the factory-builder skill.
 *
 * Kept in step with src/factory-model.js (normalizeFactory, validateFactory).
 */

export const FACTORY_GUIDE = `# Building a Crundi factory

A factory is a reusable team for large pieces of work: a Manager (a chat the
person watches) and employees with personas, wired as a flow of steps and
decisions. Runs start from a change request (CR), work in their own git
worktree, and end with the Manager presenting the result for the person to
merge. Factories are global; any project can run one.

Workflow:
1. Ask the person what kind of work it is for and how their team works, if
   they have not said. Keep it simple: 3 to 6 people is plenty.
2. Write the definition (below), call factory_validate, fix every error, and
   consider the warnings.
3. Save it with factory_save. Positions are laid out for you when you leave
   them out. Tell the person they can open it in the Factories tab to adjust.
4. Only start it (factory_start) or add triggers (factory_triggers) if they
   ask.

## The definition

{
  "name": "Feature team",
  "description": "What it is for. /skill-name mentions work here.",
  "settings": { ... },          // all optional, see Settings
  "roster": [ ... ],            // the people; must include id "manager"
  "nodes": [ ... ],             // the flow
  "edges": [ ... ]              // connections
}

Ids: letters, digits, _ and -, unique. The Manager's roster id is always
"manager".

### Roster
{ "id": "dev", "name": "Developer", "persona": "…how they work, what they care about…",
  "model": "" | "sonnet" | "opus" | "haiku", "effort": "" | "low" | "medium" | "high",
  "memory": "keep" | "fresh",   // fresh: a new session every visit (good for reviewers)
  "skills": ["frontend-design"],// skills they should use (names, without /)
  "presets": ["packages"],      // extra permissions for this person (see Settings)
  "tools": { "allow": ["Bash(docker compose up:*)"], "deny": ["Edit", "Write"] } }
Reviewers and researchers usually get "deny": ["Edit","Write","MultiEdit","NotebookEdit"].

### Nodes
Every node: { "id", "type", "label" }. Types:
- start: exactly one. The CR enters here. Connect it to the Manager.
- manager: exactly one. Outputs "out" (optional: to the first step) and
  "finish" (required: to End). It decides when the work is done.
- employee: { "employeeId": "dev" }. One output "out". The same person may
  appear in several nodes (same memory).
- decision: { "decision": { "kind": "yesno" | "one" | "many", "question": "…",
  "options": [{ "id": "ui", "label": "UI" }], "answeredBy": null | "manager" | rosterId,
  "review": false } }.
  Outputs: yesno -> "yes", "no"; one -> each option id; many -> each option
  id plus "none". "many" fans out in parallel and needs a matching Join.
  Decisions are answered by whoever arrives (an employee or the Manager
  answers in the same handoff). After a Join, a Wait or Start, set answeredBy.
  "review": true on a yes/no makes it a review gate: "no" needs an open
  blocking finding, "yes" needs none left open.
- join: { "join": { "forkId": "<the many-decision id>" } }. Output "out".
  Waits for the branches that actually started.
- ask: { "ask": { "prompt": "Ship it?", "replies": [{ "id": "go", "label": "Go" }] } }.
  Asks the person (notification) and waits. Each reply is an output.
- wait: { "wait": { "kind": "time" | "until" | "command" | "signal",
  "minutes": 10, "at": "07:30", "command": "gh run watch --exit-status",
  "every": 60, "timeout": 60, "what": "the deploy" } }.
  time and until have one output "out"; command and signal have "done" and
  "timeout". A command wait runs in the worktree until it exits 0.
- end: exactly one. Only the Manager's "finish" reaches it.

### Edges
{ "id": "e1", "source": "a", "sourceHandle": "yes", "target": "b",
  "maxRounds": 3, "requireNote": true, "noteLabel": "What to change" }
- sourceHandle is the output name ("out" when the node has one output).
- Every output must be connected (Manager "out" is optional).
- Every loop needs a round limit: put "maxRounds" on the edge that goes back.
  When it runs out the Manager is told why and decides.
- requireNote: whoever takes this route must write the note (e.g. review
  feedback going back to the developers).
- Note edges carry instructions without moving the work:
  { "id": "n1", "kind": "note", "source": "manager", "target": "reviewNode",
    "noteLabel": "What to check" } (from an agent node to an agent node).

### Settings (all optional)
{ "managerMode": "normal" | "bypass",     // bypass: the Manager runs unchecked
  "teamMode": "ask" | "bypass",           // bypass: employees too (recommend it when the Manager is bypassed)
  "presets": ["git-read","tests"],        // everyone may: git-read, tests, packages, web
  "rules": ["Bash(make e2e:*)"],          // more allow rules for everyone
  "managerRules": true,                   // the Manager may grant standing rules
  "gatekeeper": { "enabled": false, "model": "haiku" },
  "mergeMode": "ask" | "auto",
  "autoRecover": false,
  "setupCommand": "npm ci",
  "caps": { "maxHandoffs": 200, "maxMinutes": 480 },
  "crTemplate": [ { "id": "goal", "label": "Goal", "hint": "…", "required": true, "list": false } ] }
Leave crTemplate out for the default (Goal, Current situation, Expected
result, Done when, Out of scope, Constraints and references).

## Rules the validator enforces
- One start, one manager, one end; start -> manager; manager finish -> end.
- Every node reachable from Start, and every node can get back to the
  Manager (so the run can always finish).
- Every output wired; every loop has a round limit somewhere on it.
- A many-decision has exactly one Join, and its branches meet only there.
- Decisions after a Join, Wait or Start name who answers.

## Shapes that work
- Solo: start -> manager -> dev -> manager (maxRounds 5), manager finish -> end.
- Build and review: manager -> dev -> reviewer -> "Approved?" (yes/no,
  review: true, answered by the reviewer); no -> dev (maxRounds 3,
  requireNote); yes -> manager.
- Parallel: manager -> "Which parts?" (many: ui, api) -> fe / be -> Join ->
  manager (maxRounds 5).
- Plan first: manager -> "Needs a plan?" (yes -> planner -> manager, maxRounds 2).
- Ship gate: ... -> "Ship it?" (ask) -> go: manager; hold: manager.
- CI gate: dev -> wait (command: "gh run watch --exit-status", timeout 30)
  -> done: reviewer; timeout: dev (maxRounds 2).
Factories suit large features and new projects carried from start to
finish; everyday changes are better done in a normal chat.
`;
