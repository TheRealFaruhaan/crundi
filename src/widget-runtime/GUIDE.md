# Building a Crundi widget

A widget is a small live UI that you design and Crundi shows next to the chat: beside it, in its own workbench pane, as a full tab, inline in the transcript, or as a chip in the top bar. The person may be on a phone.

## Whether to build one

Build a widget when something is better **seen than read**, and will be looked at more than once:

- progress on a task that runs for minutes (steps, what is running now, test results)
- a table or chart from a file or database the person will keep checking
- a build, deploy, queue or service board
- a log tail, a comparison, a set of options to pick from

Do not build one for a quick answer, a single number, or anything a sentence covers. A two-minute fix does not need a dashboard. If you open one unasked, say so in one line, and close it when the work is done (`widget_close`) unless it is meant to stay.

For live progress on your own work, you usually do not need to push anything: bind a `session` source and the widget follows the chat by itself.

## The loop

1. `widget_open` with an `id` (and `slot`). It returns `sourceDir`.
2. Write `index.html` and `widget.json` there with your normal file tools. Saving hot-reloads the widget.
3. `widget_render` — read the report, **look at every screenshot**, fix, render again. Do this before you tell the person it is ready. Check a phone frame as well as a desktop one.
4. `widget_get` later to see faults from the live page, whether the person moved or closed it, and anything they did in it.

If `widget_get` says the person closed or moved the widget, leave it that way.

## Files

```
.crundi/widgets/<id>/
  widget.json      manifest (optional for a push-only widget)
  index.html       body fragment: markup, <style>, <script>
  fixtures/*.json  { "sourceName": value } per state, for widget_render
  anything else    images, .js, .css referenced relatively are inlined
```

`index.html` is a **fragment**, not a page. The design tokens, the kit CSS and `window.crundi` are already loaded. No `<html>`, no `<head>`.

The frame has **no network and no storage of its own**. `fetch`, CDN scripts, web fonts, `localStorage` and external images do not work, by design. Data comes from sources; persistence is `crundi.store`. Put any library you need in the widget folder and reference it relatively (512 KB a file).

## widget.json

```json
{
  "title": "Orders",
  "slot": "cell",
  "sources": {
    "task":   { "kind": "session" },
    "stats":  { "kind": "push" },
    "orders": { "kind": "sqlite", "path": "data/app.db", "query": "select id, total, status from orders order by id desc limit 50" },
    "log":    { "kind": "file", "path": "logs/build.log", "format": "lines", "tail": 200 },
    "cfg":    { "kind": "file", "path": "config.json" },
    "disk":   { "kind": "command", "run": "df -h / | tail -1", "every": 30 },
    "health": { "kind": "http", "url": "http://localhost:3000/health", "every": 10 },
    "board":  { "kind": "crundi", "what": "kanban" }
  },
  "actions": {
    "retry":  { "kind": "command", "run": "npm run build -- --only {{target}}" },
    "done":   { "kind": "tool", "tool": "kanban_move_task", "args": { "taskId": "{{id}}", "status": "done" } },
    "ask":    { "kind": "prompt", "text": "Explain why {{test}} failed." },
    "picked": { "kind": "event" }
  }
}
```

### Sources

| kind | reads | refreshes |
|---|---|---|
| `push` | what you send with `widget_set_data` | when you push |
| `session` | this chat's live activity (see below) | as it happens, at no token cost |
| `file` | `path`; `format`: `json`, `jsonl`, `csv`, `lines`, `text` (guessed from the extension); `tail` for line formats | when the file changes |
| `sqlite` | `path` + `query` (or `queries: {name: sql}`), read-only, `limit` rows (default 500) | when the database file changes; `every` seconds to poll, `always: true` to re-run regardless |
| `command` | stdout of `run`; `format`, `every` (min 2 s), `timeout` | on the interval |
| `http` | body of `url`; `format`, `every` | on the interval |
| `crundi` | `what`: `kanban`, `services`, `schedules`, or `stats` (this machine: `cpu.pct`, `cpu.cores[]`, `mem.pct/used/total`, `disk.pct/used/total`, `net.rxPerSec/txPerSec`, `load`, `history.cpu[]`, `history.mem[]`) | when it changes; `stats` every 2 s (`every` to change) |

Paths are relative to the project; a path starting with `./` is relative to the widget's own folder, which is where data that belongs to the widget should live (see Global widgets). A value is capped at 1 MB: narrow it with `tail`, `limit` or a tighter query.

A `session` source gives:
`{ state, working, title, model, prompt, now, lastText, todos:[{text,active,status}], progress:{done,total,pct}, tools:[{name,label,status}], files:[{path,short,edits}], agents:[{description,status,step}], counts:{tools,edits,commands,errors}, costUsd }`.
`state` is `idle`, `working`, `waiting` or `needs-input`. `todos` come from your own todo list, so keeping that list current is what makes the dashboard good.

