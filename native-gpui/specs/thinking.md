# 2.15 Thinking

A thinking block is Claude's reasoning, shown in the conversation where it
happened. It has two forms:

- **The thinking block** (`.cc-think`), when the block has text: a thin bar
  on the left, a head line "▾ Thinking" and the reasoning under it in small
  muted italics. It is open while Claude thinks (the label shimmers and a
  caret blinks at the end of the text), folds itself away a little after it
  finishes, and opens and closes on a click.
- **The "Thought" line** (`.cc-thought`), when the block has no text: a
  small dot and "Thought" or "Thought for ~1.2k tokens". Models from Opus 4.7
  on withhold the reasoning (this machine's `claude-opus-5-5` always does),
  so in practice this is the form most chats show. It is flat: nothing to
  open.

Phase 1 drew only the "Thought" line, as a click target that opened the text
underneath. This screen replaces that with both web forms.

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.

Source (`app/vendor/claude-chat.js`):
- CSS 89-102: `.cc-think`, `.cc-think-head`, `.cc-think-body`,
  `.cc-think.cc-collapsed`, `.cc-think.live` (the shimmer, `@keyframes
  cc-shimmer`), `.cc-think-caret` (`cc-blink`), the reduced-motion rule,
  `.cc-thought`, `.cc-thought-dot`, `.cc-caret` (its turn when closed).
- `thinkUser` / `thinkAuto` / `thinkTimers` / `THINK_LINGER_MS` 1813-1816.
- `thinkCollapsed()` 1817, `autoCloseThinking()` 1825, `thinkingChanged()`
  1834, `thinkingNode()` 1844.
- `addEntry()` 4589 (a new entry calls `thinkingChanged(e, true)`), the
  `patch` frame 4975 (`thinkingChanged(e, false)`), the `delta` frame 4995
  (appends the text, repaints, scrolls the body to its end).
- Server `src/claude-ui.js`: `handleStreamEvent` 2060-2100 (a
  `content_block_start` of type thinking makes the entry `{kind:'thinking',
  text:'', streaming:true, tokens:0}`; `thinking_delta` sends `delta` frames
  with the text and patches `tokens` from `estimated_tokens`;
  `content_block_stop` patches `streaming:false`); the transcript reader
  245-250 (from history: `{kind:'thinking', text, tokens:0}`, no
  `streaming`).

## Screenshots

`thinking/web-<state>.png` at 1400×860, one wide pane. A `-2x` / `-4x` file is
the named region cropped and enlarged. The thinking text was injected through
the page's WebSocket (`entry`, `delta` and `patch` frames, the server's own
path), since this machine's model returns no thinking text.

| State | What it shows |
|---|---|
| `web-live` (+`-2x`) | **Thinking, live.** Open; "▾ Thinking" with the label mid-shimmer; the text in muted italics with the blinking caret after the last word. |
| `web-shimmer-frames-4x` | The label at three moments of the shimmer (0.3 s, 0.8 s, 1.25 s into its 1.6 s cycle): a bright band (`text-primary`) sliding right to left over `text-secondary`. |
| `web-long-live` | **Long thinking, live.** The body stopped at its maximum height and scrolls inside itself, kept at its newest line while text streams in; a thin scrollbar at its right. |
| `web-done-open` | **Just finished** (within 5 s): no shimmer, no caret, still open. The web's repaint put the body's scroll back at the top (see Behaviour). |
| `web-collapsed` (+`-2x`) | **Folded by itself** 5 s after it finished: "▸ Thinking", the bar beside the one line. |
| `web-reopened` | **Opened again by a click**: the whole body back, at its maximum height, scrolling. |
| `web-thought-lines` (+`-2x`) | **The "Thought" line**: "Thought for ~1.2k tokens", "Thought for ~640 tokens", "Thought" (no tokens known), each between replies. |
| `web-newer-folds-older` | **A newer thought folds the earlier ones**: the first block (closed by the reader), the second (closed by the rules the moment the third started, though it was still streaming), a tool card, the third open and live. |
| `web-text-larger` | A+ twice (×1.15): the text and the head scale; the bar and the gaps do not. |
| `web-text-smaller` | A- to ×0.85. |

## Parts and arrangement

