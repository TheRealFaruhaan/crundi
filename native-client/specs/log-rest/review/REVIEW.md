# 2.16 The rest of the log — design review, round 1 (+ F85–F88)

Same setup as ../../thinking/review/REVIEW.md. Reply markdown, notices,
errors and results were fed the same frames on both sides.

| Region / state | Crop | Verdict |
|---|---|---|
| Reply: heading, bold/italic, links, bare URL, code spans | reply-text-2x.png | matches, except that the **path `/tmp/proj/client.js:12` and the path chip `src/server.js` are plain** (web: accent-hover link with a dotted underline; PLAN names only the click as waiting for phase 3) |
| User bubble with a link | reply-bubble-link-2x.png | **web: underlined link in the bubble's colour that can be clicked; native: plain text** (not named) |
| Code block, lists, quote, rule | reply-code-lists.png | the code block keeps its lines whole (no sideways thumb: named gap). Nits: the 8px gap below a list is missing (numbered, bullets and quote run together); the rule is a faint hairline where the web has a 2px light line |
| Copy button on a code block: hover, click, tick, "Copied", clipboard | copy-code-hover-2x.png, copy-done-web-native-2x.png, native-copied.png | matches (clipboard holds the block exactly) |
| Link click | (testctl lastOpened) | real clicks on `[the docs]` and the bare URL record the right URLs |
| Notices, errors, failed result, end-of-turn line | notices-errors.png | matches |
| Narrow window (900) | narrow-errors.png | wraps like the web. **After the resize the log is left 50px short of the bottom while following it** (narrow-resize-not-at-bottom.png) |
| Jump button: new dot, hover | jump-new-web-native-2x.png, jump-hover-web-native-2x.png | matches |
| Jump click | native-dragging.png | **after the click the log is locked at the bottom**: wheel steps and dragging the thumb no longer move it (reproduced on a fresh client) |
| F87 | (log probe) | **one wheel notch up (117 from the bottom) leaves stick on; the next arrival pulls the log to the bottom.** With three notches, an arrival leaves the log in place |
| Media (section 4) | — | not built (F89) |

Fixes: F85 (dots centred, ../../tool-cards/review/r2/f85-dots-3x.png and
f85-dots-aplus-3x.png), F86 (strike only under the text, r2/f86-todo-strike-3x.png)
and F88 (Write band 3 lines, r2/f88-write-2x.png) are verified.