**Approval.** `command` and `http` sources, files outside the project, and `command` / `tool` / `prompt` actions need the owner's one-tap approval, shown on the widget. Until then those sources read as an error `needs-approval`; draw that state plainly. Changing any of them asks again. Everything else needs nothing.

### Actions

Called from the widget with `crundi.action(name, params)`. `{{param}}` is filled from `params` (shell-quoted in commands).
`tool` actions may use: kanban_add_task, kanban_update_task, kanban_move_task, kanban_add_todo, kanban_update_todo, start_service, stop_service, restart_service, schedule_set_enabled, send_message_to_user.
`event` actions (and `crundi.emit`) leave a note that you read with `widget_get`: the way to let the person pick something for you.

## Inside the frame: `window.crundi`

```js
crundi.onData(function (data, changed) {      // runs now and on every change
  render(data.orders || []);                   // data[sourceName]
});
crundi.errors.orders                           // why a source failed, if it did
crundi.context                                 // { frame, platform:'desktop'|'mobile', width, height, touch }
crundi.onContext(fn)                           // resized or moved to another slot

crundi.store.get('tab', 'all'); crundi.store.set('tab', 'open');   // survives reloads
crundi.action('retry', { target: 'web' }).then(r => ...)           // r.stdout for commands
crundi.emit('picked', { option: 2 })           // a note for Claude
crundi.prompt('Run the failing test again')    // needs "allow": ["prompt"]
crundi.openLink(url); crundi.toast('Saved')

crundi.h('div', { class: 'c-card', onclick: fn }, 'text', childNode)
crundi.esc(text)                               // ALWAYS escape data you put in innerHTML
crundi.fmt.num / compact / pct / money / bytes / duration(ms) / ago(t) / time(t) / date(t)
crundi.icon('check')                           // SVG string
crundi.chart.spark(values) / bars(items) / line(series) / donut(parts)   // SVG strings
crundi.get(obj, 'a.b.c', fallback)
```

Without script: `<span data-bind="task.now" data-empty="idle"></span>`, `data-fmt="ago"`, and `<div data-show="task.working">…</div>`.

Icons: check x plus minus play pause stop refresh clock alert info file folder terminal code edit search database server activity chart trend trend-down arrow-up arrow-down arrow-right chevron-right chevron-down external git box zap user users cart dollar globe cpu list grid bell star flag circle loader send trash download eye lock bug layers.

Charts are sized by their box: `<div class="c-chart-box">` + `innerHTML = crundi.chart.line(...)`. `bars(items, {horizontal:true})` returns labelled rows and is the right choice when names matter.

Errors in your script are reported to you (`widget_get`, `widget_render`); they do not show to the person as a broken page beyond an empty area, so always draw an empty state.

## Design

Use the tokens and the kit. Hard-coded colours are flagged by lint and drift from the app.

**Tokens**

```
{{TOKENS}}
```

Semantic use: `--green` good/done, `--yellow` waiting/warn, `--red` failed, `--sky` info, `--accent` the one thing to act on. `--text-secondary` for labels, `--text-muted` only for decoration (it fails contrast for real text). Dark theme only.

**Kit classes** (prefix `c-`)

- layout: `c-stack` (`tight`), `c-row` (`wrap` `between` `top`), `c-grow`, `c-grid` (`wide` `two`), `c-divider`, `c-scroll`
- text: `c-title`, `c-h` (small caps section label), `c-muted`, `c-faint`, `c-small`, `c-mono`, `c-num`, `c-truncate`, `c-ok` `c-warn` `c-err` `c-info` `c-accent`
- surfaces: `c-card` (`flush`)
- stat tile: `c-stat` > `c-stat-label`, `c-stat-value`, `c-stat-sub`; `c-delta up|down`
- status: `c-badge` (`ok warn err info`), `c-dot` (`ok warn err info accent`, `pulse`), `c-spin`
- controls: `c-btn` (`primary ghost danger small block`), `c-input`, `c-tabs` > `c-tab active`
- progress: `c-progress` (`ok warn err thick`) > `<span style="width:40%">`
- lists: `c-list` > `c-item`; `c-kv` > `dt`/`dd`; `c-steps` > `c-step done|active|failed` > `c-step-mark` + text
- table: `c-table-wrap` > `table.c-table` (`th.r` / `td.r` right-aligned numbers)
- log: `c-log` (`.err .warn .ok` lines)
- charts: `c-chart-box` (`small`), `c-donut`, `c-legend` > `c-swatch`
- chip face: `c-chip-spark`, `c-chip-meter` (`ok warn err`), `c-chip-label`
- states: `c-empty`, `c-error`, `c-skel`
- responsive: `c-hide-narrow`, `c-only-narrow`

