# Voice assistant — build plan

Status: Phases 0 and 1 built and verified; Phase 3 built, awaiting the phone. Written 2026-11-04, updated 2026-11-05.
Design and decisions (D1–D41): [voice-assistant.md](voice-assistant.md).

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

Built as **Voice Lab**, a debug-only screen in the Android app
(`android/app/src/debug/…/voicelab/`; not in release or Play builds). Get it:
Actions › Android › the run for branch `claude/voice-assistant` › artifact
`crundi-android-github-debug`. The debug app has the same package name as the
installed app, so installing it over a release or Play install needs that
uninstalled first.

On the phone: open **Crundi Voice** from the launcher, grant the microphone,
run the checks, then **Copy report** and paste it back.

| # | Check | Decides |
|---|---|---|
| 0.1 | Side-key `VoiceInteractionSession` shows over the lock screen, and from it a `microphone` foreground service can be started | D1, D4 |
| 0.2 | "Hey Google, open Crundi Voice" from the lock screen: opens, or asks to unlock | D2, D18 |
| 0.3 | Which en-GB male voices are installed (Google TTS, Samsung TTS); pick the best | D19 |
| 0.4 | On-device `SpeechRecognizer` availability and accuracy on the phone | D27 |
| 0.5 | sherpa-onnx keyword spotter: false triggers on "crundi" in a noisy room over an hour; battery drain over an hour | D3, D4 |

Results go back into the design doc.

### Phase 0 results — 2026-11-05, Samsung S24 Ultra (SM-S928B), Android 17 (API 37)

| # | Result |
|---|---|
| 0.1 | **Works.** Side key opens the assistant overlay locked and unlocked; a microphone foreground service started from the overlay records real sound in both cases (peaks 4,000–10,500). The lab screen opens over the lock screen. Background-start controls not run. |
| 0.2 | **Conflict found:** once Crundi is the default digital assistant, "Hey Google" no longer works — the hotword belongs to whichever app is the default assistant. D1 (side key via default assistant) and D2 (Gemini) can't both hold. Needs a decision. |
| 0.3 | Google TTS has 13 en-GB voices (6 on-device, 6 network, 1 default). **Rejected: not close enough to the wanted voice.** A premium cloud voice is needed. |
| 0.4 | On-device recognition is fast (2–8 ms after speech) but **inaccurate**: "Add the UI overhaul of the home screen as a kanban task" came out as "Mdua of the home screen as a compound task". The default (network) recogniser was no better ("…as a company"); errors 7 (no match) and 11 (server disconnected) also seen. Gemini gets the same sentence right. **D27 needs revisiting.** |
| 0.4b | **Hints don't fix it.** Same sentence, language en-US, Android 13+ word hints on: on-device gave "At DUI overhaul of the home screen as a company task"; default gave "at the UI overhaul of the home screen as the compound task" (best, 0.86). "kanban" never came through in 20+ guesses; on-device gives no confidence scores. Frequent error 11 (server disconnected) on the first try after the previous recogniser was destroyed; a reused recogniser should avoid it. |
| 0.5 | Not run yet. |

## Phase 1 — server brain, typed (no Android yet) — BUILT 2026-11-05

As built (`src/voice/`, `scripts/test-voice.mjs`, page at `/voice`):
- The tools call Crundi's stores and managers directly, not its HTTP routes.
  Read-only is enforced by the closed tool list (pinned by the test: no write
  tool can be added by accident), not by a `ROLE_VOICE` route allowlist.
  `/api/voice/*` is owner-only (in `COLLABORATOR_NEVER`).
- Credentials inside settings and schedules show as "set"/"not set"; secret
  values and collaborator chat keys never reach the model.
- History is append-only; it restarts after 80 messages until Phase 2.
- Prompt caching: 1-hour breakpoint on system (+ tools), 1-hour automatic
  breakpoint on the conversation, nothing volatile before the new user
  message, fixed model/effort. `POST /api/voice/warm` (called when a voice
  session opens) re-sends the last request with `max_tokens: 0` to refresh
  the cache while Master speaks. Each answer reports tokens read from cache.
