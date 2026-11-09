# Skills taxonomy: what games tag themselves with

Games list skill ids in `meta.json` `skills` (main one first, 1–4 ids). The parent
progress view groups them by **area**, Cru uses them to pick a game ("let's do some
counting"), and the library measures a level per skill from first-try answers.

**The code is authoritative**: `TAXONOMY` in `src/voice/kid-skills.js` (library README
in `src/voice/kid-games/README.md`). The library **drops any skill id it does not
know** when a game goes live, so a misspelt or invented id silently loses the game's
tagging. The ids below mirror the code as of November 2026; if they differ, the code
wins. Ids are stable: never rename or reuse one; adding one means adding it to
`kid-skills.js` first.

Bands: **B1** 12–24 months · **B2** 2–3 years · **B3** 3–4 · **B4** 4–5 · **B5** 5–7.
"(intro)" = light exposure only: name it, show it, never quiz it.

## Areas and why these groups

The areas merge the Head Start Early Learning Outcomes Framework (ELOF, US, birth to
5) domains and the EYFS (England, birth to 5) areas of learning into groups a parent
recognises:

| Area id | Label | ELOF domain / subdomain | EYFS area |
|---|---|---|---|
| `language` | Language and literacy | Language & Literacy (Vocabulary; Phonological Awareness; Print & Alphabet Knowledge) | Communication & Language; Literacy |
| `numbers` | Numbers and maths | Cognition: Mathematics Development (Counting & Cardinality; Operations) | Mathematics |
| `shapes-colours` | Shapes and colours | Cognition: Mathematics (Geometry & Spatial Sense) | Mathematics (shape and space) |
| `memory` | Memory and attention | Approaches to Learning (cognitive self-regulation: attention, working memory) | Characteristics of Effective Learning |
| `logic` | Problem solving and logic | Cognition (reasoning, patterns, classification); Approaches to Learning | Mathematics (pattern); Understanding the World |
| `motor` | Fine motor | Perceptual, Motor & Physical Development (fine motor) | Physical Development |
| `world` | Knowledge and the world | Cognition: Scientific Reasoning; infant–toddler Exploration & Discovery | Understanding the World |

## The ids in use now

| id | Label | Area | Bands | Covers |
|---|---|---|---|---|
| `vocabulary` | First words | language | B1–B5 | naming pictures, "where is the…?", new words |
| `letters` | Letter recognition | language | B3–B5 (B2 intro) | finding and naming letters |
| `letter-sounds` | Letter sounds (phonics) | language | B4–B5 (B3 intro: s, m, a…) | letter → sound, beginning sounds |
| `upper-lower` | Big and little letters | language | B4–B5 | matching upper and lower case |
| `counting` | Counting | numbers | B2–B5 (B1 intro: "one, two") | one-to-one counting; subitising 1–3 then to 5 |
| `numbers` | Number recognition | numbers | B3–B5 | numerals 1–5, then 1–10, then 1–20 |
| `more-less` | More and less | numbers | B3–B5 | which has more, same, fewer |
| `adding` | Simple adding | numbers | B5 (B4 intro, objects within 5) | combine / take away small sets |
| `shapes` | Shapes | shapes-colours | B2–B5 | circle, square, triangle first |
| `colours` | Colours | shapes-colours | B2–B5 (B1 intro) | point to a colour, then name it |
| `memory` | Memory pairs | memory | B3–B5 (B2: face-up matching only) | hidden-card memory, "what was there?" |
| `focus` | Focus time | memory | B2–B5 | looking carefully, staying with a game (measured, not trained by pressure) |
| `matching` | Matching | logic | B1–B5 | find the same one |
| `sorting` | Sorting | logic | B2–B5 | groups by colour, shape, kind |
| `patterns` | Patterns | logic | B3–B5 | what comes next: AB, then ABB, ABC |
| `thinking` | Thinking questions | logic | B3–B5 | odd one out, simple puzzles, cause and effect, order |
| `writing` | Tracing letters | motor | B4–B5 | lines and curves (B4), letters and numerals (B5) |
| `drawing` | Drawing | motor | B1–B5 | free marks, colouring, drawing pad |
| `world.animals` | Animals | world | B1–B5 | names and sounds (B1–B2), babies and homes (B3–B4), food and fun facts (B4–B5) |
| `world.body` | The body | world | B1–B5 | body parts (B1–B2), senses (B3–B4), simply how it works (B5) |
| `world.everyday` | Everyday things | world | B1–B5 | food, vehicles, clothes, home objects, what things are for |
| `world.nature` | Nature and weather | world | B2–B5 | weather, day/night, seasons, plants |
| `world.places` | Places | world | B3–B5 | home, park, shop, beach, sea, sky, space; simple maps at B5 |
| `world.facts` | Fun facts | world | B4–B5 (B3 picture "which one" questions) | true, simple, kid-safe facts and picture trivia |

## Proposed, not yet in the code (do not put in meta.json until added)

Research supports these as distinct early skills; they map onto existing ids for now.

| Proposed id | Label | Bands | Use for now | Basis |
|---|---|---|---|---|
| `listening` | Listening and following directions | B1–B5 | `vocabulary` | CDC: one-step (18 m), two-step (30 m) directions; ELOF Attending & Understanding |
| `rhyme` | Rhymes and sound play | B3–B5 | `letter-sounds` | CDC 5 y: recognises simple rhymes; ELOF Phonological Awareness |
| `blending` | Sounding out words | B5 | `letter-sounds` | phonics sequence after letter–sound knowledge |
| `sizes` | Big and little, measuring | B2–B5 | `more-less` | ELOF Measurement |
| `position` | In, on, under | B3–B5 | `shapes` | ELOF Geometry & Spatial Sense |
| `feelings` | Naming feelings | B3–B5 (B2 intro: happy/sad) | none | ELOF / EYFS social-emotional; children match emotion faces before naming them, happy and sad first (~3½), fear and surprise later (~4½–5) |
| `calm` | Calming down | B3–B5 | none | EYFS Personal, Social & Emotional (self-regulation) |
| `turn-taking` | Taking turns | B4–B5 | none | CDC 5 y: takes turns in games |
| `music` | Music and rhythm | B1–B5 | `drawing` (creative) | EYFS Expressive Arts & Design |

A social-emotional area (`feelings`) and a creative area are well supported by both
frameworks and worth adding to `kid-skills.js`; until then, feelings games should not
be built for the library (their play would be untracked).

## Trivia and facts (the bank)

`world.*` ids are also the `area` of items in the trivia bank (`src/voice/kid-games/
trivia.json`, plus `<dataDir>/kid-games/trivia.extra.json` for workshop additions).
Item kinds: `name` ("Where is the cat?"), `sound` ("Who says meow?"), `trivia` (a
question), `fact` (a fact alone). Questions ≤ 120 characters, facts ≤ 160, 2–4
choices, every choice a picture the kit can draw, the answer one of the choices, no
URLs or markup. A game that uses the bank declares `"uses": ["trivia"]` in meta.json
and reads `window.CRU_BANK`.

## Using it

- `"skills": ["letter-sounds", "vocabulary"]` — first id is the main one.
- `ageRange` within the bands of the main skill.
- The game is told what the child knows at start (`window.CRU_INIT`: levels, mastery,
  focus, avoid per skill — see the README); start one step below the level.
- The parent view says what was practised, never ranks or grades the child.
