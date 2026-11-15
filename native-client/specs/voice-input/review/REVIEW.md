# 2.24 Voice input: design review, round 1

**Not approved.** 3 blocking differences (below).

## How it was run

- Test server `tools/test-server.sh 4814`, with `tools/deepgram-stub.mjs 4816` (`DEEPGRAM_HOST`, `NODE_TLS_REJECT_UNAUTHORIZED=0`), key `stub`, voice `aura-2-draco-en`.
- The spec's layout: *Fruit list…* (plan mode, draft "A half-typed draft") on top, *Second chat* (62 %) and *Narrow chat* (38 %) below, 1400×860.
- Native debug build, `CRUNDI_TEST_MIC_WAV` (a 180 Hz tone). No sound card on this machine, so a private ALSA config (`ALSA_CONFIG_PATH`, `pcm.!default null`) gave it an output. The stub was SIGSTOPped mid-reply to hold *Speaking*.
- Real X input (xdotool) for the taps, holds, the slide onto ×, hovers and the voice line; the stub's `/ctl` for the words; `speak` through `/api/mcp/call`, and once from a real Claude (Allow on its Speak permission, said in voice mode).

## Files

`<state>-top.png`, `-second.png`, `-narrow.png`: web over native, each pane's composer. `<state>-full.png`: web on the left, native on the right. The web shot some states in a different pane from native:
- after-ptt-speaking: web *Second chat*, native top;
- err-*: web *Second*/*Narrow*, native *Narrow* (refused, ended) and *Second* (closed, no microphone);
- no-voice-server: native with the key cleared, then restarted.

Extra native shots:
- `native-aplus-strip.png`: A+ ×3, the strip's texts scale.
- `native-after-ptt-paused.png`: the voice line "Paused".
- `native-err-no-microphone.png`: no capture device.
- `native-real-speak-allowed.png`: a real `speak`, said.

## Matches

- The mic is hidden with no key, and shown with one.
- Its place in the row:
  - wide: between attach/Stop and Send;
  - narrow: between the clock and Send.
- Hover look.
- On: the send icon and the pulse ring; the sky ring while speaking; the outlined "away" mic.
- A tap turns voice mode on; a hold of 350 ms or more is push to talk.
- The box: read only, accent fill, the italic "Listening…", growing and following the newest words.
- The draft comes back exactly.
- Words: interim, then the utterance sent as `voice input: …`. The time restarts; scratch that drops the words and keeps the time; send now.
- × drops the words and stops the reply and its queue.
- Slide onto ×: the red-dim strip and a bigger red ×; let go there and nothing is sent.
- Let go elsewhere: sent.
- Pause by a tap while speaking, then resume after 1.5 s.
- The voice line:
  - "Speaking…" / "Paused" after a hold, and a click stops the reply;
  - "Paused: listening in another chat", and the chat resumes after the hold;
  - "Voice mode moved to another chat.".
- The narrow strip fills the row.
- Esc.
- Errors: Deepgram refused, connection closed, chat ended, no microphone ("for Crundi", named).
- A+/A- scale the strip.

## Blocking

1. **Wide composer: the strip takes the row and squeezes the box.**
   - Every recording state in a wide pane: listening, interim, long, sent-working, speaking, paused, cancel-hover, ptt-held, ptt-slide-cancel, paused-other-chat (*Second chat*).
   - Web: the box keeps the row (`flex:1`), and the strip is as wide as its contents (at least Send's width, about 145 px here).
   - Native: in the 1148 px top pane, the strip is about 575 px and the box about 420 px. The strip is about 350 px in *Second chat*.
   - When a hold starts, the mic jumps left (x 1301 → 745), so the held pointer ends up over the strip.
2. **The slide-cancel status says "Let go to cancel".**
   - Web (`web-ptt-slide-cancel`): the strip keeps its status, *Listening*, in red. "Let go to cancel" is only the mic's aria-label (claude-chat.js 3382).
   - Native: the visible strip text changes to "Let go to cancel".
   - This is a wording difference, not a Slint gap. PLAN.md names it, but nothing forces it.
3. **No device voice.**
   - Web: with no Deepgram voice chosen (`ttsVoice` is `''`, the server's default, src/voice/settings.js 71), or over the Deepgram limit, replies are said with the browser's voice (en-GB, `speechRate`), with *Speaking*, the sky ring and the line.
   - Native: the reply is not said, and nothing shows it.
   - Spec §10.4 asks for the OS voice (`tts` crate). Only "a system with no voice" may be a Known gap. This is a missing feature, not a Slint gap.

## Not findings

- The mp3 fallback: every server with `chat-voice` already streams PCM (`stream=1` predates `chat-voice`), so it can't happen.
- The mic has no keyboard focus. That is the same as every native button (a PLAN gap).
- No echo gate: the spec's allowed fallback, listed in PLAN.md.
