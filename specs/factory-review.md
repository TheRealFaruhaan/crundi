# Factory — inside the run: a role-play review

Written 2026-10-21, after cards 1–8. Each scenario is played from the seat
of one person in a run, using what the code actually does today (and what
was seen in the real Haiku runs), to find where the work gets hard.

---

## The Manager's seat

### M1. A vague task on a Monday morning
**Situation.** The person starts *Standard* with "Add dark mode" and walks away.
My first message is the step: "Needs plan?" then "Which workers?", with the
answers tree and an example. I don't know if they want a toggle, follow the
system setting, or both.

**What happens.** I can talk to them in my chat and end my turn; the run
waits, and their phone's live card says "needs you". That works, but only if
they look. If I guess instead, the whole team builds on my guess.

**Hard part.** The start dialog asks for one free-text task. Nothing invites
the person to say what "done" looks like, so I start under-informed.

**Improve.** Give the start dialog an optional "Done when…" field (acceptance
criteria) that rides along with the task to everyone, and tell the Manager in
its prompt: when the task is ambiguous, ask first — the run waits.

### M2. After the Join: "Security analysis needed?"
**Situation.** Frontend and backend devs both handed off; their branches were
merged; I am asked "Security?" and then "Which reviewers?".

**What happens.** I see two one-line summaries. To judge security I need the
change itself. I know the worktree path, but not where the run started, so
`git diff` shows nothing (Crundi commits every step). In the real run my
Haiku Manager noticed exactly this: *"The Developer's empty git diff --stat is
consistent with Crundi having already committed the step."* It had to reason
its way around it.

**Hard part.** Nobody in the run is told the base commit, nor given the run's
diff. Reviewers suffer most (see W4).

**Improve.** Every step message carries: the base commit, the run's diff stat
so far (`git diff <base>`), and what changed since this person's last step.
Cheap, and it removes a whole class of guessing.

### M3. Three people asking for permission while I think
**Situation.** In normal mode, three employees each run a few shell commands
(npm install, npm test, a curl to the docs). Batching (1.5 s) groups the ones
that arrive together, but each batch lands as a new message in my chat,
often mid-turn.

**What happens.** I answer correctly and grant rules ("Bash(npm test:*)") so
repeats stop. But my context fills with permission chatter, and every batch
pulls me out of whatever I was judging.

**Hard part.** The obvious safe things (tests, builds, linters for this
project) are asked for one by one, every run.

**Improve.** (a) Team-wide presets in the factory editor: "tests, builds and
linters", "web search and docs", "package installs" — granted from the start.
(b) Suggest rules from the project itself (its package.json scripts, Makefile
targets). (c) Show permission batches compactly and keep them out of later
step messages.

### M4. The review loop keeps going round
**Situation.** The UI reviewer has said "not approved" three times. The loop
limit (3) is reached and I get: "The loop 'Approved?' → 'Which workers?'
reached its limit of 3 rounds."

**What happens.** I can reopen the loop (twice), finish, or ask the person.
To decide I have to scroll back through my chat to remember what the
reviewer kept asking for and whether the devs actually did it.

**Hard part.** The escalation says *that* the loop ran out, not *why*.

**Improve.** The escalation message includes the last two rounds' review
notes side by side and the diff between rounds (did anything change?). The
no-progress detector already knows when nothing changed; say so in words.

### M5. A merge conflict at the Join
**Situation.** Both devs edited the same CSS file. The merge at the Join
conflicts; I am told: "Have someone merge them into … and resolve the
conflicts, then carry on."

**What happens.** "Have someone" — but the flow I'm in has no step for that,
my instructions say not to touch git or do the work myself, and I can only
route by answering the questions in front of me. In practice I'd break my
own rules and resolve it.

**Hard part.** There is no way for the Manager to hand an ad-hoc job to a
team member outside the drawn flow.

**Improve.** **Delegation**: a `factory_delegate` tool — "Frontend dev: merge
branch X into main and resolve the conflict, then hand back". It always
returns to the Manager, so it cannot break termination, and it counts
against the handoff cap. Useful far beyond conflicts (a quick check, a
second opinion, a small fix after review).

### M6. "Merge it."
**Situation.** I presented the result; the person replies in my chat "looks
good, merge it". (The CLI even suggested exactly that reply in the real run.)

**What happens.** I can't: merging is a button on the card, and I'm told not
to merge with git. I'll tell them to press the button — clumsy.

**Improve.** A `factory_result` tool for the Manager: merge, keep, discard
or reopen — only when the person has said so in the chat. The card stays
the main way; this just makes the conversation work.

