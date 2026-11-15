# 2.4 Live state: design review, round 1

Isolated server :4814 (tools/test-server.sh), seeded as the spec says: Demo with
idle / working / question / approval / waiting / exited chats; Other (working +
idle), Third (waiting), Fourth (idle), Fifth (none). Native :4815 on DISPLAY=:99,
1400x860. Judged by the rewritten DESIGN.md "Reviewing a screen" (layout,
behaviour, theme; pixel offsets are not findings).

`window-<state>.png`: web left, native right, for each state of the spec.
`*-WxH+X+Y.png`: web over native, zoomed (headers ×3, sidebar ×2, folded ×3).

| State | Result |
|---|---|
| Pane dot idle / working / needs input (ring) / waiting / exited | match (`mixed-head-*`, `right-head-*`) |
| Badge WORKING / NEEDS INPUT / WAITING, after the title, none for idle/exited | match |
| Dot tooltip (raw state) | `native-tip-*.png`: exited, needs-input, waiting |
| Narrow pane: badge kept, title elides | match (`titles-head-340x28+940+90`) |
| Full screen: long form, working badge, 160 cap + badge | match (`full-*`) |
| Project dots, five states, active needs input | match (`mixed-sidebar-*`) |
| Fifth active with nothing: green; Demo inactive needs input | match (`active-none-sidebar-*`) |
| Answer (question, then approval): not drawn ahead (answer() only posts); Demo needs input → working | match (`active-working-*`) |
| Live exit of an idle chat in a wide pane | match (`other-wide-*`) |
| rename_chat live, typed rename live, the later auto rename refused | match |
| A live rename keeps the draft and the focus | `native-draft-before/after.png` |
| Folded rail, inactive states (running / working / needs input / waiting) | match in arrangement |
| **Folded rail, active avatar** | **differs: the gradient fill is a square** (`folded-*`) |
| **Folded rail, initials** | **differs: "D", "O", "T", "F" against "DE", "OT", "TH", "FO", "FI"** |

Not findings under the new rule (pixel only): badge 1px higher (y96 vs 97);
sidebar row pitch 34 vs 32 and folded 48 vs 54; initials cap 8 vs 9px.
