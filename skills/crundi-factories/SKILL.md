---
name: crundi-factories
description: Build a Crundi factory — a reusable AI team (a Manager plus employees with personas, wired as a flow of steps, decisions, parallel branches, review loops, waits) that takes a large feature or a new project from a change request to a merged result in its own git worktree. Use when the crundi MCP server is connected and the person asks to create, design, change or start a factory, a team, a pipeline of agents or a "workflow of Claudes", or to add a trigger that starts one (a schedule, a kanban column, a webhook).
---

# Crundi factories

The tools are `mcp__crundi__factory_*`. **Call `factory_guide` first**: it is the full reference (the JSON format, node and edge types, settings, what the validator enforces, shapes that work). This file is the judgement around it.

## Is a factory the right thing?

A factory pays off for large, multi-part work carried from start to finish: a new feature across UI and server, a new project, a migration with review. Each run costs several chats and many turns. For a bug fix, a small feature or a question, say so and suggest doing it in a normal chat instead.

## Building one

1. Find out how the person's team works, if they have not said: who plans, who builds, who reviews, what must be checked before it ships. Mirror that; do not invent extra roles. Three to six people is plenty.
2. Write personas the way you would brief a colleague: what they own, how they work, what they never do. Give each person the skills that fit their job (`skill_list` shows what this machine has) and narrow permissions: reviewers and researchers read only.
3. Every loop needs a round limit. Review loops go back with a required note ("What to change") and a yes/no decision marked as a review gate.
4. `factory_validate` until there are no errors; read the warnings. Then `factory_save`. Positions are laid out for you.
5. Tell the person it is saved and that they can open it in the Factories tab to adjust it. Copying a provided template and changing it is often quicker than starting from nothing (`factory_list`).

## Starting and triggers

Start a run (`factory_start`) or add a trigger (`factory_triggers`) only when asked. A run starts from a change request: a title plus the factory's CR sections (by default goal, current situation, expected result, done when, out of scope, constraints). Ask for what is missing rather than guessing: "done when" is what the reviewers and the Manager hold the work to.