- Live speech to text: WebSocket `/api/voice/listen` (owner only) relays the
  microphone to Deepgram's streaming API (interim words, end of speech by
  endpointing + utterance end); audio is paused while Crundi talks. The
  `/voice` page's "Start listening" uses it.
- Deepgram Aura voices: `GET /api/voice/voices` (Deepgram's own list, with
  accent and gender; known Aura-2 names if unreachable), `POST
  /api/voice/speak` (mp3). Setting `ttsVoice`; empty = the device's voice.
- `/voice` test page: settings (model, limit, keys), type or hold-to-talk
  (browser recording → Deepgram on the server → Claude), reply spoken with the
  browser's en-GB voice, detail shown.

### Phase 1 verified against the real services — 2026-11-05

Run on the server with the owner's keys (Claude from the secrets vault,
Deepgram saved into Voice assistant settings via `secret_run`, never seen),
over a copy of the live data. `scripts/voice-live.mjs` repeats the Claude
part: real agent, real tools, a data copy, settings in memory.

| # | Check | Result |
|---|---|---|
| V1 | Claude turn, Haiku 5.5 | **Works.** "How much usage do I have left?" → "Plenty left, Master. The five-hour window is at 2 percent and the weekly window is at zero…", detail on screen, 3.9 s, $0.0004. "What's in the todo column of crundi?" → correct (empty), 1.8 s. |
| V2 | Caching | **Works.** warm() writes the 3.3k-token prefix; every turn after reads 90–96% from cache. One miss found and fixed (below). |
| V3 | Deepgram one-shot | **Works.** The kanban sentence (spoken by Aura-2 Draco) came back word for word, "kanban" included, confidence 0.99, 0.4 s. Synthetic speech is easy: a real-microphone check on `/voice` is still worth doing. |
| V4 | Deepgram live | **Works**, both WebM/Opus (browser) and raw 16 kHz PCM (phone). Each sentence arrives as an utterance ~0.9 s after it ends; the stream survives a 15 s pause. All `stream.js` assumptions held (Results / UtteranceEnd / SpeechStarted, is_final, speech_final, KeepAlive, CloseStream, `Authorization: Token`). |
| V5 | Voices | **Works.** `/v1/models` has the shape `parseVoices()` expects: 103 TTS models, 53 English after filtering, accent and gender in metadata. British male: **aura-2-draco-en** ("warm, approachable, trustworthy, baritone") and the older aura-helios-en. |
| V6 | Flux | **Flux is speech-to-text, not text-to-speech**: a conversational model on `/v2/listen` (`flux-general-en`) with turn detection built in (~260 ms end of turn, `StartOfTurn` / `EndOfTurn` / `EagerEndOfTurn`). Its docs don't mention keyterm prompting, which is why Deepgram was chosen. Kept Nova-3; Flux is a proposal (below). |

Fixed from these runs:
- `get_kanban` returned whole boards (descriptions, deleted tasks, history):
  Crundi's ran past the 30k tool-result cut, so "which project has the most
  tasks" took 20 calls, 13 s and a guess. Now one line per task plus counts;
  no project gives every project's counts in one call (2 s, 96% cached).
- The live stream takes `?encoding=linear16&sample_rate=…` (whitelisted) for
  the phone's PCM; KeepAlive every 4 s and always while paused.
- 1-hour cache writes were priced at 1.25x; they cost 2x.
- Capability `voice-assistant`; `/api/voice/turn` takes `locked`; the usage
  modal shows the API spend (D41).

Proposals (the owner decides):
- **Voice: aura-2-draco-en** as the suggested Deepgram voice (D19): British,
  male, calm baritone, a stock voice. Not set as the default yet.
- **Flux** for live listening, if Deepgram confirms keyterms work with it:
  quicker, better end-of-turn than endpointing + utterance end.
- **Web and PC voice (D39)** right after Phase 3, as Phase 3b: the same
  screen on the web workbench, and so on the PC. The PC's always-on
  microphone (D40) needs the wake word, so it goes with Phase 6.

Original plan:

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
- `src/voice/stt.js` — Deepgram (D37): the phone streams audio over the
  existing connection, the server relays it to Deepgram's streaming API
  with keyterms (Crundi, kanban, mindmap, project and service names, read
  live), and returns partial and final text. Key in Voice assistant
  settings; Deepgram spend shown next to Claude spend.
