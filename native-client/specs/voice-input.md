# 2.24 Voice input in a chat

The composer's **mic** lets the person talk to a chat instead of typing.
There are two ways to use it:
- **Voice mode** (a tap): the chat listens until it is tapped off. Each time
  the person stops talking, what they said is sent as
  `voice input: <words>`, through the normal send path.
- **Push to talk** (hold the mic): it listens while held and sends when let go.

While the chat is recording, the **message box shows the live transcript**
(read only; the typed draft comes back afterwards). Send's place shows the
**recording strip**: a dot, a level meter, the elapsed time and a status. The
mic turns into a send button, and a **cancel ×** sits at the far right.

Claude answers a `voice input:` with its `speak` tool, which arrives as a
`voice-say` event. The chat **says it aloud** while in voice mode, and for 10
minutes after a push to talk.

Rule 2 of DESIGN.md applies: every size and colour below is a theme token.
Cru (the voice assistant's own screen, the top bar's Cru button, "Hey Cru"
and the Ctrl+Space push-to-talk shortcut) is **phase 6**. Only the chat's
side of the hand-over with Cru is described here, so phase 6 can connect to
it (§7).

Sources:
- `app/vendor/claude-chat.js`
  - CSS 453-505 (`.cc-mic`, `.cc-vline*`, `.cc-rec*`, `.cc-input-rec`); 132-135 (`.cc-btn.primary`).
  - The rules, as pure functions: `voiceLogic` 803-924 (port these to Rust with their tests).
  - Shared state across chats: `voiceViews`, `voiceClaim`, `BroadcastChannel('crundi-chat-voice')` 926-948.
  - `voiceSettings` 950-960 (one answer, shared by every chat for 60 s).
  - The mount: the mic, rec box, cancel and voice line 1011-1046.
  - Voice mode 2877-3050: `vc` state, `boxOn`/`boxText`/`boxOff`, `recPaint`, `recTick`, `voiceShow`, `voicePaint`, `voiceCheck`.
  - Playback 3051-3095: `voiceOutCtx`, `voiceOutNode`, `playLevel`.
  - The microphone 3096-3256: `micOpen`, `micSend`, `micPause`, `micHold`, `micRelease`, `ownerSpoke`, `micClose`, `micHeard`.
  - Sending and start/stop 3257-3305: `voiceSend`, `cruTone`, `voiceFail`, `voiceStart`, `voiceStop`.
  - Push to talk and the controls 3306-3468: `pttStart`, `pttRelease`, the pointer handlers, `micAct`, `voiceSendNow`, `voiceCancelWords`, `pttCancel`, `onVoiceKey`.
  - Sharing 3470-3540; replies 3541-3735: `onVoiceSay`, `replyTick`, `speakNext`, `speakStop`, `speakDeepgram`, `speakBrowser`.
  - Events: `voice-say` 5047; `exit` stops voice 5077; `destroy` 5117-5123.
- `app/vendor/crundi-voice.js`
  - `voicedShare` 128, `createGate` 162, `createPreroll` 355, `makeLoopback` 386: the "talk over the reply" gate.
  - `tone` 1851 (TONES 1843-1847): the listen and sent tones.
  - Ctrl+Space 2662-2700: Cru's shortcut, not the chat's.
- `src/webapp-html.js` 8452: the chat gets `token: () => token` (the parked Peek view, 8832, gets no token, so it has no mic).
- Server:
  - `src/webapp.js`: 9035-9066 (the `/api/voice/listen` upgrade), 5427 (`GET /api/voice/settings`), 5842-5880 (`POST /api/voice/speak`).
  - `src/voice/stream.js`: the protocol (header 1-40), `applyDiscard` (the cancel words) 146-163, `relay` 262-408.
  - `src/claude-ui.js` 3250-3257: `voiceSay` emits `voice-say`.
  - `src/mcp-stdio.js` 360: the `speak` tool. `src/system-prompt.js` 65: Claude's rule for `voice input:`.
  - `src/client-version.js` 289-293: the capability `chat-voice`.

## Screenshots

`voice-input/web-<state>.png`, at 1400×860. The layout is one wide pane on
top (*Fruit list…*, in plan mode, with the draft "A half-typed draft"), with
two below: *Second chat*, wide (705), and *Narrow chat* (435, the narrow
composer). `-2x` is the composer cropped and doubled.

These are real runs. A test server was pointed at a stand-in for Deepgram
(`tools/deepgram-stub.mjs`, see §9), so the server's `/api/voice/listen`
relay, its cancel words and `/api/voice/speak` all ran for real. A real Claude
answered a voice input and called `speak`. In the page, only
`getUserMedia` was stubbed: a 180 Hz tone stands in for the microphone, so
real 16 kHz PCM frames went out (38 frames / 113 kB in the first 1.5 s). It
also stubbed the two microphone failures.

