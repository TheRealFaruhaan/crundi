# Kid games library

The games Cru plays with a child in kid mode. This file is the authoritative
description of the library's technical conventions; the baby-games skill
points here.

## Where things are

- Shipped with Crundi: `src/voice/kid-games/builtin/<id>.html`, listed in
  `src/voice/kid-games/catalog.json`, and `kit.js` (put in front of every game).
- The live library: `<dataDir>/kid-games/<id>/` with
  - `index.html` the game (what `start_game` loads)
  - `index.prev.html` the version before the last change (rollback)
  - `meta.json`
- `<dataDir>/kid-games/plays.jsonl` the play log, one line per game played.
- `<dataDir>/kid-games/CHANGES.md` what the overnight workshop changed.
- `<dataDir>/kid-games/skills.json`, `progress.jsonl` the child's levels and their daily history.
- `<dataDir>/kid-games/reflex.extra.json`, `trivia.extra.json` lines and questions the workshop added (checked).

Built-in games are copied into the library on first start, and refreshed from
Crundi when a newer built-in version ships and the library copy was never
changed by the workshop.

## meta.json

```json
{ "id": "find-letter", "title": "Find the letter", "skills": ["letters"],
  "ageRange": "3-4", "version": 2, "createdBy": "builtin|workshop",
  "createdAt": "ISO", "updatedAt": "ISO", "enabled": true,
  "options": "what start_game options it takes, in words",
  "validation": { "ok": true, "at": "ISO", "problems": [] } }
```

- `id`: lower-case letters, digits and dashes; the folder name.
- `skills`: 1-4 ids from the taxonomy below, main one first. Unknown ids are
  dropped when a game goes live.
