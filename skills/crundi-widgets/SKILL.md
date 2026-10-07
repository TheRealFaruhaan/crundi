---
name: crundi-widgets
description: Build live UI panels ("widgets") inside Crundi — a dashboard, table, chart, progress board or log view that you design in HTML, place beside the chat (docked, as a workbench pane, a full tab, an inline card or a top-bar chip), feed from pushed values, the chat's own activity, a file, a SQLite database, a command or a URL, and check yourself with screenshots on desktop and phone before the person sees it. Use when the crundi MCP server is connected and something would be better seen than read — showing progress on a long task, visualising data from a file or database, a build/test/service board, anything the person will glance at repeatedly — or when they say "panel", "widget", "dashboard", "show me live", "visualise", "chart this" or "keep an eye on".
---

# Crundi widgets

A widget is a small live UI you author and Crundi shows inside its own interface. It runs in a sandboxed frame with no network and no access to the page around it; data reaches it through sources the Crundi server reads.

The tools are `mcp__crundi__widget_*`. **Call `widget_guide` first**: it is the full reference (file layout, the `crundi` API in the frame, the design tokens and kit classes for this install, source kinds, slots, a complete example). This file is only the judgement around it.

## Decide whether to build one

Worth it:

- A task that runs for minutes, where the person wants to see where it is.
- Data they will look at more than once: a table from a database, a trend from a log, a board of services or tests.
- A choice they need to make from several options with detail.

Not worth it: a quick answer, one number, a list a sentence covers, a two-minute fix. A widget nobody needed is noise on a small screen.

If you open one without being asked, say so in one line, and close it (`widget_close`) when the work is done unless it is meant to stay (`lifecycle: "pinned"`).

## The loop, every time

1. `widget_open` with an id and a slot. It returns `sourceDir`.
2. Write `index.html` and `widget.json` there with your ordinary file tools. Saving reloads the widget.
3. `widget_render`. Read the report, then **look at each screenshot**. Lint finds overflow, clipped text, small tap targets and poor contrast; only you can tell whether it looks good. Include a phone frame: the person is often on one.
4. Fix and render again until it is clean and looks right. Then tell the person.

Do not report a widget as done without step 3.

## Picking the data source

- Showing your own progress: a `session` source. It follows this chat's todo list, running step and touched files with no pushing and no tokens. Keep your todo list current and the dashboard is good.
- A value only you know (a result you computed, a summary): `push`, updated with `widget_set_data`.
- Anything on disk: `file` or `sqlite`. These keep working after this chat ends, so a pinned dashboard stays live at no cost.
- A `command` or `http` source when nothing else can produce the value. The owner must approve these once on the widget; say that you have asked.

## Picking the slot

- `dock` for live progress on this chat's task. Keep it short: it shares the pane with the conversation.
- `cell` (with `beside`) for something used alongside the work.
- `tab` for dense dashboards and wide tables.
- `inline` for a one-off result that belongs at this point in the conversation.
- `chip` for a single status worth a glance from anywhere.

The person can move, collapse or close a widget. `widget_get` tells you if they did; leave it as they put it.

## Design, briefly

Use the tokens and the `c-` kit from the guide; do not hard-code colours. Design for a 380px-wide frame first. One clear thing at the top. Draw the empty and error states, not only the full one, and write a fixture for each so `widget_render` can show them. Escape any data you put into `innerHTML` with `crundi.esc`.