| State | What it shows |
|---|---|
| `web-no-voice-server` | **No voice assistant** (`deepgramKeySet: false`): no mic in any composer. An **older server** (`/api/voice/settings` 404) looks exactly the same. |
| `web-idle-mic` | **The mic, idle**: between attach and Send (wide), and in the narrow pane's action row between the clock and Send. |
| `web-mic-hover` | Hover: icon `accent-hover`, edge `accent`. |
| `web-voice-mode-listening` | **Voice mode on, nothing heard yet.** The box is read-only, with the italic placeholder "Listening…" on an `accent-dim` fill with an `accent` edge. The mic is an accent square showing the **send** icon. The strip in Send's place: ● ▮▮▮▮▮ `0:04` *Listening*. The × is at the far right. The draft is hidden. |
| `web-voice-mode-interim` | **Words arriving live:** "Could you add a short README" in the box. The level bars are up. |
| `web-voice-mode-long` | **Long speech:** the box grows (to its max height) and follows the newest words. |
| `web-scratch-that` | **The cancel words:** "…actually scratch that" then a pause. The server sends `discard`: the box is empty again and nothing is sent. Voice mode carries on (the time is not reset). |
| `web-off-draft-back` | **Voice mode off** (a tap on the mic with nothing heard): the strip and × go, Send is back, and **the draft "A half-typed draft" is back** in the box. |
| `web-sent-working` | **An utterance sent:** after a pause, "voice input: What day of the week is it today?…" appears as the person's bubble. The chat is WORKING (Stop shows). Voice mode keeps listening, and **the time restarts at 0:01**. |
| `web-speaking` | **Claude speaking** (`voice-say`, played from `/api/voice/speak`): status *Speaking*, and the mic has a `sky` ring instead of the pulse. Tooltip "Pause what Claude is saying". |
| `web-paused` | **The reply paused** (a tap on the mic while Claude speaks): status *Paused*. It carries on by itself after 1.5 s of quiet. |
| `web-cancel-hover` | **× hovered**, with words in the box while Claude is speaking: × turns solid `red` with white ✕. |
| `web-ptt-held` | **Push to talk, held** (the mouse held on the mic for ≥ 350 ms): the same strip. The box shows the words so far ("Make the table header bold and sort the"). |
| `web-ptt-slide-cancel` | **Held, slid onto ×:** the strip turns `red-dim` with a `red` edge, red text and bars. The × is solid red and **grows** (scale 1.12). Letting go here drops it. |
| `web-ptt-sending` | **Let go elsewhere:** status *Sending* while the server's last words are awaited (up to 2.5 s). Then it is sent. |
| `web-after-ptt-speaking` | **After a push to talk, the reply is said** (voice mode is off): a **voice line** appears above the box, with a `sky` dot and an italic muted "Speaking…". A click on the line stops it. |
| `web-err-mic-denied` | **Microphone not allowed:** a red voice line, "The microphone is not allowed. Allow it for this site to talk to this chat." It goes after 6 s. |
| `web-err-no-microphone` | **No microphone:** the web shows **the same** message (every `getUserMedia` failure lands in one catch). The recap card here is the chat's idle recap (2.17), not part of this screen. |
| `web-err-deepgram-refused` | **The server's error**, from the socket's `{type:'error'}`: "Deepgram refused the key. Check it in Settings › Voice assistant." |
| `web-err-connection-closed` | **The socket closed** under voice mode: "Listening stopped (the connection closed)." |
| `web-err-chat-ended` | **The chat has exited** (narrow pane): a tap gives "This chat has ended." |
| `web-paused-other-chat` | **Two chats:** the top chat is in voice mode, then the mic of *Second chat* is held. The top chat **pauses**: a voice line with a `yellow` dot, "Paused: listening in another chat". Its mic is outlined (`accent` edge and icon, no fill, no pulse), and its draft is shown again. When the hold ends, it resumes by itself. |
| `web-moved-to-other-chat` (+`web-narrow-recording`) | **Voice mode moved:** voice mode was turned on in *Narrow chat*, so the top chat goes off with the note "Voice mode moved to another chat." (`yellow` dot, 6 s). In the narrow pane, the strip **fills the rest of the action row**, the × sits beside it, and the box wraps the words. |
| `web-cru-ask` | (phase 6, for reference) **Cru's screen is open** and the mic is tapped: "Cru is listening. Switch to this chat?" with **Switch** (accent, 600) and **Not now** at the line's right. |

Not shot, because they look the same as a state above:
- the keyboard (Enter/Space on the focused mic is a tap);
- the browser's own voice (Deepgram over its monthly limit, or no voice
  chosen): it looks like `web-speaking`;
