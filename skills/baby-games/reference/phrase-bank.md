# The phrase bank: instant voice lines for game events

Kid mode answers game events with pre-written lines at once (a reflex layer, no LLM
turn); Cru's own conversational reply, when there is one, comes on top. This file is
how to write those lines. **The library README (`src/voice/kid-games/README.md`) is
authoritative for the file name and format**; until it documents one, use the
format below.

## Instant first, random always

- Reflex lines are the main voice of a game: they play at once, while anything Cru
  thinks up comes a beat or two later. Cover every routine event with them; Cru's own
  words are for the rare moments that need judgement.
- Write many variants per event (8–12+ for `correct` and `miss`), of different shapes
  and lengths, so random picks never feel repetitive; the host never plays the same
  line twice in a row. More variants beat longer lines.

## Rules for every line

- **Short.** 12 m–3 y: ≤ 8 words. 3–5: ≤ 10. 5–7: ≤ 14. One idea per line.
- **Specific beats generic.** Put the thing in: "You found the B!" not "Good!".
  At least two thirds of `correct` lines use `{item}`.
- **Vary.** At least 8 lines per event per band (12+ for `correct`); the player never
  repeats any of the last 3 lines used for that event. "Good job" / "Great job" may
  appear at most once per list — they are the loop we are avoiding.
- **Effort praise for misses**, never negative words: no "wrong", "no", "oops",
  "uh-oh", "bad", "fail", "lose", "hurry", "come on", "again?!". A miss line invites
  another look or listen.
- **Process praise** for older children: what they did ("You counted every one!"),
  not who they are ("You're so smart"). No "good girl/boy".
- **Letter sounds as phonemes, written for the speech engine**: continuous sounds
  stretched (`sss`, `mmm`, `fff`, `lll`, `nnn`, `rrr`, `vvv`, `zzz`), stop sounds as a
  quick repeat anchored by a word (`b, b, ball`; `t, t, tiger`) rather than "buh/tuh".
  Use the `{sound}` slot; the game supplies the phoneme spelling.
- **Name sparingly**: `{name}` in at most 1 line in 5, and only if it is set
  (lines with `{name}` are skipped when there is no name).
- Exclamation is warm, not shouty: one `!` per line at most.
- Nothing about time pressure, scores, rankings or other children.

## Slots

| slot | filled with | example |
|---|---|---|
| `{item}` | the thing just found or asked for, as said aloud | `the B`, `red`, `three ducks`, `the cow` |
| `{name}` | the child's first name from the parent's instructions | `Sara` |
| `{sound}` | the phoneme spelling for a letter, or an animal sound | `sss`, `b, b, ball`, `moo` |
| `{count}` | a number word | `three` |
| `{level}` | the new level as words, for level-up | `three` |

A line whose slot has no value is skipped, never spoken with a gap.

## Events (same names as the game events)

| event | when | line purpose |
|---|---|---|
| `correct` | right answer (first try or helped) | celebrate, name the thing |
| `miss` | a wrong choice | effort praise + invite another look (none for 12–24 m) |
| `hint` | the game glowed / narrowed / showed | point the child to it |
| `level-up` | difficulty went up (not spoken on a step down) | "you're ready for more" |
| `round-done` | a round ended | short bridge to the next |
| `finished` | the game ended | what they did, warmly; offer more |
| `idle` | no touch for ~30 s | gentle invitation, never "are you there?" pressure |
| `stuck` | touching but no progress ~20 s | offer help, make it easier |

## Format (until the README says otherwise)

```json
{
  "version": 1,
  "bands": ["1-2", "2-3", "3-4", "4-5", "5-7"],
  "lines": {
    "correct": {
      "2-3": ["{item}! Yes!", "You found {item}!", "There's {item}!"],
      "any": ["You found {item}!"]
    },
    "miss": { "3-4": ["Good try! Let's look again."] }
  },
  "games": {
    "duck-count": { "correct": { "any": ["{count} ducks! Quack, quack!"] } }
  }
}
```

- `lines.<event>.<band>`: band keys are age ranges; the player uses the band that
  contains the child's age, falling back to `any`. A band may say `"1-3"` to cover
  several.
- `games.<game-id>` overrides for one game (its own vocabulary), merged over `lines`.
- Lines are plain text, no markup, ≤ 120 characters, no URLs.

