# Voice assistant ("Hey Crundi") — research

Status: research only, not started. Written 2026-11-04. Build plan: [voice-assistant-plan.md](voice-assistant-plan.md).

Goal: a voice assistant on the Android app, woken by "Crundi" or "Hey Crundi",
that runs on **Claude API credits** and can control every project and read
settings and usage.

## 0. Decisions so far

| # | Decision | Date |
|---|---|---|
| D1 | **Default trigger: Samsung side key** (Crundi as default digital assistant → Side button → Digital assistant). No always-on listening, no battery cost, works over the lock screen (§3.2). | 2026-11-04 |
| D2 | **Gemini is a supported option too:** "Hey Google, open Crundi (Voice)" opens the voice screen, which goes back to sleep after silence (§3.5). AppFunctions to be revisited once Gemini supports third-party apps publicly. | 2026-11-04 |
| D3 | **"Hey Crundi" in the app:** a toggle; when on, Crundi listens for "Hey Crundi" whenever the app is open (foreground only, no foreground service needed). | 2026-11-04 |
| D4 | **"Hey Crundi" in the background:** off by default, switched on either by a toggle in settings or by the voice agent itself through a tool ("keep listening for the next hour"). Runs as a microphone foreground service. | 2026-11-04 |
| D5 | **Background listening turned on by voice always has a time limit.** The agent's tool requires a duration; the service stops itself when it runs out. | 2026-11-04 |
| D6 | **Durations:** 15 min, 30 min, 45 min, 1 h, 2 h. Default **30 min**. When the voice agent turns background listening on, it must pick one of these (2 h is the maximum); no duration given means 30 min. | 2026-11-04 |
| D7 | **Settings toggle offers both:** "Until I turn it off" or a time limit from the D6 list. | 2026-11-04 |
| D8 | **Model is switchable** in Settings → Voice assistant. Default **Claude Haiku 5.5** (`claude-haiku-5-5`). | 2026-11-04 |
| D9 | **Reads everything:** all Crundi settings, services (status, logs), notes, kanban, mindmap, schedules, usage, projects and their folders and files. | 2026-11-04 |
| D10 | **Edits nothing directly:** no file edits, no critical settings. | 2026-11-04 |
| D11 | **Panels:** sees the open panels (chats, terminals), reads their latest output, and can send input to them. | 2026-11-04 |
| D12 | **Turn-end callback:** the voice agent can set itself to be triggered when a turn ends, and Crundi opens on the place where it was asked. | 2026-11-04 |
| D13 | **Voice may edit notes, kanban and mindmap** (exception to D10). Kanban and mindmap deletes are already soft and recoverable. | 2026-11-04 |
| D14 | **Voice may create and edit project-scoped panels** (open, close, arrange chats, terminals, Files/Git/Kanban/Mindmap panels in a project). | 2026-11-04 |
| D15 | **Input to panels:** if the person clearly asked to send or type something, send it without a read-back. If the intent is unclear, read back first. Long prompts are summarised in one line in the read-back, never read in full. | 2026-11-04 |
| D16 | **Secrets:** voice lists secret names, never values. | 2026-11-04 |
| D17 | **Critical settings** (voice can't change them): login (Telegram, password, authenticator), tunnel and forwards, collaborators and access policy, secrets, backup and restore, updates, skills, MCP permissions. Voice may change its own voice settings and background listening (D4–D7). | 2026-11-04 |
| D18 | **Turn end when Crundi isn't open:** notification that opens where it was asked (full-screen if permitted); may speak the result; a spoken reply needs background listening on. | 2026-11-04 |
| D19 | **Voice: a calm, British, butler-style AI voice**, in the spirit of Jarvis but not a copy (see §2.4). | 2026-11-04 |
| D20 | **The user is "Master."** When Crundi addresses the user it says "Master" ("Yes, Master."). It doesn't have to on every reply; examples like "Yes.", "On it.", "Roger." or "Affirmative." are fine. These are examples, not a fixed list: it picks its own words, so it sounds cool, not robotic. | 2026-11-04 |
| D21 | **Input goes to chats only, never terminals.** Voice can still read terminal output (D11) and open or close terminal panels (D14), but can't type into them. Supersedes the terminal part of D11/D15. | 2026-11-04 |
| D22 | **Voice can create projects.** Same as the app's own "add project": a new folder under the projects folder (or registering an existing one). Only file-system change voice may make (exception to D10). | 2026-11-04 |
| D23 | **Voice can start chats in any project**, with the project's default model, effort and mode unless told otherwise; the first message follows D15. | 2026-11-04 |
| D24 | **Voice can close any chat, with confirmation** ("Close the API chat in Crundi? It's idle." → "yes"). Closing works like closing it in the app: the pane is removed (`src/panes.js` forgets it). Voice can also park/sleep a chat instead (D31). | 2026-11-04 |
| D25 | **API key kept on the server** (Voice assistant settings), never sent to the phone. **Spending limit, default $50, adjustable**; when reached, voice stops and says so. Per month. | 2026-11-04 |
| D26 | **One memory, not per project.** Crundi is a single assistant across everything; it remembers, and compacts while idle (§2.5). | 2026-11-04 |
| D27 | **Speech-to-text: Android's built-in on-device recognizer.** | 2026-11-04 |
| D28 | **Long answers** (files, logs, long chat replies): short spoken summary, full text on screen. | 2026-11-04 |
| D29 | **Phone first.** Web UI and desktop later, on the same server side. | 2026-11-04 |
| D30 | **Memory gets its own searchable store:** keyword search plus local embeddings, as long as it stays light on CPU (§2.5). | 2026-11-04 |
| D31 | **Voice can close or park/sleep any chat**, both with confirmation. Parked keeps the pane in place with Resume (`src/panes.js`). | 2026-11-04 |
| D32 | **Every setting change is confirmed with the user first**, including the ones voice is allowed to change (its own voice settings, background listening per D4–D7). One short question, e.g. "Background listening for 30 minutes?" → "yes". Critical settings (D17) stay off-limits either way. | 2026-11-04 |
| D33 | **Trigger mode is the user's choice** (supersedes the D1/D2 pairing, which Phase 0 showed can't both hold). **A:** Crundi is the default digital assistant; hold the side key, works locked; no "Hey Google". **B:** Gemini stays default; side key double-press (or "Hey Google, open Crundi Voice") opens Crundi Voice. The app supports both and the Voice assistant settings explain how to set each up. | 2026-11-05 |
| D34 | **Default voice: Google TTS `en-gb-x-rjs-network`** (British male). Voice names come from the TTS engine, not the phone, but which voices are installed varies by device; fall back to `en-gb-x-rjs-local`, then any en-GB voice. The user can pick any installed voice in Voice assistant settings. A premium cloud voice stays a later option. | 2026-11-05 |
| D35 | **Speech-to-text: start with Android's on-device recogniser** (D27 kept), improved by vocabulary hints and by sending all alternatives to Claude (§2.3). Whisper on the Crundi server is a later option if it proves quick enough on the user's machine. | 2026-11-05 |
| D36 | **No Whisper on the Crundi server.** It runs in the cloud on about 4 shared cores, sometimes fewer: a good Whisper model would take several seconds per command and compete with Claude Code sessions. If on-device + hints + Claude's correction isn't accurate enough, the next step is a cloud speech-to-text API (pay per minute, under a second), key on the server. | 2026-11-05 |
| D37 | **Speech-to-text: Deepgram** (Nova-3, streaming, keyterm prompting with Crundi's vocabulary). Supersedes D27/D35 after Phase 0 (results 0.4 and 0.4b in the plan). Key on the Crundi server (Voice assistant settings), never on the phone. New accounts get a $200 free credit. Android's on-device recogniser stays as an offline fallback. | 2026-11-05 |
| D38 | **Anthropic has no speech-to-text for the API.** The Messages API takes text, images and PDFs, not audio. Claude Code's voice dictation uses Anthropic's own speech-to-text, but only for claude.ai sign-ins, not API keys, so Crundi can't use it. Re-check when Anthropic adds audio. | 2026-11-05 |

### Background listening (D4, D5) — how it fits Android's rules

- The voice agent's tool call reaches the phone while a voice session is on
  screen, so Crundi is in the foreground and is allowed to start the
  microphone foreground service. Same for the settings toggle. To verify on
  device: that a side-key `VoiceInteractionSession` window counts as
  foreground for this rule.
- Tool shape (server → phone): `set_background_listening({ minutes })`, with
  `minutes` an enum of 15 | 30 | 45 | 60 | 120 (D6), default 30. "Until
  turned off" is not available to the voice tool, only to the settings
  toggle (D7). Turning it off needs no duration.
- The service keeps its own deadline and stops itself at it; the ongoing
  notification shows the time left and a Stop button.
- If Android or Samsung kills the service it can't restart itself from the
  background. Post a notification ("Hey Crundi stopped — tap to resume").

### Access (D9–D11) — notes

- Secrets (D16): names only, never values.
- **Input to chats (D15, D21).** Terminals are read-only to voice. A message
  to a Claude Code chat can still ask it to edit files, so this is where D10
  is enforced. Design:
  - `send_to_chat({ chat, text, intent })` (no terminal equivalent) where `intent` is `explicit`
    (the person said "tell it…", "type…", "send…", "reply…") or `inferred`
    (the agent decided to send something on its own reading).
  - `explicit` → send. `inferred` → read back, then send on "yes".
  - Read-back of a long prompt: one line, e.g. "I'll ask the API chat to
    refactor the auth middleware and add tests, about 120 words. Send?"
    The full text is shown on screen.
  - The model labels `intent` itself, so it's a guide, not a guarantee. Extra
    server-side guard to consider: always read back for terminals when the
    text looks destructive (`rm`, `git push --force`, `reset --hard`, `drop`).
- Critical settings: see D17. Other settings stay read-only under D10 unless
  added later.

### Turn-end callback (D12) — how it fits

- Server side already exists: `onTurnEnd` in `src/claude-ui.js:2169` and the
  `turn-end` trigger in `src/chat-schedule.js`. A voice watch is one more
  listener: `{ chatId | terminalId, project, askedFrom }`.
- On the phone, Android blocks apps from opening themselves from the
  background (Android 10+). Ways to "open where it was asked":
  - **Screen on and unlocked, Crundi in front:** navigate there and speak
    directly.
  - **Otherwise:** a high-priority push (existing `src/mobile-push.js` /
    `PushService`) shows a notification; tapping it opens the asked-from
    screen with the voice session. With the opt-in full-screen-intent
    permission (already used by urgent alerts) it can take over the screen
    like an alarm.
  - **Speaking the result without a tap:** possible from the push with
    text-to-speech, but listening for a reply then needs background listening
    to be on (D4), because the microphone can't be started from the
    background.
- "Where it was asked" = the screen and panel the voice session was opened
  from, saved with the watch.

### Lock screen

- Side key / assist gesture: the assistant session shows over the lock screen,
  no unlock needed.
- Gemini: usually asks to unlock before opening another app (to test on
  device), even though Crundi's voice screen can use `showWhenLocked` like
  `AlertActivity`.
- While locked, allow read-only answers only; anything that changes state
  asks to unlock (§4).

## 1. Findings that change the plan

1. **Porcupine is probably no longer free.** Community reports say Picovoice
   ended its free tier and disabled free AccessKeys after 2026-06-30, moving
   to enterprise-only. Not confirmed on Picovoice's own pricing page. Plan on
   an open-source engine.
2. **Android 14+ won't start a microphone foreground service from the
   background.** The app targets SDK 36 (`android/app/build.gradle.kts`), so
   the listener must be started while Crundi is on screen. If it's killed it
   can't restart itself, and it can't start on boot.
3. **The Claude API has no speech input or output** (D38). Speech-to-text and text-to-speech run on the phone (or in another
   engine).
4. **API credits are a separate bill from the subscription.** Crundi
   deliberately drives the logged-in `claude` CLI rather than the Agent SDK
   (`src/claude-ui.js:10-15`). The voice assistant would use an API key, which
   is metered. Useful split: the voice "brain" runs on API credits and only
   dispatches; heavy coding is delegated to Claude Code chats on the
   subscription.
5. **Spelling doesn't matter to the detector.** Wake-word engines match sound;
   "Crundi" and "Krundi" are the same word to them. Spelling is display only.

## 2. Architecture

```
PHONE (android/)                              SERVER (src/)
Wake word ("hey crundi")  ─┐
Side button / assist      ─┼─▶ VoiceInteractionSession (overlay, lock screen)
Quick Settings tile       ─┘        │
                                    ▼
             On-device speech-to-text (Android SpeechRecognizer)
                                    │ text
                                    ▼
             POST /api/voice/turn  ───────────────▶  voice.js: Claude API tool loop
                                                       (API key stays on server)
                                                       tools = Crundi's existing
                                                       project tools
             ◀─────── streamed reply text ─────────
             TextToSpeech speaks it
```

The Claude call runs on the server because:
- the API key never leaves the machine;
- Crundi's tools already live there (`src/mcp-stdio.js`: services, kanban,
  schedule, `get_usage`, mindmap…) and can be reused as Messages API tools;
