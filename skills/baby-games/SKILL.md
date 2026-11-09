---
name: baby-games
description: Design and build learning games for babies, toddlers and young children (about 12 months to 7 years) — letters and phonics, counting, matching and memory, colours and shapes, animals and the world, fun facts and trivia — as one self-contained offline HTML file with big tap targets, no reading needed, a celebration for every success, a gentle wrong-answer ladder (errorless for the youngest), calm-but-bright colour palettes, warm short voice lines, a natural ending and no dark patterns, tagged with a stable skills taxonomy (meta.json `skills`). Age-banded rules for interaction, number of choices, target size, session length, praise and difficulty. Use for Crundi's nightly games workshop (kid-games library), for Cru's kid mode when it starts or writes a game on the live screen, and whenever anyone asks for a game, activity or quiz for a baby, toddler, preschooler or young child.
---

# Learning games for little children

You are building for someone who cannot read, has small imprecise hands, a short
attention span, and learns best when they succeed. Every rule below follows from
that. The research behind the rules is in `reference/research-notes.md`, the
sources in `reference/sources.md`, a ready skeleton in `reference/game-template.html`,
voice lines in `reference/voice-lines.md`, the event phrase bank in
`reference/phrase-bank.md`, paste-ready code in `reference/snippets.md`, and the
skills taxonomy in `reference/skills-taxonomy.md`.

Who reads this:
- **The games workshop** (headless Claude Code in `<dataDir>/kid-games`, nightly):
  improves existing games and adds one new basic learning game per night.
- **Cru in kid mode**: starts library games with `start_game`, or writes a quick game
  as `show_screen` html; it speaks to the child.
- **Any chat** asked for a kids' game.

## The rules that are never broken

1. **Pick one age band and one learning goal** before writing anything. A game
   without a goal is entertainment; a game for "2–7" serves nobody.
2. **No reading required.** Every instruction is spoken and shown (picture, pointing
   hand, glow). Text on screen is only the thing being learned (a letter, a numeral).
3. **Celebrate every success**, specifically ("You found the B!"), and vary it.
   Use the instant reflex lines for it (no thinking delay), picked at random from
   many variants so it never sounds the same.
4. **Never punish.** No buzzers, red crosses, sad or crying characters, lives,
   lost stars, "game over", or scores that go down. A miss leads to help, not failure.
5. **No time pressure** under 5, and none by default at any age.
6. **Big targets, simple gestures**: tap first; drag only from 4–5 and always with a
   tap-then-tap alternative.
7. **A natural ending**: a fixed number of rounds, a finish party, then stop. Never
   loop forever or auto-start the next game.
8. **Offline and private**: one HTML file, everything inline, no network, no links
   out, no ads, no purchases, no names or data collected.
9. **Safe to look at**: no more than 3 flashes a second, no large flashing red,
   gentle motion, and honour `prefers-reduced-motion`.
10. **Easy is the point.** Aim for the child getting about 4 in 5 right on the first
    try. Step up slowly, step down fast.
11. **Every interaction gets sight AND sound.** Every tap: a ripple/pop + a soft tap
    sound. Every right answer: a celebration animation + a happy sound + a voice line.
    Every miss: a gentle visual nudge (wobble, glow toward the right one) + a soft,
    non-harsh sound + an encouraging line. Progress and the finish: visual + sound
    fanfare. Sounds are made in the page (Web Audio), short, soft, and muted when the
    host says so; motion gives way to colour/scale changes under reduced motion.

## The age bands

| | **12–24 months** | **2–3 years** | **3–4 years** | **4–5 years** | **5–7 years** |
|---|---|---|---|---|---|
| Screen advice | Avoid under 18 mo (AAP); 18–24 mo only with a grown-up playing too | ≤1 h/day total, high quality, ideally with a grown-up | same | same | consistent limits, other activities first |
| What to build | cause and effect: tap anything, something lovely happens and is named | find-the-one (2 choices), naming pictures, colour match, first counting 1–3 | colours, shapes, counting to 5, letters in own name, 3-pair matching | letter sounds, upper/lower match, counting to 10, numerals 1–10, patterns, memory | beginning sounds, blending CVC words, counting to 20, adding within 5–10, tracing |
| Interactions | tap anywhere (whole screen is the target) | single tap | single tap | tap; tap-then-tap move; gentle drag with big snap | tap, drag, simple trace |
| Choices per question | 1 (or 2 where both are fine) | 2 | 2–3 | 3–4 | 3–6 |
| Min target | ≥ 1/4 of the screen | ≥ 140 px | ≥ 120 px | ≥ 100 px | ≥ 80 px (2 cm absolute floor) |
| Gap between targets | n/a | ≥ 40 px | ≥ 32 px | ≥ 24 px | ≥ 20 px |
| Rounds / session | 5–8 taps, 1–3 min | 4–5 rounds, 3–5 min | 5–6 rounds, ~5 min | 6–8 rounds, 5–8 min | 8–10 rounds, 8–12 min |
| Praise style | label + delight: "Ball! Bouncy ball!" | label + "You did it!" | specific: "Yes, that's red!" | process: "You listened for the sound!" | process + strategy: "You counted on from 5 — clever thinking!" |
| Feedback sound | tap sound + the thing's own sound/name on every tap | tap + success; a miss gets only the soft tap sound | tap, success, gentle miss | same | same; fanfare on level-up too |
| Wrong answers | none exist (errorless) | none shown: wrong choice names itself, answer glows | try again → hint → narrow → show | same, hint is about the skill | same; a soft neutral sound is fine |
| Difficulty rule | none; just variety | fixed easy | up after 4 first-try rights in a row; down after 1 helped round | up after 3 in a row; down after 2 helped rounds | up after 3 in a row; down after 2 helped rounds |
| Palette | Bold (dark) | Bright | Bright | Bright or Calm | Calm |