- a voice input while Claude works: it goes to the queue (2.9), like any send.

## 1. Parts

```
.cc-composer
  .cc-wrap
    .cc-vline            the voice line (hidden unless it has something to say)
      dot  text  [Switch] [Not now]
    .cc-inrow
      .cc-input          the message box; while recording: the live transcript
      .cc-actions
        attach  [clock: narrow]  [Stop]  MIC  Send | (recording) STRIP  ×
```

| Part | Web | Token |
|---|---|---|
| **Mic, idle** | 34×34 square, 1px `border`, `bg-primary`, icon 16 (the mic: capsule, stand, base) in `text-secondary`, radius 6. Between attach/Stop and Send. Hidden until the server says it can listen. | `control` square, `r-control`, `icon-lg`, the existing control-square widget |
| Mic, hover | icon `accent-hover`, edge `accent` | as other control squares' hover |
| **Mic, on** (voice mode, a hold, or sending) | fill `accent`, edge `accent`, icon white; a pulsing ring, 2 → 6 → 2 px of `accent-dim`, 1.6 s. While recording, the icon is the **send** arrow (paper plane), not the mic. | `accent`, a `SoftShadow` or a ring in `accent-ring` animated over the scale's `shimmer` (1600 ms) |
| Mic, on + Claude speaking | no pulse; a still 3px ring of `sky` at 35 % | `sky-ring` (or `sky-edge`) |
| Mic, away (paused for another chat) | `bg-primary`, edge `accent`, icon `accent-hover`, no ring, no pulse; the idle mic icon | |
| **Strip** (`.cc-rec`, in Send's place) | Send's primary button look (accent fill, white text, 600, 12 × fs, radius 6, padding 6 12). Its min-width is Send's width, measured as recording starts. Contents centred, gap 7: **dot** 7×7 white, blinking (opacity 1 → .35, 1.2 s); **bars** 5 × (3 wide, gap 2, radius 2, 3-16 high, white); **time** "m:ss" 11.5 × fs, tabular figures; **status** 11 × fs, 500, 92 % opacity: *Listening* / *Sending* / *Speaking* / *Paused*. Not clickable (`role=status`). No hover change. | the Send button's style; gap `sp-6`; the dot `sp-6` (or `sp-8`) round with `blink`; the bars 3 px wide (`sp-2` + 1px hairline is fine) at `sp-2` gaps, radius `r-chip`/2, `sp-16` max; `text-11` for time and status (status `w-medium`) |
| Strip, cancelling | `red-dim` fill, `red` edge, `red` text and bars | `red-dim`, `red` |
| **× (cancel)** | 34×34, 1px `border`, `bg-primary`, a 16 px ✕ (stroke 2.2) in `text-secondary`, radius 6, at the row's far right. Hover, or held over while holding: solid `red`, white ✕. Held over: also grows to 1.12. Tooltip "Cancel: drop what was heard (when holding, slide here and let go)". | `control` square, `red`; the growth as a 1-step size change or a scale animation (rule 5 if Slint can't scale) |
| **Box while recording** | read only, no caret, `accent-dim` fill, `accent` edge (and the focus ring if focused), text `text-primary`; placeholder "Listening…" in italic `text-secondary`; wraps, grows to its max, scrolls to the newest words | the composer box (2.5) with `accent-dim`/`accent`; italic placeholder |
| **Voice line** (`.cc-vline`) | above the box, inside the wrap; a row, gap 7, padding 0 2 6, 12 × fs, `text-secondary`; one line, ellipsis | gap `sp-6`, bottom `sp-6`, `text-12` |
| Line dot | 8×8 round. Speaking: `sky`. Note or ask: `yellow`, still. Error: no dot. (A plain "Listening" dot in `accent` with the pulse exists in the CSS, but it is never shown now that the words are in the box.) | `sp-8` round |
| Line text | normal: `text-secondary`; "Speaking…" (and "Paused" after a hold): italic `text-muted`; error: the whole line in `red` | |
| Ask buttons | at the right, gap 6: **Switch** (accent fill, white, 600) and **Not now** (`bg-primary`, `border` edge, `text-primary`), padding 3 10, 11.5 × fs, radius 6; hover edge `accent` | small buttons (`control-sm` high), `text-11` |

The texts scale with A-/A+ (`--cc-fs`): the strip's time and status, the
voice line and the ask buttons. Icons and squares don't.

### Narrow (the composer's narrow layout, 2.5)

The mic stays in the action row, between the clock and Send. While
recording, Send's place (the strip) **takes the rest of the row**
(`flex:1 1 0`), and the × keeps its square beside it. The order is attach,
clock, mic (send icon), strip, ×. See `web-narrow-recording`.