## Starter bank

### 12 m–3 years (≤ 8 words; label + delight; no miss lines)

- **correct**: "{item}! Yes!" · "You found {item}!" · "There's {item}!" · "Hooray, {item}!" ·
  "{item}! You did it!" · "Yes! That's {item}!" · "Look, {item}!" · "Wow, {item}!" ·
  "{sound}! It's {item}!" · "You tapped {item}!" · "Yay! {item}!" · "{name} found {item}!"
- **miss** (2–3 only; the tapped thing names itself): "That's {item}! Let's find another." ·
  "Ooh, {item}! Look again." · "That one's {item}. Where's the other?"
- **hint**: "Look! Here it is!" · "This one! Tap it!" · "See it shining?" · "Over here!" ·
  "It's right here!" · "Peek! There it is!"
- **round-done**: "Here comes another!" · "Ready? Look!" · "One more!" · "What's next?"
- **finished**: "All done! Hooray!" · "You found them all!" · "Bye-bye, friends!" ·
  "That was fun!" · "Yay, {name}! All done!"
- **idle**: "Tap a picture!" · "Who's hiding? Let's look!" · "Can you tap me?" · "Peekaboo!"
- **stuck**: "Let's do it together!" · "Look, it's shining!" · "I'll help you!"

### 3–5 years (≤ 10 words; specific; effort praise)

- **correct**: "Yes, that's {item}!" · "You found {item}!" · "Well spotted, {item}!" ·
  "That's it! {item}!" · "{item}! You listened carefully." · "Right! {sound}, {item}!" ·
  "You looked so carefully. {item}!" · "Hooray, {item}!" · "{count}! You counted them all!" ·
  "Yes! {item}, just right!" · "You got it, {item}!" · "{name}, you found {item}!"
- **miss**: "Good try! Let's look again." · "Nearly! Have another look." ·
  "Good thinking! Try another one." · "Hmm, let's listen again." ·
  "That's {item}. Can you find the other?" · "Keep looking, you can do it." ·
  "Let's count together!" · "Listen: {sound}…"
- **hint**: "Look for the shiny one!" · "Listen: {sound}… {item}." · "It's one of these two!" ·
  "Here it is! Tap it!" · "Look at this one!" · "Let's count them one by one."
- **level-up**: "You're ready for a bigger one!" · "Ooh, let's try a bit more!" ·
  "Level {level}! You're getting so good at this."
- **round-done**: "Ready for the next one?" · "Here's another!" · "Let's do one more!" ·
  "Next one coming!"
- **finished**: "You did it! All done!" · "You found every one!" ·
  "You know {item} now!" · "What a lot of learning!" · "Want to play again, or something new?"
- **idle**: "Shall we play some more?" · "I've got another one for you!" ·
  "Tap the one you think!" · "Ready when you are."
- **stuck**: "Let's do it together." · "Want a little help? Look here." ·
  "Let's make it a bit easier." · "Listen with me: {sound}…"

### 5–7 years (≤ 14 words; process and strategy)

- **correct**: "You sounded it out: {item}!" · "You counted every one — {count}!" ·
  "Yes! You checked each letter." · "{item} — great listening!" ·
  "You remembered where it was!" · "Clever thinking, {item}!" ·
  "You kept going and got {item}!" · "Spot on, {item}!" · "You worked that out yourself!" ·
  "Brilliant noticing — {item}!" · "{item}! Your practice is paying off." · "Nice one, {name}!"
- **miss**: "Good try — have another go." · "Not that one. Listen to each sound." ·
  "Try counting them one by one." · "Close! Check the first sound." ·
  "Mistakes help us learn. Try again!" · "Have another look — you can do it."
- **hint**: "Here's a clue: {sound}…" · "It's one of these two." · "Start with the first sound." ·
  "Count on from {count}." · "Here it is — now you try."
- **level-up**: "You're ready for a challenge!" · "Level {level} — let's stretch our brains!"
- **round-done**: "On to the next!" · "Here's a new one." · "Ready?"
- **finished**: "You built them all — great sounding out!" · "You practised so hard today!" ·
  "Look how many you did!" · "Again, or something new?"
- **idle**: "Want to keep going?" · "Your turn when you're ready!"
- **stuck**: "Shall we try an easier one?" · "Let's work it out together." · "Here's a clue."