A child's age is a starting point, not a cage: Cru knows the age from the parent's
instructions (`- Age:` in kid-mode.md) and should pick the band below if unsure.
Ranges in `meta.json` should be honest (e.g. `"2-4"`), not "0-99".

## Celebrate every success

A celebration is **sight + sound + words**, in that order of speed: the tapped thing
reacts instantly (< 100 ms: grow, wiggle, sparkle), a happy sound plays, then a short
voice line names what was achieved.

| Band | Small success (each right answer) | Finish party |
|---|---|---|
| 12–24 m | every tap: the object bounces and says its name/sound ("Moo!") | none needed; fade gently to a "bye-bye" wave |
| 2–3 | thing grows + a soft chime + "Yes! Ball!" | 2–3 s: balloons or bubbles float up, "You found them all!" |
| 3–4 | sparkle burst + chime + specific line | 3 s: a character dances, stars fill a row, summary line |
| 4–5 | sparkle + rising two-note chime + process praise | 3 s party + what they learned: "You know the sounds s, a and t!" |
| 5–7 | quick sparkle + chime; save words for streaks and hard ones | short party + effort summary and one "next time" invitation |

Rules:
- **Vary it.** Rotate at least 4 celebration visuals (sparkles, stars, bubbles,
  balloons, a dancing character, a rainbow sweep) and many voice lines; never the
  same line twice in a row. `reference/voice-lines.md` has a bank.
- **Specific beats generic.** "You found the B! B, ball!" teaches; "Great job!" alone
  does not. Name the thing, the sound, the number.
- **Praise the doing, not the child.** "You kept trying!", "You counted every one!"
  — not "You're so smart". Process praise to toddlers predicts later persistence
  (Gunderson et al. 2013).
- **Short.** 1–2 s for a round, ≤ 3 s for the finish. A long celebration becomes a
  wait. Let a tap skip it.
- **Never for a wrong answer** — do not say "Great job!" to a miss; that confuses
  the lesson. Say "Good try!" (effort) and help. A helped answer still gets a warm,
  smaller celebration ("You found it!").
- **No reward loops.** No random prizes, collect-them-all, streak counters that
  break, or "come back tomorrow". Stars shown on screen are a progress row for this
  game only and never go down.

## The wrong-answer ladder

| Band | 1st miss | 2nd miss | 3rd miss |
|---|---|---|---|
| 12–24 m | there is no miss: every target is right, or a non-target just says its own name happily and the target glows | — | — |
| 2–3 | the tapped one names itself kindly ("That's the duck!") and the right one starts to glow | Cru points: "Here's the ball!" (bounce, hand pointer) — the child taps it, celebrate | — |
| 3–4 | "Good try! Look for the… red one." + glow on the answer after ~2 s | one wrong choice gently fades away (narrow) | show it: "It's this one! Tap it!" → smaller celebration |
| 4–5 | "Hmm, not that one. Listen: sss… sun." (the hint teaches the skill) | narrow to two | show and tap |
| 5–7 | "Try again!" (+ optional soft "hmm" tone) | hint about the strategy ("Count them one by one") | narrow, then show |

- The wrong choice reacts softly (a small wobble or dim), never red, never a buzzer,
  never a big shake. For the two youngest bands, no "wrong" reaction at all.
- A helped round still moves on. It counts as "helped" for difficulty, not as failure.
- Do not remove the child's control: after a hint, let them do the final tap.
- Silence (no tap for ~8–10 s) is not a miss: replay the question, then glow the
  answer. For 12–24 m, idle just means a gentle invitation animation.
