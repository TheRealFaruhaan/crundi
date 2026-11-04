# Voice assistant ("Hey Crundi") — research

Status: research only, not started. Written 2026-11-04.

Goal: a voice assistant on the Android app, woken by "Crundi" or "Hey Crundi",
that runs on **Claude API credits** and can control every project and read
settings and usage.

## 1. Findings that change the plan

1. **Porcupine is probably no longer free.** Community reports say Picovoice
   ended its free tier and disabled free AccessKeys after 2026-06-30, moving
   to enterprise-only. Not confirmed on Picovoice's own pricing page. Plan on
   an open-source engine.
2. **Android 14+ won't start a microphone foreground service from the
   background.** The app targets SDK 36 (`android/app/build.gradle.kts`), so
   the listener must be started while Crundi is on screen. If it's killed it
   can't restart itself, and it can't start on boot.
3. **The Claude API has no speech input or output** as far as official sources
   show. Speech-to-text and text-to-speech run on the phone (or in another
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

- Default: Claude Opus 5.5 (`claude-opus-5-5`) at `output_config.effort: "low"`,
  with a system line "Latency-sensitive; begin your visible answer immediately".
- Cheaper option to evaluate: Claude Haiku 5.5 (`claude-haiku-5-5`),
  $0.10/$0.50 per MTok against $4/$20 — likely enough for routing. Owner's call.
- Cache the system prompt and tool list (stable prefix), so repeat turns are
  cheaper and faster. Keep timestamps and per-turn state after the breakpoint.
- Stream the reply so TTS can start on the first sentence.
- On Opus 5.5 / Sonnet 5.5, forced `tool_choice` is a 400: use `auto`.

### 2.3 Speech on the phone

- Speech-to-text: `SpeechRecognizer.createOnDeviceSpeechRecognizer` (API 31+).
  `RECORD_AUDIO` is already in the manifest. Fully offline alternative:
  sherpa-onnx speech recognition models.
- Text-to-speech: Android `TextToSpeech` (Samsung voices available).

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
2. **Android assistant:** `VoiceInteractionService` + session + on-device STT +
   TTS, so the assist gesture and side key work.
3. **Wake word:** sherpa-onnx microphone foreground service behind an opt-in
   switch.
4. **Write actions** with confirmations, then delegation to Claude Code chats.

## 6. Open questions

- Opus 5.5 at low effort vs Haiku 5.5 for the voice brain — measure latency
  and cost on real commands.
- Does being the default assistant exempt the mic service from the
  background-start rule on One UI? Test on device.
- False-accept rate of sherpa-onnx on "crundi" in a noisy room; switch to a
  trained openWakeWord model if too high.

## Sources

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
