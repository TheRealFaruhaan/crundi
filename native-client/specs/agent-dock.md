# 2.18 The agent dock

The work a chat has running **beside** the conversation: Claude Code's
sub-agents (the Agent / Task tool) and its background commands and Monitors
(`run_in_background`). The server routes their turns out of the transcript;
the web shows them as **badges** floating over the bottom right of the log,
a **list** per kind that a badge opens, and **one agent's panel** (its
prompt and its own transcript) opened from the list or from the agent's row
in the log.

Two kinds, counted and listed apart but behaving alike:

| Kind | Badge | List title | Back link | Opens to |
|---|---|---|---|---|
| `agent` (a sub-agent: the record has a `subagentType`) | robot icon | "Agents" | "‹ Agents" | its prompt, summary and transcript |
| `task` (a background command or Monitor) | terminal icon | "Commands" | "‹ Commands" | its tool row from the chat, expanded (its output) |

They share the floating dock of 2.17 (`.cc-agents`): the badges row is its
first item, above the recap and the suggestion. Rule 2 of DESIGN.md applies:
every size and colour below is a theme token.

Source:
- `app/vendor/claude-chat.js`
  - CSS 309-325 (`.cc-agents`, `.cc-agbadges`, `.cc-agbadge*`, the two
    one-shot animations), 327-348 (`.cc-aglist*`, `.cc-agrow*`,
    `.cc-agpanel-back`), 350-366 (`.cc-agpanel*`, `.cc-agentrow*`),
    111-114 (`.cc-spin`, `.cc-dot-ok`, `.cc-dot-err`).
  - The dock and badges 1182-1195; `isDismissed` / `markDismissed`
    768-800 (localStorage `crundi_chat_dismissed`).
  - `agentRec` 4157, `agentLabel` 4167, `agentRunning` 4172, `kindOf` 4178,
    `shownOf` 4181, `dockLayout` 4193, `KINDS` 4211, `badgeEvent` 4226,
    `syncBadge` 4241, `persistDismissed` 4272, `dismissAgent` 4285,
    `dismissAllShown` 4295, `applyAgent` 4306, `addAgentEntry` 4335,
    `patchAgentEntry` 4341, `appendAgentEntry` 4356, `paintAgentPanel` 4370,
    `findToolEntry` 4390, `toggleAgentPanel` 4407, `closeAgentPanel` 4465,
    `agentRowSub` 4476, `paintAgentList` 4486, `openAgentList` 4517,
    `closeAgentList` 4562, `toggleAgentList` 4573, `resetAgents` 4578,
    badge clicks 4777-4778.
  - The agent row in the log (`.cc-agentrow`, "transcript ›") 1906-1915
    (2.14 drew it; its click is this screen's).
  - `applyHistory` 4915-4924 (the stored `agents`, replayed with no
    animation), the `ui-event` switch 5048-5050 (`agent`, `agent-entry`,
    `agent-patch`).
- Server `src/claude-ui.js`
  - Sub-agents 709-1000: `getAgent` 764 (the record), `agentMeta` 778,
    `emitAgent` 786, the 5-minute auto-dismiss 790-845
    (`AGENT_LINGER_MS`), `agentEntry` 857, `agentPatch` 869,
    `handleAgentAssistant` 876, `handleAgentUser` 895, `handleTaskEvent`
    916 (kinds, `step`, `progress`, status, `usage`, summary).
  - `dismissAgents` 3274; the snapshot's `agents` 3180; persisted with the
    chat (`persistNow` 553; an agent's `messages` are dropped first when the
    record is too big, hence "transcript was not kept").
  - A running, undismissed `task` keeps the chat in `waiting` (snapshot
    `busyTasks`, about 3157).
- `src/webapp.js` 6486, 6533 (`POST …/dismiss-agents`).

## Screenshots

`agent-dock/web-<state>.png` at 1400×860. A `-2x` file is the named region
cropped and enlarged. The first states come from a real turn (two
`general-purpose` agents, a background `sleep 240`, a Monitor); the rest add
agents and entries through the page's WebSocket as the server's own
`ui-event` frames.

| State | What it shows |
|---|---|
| `web-badges-running` (+`-2x`) | **Both badges, busy.** Commands "3" with a spinner (left), Agents "2" with a spinner (right); the icon of a busy badge is `accent-hover`. |
| `web-badge-hover-done` (+`-2x`) | **Agents all finished, hovered**: a green dot instead of the spinner, the border `accent`, the text `text-primary`. Commands still busy beside it. |
| `web-badge-pop` (+`-2x`) | **An agent arrived** (frozen mid-animation): the badge scaled up, an accent ring going out. |
| `web-badge-done-ring` (+`-2x`) | **An agent finished** (frozen): green border and a green ring going out. |
| `web-list-agents` (+`-2x`) | **The Agents list**, two finished: head "Agents" · "2 finished" · Dismiss all · ✕; rows with a green dot, title, mono sub "general-purpose · finished · 25k tok", › and ✕; the foot note. The first row has focus (the list focuses it on opening). |
| `web-list-row-x-hover` (+`-2x`) | A row hovered (`bg-hover`) with its ✕ hovered (red). |
| `web-list-dismiss-all-hover` (+`-2x`) | "Dismiss all" hovered: red text and border. |
| `web-list-after-dismiss-one` | After a row's ✕: the row is gone, the head says "1 finished". |
| `web-list-commands` (+`-2x`) | **The Commands list**: "1 running · 2 finished", the running row first with a spinner and "running"; its own foot note. |
| `web-list-mixed` (+`-2x`) | **Running, failed and finished**: running first (spinner, sub shows the current step "Explore · Reading test/phase2.mjs"), a failed one (red dot, "failed · 812 tok"), a finished one (green dot). A long title is cut with an ellipsis. |
| `web-panel-from-list` (+`-2x`) | **One agent's panel, from the list**: "‹ Agents", the title, the sub "general-purpose · completed · 1 tools · 21k tok", ✕; the prompt box, the summary as a reply, then its transcript (text, a tool card, a thought, text). |
| `web-panel-back-hover` (+`-2x`) | "‹ Agents" hovered (`text-primary`). |
| `web-panel-command` | **A background command's panel**: "‹ Commands", "running"; its Bash tool row from the chat, expanded (command, description, output so far). |
| `web-panel-from-row-long` | **Opened from the log's agent row** ("transcript ›"): no back link; a long transcript, opened scrolled to its end. |
| `web-panel-waiting` (+`-2x`) | **A running agent with no step yet**: "Waiting for the agent’s first step…" (italic, muted), no prompt box (it had none). |
| `web-panel-live` (+`-2x`) | **Steps arriving live**: text, a finished Read, a running Bash (spinner); the sub now "general-purpose · Running git log · 2 tools · 4k tok". |
| `web-panel-prompt-long-title` (+`-2x`) | A long title in a wide panel and a three-line prompt box. |
| `web-narrow-badges-jump` (+`-2x`) | **A narrow pane** (378): the badges at the bottom right, the jump button beside them (badges alone do not lift it). |
| `web-narrow-list` (+`-2x`) | The list in a narrow pane: titles and subs cut with an ellipsis, › and ✕ kept; the foot wraps. |
| `web-narrow-panel` (+`-2x`) | The panel in a narrow pane: back link and ✕ kept, the title cut, the sub cut to almost nothing first. |
| `web-after-dismiss-all` (+`-2x`) | **After "Dismiss all"** on Agents: the list closed, the Agents badge gone, Commands still there. |

## Parts and arrangement

### The badges row (first in the dock)

```
                                   … log …
                                   ┌──────────┐ ┌──────────┐
                                   │ >_ 3  ◌  │ │ 🤖 2  ●  │   ← commands, agents
                                   └──────────┘ └──────────┘
                                   ┃ WHILE YOU WERE AWAY   ×    ← recap (2.17)
                                   …suggestion (2.17), always last
```

| Part | Web | Token |
|---|---|---|
| Row | right-aligned, gap 6; hidden while both badges are | `sp-6` |
| Order | the commands badge, then the agents badge | |
| Badge | a pill, at least 30 high; padding 4 10 4 8; gap 6; `bg-secondary`; 1px `border`; shadow 0 6 20 black 50 %; text 12 × fs, `text-secondary` | `control-sm`; `sp-4` / `sp-10` / `sp-8`; `sp-6`; `r-pill`; `shadow-menu` |
| Icon | 15 × fs; agents: the robot (lucide `bot`), commands: the terminal prompt; `accent-hover` while any of its kind runs, otherwise the badge's text colour | `icon-md` × fs (one step off); needs a `bot` icon in `ui/icons` |
| Count | mono, 600, tabular, `text-primary` | `mono`, `w-semibold` |
| State mark | a spinner (9, 1.5px ring, `border` with an `accent` top) while any runs; else a 6px `green` dot | the shared `Spinner`, the shared dot (`sp-6`) |
| Hover / open | border `accent`, text `text-primary` (the list of that kind is open = "open") | |
| Focus | 2px `accent` outline, 2 off | the shared focus ring |
| Tooltip | "3 background commands, 1 running — click to see them"; "1 agent, all finished — click to see it". One/many: agent/agents, background command/background commands; "it"/"them" | the shared tooltip |
| Arrival | 0.5 s: scale 0.6 → 1.14 → 1 with an `accent` ring growing to 9 and fading | `motion` + a small overshoot; `accent-ring` |
| Finish | 0.9 s: border and a ring in `green`, scale 1.08 at 30 % | `green-ring` |

### The list (a sheet over the log)

| Part | Web | Token |
|---|---|---|
| Sheet | over the **log area only** (inset 8; the composer, goal bar and header stay); `bg-primary`; 1px `border`; radius 10; shadow as the badge; above the dock and the plan; fades in 0.16 s | `sp-8`, `r-card`, `shadow-menu`, `motion-fast` |
| Head | padding 8 10, gap 8, a `border-subtle` rule under it | `sp-8`, `sp-10`, `sp-8` |
| Title | "Agents" / "Commands", 13 × fs, 600, `text-primary`, never shrinks | `text-13`, `w-semibold` |
| Sub | mono 10.5 × fs `text-muted`, fills, one line with an ellipsis: "2 running · 2 finished", "1 finished", "1 running" | `text-11`, `mono` |
| Dismiss all | a pill button: 1px `border`, `text-secondary` 11 × fs, padding 3 9; hover: `red` text and border | `r-pill`, `text-11`, `sp-2`/`sp-8` |
| ✕ | "✕" 15 × fs `text-muted`, padding 2 4, at least 24 wide; hover `text-primary`; tooltip "Close" | the `x` icon at `icon-md`, `control-sm` box |
| Body | scrolls; padding 6 | `sp-6` |
| Row | padding 9 8, gap 9, radius 6; hover and keyboard focus: `bg-hover`; the whole row is a button (tab stop) | `sp-8` all round (one step off), `sp-8` gap, `r-control` |
| Row mark | a 12-wide column: spinner (running), `red` dot (failed), `green` dot (anything else) | `sp-12` |
| Row title | `text-primary` (finished: `text-secondary`), 13 × fs, one line, ellipsis | `text-13` |
| Row sub | mono 10.5 × fs `text-muted`, one line, ellipsis, 2 under the title | `text-11`, `sp-2` |
| › | `text-muted` | the `chevron-right` icon, `icon-sm` |
| Row ✕ | "✕" 13 × fs `text-muted`, padding 6, at least 28 wide, radius 6; hover `red`; tooltip "Dismiss" (aria "Dismiss <title>") | `control-sm`, `r-control` |
| Foot | padding 8 12, a `border-subtle` rule above, `text-muted` 11 × fs, line 1.45, wraps | `sp-8`, `sp-12`, `text-11`, `lh-ui` |
| Foot text | Agents: "A finished agent leaves this list after 5 minutes. Its transcript stays on its Agent row in the chat." Commands: "A finished command leaves this list after 5 minutes. Its output stays on its row in the chat. Dismissing one does not stop it." | |

Row texts:
- **Title** (`agentLabel`): a task: `description` ‖ `summary` ‖ "Background
  task"; an agent: `description` ‖ "<subagentType> agent" ‖ "Agent".
- **Sub** (`agentRowSub`), joined by " · ": `subagentType` (if any); running:
  `progress` ‖ `step` ‖ "running"; otherwise "finished" for `completed` or
  no status, else the status as is ("failed", "stopped", "killed"); then the
  tokens when `usage.total_tokens`: "21k tok" (≥ 1000, rounded) or "812 tok".
- **Order**: running first; then finished, newest `endedAt` first; ties keep
  arrival order.

### One agent's panel (the same sheet)

| Part | Web | Token |
|---|---|---|
| Sheet | as the list; it replaces the list (one sheet at a time) | |
| Back | only when opened from the list: "‹ Agents" / "‹ Commands", `text-muted` 12 × fs, padding 2 6 2 0; hover `text-primary`; tooltip "Back to the list" | `text-12` |
| Title | `agentLabel`, 13 × fs 600 `text-primary`, fills, ellipsis | |
| Sub | mono 10.5 × fs `text-muted`, ellipsis; it gives way **first** (the title keeps its room) | `text-11`, `mono` |
| ✕ | as the list's (no tooltip) | |
| Body | scrolls; padding 10 12 | `sp-10`, `sp-12` |
| Prompt box | only with a `prompt`: `bg-secondary`, 1px `border-subtle`, radius 6, padding 7 9, 10 under it, `text-secondary` 12 × fs, keeps line breaks, wraps anywhere, at most 150 high then scrolls | `r-control`, `sp-8`, `sp-10`, `text-12`; a structure token for the 150 cap (e.g. `cmd-max-h` if it fits) |
| Summary | with a `summary`: the reply's markdown, as an assistant entry | the log's reply |
| Command output | a `task`: its tool row from the chat (same `toolUseId`), **expanded** | the tool card of 2.14 |
| Transcript | the agent's own entries, drawn exactly as the log draws them (text, thinking, tool cards with their states and results, 2.14/2.15) | the log's entry components |
| Empty | italic `text-muted`: agent running "Waiting for the agent’s first step…"; agent ended "This agent’s transcript was not kept."; task running "Running in the background — it reports when something happens."; task ended "This task has finished." | |

Panel sub (`paintAgentPanel`), joined by " · ": `subagentType` (if any);
running with `progress` ‖ `step`: that; otherwise `status` ‖ "done" (raw:
"running", "completed", "failed"); then "<n> tools" for `usage.tool_uses`
(web says "1 tools") and "<n>k tok" for `usage.total_tokens` (rounded).

## Behaviour

### What is shown

- A record is **shown** while not dismissed. Each badge counts its kind's
  shown records; it hides at 0. The spinner shows while any of them runs
  (`status` "running" or none).
- **The server dismisses by itself** 5 minutes after a record ended
  (`AGENT_LINGER_MS`): an `agent` event with `dismissed: true` arrives and it
  leaves the badge and the list. A running one is never auto-dismissed.
- **Arrival and finish animations** play only for live news: not while a
  stored session is replayed (`ui-history`), not while a list or panel is
  open, none with reduced motion. Arrival: a record becomes shown (new, or
  its kind changed). Finish: a shown record goes from running to not running
  (the web plays the green one for a failure too).
- The dock pads the log by its height + 14 while a badge (or a suggestion)
  shows, so the newest line can scroll clear (2.17). The jump button stays
  at its usual place beside the badges; only a recap or suggestion lifts it.

### Controls

| Control | Does |
|---|---|
| A badge | Opens its kind's list (closing any panel or other list); clicked again, closes it. |
| A list row (click, Enter, Space) | Opens that record's panel, with the back link. |
| A row's ✕ | Dismisses that one (does not stop it); the list repaints; the last one gone closes the list. |
| Dismiss all | Dismisses every shown record **of this kind** (running ones too) and closes the list; the other kind is untouched. |
| The list's ✕ | Closes the list. |
| Esc in the list | Closes it and puts focus back on its badge. |
| The log's agent row ("transcript ›", 2.14) | Opens that agent's panel without a back link; clicking the same row again closes it. Works for a dismissed agent too (its row stays). |
| Back | Closes the panel and opens the list of the record's kind. |
| The panel's ✕ | Closes the panel. (The web has no Esc for the panel.) |

Opening the list focuses its first row; Tab walks rows and their ✕. The
panel opens scrolled to its end.

Dismissing a running **command** also stops it holding the chat in
`waiting`: the server counts only undismissed running tasks, so the header
can drop to idle.

### Live updates

| Event (`ui-event`) | Effect |
|---|---|
| `{type:'agent', agent:<record>}` | The record replaces the known one (it is the whole record; `count` kept if absent). Badges, an open list (scroll kept) and an open panel's head repaint. `dismissed: true` hides it. Also repaints the log's agent row of that `toolUseId` (2.14). |
| `{type:'agent-entry', toolUseId, entry}` | Appended to that record's transcript; if its panel is open, drawn at the end (the "empty" line goes), staying at the bottom when the reader was within 40 of it. |
| `{type:'agent-patch', toolUseId, id, patch}` | That entry patched; redrawn in place if its panel is open (a tool's result arriving). |
| `ui-history` | All records rebuilt from `session.agents` (each with its `messages`); any open list or panel closes; no animation. |

## Data and endpoints

**The record** (`session.agents[]` in the snapshot, with `messages`; the
`agent` event carries it without `messages` and with `count`):

```
{ toolUseId, kind: 'agent'|'task', taskId, description, step, progress,
  subagentType, prompt, status: 'running'|'completed'|'failed'|…,
  summary, usage: { total_tokens, tool_uses, … } | null, lastTool,
  startedAt, endedAt, lastAt, dismissed?, autoDismissed?, count }
```

Entries (`messages[]`, `agent-entry`): `{id, kind: 'assistant-text'|'thinking',
text, tokens}` or `{id, kind: 'tool', toolUseId, name, input, status:
'running'|'done', result, isError}`, the log's own shapes. `description` is
cut at 70 characters by the server, a text at 20 000; at most 150 entries per
agent, 40 records per chat.

**Dismissing**: `POST /api/ui-sessions/:id/dismiss-agents` with
`{toolUseIds: [..]}` (or `{all: true}`: every *finished* one) → `{ok,
dismissed: n}`. The server marks them, sends each as an `agent` event to
every open view, and saves the chat. The web also keeps the ids in
localStorage (`crundi_chat_dismissed`, 90 days, 2000 ids) for instant
feedback; the native client applies the dismissal at once, posts it, and
needs no local store (the server's copy is what survives). A failed post
leaves it dismissed in this view; a 404 says "not supported by this server".

No capability is needed: the route and the events predate the native client.

## Named differences and web accidents

- **"1 tools"**: the web's plural slip; the native says "1 tool".
- **Panel without Esc**: the web's panel has no Esc (the list has). The
  native keeps it the same (rule 1); a later change would add it to both.
- **Green finish ring on a failure**: the web plays the same "done" ring;
  kept.
- **The badge keeps its old tooltip when hidden**: invisible, not copied.
- The animations are one-shot; the native may approximate the overshoot with
  Slint's easing (PLAN.md, Known gaps, if it cannot).

## What the native client has today

`src/chat.rs` keeps `agents` (merged from `agent` events) for the log's
agent rows (2.14) only. Missing: the badges, the list, the panel, the
per-agent `messages` with `agent-entry` / `agent-patch`, dismissing, and the
row's click. The dock's column, width (`chat-dock-w`) and placement exist
from 2.17.

## End-to-end test (phase2.mjs)

A turn that starts a sub-agent (or a seeded `agent` + `agent-entry` through
the test server): the agents badge appears with its count; clicking it opens
"Agents" with the row; the row opens the panel (prompt, sub-tool); back
returns to the list; ✕ on the row dismisses it, the badge goes, and
`dismiss-agents` reached the server (the record comes back `dismissed` in
the snapshot after a reload).

## Checklist

- [ ] Badges row first in the dock; commands left, agents right; each
      hidden at 0.
- [ ] Badge: pill, icon (robot / terminal), mono count, spinner while any
      runs (icon `accent-hover`) else green dot; hover/open accent border
      and `text-primary`; focus ring; tooltip wording.
- [ ] Arrival and finish animations, live only (not on replay, not while a
      sheet is open).
- [ ] Log padded by the dock while a badge shows; jump button not lifted by
      badges alone.
- [ ] List: sheet over the log only; head (title, counts sub, Dismiss all,
      ✕); rows (mark, title, sub, ›, ✕) in the right order; foot text per
      kind.
- [ ] Row texts: title fallbacks, sub (type · step/finished/status · tok).
- [ ] Row hover/focus `bg-hover`; ✕ hover red; Dismiss all hover red.
- [ ] Row click / Enter / Space opens the panel with back; Esc closes the
      list and focuses the badge; the first row focused on open.
- [ ] Dismiss one; Dismiss all (this kind only, closes the list); the list
      closes when empty; POST `dismiss-agents`; survives a reload.
- [ ] Server auto-dismiss (`dismissed: true`) hides a record live.
- [ ] Panel: back (from the list only), title, sub giving way first, ✕;
      prompt box (capped, scrolls); summary; a command's tool row
      expanded; the transcript as the log draws it; the four empty texts.
- [ ] Panel live: entries append (stick to the bottom), patches redraw,
      the sub updates.
- [ ] The log's agent row opens the panel without back and toggles it.
- [ ] Narrow pane: badges, list and panel keep their buttons; texts cut.
- [ ] Tokens only; screenshots side by side under `agent-dock/review/`.