### M7. A long run
**Situation.** Hours in, a dozen handoffs, two review rounds, many
permission batches.

**What happens.** My context grows with every step message (each repeats the
task and the answers tree) and every batch. Eventually Claude compacts, and
I lose details I needed.

**Improve.** Keep step messages lean after the first (task once, then
"same task"), and give the Manager a `factory_status` tool that returns the
run in one block (where everyone is, what's done, the diff stat, loop
counts) so it can re-orient after compaction instead of scrolling.

---

## The workers' seats

### W1. Frontend dev in a parallel branch
**Situation.** "Your step: Frontend dev. Work in: …/run_x/ui-w3". My session
was started in …/run_x/main (the run worktree), with the run folder added.

**What happens.** File tools work wherever I point them, but my shell starts
in `main`. One `npm run build` without `cd` builds the wrong tree; one edit
with a relative path lands in `main`, where the backend dev's branch will be
merged later.

**Hard part.** Real, and silent until the Join merges surprising files.

**Improve.** (a) For branch work, deny writes outside *this step's*
workspace (the permission hook already checks paths; tighten it per token).
(b) Start each step's message with the `cd` line to run first. (c) Better:
give branch steps their own session started in the branch folder when the
employee has fresh memory.

### W2. Two dev servers, one port
**Situation.** Frontend and backend devs both start a dev server to check
their work. Both pick 5173 (or 3000).

**What happens.** The second fails; on this machine a careless choice could
even hit a reserved port (the deny list stops 80/443/8888/22/53, not 3000).

**Improve.** Give each workspace its own port range (PORT=4xxx) in the step
message and environment, and say so: "use ports 4310–4319".

### W3. Waiting for the Manager
**Situation.** I need `npm install sass`. My request goes to the Manager, who
is in the middle of a long turn reviewing something else.

**What happens.** I am blocked until it answers. Nobody can see that I'm
blocked except in my own (hidden) chat and the Manager's queue.

**Improve.** Show "waiting for the Manager's permission" on my node in the
live graph and on the panel; presets (M3) make most of these unnecessary.

### W4. The reviewer who can't see the change
**Situation.** "Your step: UI reviewer". Fresh memory, read-only. The message
has the task, the previous summary, and the Manager's note "Check contrast".

**What happens.** `git diff` and `git status` show nothing — the devs' work
was committed at their handoffs. I don't know the base commit. I end up
reading files and guessing what changed. A careless reviewer approves.

**Hard part.** This is the most important gap: review is the factory's quality
gate, and the reviewer is blind by default.

**Improve.** (M2) Base commit and `git diff <base>..HEAD` in the step; for a
returning loop, the diff since the last review too.

### W5. The fresh-eyes reviewer, round two
**Situation.** Memory "fresh": round two is a new session. I review again
from scratch.

**What happens.** I find *new* nitpicks instead of checking that round one's
requests were done. Goalposts move; the loop runs out (M4).

**Improve.** Fresh memory should mean a fresh session, not amnesia about the
job: pass "Last round you asked for: …" into the next review step.

### W6. The Planner's plan nobody reads
**Situation.** The Planner writes plan.md into the shared folder and hands
off. Devs are told a shared folder exists, not that it has a plan in it.

**Improve.** Step messages list what is in the shared folder and who changed
it last ("plan.md — Planner, step 2").

### W7. A question for "the user"
**Situation.** I'm unsure whether to support IE11 and reach for
AskUserQuestion.

**What happens.** Declined: "Nobody is watching this step… decide yourself,
or hand off with escalate." Good. Escalating sends it to the Manager, who can
ask the person. This works as designed.

---

## What to build next, in order

| # | Improvement | Fixes | Size |
|---|---|---|---|
| 1 | Run context in every step: base commit, run diff stat, changes since your last step, shared files | M2, W4, W6 | small |
| 2 | `factory_result` tool: the Manager merges/keeps/discards/reopens when the person says so | M6 | small |
| 3 | Reviewer continuity: last round's requests passed to fresh-memory reviewers | W5, M4 | small |
| 4 | Loop-limit escalation says why: last notes, diff between rounds | M4 | small |
| 5 | `factory_delegate`: ad-hoc job for any employee, always back to the Manager | M5 | medium |
| 6 | Branch confinement: writes only in this step's workspace; the `cd` line; per-workspace ports | W1, W2 | medium |
| 7 | Permission presets in the editor and rules suggested from the project's scripts | M3, W3 | medium |
| 8 | Blocked-on-permission shown on the live graph and panel | W3 | small |
| 9 | `factory_status` and leaner step messages for long runs | M7 | medium |
| 10 | "Done when…" field at start | M1 | small |