### The thinking block

```
┃ ▾ Thinking                         ← head: caret + label, one line
┃ reasoning text in muted italics…   ← body: wraps, keeps line breaks,
┃ …and more|                            caret while streaming
```

| Part | Web | Token |
|---|---|---|
| Bar | left border 2px, `border` | `sp-2`, `Theme.border` |
| Text start | 9px after the bar | `sp-10` (bar + padding = the head's x) |
| Head | one line (12 × 1.55 = 18.6 tall), not italic, `text-secondary`; caret then label, gap 5 | `text-12`, `line-chat`, gap `sp-6` |
| Caret | ▾ open, turned −90° (▸) closed, turns in 0.15 s | the shared caret glyphs ▾ / ▸ (as the tool cards) |
| Label | "Thinking" | |
| Body | 4 below the head; 12 italic `text-muted`, line 1.55; `pre-wrap` (keeps newlines and spaces, wraps long lines) | `sp-4`, `text-12`, italic, `line-chat` |
| Body max height | 260, then it scrolls (thin scrollbar) | `composer-max * 1.5` (270, one step off), `ScrollThumb` |
| Live caret | 2 wide, 1em tall, `text-muted`, 2 after the last character, blinks (1 s, on half / off half) | `sp-2` × `text-12`, a theme duration for the blink |
| Shimmer | the label's colour is a gradient `text-secondary` 0% → `text-primary` 45% → `text-secondary` 90%, 2.2× the label's width, sliding from 120% to −120% in 1.6 s, linear, forever | a theme duration for the period (new, like `spin`) |
| Fold | body max-height to 0 in 0.28 s, opacity to 0 in 0.2 s, its 4px top gap to 0 | `motion` (220 ms) |

Measured (wide pane, ×1): block 1116 × 59.8 open with two lines, 18.6 closed;
head 18.6; body top = head bottom + 4; live caret 2 × 12.

### The "Thought" line

```
• Thought for ~1.2k tokens
```

| Part | Web | Token |
|---|---|---|
| Dot | 5 round, `text-muted` at 55% opacity, centred on the line | `sp-6` (one step), `text-muted` with the opacity as a theme colour or an `opacity` |
| Gap | 6 | `sp-6` |
| Text | 12 italic `text-muted`, line 1.55 | `text-12`, italic, `line-chat` |
| Wording | "Thought"; with `tokens` > 0: "Thought for ~N tokens", N as is under 1000, else N/1000 with one decimal and "k" (1234 → "~1.2k", 640 → "~640") | |

Not clickable, not selectable (`user-select:none`), no hover.

Both forms sit in the log like any entry: the pane's width, the entry gap
(`sp-10`) below.

## Controls and behaviour

- **Click the head** (anywhere on the line, pointer cursor): opens or closes
  the block, with the fold animation. The choice is remembered by entry id
  for as long as the pane shows the chat, and **it wins** over every rule
  below: a block the reader opened stays open, one they closed stays closed.
  The head is not a focus stop; no keyboard.
- **Open or closed, by the rules** (when the reader has not chosen):
  1. A block that arrives **streaming** opens. At that moment every other
     block the rules had opened closes at once — even one still streaming
     (its label keeps shimmering while folded, `web-newer-folds-older`).
  2. When an open block **stops streaming** (`patch {streaming:false}`), it
     closes **5 s later** (`THINK_LINGER_MS`), unless the reader chose
     meanwhile.
  3. A block that is not streaming when it first appears (history after a
     reload or restart, or a replay) is **closed**.
- **Live** (only while `streaming`): the label shimmers and the caret
  blinks after the text. Each `delta` frame appends text; the body then
  scrolls to its end so the newest line is in view. When the body is
  shorter than its maximum it simply grows.
- **When it finishes**: shimmer and caret go. The web rebuilds the node on
  that patch, which puts the body's scroll back at the top
  (`web-done-open`); that is the rebuild's accident. **Named difference:**
  the native body keeps its scroll position.
- **The log**: while the reader follows the bottom, a growing block keeps
  the log at the bottom (2.16's stick rule). Folding a block above the
  reader's position moves what is below it up, as any height change does.
- **Text size** (A- / A+, `fs`): the head and body text scale (12 × fs);
  the bar, gaps, dot and caret width do not; the max height does not.
- **Selection**: the body's text can be selected and copied; the head's and
  the "Thought" line's cannot.
- **Reduced motion** (the system setting): no shimmer (label plain
  `text-secondary`), no blink, no fold animation. Native: honour it if the
  platform setting is readable, otherwise always animate (name it).
- **The "Thought" line, live**: an empty block that is streaming shows the
  same line; its token count rises as `patch {tokens}` frames arrive. No
  shimmer (the web only shimmers the block form).
- A thinking entry inside an agent's transcript (2.18) is drawn the same way.

## States

| State | Look |
|---|---|
| Live, open | ▾, shimmering label, text with blinking caret |
| Live, long | body at its maximum, scrolling, kept at the newest line |
| Live, folded (a newer one started) | ▸, label still shimmering, one line |
| Done, lingering | ▾, plain label, no caret, 5 s |
| Done, folded | ▸ Thinking, one line |
| Opened / closed by the reader | stays so until the pane is left |
| From history | ▸, folded |
| No text | the "Thought" line, with or without tokens |
| Text larger / smaller | text scales; bar, gaps, dot fixed |
| Narrow pane | the body wraps; head unchanged |

## Data and endpoints

No endpoints. The entries come from the chat's stream (specs/live-state.md):

- `ui-history`: `session.messages[]` with
  `{ id, kind:'thinking', text, tokens, streaming? }` (no `streaming` from
  history: closed).
- `ui-event` with `event.type`:
  - `entry` `{ entry:{ id, kind:'thinking', text:'', streaming:true,
    tokens:0 } }` — a block starts;
  - `delta` `{ id, text }` — text appended (the increment only);
  - `patch` `{ id, patch:{ tokens } }` — the estimate for an empty block;
  - `patch` `{ id, patch:{ streaming:false } }` — it finished.

## End-to-end test (phase2.mjs)

The real CLI here returns no thinking text, so drive it through the test
control's frame injection (as 2.14 did for the cards the CLI lacks):

1. Inject `entry` thinking `th1` streaming with a sentence: the block is
   open, the label is "Thinking", the caret shows.
2. Inject 30 `delta` lines: the body stops at its maximum height and its
   scroll is at the end.
3. Inject `entry` `th2` streaming: `th1` is folded at once; `th2` is open.
4. Patch `th2` `streaming:false`: still open; after 5 s (allow 7) folded.
5. Click `th2`'s head: open; wait 6 s: still open (the reader's choice).
   Click again: folded.
6. Inject a thinking entry with `text:''` and `tokens:1234`: the line reads
   "Thought for ~1.2k tokens"; one with no tokens reads "Thought".
7. Reload the chat (re-subscribe): every block with text is folded.

`cargo test`: the token wording (0 → "Thought", 640, 999, 1000 → "~1.0k",
1234 → "~1.2k") and the open/closed rules as a small state machine (user
choice wins; streaming start folds the others; finish starts a 5 s timer;
non-streaming first sight is closed).

## Checklist

- [x] Block with text: bar `border` at the left, head "▾ Thinking" in
      `text-secondary` (not italic), body 12 italic `text-muted`, `line-chat`,
      keeps newlines.
- [x] Body stops at its maximum height and scrolls (`ScrollThumb`); kept at
      the newest line while text streams.
- [x] Live: the label shimmers (gradient sliding, theme period); a caret
      blinks after the text. Both go when it finishes.
- [x] Opens when it starts streaming; folds 5 s after it finishes; folds at
      once when a newer block starts; folded from history.
- [x] A click on the head toggles it (fold animation, caret ▾ / ▸) and the
      reader's choice wins over every rule.
- [x] Finishing does not jump the body's scroll (named difference).
- [x] No text: "Thought" / "Thought for ~N tokens" (k with one decimal from
      1000), the 55% dot; not clickable.
- [x] Text scales with A- / A+; bar, gaps and dot do not.
- [x] The log stays at the bottom while a followed block grows.
- [x] Body text is selectable; head and "Thought" are not.
- [ ] phase2.mjs steps 1-7 pass; cargo tests cover the wording and the
      rules; snapshots of every state saved under `thinking/review/`.