- the phone already pairs with the server.

### 2.1 What voice can reach

| Ask | Source |
|---|---|
| Subscription usage (5-hour / weekly %) | `src/usage.js` (already used by `get_usage`) |
| API credit spend of the voice assistant | sum `response.usage` per turn locally; org-wide reports need an Admin API key (`sk-ant-admin…`) and raw HTTP (`/v1/organizations/usage_report/messages`, `/cost_report`) — optional |
| Projects | list projects, kanban, services (start/stop/restart/logs), schedules, git status |
| Delegate | "start a chat in project X saying …" → opens a Claude Code chat on the subscription |
| Settings | read-only first; changes need on-screen confirmation |

### 2.2 Model and latency

- Default (D8): Claude Haiku 5.5 (`claude-haiku-5-5`), $0.10/$0.50 per MTok.
  Switchable in settings, e.g. to Claude Sonnet 5.5 or Claude Opus 5.5 at
  `output_config.effort: "low"`. System line: "Latency-sensitive; begin your
  visible answer immediately".
- Haiku 5.5 has no server-side refusal fallback; handle `stop_reason:
  "refusal"` by saying so.
- Cache the system prompt and tool list (stable prefix), so repeat turns are
  cheaper and faster. Keep timestamps and per-turn state after the breakpoint.
