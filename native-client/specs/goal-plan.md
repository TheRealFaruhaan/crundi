# 2.19 Goal and plan

Two things a chat can be working **towards**, each shown only while it
exists, so a normal chat is unchanged:

- **The goal** (`/goal <condition>`, Claude Code's goal mode): a bar
  between the log and the composer (`.cc-goalbar`) saying what the chat is
  working towards, whether the loop is running, how many checks it made,
  and a **clear** button; plus a **verdict card** in the log for each check
  that found the goal not yet met (`.cc-gv`).
- **The plan** (Plan mode's `ExitPlanMode`): a pill **PLAN** in the pane
  header whose colour is the plan's status, and a **plan panel**
  (`.cc-plan`) over the log that the pill opens and closes.

The header's **PLANNING** tag (Plan mode, no plan yet) is 2.6's; this
screen's pill takes its place once a plan exists. Rule 2 of DESIGN.md
applies: every size and colour below is a theme token.

Source:
- `app/vendor/claude-chat.js`
  - CSS 368-393 (`.cc-plan*`, `.cc-goalbar`, `.cc-goal-*`, `.cc-gv*`).
  - Build 1201-1220 (the plan panel inside the log wrap; the goal bar
    between the log wrap and the composer), `plan` / `planOpen` 1291.
  - The verdict entry `goal-verdict` 1798-1803.
  - `planStatusLabel` 4788, `renderPlan` 4794, `notifyMeta` 4813,
    `togglePlan` 4820, the panel's Close 4827, `renderGoal` 4829,
    `clearGoal` 4851.
  - `applyHistory` 4925-4929 (`session.goal`, `session.plan`), the
    `ui-event` switch 5013 (`goal`) and 5065-5072 (`meta` with `plan`).
  - `togglePlan` exported 5109.
- `src/webapp-html.js`
  - `.term-plan-tag` CSS 1269-1280.
  - `planTagFor` / `planTagHtml` 8768-8783, the live header update
    9139-9150, `applyChatMeta` 9175-9183, the click `chat-plan` 16509-16514.
- Server `src/claude-ui.js`
  - Goal: `goalSnapshot` / `emitGoal` 2019-2025, `noteGoalCommand` 2030
    (on every sent message), `handleHookFeedback` 2045 (a verdict: an entry
    `goal-verdict` and `verdicts + 1`), a turn's result ends the loop 2313.
  - Plan: `planSnapshot` 1065, `setPlan` 1077 (a `meta` event), captured
    from `ExitPlanMode`'s approval request 2395-2410 (`proposed`), approved
    or denied 2939-2943 (`executing` / `rejected`), leaving Plan mode with a
    proposal out 1724-1734 (`executing`); `planStatus` in the chat list 489.
  - The snapshot's `goal`, `plan` 3181-3182. Neither is persisted: a
    server restart or a resumed chat starts without them.

## Screenshots

`goal-plan/web-<state>.png` at 1400×860; three panes (378 / 454 / 338) or
the middle one full screen. A `-2x` file is the named region cropped and
enlarged. The goal was set with a real `/goal`, the plans are two real
`ExitPlanMode` proposals (one allowed, one denied); the checks and the
stopped loop were injected as the server's own `goal` / `entry` frames.

| State | What it shows |
|---|---|
| `web-goal-running` (+`-2x`) | **A goal, loop running**: spinner + "GOAL", the condition cut with an ellipsis, "running", "clear". |
| `web-goal-clear-hover` (+`-2x`) | "clear" hovered: `red` text and border. |
| `web-goal-set` (+`-2x`) | **The loop ended with no check** (the goal was met in one turn): no spinner, "set". |
| `web-goal-checks-verdicts` (+`-2x`) | **Two checks**: two verdict cards in the log ("Goal not yet met · check 1/2" and the reason), the bar "2 checks", spinner on. |
| `web-goal-stopped` (+`-2x`) | **The loop stopped after checks**: no spinner, "2 checks", and a second line "The goal loop has stopped. Send /goal to ask for its current verdict." |
| `web-goal-cleared` | **After clear**: "/goal clear" sent as the person's message, Claude's answer, the bar gone. |
| `web-pill-proposed` (+`-2x`) | **The PLAN pill, proposed**: `yellow` text and border, before CHAT. |
| `web-pill-hover` (+`-2x`) | The pill hovered (`bg-hover` fill). |
| `web-plan-open-proposed` (+`-2x`) | **The plan panel**: a `bg-secondary` header "PLAN" + "waiting for your approval" + "Close", the plan's markdown below, over the whole log (composer still there). |
| `web-plan-close-hover` (+`-2x`) | "Close" hovered: `text-primary`, `accent` border. |
| `web-plan-open-executing` (+`-2x`), `web-pill-executing-2x` | **Approved**: the pill `green`; the status "approved — being worked through". |
| `web-plan-live-replaced` | **A new plan while the panel is open**: the text and the status change in place (back to "waiting for your approval", pill yellow). |
| `web-plan-open-rejected` (+`-2x`), `web-pill-rejected-2x` | **Denied**: the pill muted (`text-muted`, plain `border`); "sent back for revision". |
| `web-narrow-tray-rejected` (+`-2x`) | **A narrow header**: the pill in the "…" tray, before CHAT. |
| `web-narrow-plan-open` (+`-2x`) | The plan panel in a narrow pane (454). |
| `web-goal-with-plan-and-narrow` (+`-2x`) | **Goal and plan together**: the plan panel covers the log, the goal bar stays under it; beside it a narrow pane's goal bar (338) with "3 checks". |

## Parts and arrangement

```
┌ header … [PLAN] [CHAT] …                    ┐   ← pill (2.19) or PLANNING (2.6)
│┌ PLAN  waiting for your approval   [Close] ┐│   ← plan panel, over the log only
││ ### Dark-mode toggle …                    ││
││ markdown …                                ││
│└───────────────────────────────────────────┘│
│ ◌ GOAL  The project folder conta…  2 checks [clear] │ ← goal bar
│ The goal loop has stopped. Send /goal …     │
│ composer                                    │
```

### The goal bar

| Part | Web | Token |
|---|---|---|
| Bar | full pane width, between the log and the composer; a `border-subtle` rule on top; `accent-dim` fill; padding 6 10; 11.5 × fs | `sp-6`, `sp-10`, `text-12` × fs (one step off) |
| Head row | one line, gap 8 | `sp-8` |
| Tag | while looping a spinner (12) then "GOAL", gap 5; mono 9.5 × fs, 700, wide tracking (.08 em), `accent-hover` | the shared `Spinner`, `sp-4`; `text-10`, `w-bold`, `track-wide` |
| Condition | `text-primary`, fills, one line, ellipsis (the server keeps up to 2000 characters) | |
| Count | mono 10 × fs `text-muted`: "1 check" / "N checks" when verdicts > 0; otherwise "running" (looping) or "set" | `text-10`, `mono` |
| clear | "clear", a small outlined button: 1px `border`, radius 6, `text-muted` 10.5 × fs, padding 1 7; hover `red` text and border; tooltip "Stop working towards this goal (/goal clear)" | `r-control`, `text-11`, `sp-2`/`sp-6` |
| Stopped line | only when the loop stopped **and** there were checks: "The goal loop has stopped. Send /goal to ask for its current verdict.", `text-muted`, 4 above | `sp-4` |

### The verdict card (a log entry, `kind: 'goal-verdict'`)

| Part | Web | Token |
|---|---|---|
| Card | a 2px `accent` bar on the left; `bg-secondary`; radius 0 6 6 0; padding 6 10 | `sp-2`, `r-control` on the right corners, `sp-6`, `sp-10` |
| Head | a 5px `accent` dot, "Goal not yet met · check N"; `accent-hover` 11 × fs 600; gap 6; 3 under it | `sp-6` dot (one step), `text-11`, `w-semibold`, `sp-2` |
| Body | the reason, `text-secondary` 12 × fs, keeps line breaks, wraps anywhere | `text-12` |

It sits in the log like any entry (it scrolls, it is restored from
history).

### The plan pill (pane header)

| Part | Web | Token |
|---|---|---|
| Pill | "PLAN" (upper case of "plan"); 0.62 rem, 700, tracking .06 em; 1px edge; radius pill; padding 1 7 | `text-10`, `w-bold`, `track-wide`, `r-pill`, `sp-2`/`sp-6`; the same component as 2.6's PlanTag |
| Place | wide header: just before CHAT; narrow header: in the "…" tray, before CHAT (as PLANNING, 2.6) | |
| proposed | `yellow` text and edge; tooltip "Plan awaiting your approval — click to read it" | `yellow` |
| executing | `green`; "Approved plan — click to read it" | `green` |
| rejected | `text-muted` text, `border` edge; "Plan sent back for revision — click to read it" | |
| (no plan, Plan mode) | PLANNING, `sky`, not clickable (2.6) | |
| Hover | (clickable states only) `bg-hover` fill, pointer | `bg-hover` |
| Click | toggles the plan panel | |

### The plan panel

| Part | Web | Token |
|---|---|---|
| Panel | over the **log area** (inset 0): the header, goal bar and composer stay; `bg-primary`; above the dock and the log, under the agent sheets of 2.18 | |
| Header | padding 7 10, gap 8, `bg-secondary`, a `border` rule under it | `sp-6`/`sp-8` (one step), `sp-10`, `sp-8` |
| Tag | "PLAN", mono 9.5 × fs 700 wide tracking `accent-hover` | as the goal's tag |
| Status | `text-muted` 11 × fs, fills, ellipsis: proposed (or anything else) "waiting for your approval"; executing "approved — being worked through"; rejected "sent back for revision" | `text-11` |
| Close | as the goal's clear button, "Close"; hover `text-primary` and `accent` edge | |
| Body | scrolls; padding 12 14; the plan's markdown as a reply renders it, headings 13 × fs with 14 above and 6 below (none above a first h1/h2) | `sp-12`, `sp-16` (one step), `text-13` |

## Behaviour

### Goal

- **Set**: sending any message that starts `/goal <condition>` makes the
  server set `{condition, verdicts: 0, looping: true}` and send a `goal`
  event at once, before the CLI answers. A bare `/goal` asks for status
  and changes nothing.
- **Checks**: each time Claude Code's Stop hook says the goal is not yet
  met, the server adds a `goal-verdict` entry to the log and sends the goal
  with `verdicts + 1`, `looping: true`.
- **Loop end**: the turn's result sets `looping: false` (the server never
  claims the goal was met). With checks, the stopped line appears; without,
  the count reads "set".
- **clear**: sends `/goal clear` through the normal send path (it shows as
  the person's message; the chat goes working). If the chat is busy it is
  **queued** like a typed message (2.9) instead. The server, on that
  message, sets the goal to null and sends `{type:'goal', goal:null}`: the
  bar goes.
- **Live**: `{type:'goal', goal}` replaces the bar (null hides it);
  `ui-history` takes `session.goal`.
- The bar scales with the pane's A-/A+; it has no keyboard of its own
  besides the button.

### Plan

- **Proposed**: in Plan mode Claude asks to use `ExitPlanMode`; the server
  takes the plan's markdown from that request (`input.plan`), stores
  `{text, filePath, at, status: 'proposed'}` and sends
  `{type:'meta', plan}`. The permission card (2.13 / F107) asks Allow / Deny
  as for any tool.
- **Executing**: Allow on that card, or the mode leaving Plan mode while a
  proposal is out. **Rejected**: Deny.
- **A new proposal** replaces the old plan (text and status) — live in an
  open panel.
- **The pill** follows `planStatus` in the chat list (state broadcasts) and
  the `meta` event (patched straight into the header, since a plan change
  moves no chat state). A plan outranks PLANNING.
- **The panel**: the pill toggles it; Close closes it; it closes by itself
  when the plan goes (`plan: null`). It never opens without a plan. No Esc
  (web). While open it covers the log, so a pending `ExitPlanMode` card is
  under it: the person closes the panel to answer (web, kept).
- The plan is not kept across a server restart or a resumed chat; the pill
  goes with it.

## Data and endpoints

- Snapshot (`ui-history`): `session.goal` `{condition, verdicts,
  lastVerdict, looping, startedAt}` | null; `session.plan` `{text, filePath,
  at, status: 'proposed'|'executing'|'rejected'}` | null;
  `session.permissionMode`.
- Chat list / state broadcast: each chat's `planStatus` ('' when none) and
  `permissionMode`, for the pill on a pane whose chat view is not loaded.
- Events: `{type:'goal', goal}`; `{type:'meta', plan}` (only plan changes
  carry the `plan` key: a `meta` without it is not "the plan went away");
  `{type:'entry', entry:{kind:'goal-verdict', text, condition, n}}`.
- Sending: `POST /api/ui-sessions/:id/send {text:'/goal clear'}` (the
  composer's own send, 2.5); no endpoint of its own.

## Named differences and web accidents

- **Plan lists**: the web's plan body has none of the reply's list
  styling, so numbers sit outside the panel and every item restarts at
  "1." (`web-plan-open-*`). The native draws the plan with the reply's
  markdown (hanging bullets and numbers, 2.16/F106), not the accident.
- **Narrow tray under the panel**: in a narrow pane the web's "…" tray
  opens underneath an open plan panel and cannot be seen. The native keeps
  the tray above the panel.
- **A first `###` heading** keeps 14 above it (the web resets only h1/h2):
  the native drops the top margin of any first heading.

## What the native client has today

The PLANNING tag (2.6, `PlanTag` in `ui/chat.slint`, `planning` in
`src/app.rs` from `planStatus`). Missing: the PLAN pill and its states, the
plan panel, `meta.plan` handling, the goal bar, `goal` events, the clear
button, and the `goal-verdict` card.

## End-to-end tests (phase2.mjs)

- **Goal**: send `/goal <condition>` (a seeded chat on the test server):
  the bar shows the condition and "running"/"set"; clear sends `/goal
  clear` and the bar goes.
- **Plan**: in Plan mode a turn that ends in `ExitPlanMode` (or a seeded
  `meta.plan`): the header shows PLAN (proposed); clicking it opens the
  panel with the plan's text and "waiting for your approval"; clicking again
  closes it; Allow turns the pill green and the status "approved — being
  worked through".

## Checklist

- [ ] Goal bar only while a goal is set; between log and composer;
      `accent-dim` with the top rule.
- [ ] Tag (spinner while looping), condition with ellipsis, count
      ("N checks" / "running" / "set"), clear (tooltip, red hover).
- [ ] Stopped line only when stopped with checks.
- [ ] clear sends `/goal clear` (queued while busy); the bar goes on the
      server's `goal: null`.
- [ ] Verdict card in the log: accent bar, dot, "Goal not yet met · check
      N", reason.
- [ ] PLAN pill: proposed yellow, executing green, rejected muted; tooltips;
      hover fill; before CHAT, in the tray when narrow; PLANNING when in
      Plan mode with no plan.
- [ ] Pill click toggles the panel; Close closes; `plan: null` closes and
      removes the pill.
- [ ] Panel over the log only (goal bar and composer stay); header tag,
      status wording, Close; markdown body with proper lists.
- [ ] Live: a new plan replaces the open panel's text and status; the pill
      follows `meta` and `planStatus`.
- [ ] Narrow pane: bar, tray pill and panel work; the tray shows above the
      panel.
- [ ] Tokens only; screenshots side by side under `goal-plan/review/`.