- `src/voice/budget.js` — API key from settings (server only), per-month
  spend from `response.usage` × model price, stop at the limit, default $50
  (D25).
- Routes: `POST /api/voice/turn` (streamed reply), `GET/POST
  /api/voice/settings` (model, key set/unset, limit, spend so far).
- Settings page section "Voice assistant" (web UI, for configuration only).
- Tests: tool allowlist (a voice token can't reach a critical-settings route
  or a file write), budget stop, secrets names-only, persona prompt snapshot.

Phase 1 also gets a typed-or-uploaded test for speech: the Voice Lab
records the kanban sentence and sends it through `stt.js`, to compare with
the Phase 0 results.

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

## Phase 3 — Android: side key (the default trigger) — BUILT 2026-11-05, awaiting the phone

As built (`android/app/src/main/java/mv/obitech/crundi/voice/`):
- Mode A: `CrundiVoiceService` + `CrundiSession` (Compose in the session
  window, over the lock screen) + `CrundiRecognitionService` (a stub Android
  requires). Mode B: `VoiceActivity`, launcher alias "Crundi Voice",
  showWhenLocked; it also asks for the microphone and for an unlock, which
  the overlay can't. Both explained in App settings > Voice assistant (D33).
- `VoiceController`: 16 kHz PCM (VOICE_COMMUNICATION, echo cancelling) →
  `/api/voice/listen` → `/api/voice/turn` (with `locked`) → `Speaker`;
  `/api/voice/warm` on open; `VoiceMicService` (microphone foreground
  service) while open, Phase 0's working setup.
- `VoiceMachine` (pure, JVM-tested): stream paused while speaking; barge-in
  from the local level (echo-cancelled, over the room's noise, 280 ms) with a
  0.5 s pre-roll so the first word isn't lost; what is said while thinking
  is sent next; quiet 30 s → asleep; stream lost or silent 8 s → the phone's
  recogniser (`FallbackRecognizer`, one instance reused).
- `Speaker`: the Deepgram voice a sentence at a time, next one fetched while
  one plays; the phone voice in D34's order otherwise or on failure.
- `VoiceScreen`: the live orb (bars carry the voice round the ring, the core
  swells, a comet turns while thinking), words as heard, the spoken answer,
  the shown detail in a card.
- Settings: on/off (also hides Crundi Voice), modes A and B with setup steps
  and whether Crundi holds the assistant role, voice picker (Deepgram with
  previews, British first; the phone's en-GB voices), model, limit, spend,
  key status. The Usage screen shows the API spend (D41).
- Older server: no `voice-assistant` capability or a 404 → "Update the
  server". MIN_SERVER unchanged.
- The debug Voice Lab no longer registers an assistant (two in one package
  would be ambiguous); it is "Crundi Voice Lab" in the launcher.

To check on the phone: side key locked and unlocked; barge-in (does echo
cancelling keep Crundi's own voice from counting?); the first word after a
barge-in; Deepgram voice latency; offline fallback (airplane mode with Wi-Fi
off).

Original plan:


- `voice/CrundiVoiceService` (`VoiceInteractionService`),
  `CrundiSessionService`, session UI (overlay, works over lock screen),
  `res/xml/voice_interaction_service.xml`, manifest entries.
- Microphone audio streamed to the server for Deepgram (D37), on-device
  recogniser as offline fallback → `POST /api/voice/turn` → text-to-speech
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
- Confirmation flow shared by all of these: spoken "yes" or a tap. Also
  used for every setting change voice makes (D32).
- Route allowlist for `ROLE_VOICE` grows by exactly these routes.
- Tests: each confirm-required action, and every setting change, refuses
  without confirmation; terminal
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
  never unlimited (D4–D6), confirmed before it turns on (D32); notification with time left and Stop; "stopped —
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

Order decided: **0 → 1 → 3 (side key) → 2 (memory) → 4 → 5 → 6.** Phases 1
and 3 can't overlap much (3 needs 1's route). Phase 0 can run alongside 1.