- Stream the reply so TTS can start on the first sentence.
- On Opus 5.5 / Sonnet 5.5, forced `tool_choice` is a 400: use `auto`.

### 2.3 Speech on the phone

- Speech-to-text (D27, D35): `SpeechRecognizer.createOnDeviceSpeechRecognizer`
  (API 31+). `RECORD_AUDIO` is already in the manifest. Phase 0 showed it is
  fast but mishears rare words ("kanban" → "compound"). Mitigations:
  - **Vocabulary hints:** `RecognizerIntent.EXTRA_BIASING_STRINGS` (Android
    13+) with Crundi's words: Crundi, kanban, mindmap, project names, service
    names.
  - **Send every alternative, not just the top one:** the recogniser returns
    several guesses with confidence scores. Claude gets them all, plus the
    same vocabulary, and works out the likely request ("as a compound task" +
    "kanban" in the vocabulary → a kanban task).
  - **Show what was heard** on screen; one tap to say it again.
  - **If still not good enough:** a cloud speech-to-text API with phrase
    hints (D36). Whisper on the server is ruled out on a ~4-core shared
    machine.
- Text-to-speech: Android `TextToSpeech` (Samsung voices available).

### 2.4 The voice (D19)

Cloning or imitating Paul Bettany's actual voice is off the table (it's a real
person's voice). A calm, dry, British male "AI butler" voice gets the feel.
Candidates, cheapest first:

| Option | Where it runs | Notes |
|---|---|---|
| Android `TextToSpeech`, Google engine, en-GB male voice | phone, free, offline | Pick the voice by `Voice.name`/locale; quality varies by voice pack. |
| Samsung TTS, UK English male | phone, free | Availability depends on One UI version. |
| Google Cloud TTS (en-GB Chirp HD / Neural2 male) | cloud, per character | Very natural; adds a network hop. |
| Azure Speech (e.g. en-GB-RyanNeural) | cloud, per character | Natural British male; supports speaking styles via SSML. |
| ElevenLabs library voices ("British butler" style) | cloud, per character | Most characterful; most expensive. Use a stock library voice, not a clone. |

Plan: start with on-phone Android TTS (free, works offline), make the voice
selectable in Voice assistant settings, and add one cloud voice as an option.
Persona in the system prompt: concise, dry wit, polite. Addresses the user as
"Master" when it addresses them at all (D20), e.g. "Yes, Master." Short
acknowledgements are fine without it. "Yes.", "On it.", "Roger.",
"Affirmative." are examples to set the tone, not a list to choose from: it
uses its own words. Cool, not robotic.

### 2.5 Memory (D26)

One running conversation for the whole assistant, a long-term notebook, and
a searchable store over both (D30).

1. **Running conversation.** Every voice turn is appended to one history on
   the server. Recent turns are sent as-is.