## 2. Showing the mic

- The chat asks `GET /api/voice/settings` when it mounts. The answer is
  shared by every chat for **60 s** (`voiceSettings`).
- The mic shows only when the status is 200, `ok !== false` and
  `deepgramKeySet === true` (`voiceLogic.available`). Otherwise (no key, an
  error, a 404 from an older server, no network) it stays hidden, with **no
  message**.
- It is checked only at mount. A key added later shows the mic in panes
  opened (or remounted) after it, as on the web.
- No token, no mic: the parked Peek view (2.23) never has one.
- **Native:** also require the server's capability **`chat-voice`**
  (`/api/client-version` `capabilities`). The web is always served by its own
  server, so it never needs this check. The native client can meet an older
  server: one with `voice-assistant` but no chat voice would get a mic whose
  input Claude can't answer aloud. This is a named difference (§10).

## 3. Controls

### The mic

| Input | Not recording | Voice mode on |
|---|---|---|
| **Press < 350 ms** (a tap), or Enter/Space when focused | **voice mode on** (`voiceStart`) | words in progress (`interim` not empty): **send them now** (`voiceSendNow`). None: **voice mode off** (`voiceStop`). |
| **Press ≥ 350 ms** (a hold) | **push to talk** (`pttStart`): listen while held | a hold is a tap (in voice mode any press is a tap) |
| Claude is speaking (not paused) | the tap **pauses the reply** first, and does nothing else (`micAct`) | same |
| Right button, context menu | ignored (context menu suppressed) | |

- The press is told apart from the hold by a timer started on pointer down
  (`VOICE_HOLD_MS` 350). The pointer is **captured**, so a slide off the mic
  is still followed.
- **Holding:** moving within **8 px** of the × lights it up ("cancelling"
  look; label "Let go to cancel"). Moving away lights it down again.
  - Let go **over ×**: `pttCancel` (dropped, nothing sent).
  - Let go **anywhere else**: `pttRelease` (sent).
  - A pointer cancel (the OS took the pointer): dropped.
- The keyboard has **no hold**. Enter/Space on the focused mic is a tap.
  Ctrl+Space is Cru's shortcut (phase 6): it opens Cru's screen, not the
  chat's push to talk. Native: don't bind it in phase 2.
- Tooltip / accessible label:
  - idle: "Voice input (tap: voice mode, hold: push to talk)";
  - voice mode on: "Voice mode is on: tap to turn it off";
  - recording: "Send what was heard now";
  - holding: "Let go to send (slide to cancel to drop it)";
  - Claude speaking: "Pause what Claude is saying";
  - paused for Cru: "Voice mode is paused while Cru listens: tap to take the microphone back".
- `aria-pressed` reflects on/off (native: the accessible checked state).

### The ×

- Click while holding: `pttCancel`. The hold ends, the socket closes and the
  draft returns.
- Click in voice mode: `voiceCancelWords`. It sends `{type:'discard'}`, drops
  what was heard, and **stops Claude's reply for good** (the queue is
  cleared). Voice mode keeps listening.
- Its pointer-down doesn't take focus.

### Escape

- From anywhere in this chat (the focus inside it, or nowhere), Escape ends
  voice mode (`voiceStop`) or drops a hold (`pttCancel`).
- Focus in another pane or field outside this chat: ignored.

### The voice line

- A click on it while a reply is being said **stops** it (`speakStop`).
- Switch / Not now: see §7.

## 4. Behaviour

### Voice mode (`voiceStart` … `voiceStop`)

1. On: if the chat has exited, show the error "This chat has ended." and stop.
   - If Cru's screen is open, show the ask instead (§7).
   - Otherwise, **claim** voice mode: every other chat in voice mode, here or
     in another window of the client, goes off with the note "Voice mode
     moved to another chat." (6 s).
   - Re-read the settings (fresh: the voice and speed may have changed),
     then open the microphone (§5).
2. Recording starts. The box shows the transcript (the draft is kept aside),
   the strip and × appear, the time starts at 0:00, and the mic shows the
   send icon. The status is *Listening*.
3. Socket `ready`: play the **listen tone** (§8).
4. `interim {text}`: the box shows the words. `lastHeardAt` is set.
5. `utterance {text}`: send `voice input: <text>` (whitespace collapsed,
   trimmed) through the normal send path. That path queues while Claude
   works (2.9), and nothing is sent if the chat has exited. Then:
   - play the **sent tone**;
   - restart the time at 0:00;
   - scroll the log to the bottom.
6. `discard`: the person said "scratch that", "never mind" or "ignore that"
   with nothing after it. The words are dropped and nothing is sent. Only
   the server decides this (`applyDiscard`).
