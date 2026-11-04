# Voice assistant — handover prompt

Paste everything below the line into the agent that continues the work.

> **Update 2026-11-05:** the "Not verified yet" list below has been done
> (all five work; results and fixes in the plan, "Phase 1 verified against
> the real services"), the keys are saved in Voice assistant settings, and
> Phase 3 is built in `android/…/voice/` (as built: the plan's Phase 3). Next:
> the phone test of Phase 3, then the order in the plan. New decisions:
> D39–D41.

---

You are continuing development of the **Crundi voice assistant** ("Hey Crundi")
in the repository `TheRealFaruhaan/crundi`. Research, decisions and the build
plan are done; Phase 0 (device checks) and Phase 1 (server brain) are built.
Your job: verify what's built against the real services and the phone, fix
what doesn't work, then build Phase 3 (the Android side-key assistant).

## Branches

- **Plan branch: `claude/voice-assistant-research`**: docs only. The spec
  and plan. Keep it in sync when decisions change.
- **Edits branch: `claude/voice-assistant`**: all code, plus the same spec
  files. Do your work here (or on a branch made from it). Don't push to `main`
  or `dev`, and don't open a pull request unless the owner asks.

```bash
git fetch origin claude/voice-assistant claude/voice-assistant-research
git checkout claude/voice-assistant
npm install
```

## Read first

1. `specs/voice-assistant.md`: the design. **Section 0, "Decisions so far"
   (D1–D38), is the source of truth.** Later decisions supersede earlier ones
   where they say so (e.g. D33 replaces the D1/D2 pairing, D37 replaces
   D27/D35).
2. `specs/voice-assistant-plan.md`: phases, the Phase 0 results from the
   owner's phone, and how Phase 1 was actually built.
3. `README.md` and `skills/crundi/reference/developing-on-crundi.md` for the
   project itself.

## The owner, and how they want to work

- They are addressed as **"Master"** by the assistant (D20), and the
  assistant's style is a calm, dry, British butler; acknowledgements in its
  own words, not a fixed list (cool, not robotic).
- They decide; you propose. Record every new decision as a `D##` row in
  section 0 of the spec, on both branches.
- Phone: **Samsung Galaxy S24 Ultra (SM-S928B), Android 17 (API 37)**.
- The Crundi server runs **in the cloud on about 4 shared cores** (D36): no
  heavy local models.
- API keys (Claude, Deepgram) go into Crundi's Voice assistant settings on
  the server, never into code, commits or chat.

## What exists

Server (`src/voice/`, wired in `src/webapp.js`, owner-only;
`/api/voice` is in `COLLABORATOR_NEVER` in `src/access-policy.js`):

| File | What it does |
|---|---|
| `agent.js` | Claude API tool loop. Default `claude-haiku-5-5` (switchable to Sonnet/Opus 5.5), effort `low`, append-only history (thinking blocks passed back unchanged), reply split into spoken part / `---` / shown detail, refusal handling, server-side fallback beta on Opus/Sonnet only. **Caching:** 1-hour breakpoint on the system block, 1-hour automatic breakpoint on the conversation, volatile context only in the new user message; `warm()` re-sends the last request with `max_tokens: 0` when a session opens. |
| `tools.js` | 20 **read-only** tools (projects/files inside the project, settings with credentials redacted, services and logs, notes, kanban, mindmap, schedules, usage, secret names only, chats and terminals read-only). The list is pinned by the test. |
| `settings.js` | `<dataDir>/voice.json` (0600): model, monthly limit (default $50), Claude + Deepgram keys, `ttsVoice`, monthly spend (Claude + Deepgram). |
| `stt.js` | Deepgram Nova-3, one recording, keyterms = Crundi vocabulary + live project and service names. |
| `stream.js` | WebSocket relay `/api/voice/listen` ↔ Deepgram streaming (interim words, endpointing + utterance end, pause/resume while Crundi talks). |
| `tts.js` | Deepgram Aura voices: list from `GET /v1/models`, speak via `/v1/speak` (mp3). |
| `page.js` | Test page at **`/voice`**: settings, live listening, hold to talk, typed input, voice picker. |

Routes: `GET/POST /api/voice/settings`, `POST /api/voice/turn`,
`/api/voice/reset`, `/api/voice/warm`, `/api/voice/transcribe`
(`?turn=1`), `GET /api/voice/voices`, `POST /api/voice/speak`, WS
`/api/voice/listen`.

Android: **Voice Lab** (debug builds only), `android/app/src/debug/…/voicelab/`.
It's an assistant `VoiceInteractionService` + session overlay, a microphone
foreground service test, a "Crundi Voice" launcher alias, TTS and speech
recognition checks, and a copyable report. Release and Play builds don't
contain it.

Tests: `node scripts/test-voice.mjs` (103 checks; part of `npm run check`).
In a container running as root, `test-collaborator-access.mjs` and
`test-service-stop.mjs` fail **without these changes too**; don't chase
them, and note that `npm run check` stops at the first failure.

## Not verified yet: do this first

Nothing has been run against the real Claude API or Deepgram (the building
environment couldn't reach them; all tests use fakes). With the owner's keys
saved via `/voice`:

1. **Claude turn:** type "how much usage do I have left?" and "what's in the
   todo column of <project>?". Expect a spoken short answer, detail on screen.
2. **Caching:** ask 3–4 questions in a row. The `/voice` log shows "% from
   cache" per answer; after the first it should be high (most input read
   from cache). If it stays at 0, find the invalidator (see the header of
   `agent.js`; check `usage.cache_read_input_tokens`).
3. **Deepgram one-shot:** "Hold to talk", say "Add the UI overhaul of the
   home screen as a kanban task". Android's recognisers never got "kanban"
   (Phase 0); Deepgram with keyterms should.
4. **Deepgram live:** "Start listening": words should appear while speaking
   and each sentence should be answered after a ~1 s pause. Check the
   relay's assumptions in `stream.js` against Deepgram's docs if not:
   message types `Results` / `UtteranceEnd` / `SpeechStarted`, `is_final`,
   `speech_final`, `KeepAlive`, `CloseStream`, the `Authorization: Token`
   header, WebM/Opus without an `encoding` parameter.
5. **Voices:** the picker should list Deepgram voices with accents. If
   `/v1/models` has a different shape, fix `parseVoices()` in `tts.js` (it
   falls back to a fixed list of Aura-2 names otherwise). Deepgram's docs
   mention a newer "Flux" TTS: check how it relates to Aura-2. The owner wants
   a clean British male voice "in the spirit of Jarvis". **Never clone or
   imitate the actor's real voice**; use a stock voice.
6. Report findings to the owner and record results in the plan.

## Phase 0 facts (from the owner's phone, keep in mind)

- Side key → `VoiceInteractionSession` overlay works **locked and unlocked**,
  and a `microphone` foreground service started from the overlay records
  real audio in both cases.
- Making Crundi the default assistant **disables "Hey Google"**. Hence D33:
  the trigger mode is the user's choice. A: Crundi is the default assistant
  (hold the side key). B: Gemini stays default (double-press the side key /
  "Hey Google, open Crundi Voice").
- Google TTS voices were rejected as not good enough. Default device voice if
  used: `en-gb-x-rjs-network` → `en-gb-x-rjs-local` → any en-GB (D34).
- Android on-device / default speech recognition mishears Crundi words even
  with biasing strings. Hence Deepgram (D37). Anthropic's API has no
  speech-to-text (D38).

## Next: Phase 3, the Android side-key assistant (default trigger)

Build the real thing in the main source set (not `src/debug`), per the plan
and decisions D1–D38:

- A `VoiceInteractionService` + session overlay (shows over the lock screen),
  plus the "Crundi Voice" activity for mode B. Both setups are supported and
  explained in app settings (D33).
- Microphone → stream to `/api/voice/listen` over the paired connection the
  app already uses (`android/app/src/main/java/mv/obitech/crundi/api/`) →
  utterance → `/api/voice/turn` → speak: the Deepgram voice if `ttsVoice` is
  set (`/api/voice/speak`), else the device voice (D34). Call
  `/api/voice/warm` the moment the session opens.
- On-device recogniser only as an offline fallback (D37). Reuse one
  recogniser; recreating it caused frequent error 11 in the lab.
- Locked phone: read-only answers; anything that changes state asks to
  unlock (§4 of the spec).
- Pause streaming while speaking; barge-in stops speech.
- Voice assistant section in app settings: on/off, trigger mode help, voice
  picker (device voices + Deepgram), model, limit, spend.
- The server's `MIN_CLIENT` / the app's `MIN_SERVER` rule (see
  `android/app/build.gradle.kts` and `src/client-version.js`): a new feature
  checks server capabilities and degrades, it doesn't raise the minimum.

Then the order the owner chose: **Phase 2 (memory)**, then **4 (actions,
with the confirmation rules D13–D15, D21–D24, D31, D32)**, then **5
(Gemini entry)**, then **6 ("Hey Crundi": D3–D7, durations 15/30/45/60/120
min, default 30)**.

## Building and testing Android

- CI (`.github/workflows/android.yml`) builds and unit-tests on every push
  that touches `android/`; the artifact `crundi-android-github-debug` is the
  installable debug APK.
- Locally: `cd android && ./gradlew testGithubDebugUnitTest assembleGithubDebug`.
  With the phone on adb: `adb install -r app/build/outputs/apk/github/debug/app-github-debug.apk`.
- The debug APK has the same package name as the store app; a phone with
  the Play/release install must uninstall it first.

## Rules

- Commit with clear messages; push to `claude/voice-assistant`. No PR
  unless asked.
- Keep the Claude conversation append-only and cache-friendly (see
  `agent.js`): never edit or drop earlier turns, never put changing values
  before the new user message, keep model/effort/tools fixed within a
  conversation.
- No forced `tool_choice`, no `temperature`/`top_p`/`top_k`, no thinking
  budgets on these models.
- Voice can never type into terminals (D21); write tools arrive only in
  Phase 4, each behind the confirmation rules; every setting change is
  confirmed (D32); critical settings are off-limits (D17); secret values
  never reach the model (D16).