- In `kid` terms: for 12 m–3 y log a miss with `kid.event('tap', …)` (or `'wrong'`
  silently) and speak your own gentle line; do **not** call `kid.wrong()`, whose
  default line says "Oops". From 3 up, `kid.wrong(what, line)` with your own line.

## Difficulty and session shape

- Start every session **one step below** where the child probably is. The first
  round should be almost impossible to miss.
- **Up slowly**: only after a run of first-try successes (see table). One step at a
  time: one more choice, or one more item, never both at once.
- **Down fast**: after a helped round (3–4) or two (4–7), drop a step without comment.
- Levels live in the session only (the frame has no storage). `kid.opts.level` or
  similar lets Cru start a child where they were last time.
- **Shape**: warm-up (easy) → practice → a last round that is easy again → finish
  party → stop. End on a success.
- **Ending**: say what was learned, then `kid.done(summary)`. Cru offers "again, or
  something new?". Never auto-restart. If kid mode has been on a long time, Cru (not
  the game) suggests a break: "Time for a stretch! Can you jump like a frog?"

## Layout for small hands and pre-readers

- **Landscape, one screen, no scrolling.** Size with `vmin`/`min()` so it fits the
  frame it is in (Cru's live screen, a phone, a big PC screen).
- **Keep targets off the bottom ~10%** of the screen (wrists rest there) and away
  from the edges.
- **Same place, same meaning**: question at top, choices in the middle, nothing
  else. Choices do not move while the child is choosing.
- **Minimal distraction**: plain background, one focus at a time, no decorative
  things that look tappable. Everything that looks tappable does something.
- **Tap is the main gesture.** Use `pointerdown`/`click`; works by finger and mouse.
  No double-tap, long-press, pinch, multi-finger, hover-only or precise drags.
  Ignore extra touches while a celebration is playing (debounce ~300 ms).
- **Big, clear art**: simple flat shapes, thick outlines, one object per picture,
  real-world things the child knows (ball, apple, dog). Use `kid.pic(word)` and
  `kid.shape()` or inline SVG.
- **Instructions**: spoken, short, repeated on demand (a big "ear"/speaker button
  replays the question), and shown (pointing hand, glow). Text only as a label for
  grown-ups, small and secondary.
- **Gentle motion**: ease-out, 150–600 ms, small bounces. No spinning, zooming,
  fast parallax or screen shake.
- **A cursor a child can see** (mouse/PC): never the tiny system arrow. A large
  (48–64 px), bright, friendly cursor (big arrow, star or hand) with a dark outline so
  it shows on light and dark backgrounds, switching to a pointing-hand variant over
  anything tappable, plus a soft glow on the hovered target. On touch, a brief
  colourful ripple where the finger lands instead. Snippet in the technical rules.

## Colour

Bold primaries and high contrast for the youngest; calmer, softer sets as children
grow. Never rely on colour alone to carry meaning (except in a colour-naming game,
which is the one place colour *is* the content) — pair every colour with a shape,
picture or position. About 1 in 12 boys has red-green colour blindness; a colour
game they cannot do is not their failure, so it never scolds.

All sets below were checked: contrast against their background (WCAG ratio) and
the smallest colour difference between any two colours of the set under simulated
deuteranopia, protanopia and tritanopia (Machado 2009, CIELAB ΔE; >15 is clearly
distinct).

**Bold** (12 m–3 y, and on Cru's dark screen) — background `#0B1020`
| `#FFFFFF` white 18.9 | `#FFD400` yellow 13.2 | `#FF7A00` orange 7.2 | `#2F9BFF` blue 6.5 | `#FF5FA8` pink 6.7 | `#2EE58A` green 11.4 |
Min ΔE: deutan 21, protan 33, tritan 13 (orange/pink: do not pair them as the only cue).

**Bright** (2–5 y) — background `#FFF8E7` (warm cream), ink `#1F2937` (13.9)
| `#D55E00` vermilion 3.7 | `#0072B2` blue 4.9 | `#009E73` green 3.2 | `#F0C808` yellow 1.5 | `#CC79A7` pink 2.9 |
Okabe-Ito based. Min ΔE: deutan 18, protan 23, tritan 17. Yellow and pink are
below 3:1 on cream, so give every shape a 3–4 px ink outline (do that for all of them).

**Calm** (4–7 y) — background `#F5F1E8`, ink `#2E3A59` (10.0)
| `#3B7EA1` blue 4.0 | `#C8742E` orange 3.1 | `#2F7D5B` green 4.4 | `#E2B54A` mustard 1.7 | `#E58FB0` rose 2.1 |
Min ΔE: deutan 19, protan 18, tritan 8 (blue/green: avoid as the only cue). Outline
mustard and rose.

**Colour-naming games** need the colour to look like its name. Use: red `#E53935`,
orange `#FB8C00`, yellow `#FDD835`, green `#43A047`, blue `#1E88E5`, purple
`#8E24AA`, pink `#F06292`, brown `#8D6E63`, black `#212121`, white `#FAFAFA`; on a
neutral grey `#ECEFF1` or the dark background, each with an outline. Ask about
colours that look different to colour-blind eyes first (blue/yellow before red/green).

Text and numerals: ink on the background (≥ 7:1), heavy rounded font, ≥ 64 px for a
learning letter or numeral. The kit's existing `kid.BRIGHT`/`kid.COLOURS` are fine
for decoration; prefer the sets above for anything that means something.

## Sound and voice

- **Speech is the main channel**: `kid.say(text)` speaks at once. Keep lines to one
  short sentence (≤ 10–12 words for under-4s), one idea, warm and excited but not
  shouty. Leave a beat after a question; do not talk over the child's tap.
- **Say the thing being learned clearly, twice if useful**: "B. B, ball!"
- **`kid.tell()` and `kid.done()` start a turn for Cru** (it answers out loud, and
  that takes a moment). Use them for milestones: the finish, a level-up, a child's
  free choice worth chatting about. Never on every tap; use `kid.say` and
  `kid.event` for those.
- **Sound effects** are made in the page with Web Audio (no files, no network): short
  soft sine/triangle tones, major intervals (C–E–G), 300–1200 Hz, 80–450 ms, peak gain
  ≤ 0.15, fast fade in and out. Never a buzzer, a low "wah-wah", noise, or anything
  sudden or loud. Effects sit under speech, not on top of it. The AudioContext starts
  on the first tap (browsers block sound before one).
- **Mute and volume belong to the host.** Honour `kid.opts.sound === false` /
  `kid.opts.volume` (0–1), and a `{cru:'mute', muted}` / `{cru:'volume', value}`
  message from the parent — the library README defines the final mechanism; if the
  kit gains its own sound helper (`kid.sfx`), use that instead. Never put a mute button
  where the child taps.

Use the ready-to-paste helper in `reference/snippets.md` (`sfx.tap()`, `sfx.success()`,
`sfx.miss()`, `sfx.step()` for progress, `sfx.fanfare()` for the finish; also in the template).
`sfx.miss()` is a soft rising "hmm?", not a down-tone: it invites another go. For
12 m–3 y use `sfx.tap()` on a miss, not `sfx.miss()`.
- **Pair every sound with a visual** (and every visual with a sound): tap → ripple or
  pop; success → sparkle/grow; miss → wobble + glow on the answer; progress → a star
  added; finish → party. Under `prefers-reduced-motion`, swap motion for colour,
  outline or a small scale change — the sound stays.
- **Language**: "you" and the child's actions; no sarcasm, no "wrong", no "no!",
  no "hurry", no guilt ("I'll be sad if you stop"). No "good girl/boy". Use the
  child's name at most at the start and the finish.

Good lines: "You found the B! B says b-b-ball!" · "Three ducks! One, two, three!" ·
"Good try! Let's listen again: mmm… moon." · "You kept going — you found them all!"

## Content rules

**Letters and phonics**
- Sounds before (or with) names: "This is S. S says sss, like sun."
- **Lowercase emphasis** from 4 up (that is what books are made of); uppercase for the
  very first letters and the child's name. Show upper and lower together when matching.
- **Keep confusable letters apart**: never b/d/p/q together in one question until 5+,
  and introduce them in different sessions. Likewise m/w, n/u, i/l early on.
- **Order**: the child's name letters, then a common phonics opener (s a t p i n m d…),
  not A to Z. Continuous sounds (s, m, f, l, n, r, v, z) are easiest to hear and say.
- **Pure sounds**: stretch continuous ones ("sss", "mmm"); for stop sounds (b, d, t,
  p, k, g) keep the vowel tiny and anchor with a word: "b… b… ball". The kit's
  `kid.SOUNDS` uses "buh/duh/tuh"; pass better lines rather than relying on it.
- **Pictures for words** must start with the plain sound: avoid "igloo" style
  edge cases for i, "box" for X, "queen" for Q (kw), "orange" for short o, and
  silent letters or blends ("knife", "ship" for s) in beginning-sound games.
- Tracing (5+): big letter, start dot, arrow, generous tolerance; any reasonable
  attempt passes.

**Counting and number**
- Subitise first: 1–3 for 2–3-year-olds, up to 5 for 4–5; arrange small sets in
  familiar patterns (dice, lines) before scattering them.
- Teach one-to-one: tap each object, it lights and the voice counts aloud; the last
  number is the answer ("Three! There are three ducks.").
- Same object, same size in a set (only number changes). No answer choice larger than
  the band's limit (3, 5, 10, 20).
- Numerals with quantities: show "3" next to three dots. Zero comes late.

**Matching and memory**
- Toddlers: matching **face up** (find the same one), 2 then 3 pairs.
- Hidden-card memory from about 3–4: 2–3 pairs; 4–5: 3–4 pairs; 5–7: 4–6 (8 max).
- Matched pairs stay visible and celebrated; unmatched cards turn back slowly
  (≥ 1.2 s) so the child can look. No move counter.

**Colours and shapes**
- Recognition (point to it) comes long before naming. Ask "Where is red?" not "What
  colour is this?" until about 3½–4.
- Start with circle, square, triangle; star and heart are fun extras; avoid
  oval/rectangle/diamond distinctions under 4.

**Knowing the world: information and trivia**
- 12 m–3 y: picture-and-sound naming. "Where is the cow? The cow says moo!" One big
  picture or two; the answer is spoken whatever is tapped.
- 3–4: "which one…" picture questions with 2–3 pictures: "Which one lives in the sea?"
  "Which one do we wear on our feet?"
- 4–7: simple why/how facts and picture trivia quizzes (3–4 picture answers, spoken):
  "Which animal is the tallest?" → "The giraffe! Its neck is as long as a grown-up is tall!"
- Facts must be **true** (check them; no myths like "goldfish forget in 3 seconds"),
  simple, and kid-safe: no scary or graphic content (predators eating, death, injury,
  disasters), nothing contested (religion, politics, diets), nothing frightening about
  space, weather or the body.
- **One fact at a time**, said aloud with a picture, ≤ 2 short sentences.
- Trivia never punishes: every question ends by revealing the answer with a fun fact
  ("It's the blue whale — the biggest animal ever!"), whether the child got it or not.
  A right guess gets the celebration; a miss gets "Good guess! Here's the answer…".
- Cru may share a fun fact in conversation too, the same way: one, true, with a picture
  on the screen if it can.

**Themes**: animals, food, vehicles, nature, family, music. Nothing scary, no
violence, no brands or characters you don't own, people shown kindly and diversely.

## Talking with the child

Games are not the only thing: a child will want to just talk. Kid Cru is a friend who
listens: it answers, asks them back (their day, favourite things, what they made),
answers "why" questions simply and truthfully, tells short stories where the child
picks what happens next (with simple pictures), and plays talk games (I spy, animal
sounds, rhymes). Children speak slowly, pause, and say half-words: wait longer before
answering, be forgiving, never correct pronunciation bluntly (say the word back
right, warmly: "A hippo! Yes, a hippopotamus!"), and keep turns short so the child
gets the next one. If a child says something worrying, answer kindly, encourage them
to tell a grown-up they trust, and let the parent know.

## Waiting is part of the game

Children don't wait well, so no wait is ever a bare spinner. Loading a game or a video,
waiting for a grown-up's approval, and above all a game being built live because the
child asked for it ("a dinosaur game!"): show a small animated scene themed on what is
coming (an egg that wobbles and cracks as the build progresses, blocks stacking up),
progress as fun named steps ("Drawing the dinosaurs… adding roars… hiding the eggs…")
driven by real progress, a friendly line every 8–10 seconds so there is no long
silence, and a tiny no-pressure toy to play with meanwhile (pop the bubbles, tap the
stars, with sight and sound). Finish with a "ta-da!" reveal. If it can't be done, offer
the closest thing that exists, cheerfully. The library's waiting-screen pieces are
reusable; see its README.

## The phrase bank (reflex voice lines)

**Prefer the instant lines.** A child lives in the moment: a cheer has to land the
instant the B is tapped, not two seconds later. Pre-written lines play with no delay
(their audio is cached); anything Cru has to think up arrives a beat or two late. So
cover every routine moment (each tap result, hints, level-ups, the finish) with
reflex lines, and let Cru add its own, more personal words on top: at the start, after
a few right answers or a streak, when the child misses the same thing twice, at level-ups
and the finish, in a light check-in now and then, and whenever the child talks to it
(the child can talk to Cru at any time during a game). When you build a game, write
its per-game lines rather than relying on Cru to react.

**Make them random, never the same.** Give every event many variants (at least 8–12
for `correct` and `miss`, several for the rest), mix their shapes (exclamation,
question, a sound, a name, a little joke), and fill the slots so they feel personal.
The host picks at random and never repeats a line twice in a row; add more variants
rather than longer ones, and let the nightly workshop keep adding fresh ones.

Events trigger instant pre-written lines (no LLM). Writing them is part of making a
game good: `reference/phrase-bank.md` has the rules, the slot list and a starter bank.
In short: per event (`correct`, `miss`, `hint`, `level-up`, `round-done`, `finished`,
`idle`, `stuck`) and per age band, many short varied lines with slots `{item}`,
`{name}`, `{sound}`, `{count}`; specific praise ("You found the B!"), effort praise for
misses, never a negative word, "good job" at most once per list, ≤ 8 words for
toddlers, letter sounds written as phonemes ("sss", "b, b, ball"). A game with its
own vocabulary adds per-game lines under its id. The library README defines the file
and format; until it does, use the format in the reference.

## Skills taxonomy (meta.json `skills`)

Games tag themselves with skill ids; the parent progress view groups them by area and
the library tracks a level per skill. The ids are **the ones in
`src/voice/kid-skills.js` `TAXONOMY`** — the library silently drops unknown ids, so
never invent one. Labels, bands, the ELOF/EYFS mapping and proposed future ids:
`reference/skills-taxonomy.md`.

| Area | ids |
|---|---|
| `language` Language and literacy | `vocabulary` `letters` `letter-sounds` `upper-lower` |
| `numbers` Numbers and maths | `counting` `numbers` `more-less` `adding` |
| `shapes-colours` Shapes and colours | `shapes` `colours` |
| `memory` Memory and attention | `memory` `focus` |
| `logic` Problem solving and logic | `matching` `sorting` `patterns` `thinking` |
| `motor` Fine motor | `writing` `drawing` |
| `world` Knowledge and the world | `world.animals` `world.body` `world.everyday` `world.nature` `world.places` `world.facts` |

## The library (technical rules)

The **kid-games library's README is authoritative**; this is how it works today
(`src/voice/kid-games/`):

- Layout: `<dataDir>/kid-games/<id>/index.html` (the game) + `meta.json`;
  `index.prev.html` is kept for rollback; `CHANGES.md` records what the workshop did;
  workshop trivia goes in `trivia.extra.json`. Built-ins live in
  `src/voice/kid-games/builtin/` with `catalog.json`.
- A game is one HTML fragment or page, ≤ 200 KB. The **validator refuses** before it
  goes live: any `http(s)://` URL except the SVG namespace, `fetch`/XHR/WebSocket/
  EventSource/workers/`import`, `<script src>`, `<link href>`, `<iframe|object|embed|
  base|form|meta>` (**no `<meta>` tags**), `url()` other than `data:`, `@import`,
  `eval`/`new Function`, storage APIs, navigation (`location=`, `window.open`), and
  scripts that do not parse. Art is inline SVG or `kid.pic`/`kid.shape` — never emoji.
- The host wraps it with a strict CSP (no network at all; images/sound only `data:`/`blob:`; no fonts from
  outside; no frames, forms or links that go anywhere) inside a sandboxed frame
  without same-origin, so **no localStorage/cookies** (they throw — wrap in try).
- `kit.js` is injected first and gives `window.kid`:
  `kid.opts` (options from `start_game`; also `window.CRU_OPTIONS`), `window.CRU_INIT`
  (what the child knows: levels, mastery, focus, avoid per skill — start one step
  below), `window.CRU_BANK` (the trivia bank, for games whose meta has
  `"uses": ["trivia"]`), `kid.say(text)`, `kid.tell(text)`,
  `kid.event(kind, detail)` with kind `tap | correct | wrong | level | finished`,
  `kid.correct(what, line)`, `kid.wrong(what, line)`, `kid.done(summary)`,
  `kid.pic(word, px)`, `kid.shape(name, colour, px)`, `kid.confetti()`,
  `kid.letters(default)`, `kid.shuffle/pick/sample/el`, `kid.LETTERS/WORDS/SOUNDS/COLOURS`,
  and classes `.k-title .k-grid .k-btn .k-big .k-pop .k-row .k-stars`.
- Messages to the host are `postMessage` objects:
  `{cru:'say', text}` · `{cru:'tell', text}` (Cru hears `[Kid tapped: …]`) ·
  `{cru:'event', kind, detail}`. `kid.done` sends `tell('[Game: finished] …')`.
- **Game events → host** (kid Cru hooks itself to these). Post at once, faithfully,
  one per happening, from the parent-facing bridge (`kid.game(msg)` if the kit has it,
  else `window.parent.postMessage(msg, '*')`; keep `kid.event` for the play log too):
  `{type:'game', event, item, detail, streak?, seconds?}` — `item` is the thing asked
  or tapped (`'B'`, `'red'`, `'3'`), `streak` first-try rights in a row, `seconds`
  since the round began (the whole game on `finished`).

  | event | fires when |
  |---|---|
  | `start` | the game is drawn and ready (once) |
  | `tap` | any tap on something interactive |
  | `correct` | a right answer (`detail`: `first try` or `helped`) |
  | `miss` | a wrong choice (`item` tapped, `detail` what was wanted) |
  | `hint` | the game helped: `detail` `glow` / `narrow` / `show` |
  | `level-up` | difficulty changed (`detail` `down` on a step down; never said to the child) |
  | `round-done` | a round ended (`detail` `n/total`) |
  | `finished` | the game ended (once; `detail` the summary) |
  | `idle` | no touch at all for ~30 s (once per quiet spell) |
  | `stuck` | touching but no progress for ~20 s (once per round) |

- **Host → game commands**: `{type:'command', action}`; accept only from
  `window.parent`, ignore unknown actions, never throw. `next` (skip to the next round,
  no penalty) · `easier` / `harder` (one step; easier redoes the round now, harder from
  the next) · `hint` (next rung of the ladder) · `pause` (freeze timers, sounds and taps;
  dim gently) · `resume` · `end` (go to the finish party and send `finished`) · `mute` /
  `unmute` (the game's own sound effects).
- **Voice vs the game's own feedback**: the game's instant visual + sound feedback
  always plays (< 100 ms). Kid mode speaks reflex lines for events from the phrase
  bank, and Cru may add more on top; where the host speaks event lines (the README
  says how a game knows), the game speaks only the *content* (the question, the
  letter, the count) and leaves praise to the host, so the child never hears two
  voices. Standalone, the game speaks its own praise.
- **Cru's own `show_screen` html** has no kit: only `window.cru.say/tell`. The template's
  shim covers that case too (it posts the same messages).
- **Workshop games**: write them so they work both inside
  the host (kit present) and opened alone in a browser: start from
  `reference/game-template.html`, which uses `window.kid` when present and a tiny shim
  (same message shapes, `speechSynthesis` when standalone) when not.
- `meta.json` (the workshop writes `title`, `skills`, `ageRange`, `options`, `uses`; the
  server fills `id`, `version`, dates, `enabled`, `validation`): `id` = folder name,
  lowercase kebab (parents approve games by it), `title` (≤ 60 chars, what a child or
  parent would call it), `skills` (1–4 ids from the taxonomy above, main one first),
  `ageRange` (years, e.g. `"2-4"`),
  `version` (bump on every change), and `options` (one line describing what `start_game`
  options it accepts, e.g. `letters: "SAT", rounds: 1-10, level: 1-3`). Add other
  fields only as the README defines them.
- Read every option defensively, clamp it, and have a good default: the game must
  run with `{}`.
- **Reduced motion**: add
  `@media (prefers-reduced-motion: reduce){.k-conf{display:none}.k-pop,.k-shake{animation:none}*{transition-duration:.01ms!important}}`
  and replace movement with a fade or a colour/outline change. The kit does not do
  this for you yet.
- **Cursor and touch ripple** in every game: a 56 px yellow outlined arrow, a 64 px pink
  star (hotspot centred) over tappables (`button`, `.k-btn`, `[data-tap]`), a glow on
  hover only under `(hover:hover)`, and a colourful ripple where a finger lands.
  Ready-to-paste CSS + JS in `reference/snippets.md`. Cursor images ≤ 64 px (browsers
  ignore > 128; Chromium falls back to `auto` near the frame edge). Inline images as
  `url(data:…)` (quoted or not; %-encode SVG).
- **Flashing**: ≤ 3 flashes per second, no full-screen colour flashes, never flash
  saturated red. Confetti falling is fine; strobing is not.
- No external URLs anywhere, no `<a href>`, no `window.open`, no `eval` of remote
  text, no `alert/confirm/prompt`, no fullscreen or pointer-lock requests, no
  microphone/camera.
- Never throw: wrap handlers so a bug shows a calm screen, not a blank one. Keep it
  small (aim < 60 KB) and smooth on a modest PC.
- Improving an existing game: keep its `id`, keep options compatible, bump
  `version`, and change one thing well (a clearer hint, a better celebration, a
  bug) rather than rewriting it.

## Before you ship (check every item)

- [ ] One age band, one learning goal, written in a comment at the top and in `meta.json`.
- [ ] Works with no options; every option clamped.
- [ ] Playable without reading; every instruction is spoken; a replay button exists.
- [ ] Targets meet the band's size and gap; nothing tappable in the bottom 10%.
- [ ] Tap works with finger and mouse; no drag-only, double-tap or long-press.
- [ ] Big bright outlined cursor (star over tappables), hover glow; ripple on touch.
- [ ] `skills` uses only ids from `kid-skills.js` (see the taxonomy), main one first.
- [ ] Every tap, right answer, miss, progress step and the finish has BOTH a visual
      and a sound (`sfx.*`); sounds soft, made in the page, muted when the host says.
- [ ] Posts every `{type:'game'}` event at the right moment (start, tap, correct, miss,
      hint, level-up, round-done, finished, idle ~30 s, stuck ~20 s) and handles every
      `{type:'command'}` action (next, easier, harder, hint, pause, resume, end, mute, unmute).
- [ ] New vocabulary has phrase-bank lines (≥ 8 per event, varied, specific, no negatives).
- [ ] Passes the library validator: no `<meta>`, no URLs, no storage, scripts parse.
- [ ] Every fact checked true, simple, kid-safe; trivia always reveals the answer.
- [ ] Every success: instant visual + sound + specific line, varied.
- [ ] Every miss follows the band's ladder; no buzzer, red X, lives, timer, game over.
- [ ] First round is very easy; difficulty steps up slowly and down fast.
- [ ] Fixed rounds, finish party, `kid.done(summary)`, then it stops.
- [ ] Colours from a checked set; meaning never by colour alone (except colour games).
- [ ] Confusable letters not together for under-5s; letter sounds said correctly.
- [ ] No network, links, storage, ads, purchases, personal data, external assets.
- [ ] ≤ 3 flashes/s, no red flashes; `prefers-reduced-motion` honoured.
- [ ] Sounds soft and short; nothing sudden or loud.
- [ ] Opened standalone in a browser: it runs, no console errors. Resized from phone
      to big screen: it fits without scrolling.
- [ ] `kid.tell` only at milestones, never per tap.

## One example per band (outlines)

**12–24 m — "Peekaboo Farm"** (`world.animals`, `vocabulary`; `"1-2"`). Bold palette,
dark background. Five big barn doors fill the screen; any tap opens one with a slow
ease, an animal pops up, bounces and says "Cow! Moo!" Every door is right. After all
five, the animals wave together: "Bye-bye, animals!" and it ends. Meant for a grown-up's
lap; no questions, no choices.

**2–3 — "Where's the Ball?"** (`vocabulary`, `world.everyday`; `"2-3"`). Two big pictures side by
side (ball, cup). "Where's the ball?" Tap ball: it bounces, chime, "Yes! Ball!" Tap
cup: "That's the cup!" and the ball glows; next tap on ball celebrates. Five rounds
of familiar objects, then bubbles and "You found them all!"

**3–4 — "Duck Count"** (`counting`, `numbers`; `"3-4"`; options `max: 3-5`). Ducks on a
pond in a line. Tap each duck: it quacks and the voice counts. Then numerals 1…max as
big buttons with dots under each. Start with max 3. Miss: "Let's count together!" and
ducks light one by one; second miss: the right button glows. Up to max 5 after four
first-try rights. Six rounds, ducks parade, "You counted all the ducks!"

**4–5 — "Sound Safari"** (`letter-sounds`, `vocabulary`; `"4-5"`; options `letters: "SATPIN"`).
Lowercase letter at top, "s says sss". Three animal/food pictures; "Which one starts
with sss?" Pictures exclude confusable letters and tricky words. Miss: "Listen: sss…
sun" (hint), then one picture fades, then show. Celebrate: "Sun! Sss, sun!" Eight
rounds, end with "You know s, a and t!"

**4–7 — "Animal Wonders" trivia** (`world.facts`, `world.animals`; `"4-7"`; `"uses": ["trivia"]`;
options `rounds: 5-10`, `topic: "animals"|"body"|"nature"|"places"`). One spoken
question with 3 big picture answers (4 for 5–7): "Which animal is the tallest?"
(giraffe, cat, duck). Any tap reveals: right → celebration + fact ("Yes, the giraffe!
Its neck is as long as a grown-up is tall!"); miss → "Good guess! It's the giraffe…"
+ the same fact, picture grows. No score shown except a row of collected animal
stickers (one per question, whatever the answer). Questions come from the vetted trivia bank
(`window.CRU_BANK`) or a checked list inside the file, never generated at play time. Eight questions, then a sticker parade
and `kid.done('8 animal facts, 6 guessed first time')`. For 3–4, the same game with
2 pictures and "which one…" questions ("Which one lives in the sea?").

**5–7 — "Word Builder"** (`letter-sounds`, `letters`; `"5-7"`; options `words: ["cat","sun"]`). A
picture (cat) and three empty boxes; letter tiles below (the three letters + one
distractor, never b/d together). Tap a tile then a box (or drag). Each letter sounds as
placed; full word blends "c-a-t, cat!" Wrong tile slides back softly with "Hmm, listen:
c-a-t". After two misses, the next letter glows. Ten words, finish with a parade of
the pictures built and "You built ten words — you sounded them all out!"