7. "Send now" (a tap with words in progress):
   - send `{type:'discard'}` (the server drops them), then send the words;
   - for 4 s, a late utterance that is only those words is ignored
     (`alreadySent`).
8. Off (`voiceStop`):
   - the socket closes (`micClose`) and the reply stops;
   - the strip and × go and Send comes back;
   - **the draft comes back exactly as it was**;
   - the voice line clears.
9. The chat **exits** (`exit` event), the pane closes or parks
   (`destroy`): voice mode stops, and so does a hold.

### Push to talk

1. Hold:
   - if the chat has exited: "This chat has ended.";
   - if Cru's screen is open, it is closed (a hold is asking on purpose);
   - **every other chat in voice mode pauses** (`othersPause`: "Paused:
     listening in another chat"; its socket closes and its draft shows);
   - recording starts as above; a reply being said pauses.
2. While held: each `utterance` is **added** to the transcript (`heard`),
   not sent. `interim` shows after them. `discard` clears all of it.
3. Let go:
   - status *Sending*; send `{type:'stop'}`;
   - the server flushes its last words as an utterance, then closes;
   - on close (or after **2.5 s** at most), send `voice input: <the
     utterances joined>`, or the last interim if no utterance came;
   - close the mic; the paused chats **resume by themselves**;
   - the draft comes back;
   - for **10 minutes**, Claude's replies in this chat are said aloud
     (`PTT_WINDOW_MS`).
4. Cancel (× or Escape): `{type:'discard'}`, nothing sent, the mic closes,
   the others resume, and the draft comes back.

### The level and the time (`recTick`, every frame while recording)

- Time: `m:ss` since recording began (or since the last send).
- Level: the RMS of each block, square-rooted twice (0..1):
  `sqrt(sqrt(Σx²/n))`.
- Each of the 5 bars: `max(3, round(3 + 13 · min(1, level·2.2) · wob))` px,
  with `wob = 0.55 + 0.45·|sin(t·(5 + i·1.7) + i)|`, so they ripple even at
  a steady level.
- Native: compute the level in the capture thread and send it to the UI at
  frame rate (the `frame` token). Don't redraw from the audio callback.

### Spoken replies (`onVoiceSay` → `replyTick` → `speakNext`)

**When a reply is said:** a `voice-say {id, text}` event is said when the
chat is in voice mode, or within 10 minutes of the last push to talk.
Otherwise it is **ignored, and not queued** (`shouldSpeak`). An id already
seen is skipped. Replies queue and are said one after another.

**Around the person:**
- A reply **waits** while the person is talking: a hold, its sending, a
  pause for another chat, or words heard within the last **1.5 s**
  (`QUIET_MS`).
- One being said **pauses** at the same moments.
- It **carries on from where it paused** once they have finished (checked
  every 250 ms).

**While Claude speaks:**
- The mic sends `{type:'saying', text}` then `{type:'pause'}` (`micHold`).
  The server drops its own words if they come back through the microphone.
- After the reply, plus 450 ms, it sends `{type:'resume'}` and
  `{type:'said'}` (`micRelease`).

**Talking over a reply:**
- With crundi-voice.js's gate (the web has it), the microphone keeps
  listening locally while Claude talks. When the gate decides it is the
  person (louder than the learnt echo × 2, for 300 ms, and voiced), the
  reply pauses. Then the last ~1.3 s of what they said (the pre-roll) goes to
  the server first, so their words aren't clipped.
- Without the gate, listening simply pauses while Claude talks (`micPause`).

**Which voice:**
- Deepgram, when the settings have `ttsVoice` and `deepgramLimitReached` is
  false. `POST /api/voice/speak?stream=1 {text}` (`Accept: audio/L16`)
  answers raw 16-bit little-endian mono PCM, with its rate in `X-Sample-Rate`
  or `rate=` in the Content-Type (default 24000). Play the chunks as they
  arrive.
  - An older server answers a whole mp3 instead: decode it and play it.
  - On a failure, say it with the device voice instead.
  - Pause and resume suspend and resume the output, so the reply carries on
    exactly where it was.
- Otherwise the device's own voice, at `speechRate`, en-GB (a male British
  voice where there is one).
  - Where it can't pause, it starts again from the sentence it was in
    (`sentenceStart`).
  - A cap of `8 s + 120 ms × characters`, so a lost "end" never sticks the
    queue.

**What the person sees:**
- In voice mode, the strip's status is *Speaking* / *Paused*, and the mic
  gets the sky ring.
- After a hold (voice mode off), the voice line shows "Speaking…" (or
  "Paused"), and a click on the line stops it.

## 5. The microphone and `/api/voice/listen`