2. **Compaction while idle.** After a quiet period (e.g. 10 min), Crundi asks
   the model to summarise older turns into a short "what we've been doing"
   block and replaces them with it. Options: the API's server-side compaction
   (beta `compact-2026-01-12`), or Crundi's own summary step — the latter
   fits "compact while idle" better because it runs at a time we choose, not
   when a size threshold is hit.
3. **Long-term notebook.** The Claude memory tool (`memory_20250818`) lets the
   model keep notes in files Crundi stores on the server: preferences ("Master
   prefers short answers"), standing facts ("the API project is called…"),
   ongoing threads. Survives compaction; viewable and editable in Voice
   assistant settings.
4. **Overview.** Because it's one memory, "what have we been working on this
   week?" works across all projects.

5. **Searchable store (D30).** Hybrid search, light on CPU:
   - **Keyword:** SQLite full-text search (FTS5, BM25 ranking). Practically
     free on CPU. Node 22 ships `node:sqlite`, so no new native dependency —
     to verify FTS5 is enabled in the Node build Crundi bundles (Electron/deb).
   - **Meaning** (sized for a ~4-core shared server, D36): a small local embedding model (e.g. all-MiniLM-L6-v2 or
     bge-small, ~25–35 MB, 384 dimensions) run through ONNX. Roughly 5–20 ms
     per sentence on a normal CPU, and only when something is written or
     searched; nothing runs in a loop. Cost: one new dependency (ONNX runtime,
     tens of MB).
   - **Vector search without a vector database:** at one person's scale
     (thousands to tens of thousands of entries) comparing the query against
     every stored vector takes milliseconds. Vectors live as blobs in the same
     SQLite file; no extension or server needed.
   - **Ranking:** merge keyword and meaning results (reciprocal rank fusion).
   - **What gets indexed:** every turn (before compaction removes it), the
     compaction summaries, and the notebook. So compacted detail is still
     findable.
   - **Tool:** `memory_search({ query, since?, project? })` for the agent.
   - **Alternative with zero local CPU:** a hosted embeddings API (Anthropic
     recommends Voyage AI; Anthropic has no embeddings endpoint of its own).
     Costs a little per call and needs another key and the network. Keep as an
     option if the local model is too heavy on a given machine.

Cost note: the cached prefix (system prompt + tools + summary) keeps repeat
turns cheap; compaction keeps the history from growing without limit.

## 3. Wake word on a Samsung without root

| Option | Works | Notes |
|---|---|---|
| Hardware hotword (`AlwaysOnHotwordDetector`, `HotwordDetectionService`) | No | Needs privileged permissions (`MANAGE_HOTWORD_DETECTION`, `CAPTURE_AUDIO_HOTWORD`) and a system enrolment app; reserved for Bixby/Google. |
| **sherpa-onnx keyword spotting** | Yes — **start here** | Open vocabulary: type "crundi" / "hey crundi" as the keyword, no training. Apache-2.0, Android demo exists. |
| openWakeWord (Kotlin port) | Yes — upgrade path | Train a "hey crundi" model from synthetic speech; usually fewer false triggers. |
| Porcupine | Doubtful | Free tier reportedly gone (§1). |
| Looping `SpeechRecognizer` | No | Beeps, times out, drains battery. |

### 3.1 Always-listening service rules (Android 14+)

- New foreground service, `foregroundServiceType="microphone"`, separate from
  `LinkService` (`specialUse`). Needs `FOREGROUND_SERVICE_MICROPHONE` and an
  ongoing notification.
- Start it only from the foreground: an in-app "Listen for Hey Crundi" switch.
  No start on boot, no self-restart after being killed.
- Samsung: battery Unrestricted, add to **Never sleeping apps**. The green
  mic dot stays on while listening.
- As the default digital assistant the system keeps the
  `VoiceInteractionService` bound, which should help the process survive. One
  unverified source (an /e/OS fork commit) suggests the assistant may be
  exempt from the background-mic rule; test on device, don't rely on it.
- Android 17 "background audio hardening" tightens this again for apps
  targeting API 37; retest when moving targetSdk.

### 3.2 Triggers that need no listening

- Samsung side key → Advanced features → Side button → Digital assistant,
  once Crundi is the default assistant.
- Long-press home / corner swipe (assist gesture).
- A Quick Settings tile.

All feed the same `VoiceInteractionSession`; the wake word is one entry point
among several.

### 3.3 Default assistant registration (from owner's research, confirmed)

Manifest:

```xml
<service
    android:name=".voice.CrundiVoiceService"
    android:permission="android.permission.BIND_VOICE_INTERACTION"
    android:exported="true">
    <meta-data android:name="android.voice_interaction"
        android:resource="@xml/voice_interaction_service" />
    <intent-filter>
        <action android:name="android.service.voice.VoiceInteractionService" />
    </intent-filter>
</service>
```

`res/xml/voice_interaction_service.xml`:

```xml
<voice-interaction-service
    xmlns:android="http://schemas.android.com/apk/res/android"
    android:sessionService="mv.obitech.crundi.voice.CrundiSessionService"
    android:recognitionService="mv.obitech.crundi.voice.CrundiRecognitionService"
    android:supportsAssist="true"
    android:supportsLocalInteraction="true" />
```

Then Settings → Apps → Choose default apps → Digital assistant app → Crundi.

## 3.4 Why it can't sleep and wake like Gemini

Gemini, Google Assistant and Bixby wake from deep sleep through the phone's
low-power audio chip (DSP). The chip listens for one keyphrase model and wakes
the main processor only on a match. Android's API for this is
`AlwaysOnHotwordDetector` (plus `HotwordDetectionService` on Android 11+), but
it is gated:

- It needs `MANAGE_HOTWORD_DETECTION` / `CAPTURE_AUDIO_HOTWORD`, which only
  preinstalled privileged apps get. Choosing Crundi as the default assistant
  does not grant them.
- The keyphrase model has to be in the chip vendor's own format and enrolled
  through a system enrolment app. Samsung's models for Bixby and Google's are
  proprietary.
- Root (Magisk, `/system/priv-app` + a privapp-permissions allowlist) could
  grant the permissions. On Samsung it permanently trips Knox, which breaks
  Samsung Wallet and Secure Folder, and it still gives no way to load a
  "Crundi" model into the DSP. Not worth it.

Closest options without root:

- **Software wake word in a microphone foreground service** (§3, §3.1). Works
  with the screen off, but the main processor stays partly awake, so it uses
  more battery than Gemini, shows the mic dot and an ongoing notification, and
  must be switched on from the app.
- **Piggyback on a wake word the phone already has:**
  - "Hi Bixby" + a Bixby Quick Command (e.g. "Crundi") that opens Crundi's
    voice screen. Uses the DSP, so no battery cost. Untested: whether a Quick
    Command can launch a specific activity or deep link, or only open the app.
  - "Hey Google, open Crundi". Works for any app. Google App Actions
    ("Hey Google, ask Crundi …") are uncertain since Gemini replaced Assistant.
- **No-listening triggers** (§3.2): side key, assist gesture, tile.

## 3.5 Using Gemini to wake Crundi ("Hey Google, open Crundi")

The idea: Gemini's own hardware wake word does the sleeping and listening, and
it hands over to Crundi, which turns on its own listening and goes back to
sleep when nobody is talking.

**Works today, no special permissions:**

1. "Hey Google, open Crundi" (the wake phrase for Gemini on Android is still
   "Hey Google"). Gemini opens the app on its low-power wake word; Crundi costs
   no battery while waiting.
2. Crundi opens straight to a voice screen. Because the app is now in the
   foreground, it is allowed to start the microphone foreground service
   (§1.2's background rule doesn't bite).
3. Conversation runs on Claude (§2). After N seconds of silence (e.g. 30–60 s)
   Crundi stops the mic, stops the service and closes: back to sleep.

Ways to land directly on the voice screen, to test on device:
- an `activity-alias` with its own launcher label, e.g. "Crundi Voice" →
  "Hey Google, open Crundi Voice";
- a static app shortcut (`shortcuts.xml`) named "Voice";
- the same entry Bixby Quick Commands would open (§3.4).

Limits: a locked phone usually has to be unlocked before Gemini opens an app;
you say Google's wake word first, not "Hey Crundi".

**Later, possibly: AppFunctions (Android 16+).** Apps expose functions (like MCP
tools) that Gemini can call directly: "Hey Google, ask Crundi how much usage I
have left" with no Crundi screen at all. Caveats:
- Gemini's support for third-party AppFunctions was a private preview with
  trusted testers as of May 2026; not generally available.
- Gemini becomes the voice and the brain. Crundi's functions could still
  forward the question to Claude and return the answer, but Gemini decides
  when to call them and phrases the reply.
- Worth adding once public, as a second entry point alongside 1–3 above.

## 4. Security

- Anyone (or a TV) can say "Hey Crundi". Destructive actions — stop service,
  push, delete, change settings — need an on-screen confirmation, ideally with
  biometrics. Reuse `src/approvals.js` and `BiometricPins.kt`.
- Keep the secrets vault and `request_owner_command` out of the voice tool list.
- Locked phone: read-only answers only.
- API key stored server-side only (like `ANTHROPIC_API_KEY` handling in
  `src/secret-run.js`), never sent to the phone.

## 5. Phases

1. **Server:** `src/voice.js` tool loop + `/api/voice/turn`, built from the
   existing tools, read-only (usage, projects, kanban, service status). Test
   from the web UI with typed text.
2. **Android assistant (default trigger, D1):** `VoiceInteractionService` +
   session + on-device STT + TTS, so the side key and assist gesture work,
   including over the lock screen.
3. **Gemini entry (D2):** a "Crundi Voice" launcher alias / app shortcut that
   opens the same voice screen, with the silence timeout.
4. **"Hey Crundi":** sherpa-onnx wake word, in-app toggle first (D3), then
   the background service with its toggle, the voice tool and time limit
   (D4, D5).
5. **Write actions** with confirmations, then delegation to Claude Code chats.

## 6. Open questions

- Does being the default assistant exempt the mic service from the
  background-start rule on One UI? Test on device.
- False-accept rate of sherpa-onnx on "crundi" in a noisy room; switch to a
  trained openWakeWord model if too high.

## Sources

- https://developer.android.com/ai/appfunctions
- https://android-developers.googleblog.com/2026/02/the-intelligent-os-making-ai-agents.html
- https://9to5google.com/2026/02/25/android-appfunctions-gemini/
- https://developers.google.com/assistant/app/deployment

- https://community.home-assistant.io/t/fyi-picovoice-confirmed-free-tier-accesskeys-will-stop-working-after-june-30-2026/1012744
- https://community.home-assistant.io/t/porcupine-free-tier-shutdown-alternatives-for-home-assistant-voice-users/1012382
- https://picovoice.ai/pricing
- https://k2-fsa.github.io/sherpa/onnx/kws/index.html
- https://github.com/Re-MENTIA/openwakeword-android-kt
- https://developer.android.com/develop/background-work/services/fgs/service-types
- https://developer.android.google.cn/about/versions/14/changes/fgs-types-required
- https://developer.android.com/about/versions/17/changes/bg-audio
- https://gitlab.e.foundation/e/os/android_frameworks_base/-/commit/af766934d78894af3b94a1a7a5dd8a98b4b11cfd
- https://www.outlookbusiness.com/artificial-intelligence/anthropic-launches-beta-speech-mode-for-claude-ai-chatbot-adds-voice-conversations
