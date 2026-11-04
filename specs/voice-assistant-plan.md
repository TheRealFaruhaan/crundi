# Voice assistant — build plan

Status: plan for discussion, not started. Written 2026-11-04.
Design and decisions (D1–D31): [voice-assistant.md](voice-assistant.md).

Each phase ends with something usable and testable on its own. Phone first
(D29); web and desktop reuse the server side later.

## Checked before planning

- **Full-text search works in Crundi's Node.** `node:sqlite` with an FTS5
  table runs on the repo's Node 22 (experimental warning only). Electron 41
  bundles a newer Node, so the desktop build has it too. Still to confirm in
  the Linux `.deb` (bundled Node) build.
- **Voice can reuse Crundi's permission model.** `src/access-policy.js`
  already gives the collaborator role a default-deny route allowlist, checked
  before any handler runs. Voice gets its own principal the same way, so
  D10/D17 (no file edits, no critical settings) are enforced by the server, not
  only by which tools the model is shown.
- **Voice can reuse Crundi's tools.** The MCP tools in `src/mcp-stdio.js`
  call the webapp's HTTP API (`CRUNDI_API_URL`). The voice tool runner calls
  the same API with the voice principal's token.
- **Turn-end hook exists:** `onTurnEnd` in `src/claude-ui.js` (D12).
- **Tests:** `npm run check` runs `scripts/test-*.mjs`; new server tests go
  there. Android has JVM unit and screenshot tests under `android/app/src/test`.

## Phase 0 — device checks (half a day, before Android work)

Small throwaway test app or debug build on Master's Samsung:

| # | Check | Decides |
|---|---|---|
| 0.1 | Side-key `VoiceInteractionSession` shows over the lock screen, and from it a `microphone` foreground service can be started | D1, D4 |
| 0.2 | "Hey Google, open Crundi Voice" from the lock screen: opens, or asks to unlock | D2, D18 |
| 0.3 | Which en-GB male voices are installed (Google TTS, Samsung TTS); pick the best | D19 |
| 0.4 | On-device `SpeechRecognizer` availability and accuracy on the phone | D27 |
| 0.5 | sherpa-onnx keyword spotter: false triggers on "crundi" in a noisy room over an hour; battery drain over an hour | D3, D4 |

Results go back into the design doc.

## Phase 1 — server brain, typed (no Android yet)

Goal: talk to Crundi by typing in a test page and get correct, safe answers.

- `src/voice/agent.js` — Claude API tool loop.
  - Model from settings, default `claude-haiku-5-5` (D8); streaming.
  - System prompt: persona (calm British butler, "Master", own words for
    acknowledgements — D19, D20), rules (D10, D15–D17, D21, D28).
  - Prompt caching on system + tools; volatile state after the breakpoint.
  - `refusal` stop reason handled (spoken "I can't help with that one").
- `src/voice/tools.js` — tool definitions, built from the existing MCP tools
  where they fit, plus new ones. First set **read-only**: projects, folders
  and files (read/list/search), settings (read, secrets names only — D16),
  services (status/logs), notes, kanban, mindmap, schedules, usage, open
  panels and their latest output (D9, D11).
- `src/access-policy.js` — `ROLE_VOICE` with its own route allowlist.
  Critical-settings routes (D17) and file writes are absent from it.
- `src/voice/budget.js` — API key from settings (server only), per-month
  spend from `response.usage` × model price, stop at the limit, default $50
  (D25).
- Routes: `POST /api/voice/turn` (streamed reply), `GET/POST
  /api/voice/settings` (model, key set/unset, limit, spend so far).
- Settings page section "Voice assistant" (web UI, for configuration only).
- Tests: tool allowlist (a voice token can't reach a critical-settings route
  or a file write), budget stop, secrets names-only, persona prompt snapshot.

Exit: typed questions about usage, services, kanban, notes and files get right
answers; anything outside the allowlist is refused by the server.

## Phase 2 — memory

