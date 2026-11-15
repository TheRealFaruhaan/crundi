# 2.23 Parked panes — design review, round 1

Web shots: the spec's `web-*.png`. Native: the developer's `native-*.png` (same seeded
transcripts) plus a live run on a fresh test server (tools/test-server.sh 4814), driven
with real xdotool pointer input where hover/clicks matter.

| Crop | Web over / beside native | Verdict |
|---|---|---|
| parked-card.png | card: Resume + Peek, why, what, quiet New session | same parts, same order |
| header-2x.png | parked header | **CHAT tag moved**: web right after the title (before the spacer, headHtmlParked); native pushed right beside pin |
| resume-play-3x.png | Resume's play mark | **play mark half the web's size** (12px box, polygon fills 13/24 of it: ~5x8 px vs the web's ~10x13) |
| heavy-card.png | heavy card | matches (warning, tokens yellow, two pills) |
| short.png | ≤ 330 | matches (no What, short pill) |
| very-short.png | ≤ 210 | matches (one wrapping row, tokens pill) |
| starting.png | New session (quiet) pressed | **label stays "New session"**: web turns the pressed button's label into "Starting…" (resumePane) for the quiet link too |
| peek-pill.png | Peek: Parked view pill top right | matches |

Veil: covers the whole body including the composer (checked by pixels: Send 111,117,244 live
vs 52,55,115 parked); no blur = named gap.

Live run (real xdotool input): Resume hover accent-hover, quiet New session hover text-primary,
Peek and Parked view by real clicks, a click on the veiled composer does nothing (no draft),
Resume by a real click swaps the same leaf live and focused. live-*.png.
Header live: "Chat" … CHAT tag beside pin (live-header.png) — the moved tag again.