`WebSocket ws(s)://<host>/api/voice/listen?encoding=linear16&sample_rate=16000&token=<the session token>`

- **Auth:** the token in the query (`validateToken`). A bad or missing token
  gets 401 at the upgrade, which the client sees as a close: "Listening
  stopped (the connection closed)."
- **Audio, client → server:** binary frames of **16 kHz mono signed 16-bit
  little-endian PCM**.
  - The web downsamples by averaging blocks of `rate/16000` samples (a box
    filter), from 4096-sample blocks, about 1365 samples ≈ 85 ms per frame
    at 48 kHz.
  - Up to 40 frames are buffered while the socket opens, then flushed in
    order.
  - The browser asks for echo cancellation, noise suppression and automatic
    gain.
- **Client → server, text frames:**

  | Message | When |
  |---|---|
  | `{"type":"stop"}` | push to talk let go: flush the last words as an utterance, then close |
  | `{"type":"pause"}` / `{"type":"resume"}` | stop / restart forwarding audio (Claude speaking, without the gate) |
  | `{"type":"saying","text":…}` / `{"type":"said"}` | Claude starts / stops speaking (the server's echo filter) |
  | `{"type":"discard"}` | drop what was heard so far (×, send now) |

- **Server → client:**

  | Message | Client does |
  |---|---|
  | `{"type":"ready","echoFilter":true}` | the listen tone |
  | `{"type":"interim","text":…}` | the words so far (all finished phrases + the one in progress) |
  | `{"type":"utterance","text":…,"owner":…,"held"?:true}` | voice mode: send it. Hold: add it to `heard`. |
  | `{"type":"discard","text":…}` | the cancel words: drop everything heard |
  | `{"type":"speech"}` | (not used by the chat) |
  | `{"type":"error","error":…}` | the error on the voice line, then stop. Seen: "No Deepgram key is set. Add one in Settings › Voice assistant.", "This month's Deepgram spend has reached its limit", "Deepgram refused the key. Check it in Settings › Voice assistant.", "Deepgram refused the stream (N)", "Couldn't reach Deepgram: …". |
  | `{"type":"settings"}`, `{"type":"report"…}`, `{"type":"update"…}`, `{"type":"screen"…}` | Cru's, sent to every open listen socket: **ignore** them |
  | close (not asked for) | "Listening stopped (the connection closed)." |

- The server waits `pauseMs` (settings, default 2000, 1000-5000) of silence
  before ending an utterance.
- The socket is opened on each start (voice mode or a hold), and closed on
  stop or cancel. With voice mode on while another chat holds, this one's
  socket is closed, and reopened when the hold is done.

**Native capture:**
- Use `cpal` (or similar): the default input device, whatever rate and
  format it gives, mixed to mono.
- Resample to 16 kHz. The web's box average is acceptable; a better filter is
  fine.
- Send each block of about 85 ms as Int16 LE on the socket from a worker
  thread. `tungstenite` is already a dependency.
- Make a **test hook** that reads a WAV file instead of the device (an env
  var or a testctl command), so the end-to-end test can feed known audio.
- The OS gives no echo cancellation through `cpal`. See §10.

## 6. Errors

All on the voice line, in `red`, with no dot. Each clears after 6 s if
nothing is recording. On the web, an error shown when voice mode or a hold
then starts stays up until recording ends (an accident, §10.5).

| When | Text |
|---|---|
| The chat has exited, and the mic is tapped or held | This chat has ended. |
| No audio input API (web) | This browser cannot use the microphone here. |
| The microphone is refused, or there is none (`getUserMedia` rejects) | The microphone is not allowed. Allow it for this site to talk to this chat. |
| The server's `error` | its text (above) |
| The socket closed under it | Listening stopped (the connection closed). |

Every error ends voice mode or the hold, and the draft comes back
(`voiceFail` → `voiceStop`).

## 7. Sharing the microphone

- **One chat in voice mode at a time**, across every chat of the client.
  - Turning it on in one turns it off elsewhere, with the note "Voice mode
    moved to another chat." (`yellow` dot, 6 s).
  - The web does this across its windows with a BroadcastChannel. Native: one
    process-wide owner.
- **A hold pauses the others:** a chat in voice mode pauses ("Paused:
  listening in another chat", `yellow` dot, the "away" mic). It resumes by
  itself when the hold is sent or cancelled.
- **Cru (phase 6), for the record:**
  - Tapping the mic while Cru's screen is open shows "Cru is listening.
    Switch to this chat?" with **Switch** (closes Cru, starts voice mode)
    and **Not now** (clears the line).
  - Cru opening on purpose pauses voice mode ("Paused: Cru is listening",
    the away mic; a tap takes the microphone back) or drops a hold.
  - Cru closing resumes it.
  - `CrundiChat.voiceActive` tells the PC's "Hey Cru" listener a chat has the
    microphone.
  - Native phase 2: there is no Cru screen, so none of these can happen. Keep
    a process-wide "who has the microphone" owner that phase 6 can plug into.

## 8. Tones

From crundi-voice.js `tone()`. Sine notes with an exponential envelope:
from silent, a 12 ms rise to the peak, then down to silent at the end.
Played on their own audio output.

| Tone | When | Notes (Hz, start s, length s, peak) |
|---|---|---|
| listen | the socket says `ready` | 659.25, 0, 0.11, 0.16; then 880, 0.09, 0.16, 0.16 |
| sent | a voice input is sent | 880, 0, 0.08, 0.13; then 659.25, 0.07, 0.14, 0.13 |

Native: synthesise them with `rodio` (a `Source` of sine samples). They
are short enough to play on the same output stream as the replies.

## 9. Data, endpoints and the test rig

| Call | Use |
|---|---|
| `GET /api/voice/settings` → `{ok, deepgramKeySet, ttsVoice, deepgramLimitReached, speechRate, pauseMs, …}` | show the mic; choose the voice and speed |
| `WS /api/voice/listen?encoding=linear16&sample_rate=16000&token=…` | §5 |
| `POST /api/voice/speak?stream=1` `{text}`, `Accept: audio/L16` → PCM stream, or JSON `{ok:false,error}` with 400 | Claude's voice |
| `POST /api/ui-sessions/:id/send` (the normal send, 2.5) with `voice input: <words>` | sending |
| chat event `voice-say {id, text, at}` (on the chat's `/ws` subscription) | a reply to say |
| chat event `exit` | stop voice |
| capability `chat-voice` | show the mic at all (native only, §2) |

**The test rig** (how the screenshots were made, and how the end-to-end test
can run without a Deepgram key):

```
node native-client/tools/deepgram-stub.mjs 4811 &
DEEPGRAM_HOST=127.0.0.1:4811 NODE_TLS_REJECT_UNAUTHORIZED=0 native-client/tools/test-server.sh 4810
POST /api/voice/settings {"deepgramKey":"stub"}          # deepgramKeySet: true, the mic shows
POST /api/voice/settings {"ttsVoice":"aura-2-draco-en"}  # replies through /api/voice/speak
curl -k -XPOST https://127.0.0.1:4811/ctl/say -d '{"interim":"Could you add"}'
curl -k -XPOST https://127.0.0.1:4811/ctl/say -d '{"final":"What day is it?","end":true}'   # an utterance
curl -k https://127.0.0.1:4811/ctl/stats   # frames/bytes the server relayed; texts (CloseStream etc.)
POST /api/mcp/call {tool:"speak", args:{sessionId, text}}  with the chat's dev-mcp X-Api-Key  # a voice-say
```

- `DEEPGRAM_HOST` pins the host (`src/voice/deepgram-host.js`).
- `NODE_TLS_REJECT_UNAUTHORIZED=0` lets the test server accept the stub's
  self-signed certificate.
- Both pass through `test-server.sh`, which strips only `CRUNDI_*` and
  `CLAUDE_CODE_*`.
- The stub's `/ctl/stats` proves **the frames reached Deepgram's side**: the
  server forwards binary frames only after its own checks, with
  `encoding=linear16&sample_rate=16000&channels=1` in the URL.
- The transcript landing in the box, the send, `discard` and the server's
  errors (`/ctl/mode {"mode":"refuse"}`) can all be driven this way.

## 10. Named differences and gaps for native

1. **Wording, "for this site":**
   - Native says "The microphone is not allowed. Allow it for Crundi to talk
     to this chat." There is no site in a desktop app.
   - The web uses one message for refused and absent microphones. Native
     keeps that single message, unless the owner asks otherwise.
   - macOS needs `NSMicrophoneUsageDescription` in the bundle's Info.plist,
     or the OS kills the app on first capture. Windows' privacy switch shows
     up as an error opening the device: map it to the same message.
   - Electron grants the microphone without asking
     (`app/voice-wake/controller.js` 134-143); native relies on the OS
     prompt.
2. **The `chat-voice` capability check** (§2): native only. An older server
   shows no mic, the same as the web's no-key case.
3. **Talking over a reply:**
   - The web has the browser's echo cancellation, plus crundi-voice.js's gate
     through a WebRTC loopback. `cpal` has neither.
   - Preferred: port `createGate`, `createPreroll` and `voicedShare` (pure
     maths, about 150 lines; the play level is known, because native plays
     the PCM itself).
   - Fallback: the web's own no-gate path (pause listening while Claude
     speaks). Then the person pauses or stops a reply with the mic or ×. If
     the fallback is taken, list it in PLAN.md "Known gaps".
4. **The device voice** (no `ttsVoice`, or over the Deepgram limit):
   - The web uses `speechSynthesis`. Native: the OS voice through the `tts`
     crate (SAPI / AVSpeechSynthesizer / speech-dispatcher), at
     `speechRate`.
   - If a system has none, the reply isn't said; the written reply is in the
     chat. That case is a Known gap.
5. **Web accidents not copied:**
   - Once a hold or voice mode starts, an earlier error on the voice line
     stays until recording ends (`web-paused-other-chat`: "Listening
     stopped…" under the held chat). Native clears the error when recording
     starts.
   - The `.cc-vline` "Listening…" dot with the pulse is dead CSS. It is never
     shown.
6. **Linux CI:** `cpal`/`rodio` need ALSA headers (`libasound2-dev`) in
   native-client.yml's Linux job. Windows and macOS need nothing extra.

## Checklist

- [ ] The mic is hidden unless `GET /api/voice/settings` says `deepgramKeySet` (and the server lists `chat-voice`); no message when hidden; checked at mount, cached 60 s across chats; none in Peek.
- [ ] The mic sits between attach/Stop and Send (wide), and between the clock and Send (narrow); control square, icon-lg, hover accent.
- [ ] A tap (< 350 ms), or Enter/Space, turns voice mode on. A hold (≥ 350 ms) is push to talk. In voice mode any press is a tap.
- [ ] Recording: the box is read only, `accent-dim` fill, `accent` edge, placeholder "Listening…" (italic), the live words, growing and following the newest; the draft is kept aside and **restored exactly** afterwards.
- [ ] The strip in Send's place: blinking dot, 5 level bars (formula in §4), `m:ss`, status Listening/Sending/Speaking/Paused; at least Send's width (wide), the rest of the row (narrow).
- [ ] The mic shows the send icon while recording, with the pulsing accent ring; the sky ring while Claude speaks; the "away" outline while paused for another chat.
- [ ] × at the far right: in voice mode it drops the words (`discard`) and stops the reply; in a hold, it cancels. Red on hover.
- [ ] Hold, slide within 8 px of ×: the strip turns red-dim/red, × solid red and larger; let go there and nothing is sent.
- [ ] Let go elsewhere: *Sending*, `{type:'stop'}`, sent on close or after 2.5 s, as `voice input: …`.
- [ ] Voice mode: each `utterance` is sent as `voice input: <text>`, through the normal send (queued while working); the time restarts; the log scrolls down; the sent tone plays.
- [ ] Tap with words in progress: sent now, and a late duplicate is ignored for 4 s. Tap with none: voice mode off.
- [ ] `discard` from the server ("scratch that", "never mind", "ignore that") clears the words and sends nothing.
- [ ] Escape ends voice mode or a hold, from within this chat only.
- [ ] Audio is 16 kHz mono Int16 LE binary frames on `/api/voice/listen?encoding=linear16&sample_rate=16000&token=…`, buffered (≤ 40) until open; Cru's messages on that socket are ignored.
- [ ] The listen tone plays on `ready`; the sent tone on each send.
- [ ] `voice-say` is said in voice mode or within 10 min of a hold, never otherwise; queued; skipped if its id was seen.
- [ ] Replies wait while the person talks (hold, sending, words within 1.5 s) and pause and resume around them; a tap on the mic pauses one; × or a click on the voice line stops it.
- [ ] Deepgram voice: `POST /api/voice/speak?stream=1` streamed PCM at `X-Sample-Rate`; an mp3 from an older server; the device voice as the fallback.
- [ ] `saying`/`pause` sent while Claude speaks, and `resume`/`said` after it (+450 ms); or the gate (§10.3).
- [ ] After a hold, the voice line "Speaking…" (sky dot) shows above the box, and a click on it stops the reply.
- [ ] Errors on the voice line in red, cleared after 6 s: "This chat has ended.", microphone refused or absent, the server's error text, "Listening stopped (the connection closed)."; each one ends recording and restores the draft.
- [ ] One chat in voice mode at a time ("Voice mode moved to another chat."); a hold elsewhere pauses voice mode ("Paused: listening in another chat") and it resumes after.
- [ ] Exit, close and park stop voice mode and any hold.
- [ ] Texts scale with A-/A+; tokens only; the named differences in §10 are in PLAN.md.
- [ ] The end-to-end test, on the rig in §9: a WAV fed through the test hook, frames counted at the stub, the transcript in the box, the utterance sent as `voice input: …`, `discard`, the refused-key error, and a `voice-say` played (or the gap recorded).