- `src/voice/memory.js` — one running conversation (D26), stored on disk.
- Idle compaction: after ~10 min quiet, summarise older turns into a block
  (Crundi's own summary step, at a time it chooses).
- Notebook: Claude memory tool (`memory_20250818`), files on the server,
  viewable and editable in settings.
- `src/voice/memory-index.js` — SQLite (`node:sqlite`) store:
  - FTS5 keyword index over turns, summaries and notebook;
  - local embeddings (small ONNX model, e.g. all-MiniLM-L6-v2) as blobs,
    brute-force similarity, merged with keyword results;
  - turns indexed before compaction removes them.
  - Fallback setting: hosted embeddings (Voyage AI) if local is too heavy.
- Tool: `memory_search({ query, since?, project? })`.
- Tests: compaction keeps recent turns verbatim; search finds a compacted
  detail by keyword and by meaning; CPU/time per embed measured.

Decision needed in this phase: whether the ONNX dependency (tens of MB) is
acceptable in all builds, or local embeddings ship as an optional download.

## Phase 3 — Android: side key (the default trigger)

- `voice/CrundiVoiceService` (`VoiceInteractionService`),
  `CrundiSessionService`, session UI (overlay, works over lock screen),
  `res/xml/voice_interaction_service.xml`, manifest entries.
- On-device speech-to-text (D27) → `POST /api/voice/turn` → text-to-speech
  with the chosen voice (D19). Full text shown on screen; long answers spoken
  as a summary (D28).
- Locked phone: read-only answers; changes ask to unlock.
- App settings → Voice assistant: on/off, voice picker, pointer to Android's
  default-assistant setting and Samsung's Side button setting.
- Tests: session state logic (JVM), screenshot tests for the overlay.

Exit: hold the side key, ask "how much usage do I have left", hear the answer.

## Phase 4 — actions

Now the agent may change things (exceptions to D10):

- Notes, kanban, mindmap edits (D13).
- Project-scoped panels: open, close, arrange (D14).
- `send_to_chat` with `intent` explicit/inferred and one-line read-back for
  long prompts (D15); chats only, never terminals (D21).
- Create projects (D22); start chats (D23); close or park chats with
  confirmation (D24, D31).
- Turn-end watch (D12, D18): listener on `onTurnEnd`, push to the phone,
  notification or full-screen opens where it was asked, spoken result.
- Confirmation flow shared by all of these: spoken "yes" or a tap.
- Route allowlist for `ROLE_VOICE` grows by exactly these routes.
- Tests: each confirm-required action refuses without confirmation; terminal
  input refused; voice can't write a project file.

## Phase 5 — Gemini entry

- `activity-alias` "Crundi Voice" and/or a static app shortcut that opens the
  voice screen (D2); silence timeout closes it.

## Phase 6 — "Hey Crundi"

- sherpa-onnx keyword spotter with "crundi" / "hey crundi".
- In-app listening behind a toggle (D3).
- Background listening: `microphone` foreground service, settings toggle with
  "Until I turn it off" or 15/30/45/60/120 min (D7); voice tool
  `set_background_listening({ minutes })` with the same choices, default 30,
  never unlimited (D4–D6); notification with time left and Stop; "stopped —
  tap to resume" if killed.

## Later

- Web UI and desktop voice on the same server side (D29).
- AppFunctions for Gemini once third-party support is public.
- A cloud voice option (Google Cloud TTS / Azure / ElevenLabs stock voice).

## Rough order of effort

| Phase | Size |
|---|---|
| 0 Device checks | S |
| 1 Server brain | L |
| 2 Memory | M–L |
| 3 Side key | L |
| 4 Actions | M |
| 5 Gemini | S |
| 6 Hey Crundi | M |

Phases 1 and 3 can't overlap much (3 needs 1's route). Phase 0 can run
alongside 1. Phase 2 can go after 3 if a working phone assistant matters
sooner than memory.
