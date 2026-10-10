# What's new

The tour shown once after an install or an update. **Every genuinely new
feature a person can see adds a step to it.** That is a rule of development
here, not an extra: a feature is not finished until its step is written.

**Only new features.** A bug fix is never a step, not even when it is the only
thing in the release: something that now works as it was always meant to is
not news. Neither is a refactor, a speed-up, a security fix or a change of
wording. A release made only of those has no edition, and nothing is shown
after updating to it. Ask of each step: could the person do, or see, this
before? If they could, and it merely works better now, leave it out.

## The rule

1. A new feature the user can see ships with a step in the edition for the
   version it ships in: `app/vendor/whatsnew/<version>/`.
2. The folder is named for the **exact version being released** (`1.19.25`, or
   `1.19.25-dev.1` for a dev prerelease). Bump `package.json` first, then name
   the folder to match. `npm run check` fails if the newest edition is ahead of
   the package version.
3. A dev edition is seen only by installs on the dev channel. When the work is
   released to production, **move its steps into the production version's
   folder and keep their ids.** Someone who already saw a step in the dev
   edition is not shown it again; someone on production sees it for the first
   time.
4. Bug fixes never get a step (see above), nor does anything nobody would
   notice. A release with no new feature has no folder; do not invent a step
   so that an update has something to show.
5. Keep an edition short: one step per thing worth knowing, seven at the very
   most. If two changes belong together, they are one step.

## An edition

```
app/vendor/whatsnew/1.19.25/
  steps.json        the steps, in the order they are shown
  boards.html       one animation per step
```

`steps.json`:

```json
{ "steps": [ {
  "id": "boards",                  "unique for ever; lowercase, digits, dashes",
  "tag": "Kanban",                 "the area, shown above the title",
  "title": "Boards you can share", "a few words, what the person gets",
  "body": "One or two sentences. What it is and why it matters, plainly.",
  "keys": [ { "keys": ["Ctrl", "B"], "text": "what that does" } ],
  "art": "boards.html",            "a file in this folder",
  "artTitle": "Kanban"             "the label on the little window"
} ] }
```

(The quoted notes on the right are explanation, not part of the file.)
`keys` is optional, at most five rows; a row's `keys` can be words such as
"Ask" or "Right-click" when it is not a key.

## An animation

A fragment of HTML with its own `<style>`, drawn for a stage **640 by 400
pixels**; the tour scales it to fit a desktop window or a phone.

- One root element: `<div class="wna a-yourname"> ... <style> ... </style></div>`.
  Prefix every selector with `.a-yourname` and every `@keyframes` name with a
  short prefix of its own, so two animations never collide.
- **Markup and styles only. No `<script>`, no event attributes.** The tests
  refuse them.
- It must loop (`animation: ... infinite`), with every part on the same
  duration so it stays in step. Eight to eleven seconds reads well.
- Use the app's colours, never fixed ones for surfaces and text:
  `--bg-primary`, `--bg-secondary`, `--bg-card`, `--border`, `--text-primary`,
  `--text-secondary`, `--text-muted`, `--accent`, `--accent-hover`,
  `--accent-dim`, `--green`, `--yellow`, `--yellow-dim`, `--mono`.
- Show the thing happening, not a screenshot of it: the idea being typed, the
  file being dragged, the pane growing. No text that needs reading inside the
  animation beyond a label or two.
- No emoji, here or anywhere in the app.

## Who sees what (`src/whatsnew.js`)

- **An update:** every edition newer than the one last seen, oldest first, so
  jumping several versions misses nothing.
- **A new install:** the latest edition only.
- **A dev edition:** only on the dev channel.
- **A step already seen** (by id) is never shown twice.
- Skip and Done both mark everything up to the installed version as seen.
  Settings has "What's new: Show it again" for the latest edition.

## Before releasing

`node scripts/test-whatsnew.mjs` (part of `npm run check`) checks that every
step has an animation, a tag and text, that no id is repeated, that no
animation file is left without a step, and that no animation carries a script.
Then look at it: run the dev instance on a fresh data folder and step through
at a desktop size and a phone size.
