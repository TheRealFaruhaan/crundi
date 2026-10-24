# The factory editor: web and Android

Everything the web's factory management does (the Factories tab,
`app/vendor/crundi-factories.js`, and the flow editor, `tools/factory-flow/src`),
and where the Android app has it. Android code: `ui/planning/FactoryEditor.kt`
(screen and panels), `FactoryEdit.kt` (the edits, pure and unit tested),
`FactoryGraph.kt` (the canvas, with an edit mode), `Factories.kt` (library),
`api/FactoryJson.kt` (saving without losing fields).

## Library (crundi-factories.js)

| Web | Android |
|---|---|
| New factory (a name, then a blank factory: Start, Manager, End) | New factory sheet: name, then Blank, a template, or a copy of one of yours |
| Templates: Use this (copy into the library, open the editor) | Same (Use this on the template card, or in New factory) |
| Copy | Copy on the card and on the factory page |
| Export (download `<name>.factory.json`) | Export: Share (Android share sheet) or Save as a file (system file picker) |
| Import (a `.json` file; a draft if it needs fixing) | Import (system file picker); says when it came in as a draft |
| Edit, Start, Delete | Same; issues on the factory page open the editor at the problem |

## Flow (Editor.jsx, Palette.jsx, nodes.jsx, convert.js)

| Web | Android |
|---|---|
| Canvas: pan, zoom, fit, minimap | Pan, pinch-zoom, zoom buttons, fit, go to start (no minimap) |
| Palette: people, someone new, yes/no, pick one, pick any (comes with its Join), Join, Ask me, Wait; drag or tap to add in the middle | Add panel: the same items, tap to add in the middle of the view |
| Drag a step to move it (snaps to 16) | Long-press and drag (snaps to 16) |
| Drag from an output to a step's top to connect; from a person's side dot to leave a note | Long-press a step and let go (or Connect in the step's panel), pick the output, tap the target; "Leave a note for someone" for people |
| Connecting replaces what the output led to; closing a loop sets a round limit of 3 with a message | Same |
| Rules: Finish only to End, nothing else to End, nothing into Start, notes only between people | Same (steps it cannot lead to are dimmed while connecting) |
| Click a connection to edit it | Tap the dot halfway along it, or its row in the step's panel |
| Delete key / trash: steps (not Start, Manager, End), connections (not Finish) | Trash in the panel, same limits |
| Undo, redo (100 steps, typing coalesced) | Same |
| Tidy the layout (dagre) | Tidy (the server's layoutFactory: the same top-to-bottom layering) |
| Dry run (simulate a run on the canvas) | Not on Android |

## Steps and connections (Inspector.jsx)

| Web | Android |
|---|---|
| Label on every step | Same |
| Employee step: who does it, add someone new, edit their persona | Same |
| Manager: edit the Manager's persona | Same |
| Decision: question (with /skill chips), kind (yes/no, pick one, pick any), options (add, rename, remove; routes of removed options go), review gate for yes/no, who answers | Same |
| Ask me: what you are asked, replies | Same |
| Wait: command / signal / time / time of day; minutes, until, command, try every, waiting for, give up after; switching between one route and Done / Timed out drops the routes | Same |
| Join: which pick-any decision it joins | Same |
| Start, End: explanations | Same |
| Connection: from and to, round limit, require a note and its label; note: what it is for | Same, plus "Lead it somewhere else" |

## Team (Roster.jsx)

| Web | Android |
|---|---|
| The roster: Manager and employees, steps each, issues each | Same |
| Add someone: 14 persona templates or blank (unique names, skills limited to this machine's) | Same (the templates come from src/factory-personas.js through `android/tools/gen-factory-data.mjs`) |
| Name, persona (/skill chips), model, effort | Same |
| Memory, can edit files (employees) | Same |
| Pinned skills (comma separated) | Same, plus this machine's skills to tap |
| Per-person permission presets (not when the team bypasses) | Same |
| More tool rules: always allowed, never allowed | Same |
| Remove from the team (asks; takes their steps) | Same |

## Settings and limits (FactorySettings, CrTemplate.jsx)

| Web | Android |
|---|---|
| Name, description | Same |
| Change request form: sections (label, hint, required, a list), move, remove, add, back to the default | Same |
| Manager's permissions, the team's permissions, "Give the team bypass too" | Same |
| Everyone may (presets), more rules (one per line), Manager may set standing rules, Gatekeeper and its model | Same (Gatekeeper / permission management) |
| When the work is done, recover after a crash, setup command | Same |
| Max handoffs, max minutes, Manager check-in | Same |
| Usage: pause at 5-hour %, weekly %, people at once, one at a time above % | Same |

## Validation and saving

| Web | Android |
|---|---|
| Validates in the browser as you edit (factory-model.js), badges on steps | The server's validation, returned with every save (1.2 s after the last change) and on open |
| Issues panel: errors and warnings, tap to go to the step, connection or person | Same; the factory page's issues open the editor there too |
| Autosave 1.2 s after the last change, on Back, with baseVersion | Same; Back waits for the save |
| Conflict: "Changed elsewhere", Reload | Same |
| Error: "Not saved", Retry | Same |

Saving never drops a field the app does not know: the edited factory is laid
over the server's own JSON (`FactoryJson.forSave`), matched by id in lists.
Settings fields an older server does not send are hidden.