- `ageRange`: years, e.g. `"2-3"`, `"3-4"`, `"4-5"`, `"5-7"` (the baby-games
  skill's bands; `"1-2"` for 12-24 months).
- `uses`: `["trivia"]` for a game that gets the trivia bank (`window.CRU_BANK`).

## The taxonomy (from `src/voice/kid-skills.js`, the source of truth)

Ids are stable: never renamed or reused; a new one is added to
`kid-skills.js` first, then here.

| Area id | Area | Skill ids (label) |
|---|---|---|
| `language` | Language and literacy | `letters` (Letter recognition), `letter-sounds` (Letter sounds (phonics)), `upper-lower` (Big and little letters), `vocabulary` (First words), `rhyme` (Rhymes and sound play), `blending` (Sounding out words) |
| `numbers` | Numbers and maths | `counting` (Counting), `numbers` (Number recognition), `more-less` (More and less), `adding` (Simple adding) |
| `shapes-colours` | Shapes and colours | `shapes` (Shapes), `colours` (Colours) |
| `memory` | Memory and attention | `memory` (Memory pairs), `focus` (Focus time) |
| `logic` | Problem solving and logic | `matching` (Matching), `sorting` (Sorting), `patterns` (Patterns), `thinking` (Thinking questions) |
| `motor` | Fine motor | `writing` (Tracing letters), `drawing` (Drawing) |
| `feelings` | Feelings | `feelings` (Naming feelings) |
| `creative` | Music and creativity | `music` (Music and rhythm) |
| `world` | Knowledge and the world | `world.animals` (Animals), `world.body` (The body), `world.everyday` (Everyday things), `world.nature` (Nature and weather), `world.places` (Places), `world.facts` (Fun facts) |

- `version`: bumped on every change; the server keeps `index.prev.html`.

## A game file

One self-contained HTML fragment (or page), at most 200 KB:

- No network at all. No `http:`/`https:` URLs (except the SVG namespace
  `http://www.w3.org/2000/svg`), no `fetch`, `XMLHttpRequest`, `WebSocket`,
  `EventSource`, `import`, `<script src>`, `<link href>`, `<iframe>`,
  `url(...)` other than `data:`, no `eval`/`new Function`, no storage APIs.
  It runs in `<iframe sandbox="allow-scripts">` under a CSP that blocks all of
  it anyway; the validator refuses such files before they go live.
- Art is inline SVG (or `kid.pic` / `kid.shape`); never emoji, never images
  from elsewhere.
- Big tap targets (at least 80 px, 96+ for the youngest), bright colours on
  the dark screen, very little text.
- Use the kit (`window.kid`, see `kit.js`): `kid.opts` (start_game options),
  `kid.say(text)`, `kid.correct(what, line?)`, `kid.wrong(what, line?)`,
  `kid.event(kind, detail)`, `kid.done(summary)`, `kid.tell(text)`,
  `kid.pic(word)`, `kid.shape(name, colour)`, `kid.letters()`, `kid.WORDS`,
  `kid.SOUNDS`, `kid.COLOURS`, `kid.shuffle/pick/sample/el/confetti`.

## What a game is given

Before the game's own code runs, the frame has:

- `window.CRU_OPTIONS` (`kid.opts`): the options `start_game` was given.
- `window.CRU_INIT` (`kid.init`): what the child knows, from the levels
  (`kid-skills.js initFor`):
  `{ type: 'init', levels: { skill: 1-5 }, mastery: { skill: { item: 0-1 } }, focus: { skill: [items being learnt] }, avoid: { skill: [items mastered] } }`.
  Pick the difficulty from `kid.level(skill)` and favour `focus` items
  (`kid.letters(null, skill)` does this for letters). Nothing of it is ever
  shown to the child: no scores, stars, timers or streaks on screen.
- `window.CRU_BANK` (games with `uses: ["trivia"]`).

## Messages to the voice screen

The frame posts to its parent (the kit does this for you):

| message | what happens |
|---|---|
| `{ type: 'game', event, item?, detail?, streak? }` | a game event (below): logged, an instant cheer line may play, and kid Cru may be woken |
| `{ cru: 'event', kind: 'result', skill, item, ok, ms }` | a question's first-try result (`kid.ask` then `kid.correct`/`kid.wrong`): the skill levels learn from it |
| `{ cru: 'say', text }` | said aloud at once, no turn (a letter, a word) |
| `{ cru: 'tell', text }` | kid Cru gets it as something tapped and answers (use sparingly) |

Game events (`event`): `start` (the kit sends it), `tap`, `correct`, `miss`,
`hint`, `level-up`, `round-done`, `finished` (`kid.done`), `idle` (no touch for
30 s, the kit), `stuck` (touching, nothing right for 20 s, the kit). `streak`
is the run of correct answers. The host adds `check-in`.

What happens with them: an instant line from the phrase bank (`reflex.json`,
slots `{name}`, `{item}`, `{sound}`, `{word}`, `{parent}`; never the same as the
last three for that event; at most one every 2.5 s, none while kid Cru talks)
plays the moment it comes; kid Cru (the model) is woken only for the events it
watches (`watch_game`; by default the start, every 4th correct answer, two
misses in a row on the same item, level-up, round-done, idle, stuck, the finish
and a check-in at most once a minute; the parent's chattiness changes this),
at most once every 7 s.

## Messages to the game

The host posts to the frame:

| message | what to do |
|---|---|
| `{ type: 'command', action }` | kid Cru's `game_command`: `next`, `easier`, `harder`, `hint`, `pause`, `resume`, `end`. Register with `kid.on(action, fn)`; the kit handles `pause`, `resume` and `end` itself when a game does not. |
| `{ type: 'command', action: 'progress', value, step }`, `{ type: 'command', action: 'ready' }` | the waiting scene only |
| `{ cru: 'volume', value: 0-1, muted }` | the kit's sounds follow it (Cru's volume; muted while a video plays) |

## Sounds, cursor, motion

The kit gives every game soft sounds made in the page (`kid.sound('tap' |
'yes' | 'miss' | 'done')`; taps on buttons play `tap` by themselves;
`kid.correct`, `kid.wrong` and `kid.done` play theirs), a big bright cursor
with a star over tappables (`button`, `.k-btn`, `[data-tap]`), a glow on hover,
a ripple where a finger lands, and less motion under
`prefers-reduced-motion`.

## Waiting scenes

Never a bare spinner in kid mode. `kid-games/waiting.html` (game id `_waiting`,
not in the list of games) is the scene for any wait: options `{ theme: 'egg' |
'blocks' | 'stars' | 'message', line, steps: [..], progress: 0-1 }`, moved on
by the `progress` command, `ready` for the ta-da; bubbles to pop meanwhile.
The server shows it while a game is made for the child (`make_game`) and while
the parent is asked (`ask_parent`), with an instant line every ~9 s.

## plays.jsonl

```json
{ "game": "find-letter", "startedAt": "ISO", "seconds": 140, "finished": true,
  "events": { "tap": 3, "correct": 8, "wrong": 2, "hint": 1, "level": 0, "finished": 1, "result": 8 },
  "wrongs": ["b", "d"], "options": { "letters": "BD" } }
```