**Rules that make it look right**

- The frame's width is the widget's width, so `@media (max-width: 480px)` means "this widget is narrow". A dock or a phone is about 360–400px wide. Design for that first, then let it breathe when wide.
- One clear hierarchy: a title or the single most important number at the top, detail below. Not everything is a card.
- Numbers: `c-num` (tabular), right-aligned in tables, formatted with `crundi.fmt`.
- Flex children that hold text need `min-width: 0` (the kit's `c-grow` and `c-truncate` do this) or they push the layout sideways.
- Wide tables go in `c-table-wrap`. On a phone, prefer a list of rows over a table with more than three columns.
- Touch: controls at least 44px tall on mobile (`c-btn` and `c-input` grow by themselves; custom ones do not).
- Always draw three states: loading or empty, error (`crundi.errors[name]`), and full. Write a fixture for each and render them with `widget_render` and `states`.
- Motion only where it carries meaning (a pulsing dot while working). Respect `prefers-reduced-motion`.
- Escape data: `crundi.esc()` for anything interpolated into `innerHTML`, or build nodes with `crundi.h`.

## Slots

- `dock` — attached to this chat: above it when the pane is narrow or on a phone, beside it when wide. Sizes to its content up to about 45% of the pane. Best for live task progress. Keep it short.
- `cell` — its own workbench pane, resizable; `beside: "right"|"below"` puts it next to this chat. Best for something used alongside the work.
- `tab` — full size in the Panels tab. Dense dashboards and wide tables.
- `inline` — a card in the transcript where you called `widget_open`. Sizes to content (max 460px). A one-off result.
- `chip` — one line in the top bar that opens the widget when tapped. A status worth a glance from anywhere. Two ways to fill it, your choice:
  - **Labelled** (nothing to write): the title, or `chip: { source, path, label }` to show one value beside a label.
  - **Your own face**: mark ONE top-level element in `index.html` with `data-chip`. In the top bar only that element is drawn, on one line, as wide as its content (up to about 260px, 130px on a phone); everywhere else it is hidden. Your script runs as usual, so the same `onData` that fills the panel keeps the chip live: a number, a sparkline, a meter, a coloured dot, your own icon. See "The chip face" below.

Every widget is also listed in the Panels tab, where the person can reopen, move, pin or delete it.

## The chip face

```html
<div data-chip>
  <span id="chip-ic"></span>
  <span class="c-chip-label">CPU</span>
  <b id="chip-cpu">–</b>
  <span class="c-chip-spark" id="chip-spark"></span>
  <span class="c-chip-meter" id="chip-mem"><span></span></span>
</div>
<!-- the full panel follows, as usual -->
<script>
  document.getElementById('chip-ic').innerHTML = crundi.icon('cpu');
  crundi.onData(function (d) {
    var s = d.stats; if (!s) return;
    document.getElementById('chip-cpu').textContent = crundi.fmt.pct(s.cpu.pct);
    document.getElementById('chip-spark').innerHTML = crundi.chart.spark(s.history.cpu, { fill: false });
    var m = document.getElementById('chip-mem');
    m.className = 'c-chip-meter ' + (s.mem.pct > 85 ? 'err' : s.mem.pct > 70 ? 'warn' : 'ok');
    m.firstElementChild.style.width = s.mem.pct + '%';
  });
</script>
```

- It must be a direct child of the body, and there is one per panel.
- One line, about 24px tall. Text is 11.5px. Keep it to what reads at a glance: one or two numbers and at most one small graphic.
- Sized for it: `c-chip-spark` (a 44×14 box for `crundi.chart.spark`), `c-chip-meter` (`ok warn err`) with an inner `<span style="width:40%">`, `c-chip-label`, plus `c-dot` and `crundi.icon()`.
- The chip cannot be interacted with: a tap anywhere on it opens the panel. `crundi.isChip` is true in that copy if you need to skip heavy work there.
- No `data-chip` element means the labelled chip. A panel moved to the top bar by the person shows whichever it has.
- Check it with `widget_render` and `frames: ["chip"]`.

## Global widgets

`widget_scope` makes a widget global: shown in every project, pinned, and as a chip kept in the top bar across projects and chats. Know what moves and what does not before you do it.

**Moves: the widget's own folder, and only that.**
- `.crundi/widgets/<id>/` is copied into Crundi's own store (the new `sourceDir` in the result). The widget is built and fed from the copy from then on, and the copy is in Crundi's backup.
- The original folder is left in the project. It is no longer read. Edit the files in the new `sourceDir`.
- Any data the widget keeps in its own folder comes along. **Anything that writes that data (a script, a schedule, a background process) must be repointed to the new `sourceDir`**, or the widget quietly shows old values. Do it, and tell the person.

**Does not move: project data.**
- A source that reads a project file (`data/app.db`, `logs/build.log`) keeps reading it where it is. It is not copied, not in the backup, and the widget loses it if the project is moved or removed. That is right for live project data.
- Commands still run in the project it came from, and a `crundi` source still reads that project's board.

**So, for a widget meant to be global, keep its data in its own folder** and name it with a `./` path:

```json
"sources": { "state": { "kind": "file", "path": "./state.json" } }
```

`./` means "this widget's folder", in a project or global, so the manifest does not change when it is promoted. Data files there do not count as a change to the source: rewriting `./state.json` every second updates the widget without reloading it.

`widget_scope` with `global: false` sends it back to its project; the global folder is copied over the project's.

## State that has to survive (timers, toggles, anything running)

A panel is not one long-lived page. Assume all of this:

- **Several copies run at once.** The chip in the top bar, the panel that chip opens, and a workbench pane are separate frames of the same panel, each with its own script and variables.
- **Every copy reloads often**: when the person reloads Crundi, reopens the panel, or you save a change.
- **Nothing runs while Crundi is not open in a browser.** A panel cannot ring, tick or notify from a closed tab.

So keep state out of script variables:

- Store **absolute facts** in `crundi.store`, never a countdown: `crundi.store.set('endsAt', Date.now() + 25 * 60000)`, then derive what to draw from `Date.now()` on each tick. A copy that loads later picks up exactly where things are.
- `crundi.store.onChange(fn)` fires in every copy when any copy saves, so a button pressed in the opened panel updates the chip at once.
- Do one-off effects (a sound when a timer ends) in **one** copy. The chip is the copy that is always there: `if (crundi.isChip) ring();`. Record that it rang (`crundi.store.set('rangFor', endsAt)`) so a reload does not ring again.
- A copy that loads after the moment has passed should say so ("finished 4 min ago"), since it could not have rung.
- A global chip keeps running when the person switches project. A project chip is torn down with its project.
- To reach someone who has Crundi closed, a panel cannot help. Use a schedule or `send_message_to_user` from your side.

## Frames for widget_render

{{FRAMES}}

## A complete example: live task progress, no pushing needed

`widget.json`
```json
{ "title": "Task progress", "slot": "dock", "sources": { "task": { "kind": "session" } } }
```

`index.html`
```html
<div class="c-stack tight">
  <div class="c-row">
    <span class="c-dot" id="dot"></span>
    <span class="c-title c-truncate c-grow" id="now">Starting…</span>
    <span class="c-badge" id="count"></span>
  </div>
  <div class="c-progress" id="bar" hidden><span></span></div>
  <div class="c-steps" id="steps"></div>
</div>
<script>
  crundi.onData(function (d) {
    var t = d.task || {};
    var working = !!t.working;
    document.getElementById('dot').className = 'c-dot ' + (working ? 'accent pulse' : (t.state === 'needs-input' ? 'warn' : 'ok'));
    document.getElementById('now').textContent = working ? (t.now || 'Working…') : (t.state === 'needs-input' ? 'Waiting for you' : 'Idle');
    var c = t.counts || {};
    document.getElementById('count').textContent = (c.tools || 0) + ' steps' + (c.errors ? ' · ' + c.errors + ' failed' : '');
    var p = t.progress, bar = document.getElementById('bar');
    bar.hidden = !p;
    if (p) bar.firstElementChild.style.width = p.pct + '%';
    document.getElementById('steps').innerHTML = (t.todos || []).map(function (s) {
      var cls = s.status === 'completed' ? 'done' : (s.status === 'in_progress' ? 'active' : '');
      return '<div class="c-step ' + cls + '"><span class="c-step-mark">' + (cls === 'done' ? '✓' : '') + '</span><span>' + crundi.esc(cls === 'active' && s.active ? s.active : s.text) + '</span></div>';
    }).join('');
  });
</script>
```

`fixtures/working.json`
```json
{ "task": { "working": true, "state": "working", "now": "Bash: run the test suite", "counts": { "tools": 14, "errors": 1 },
  "progress": { "done": 2, "total": 4, "pct": 50 },
  "todos": [ { "text": "Read the failing test", "status": "completed" }, { "text": "Fix the parser", "status": "completed" },
             { "text": "Run the suite", "active": "Running the suite", "status": "in_progress" }, { "text": "Update the docs", "status": "pending" } ] } }
```
