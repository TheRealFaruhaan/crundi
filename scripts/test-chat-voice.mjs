#!/usr/bin/env node
/**
 * test-chat-voice.mjs — voice mode in a chat (the composer's mic).
 *
 * The rules live in app/vendor/claude-chat.js as window.CrundiChat._voice and
 * are run here as they are: tap or hold, what is sent, when Claude's
 * 'voice-say' is said aloud, when the mic is offered. The hosts (the
 * workbench and the compact chat page) must hand the component a token for
 * the listen socket, or it shows no mic.
 *
 * Run: node scripts/test-chat-voice.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const win = {};
vm.runInNewContext(read('app', 'vendor', 'claude-chat.js'), { window: win });
const V = win.CrundiChat && win.CrundiChat._voice;
const cc = read('app', 'vendor', 'claude-chat.js');
ok(!!V && typeof win.CrundiChat.mount === 'function', 'the component loads with its voice rules beside mount');

// ─── Tap or hold ───
ok(V.HOLD_MS === 350, 'a hold is 350 ms');
ok(V.pressKind(0) === 'tap' && V.pressKind(120) === 'tap' && V.pressKind(349) === 'tap', 'a short press is a tap (voice mode)');
ok(V.pressKind(350) === 'hold' && V.pressKind(2000) === 'hold', 'a long press is push to talk');

// ─── Recording: the mic is send, cancel is at the far right ───
ok(V.recording({ ptt: true }) && V.recording({ ptt: true, interim: '' }), 'a hold is recording from the moment it starts');
ok(V.recording({ on: true }) && V.recording({ on: true, interim: 'open the' }), 'voice mode is recording the whole time it is on ("Listening…" in the box when quiet)');
ok(V.recording({ sending: true }) && !V.recording({}), 'a hold let go stays recording until its words are sent; off, the composer is as before');

// ─── The live transcript is in the message box ───
ok(V.transcript(['Open the readme.', 'Then run the tests.'], 'and push') === 'Open the readme. Then run the tests. and push', 'the box shows every phrase of this recording, then the one in progress');
ok(V.transcript([], '') === '' && V.transcript(null, 'so far') === 'so far', 'empty until something is heard (the box then says Listening…)');
ok(V.status({}) === 'Listening' && V.status({ sending: true }) === 'Sending' && V.status({ playing: true }) === 'Speaking' && V.status({ playing: true, paused: true }) === 'Paused', 'Send\'s place shows only a short status beside the dot, time and level');
ok(/function boxOn\(\)[\s\S]{0,200}draft: input\.value[\s\S]{0,120}input\.readOnly = true/.test(cc) && /function boxOff\(\)[\s\S]{0,300}input\.value = b\.draft/.test(cc), 'the box is read-only while it shows the transcript, and the typed draft comes back after');
ok(/function doSend\(\) \{\s*\/\/[^\n]*\n\s*if \(vc && vc\.box\) return;/.test(cc) && /function saveDraft\(\) \{\s*if \(vc && vc\.box\) return;/.test(cc), 'the transcript is never sent by Enter nor saved as the draft');
ok(cc.includes("input.placeholder = 'Listening…'") && cc.includes('input.scrollTop = input.scrollHeight'), 'it says Listening… when empty and follows the newest words');

// ─── Replies wait for the owner, and pause when he talks over them ───
const T0 = 1_900_000_000_000;
ok(V.QUIET_MS === 1500, 'he has finished after 1.5 s without new words');
ok(V.ownerTalking({ ptt: true, now: T0 }) && V.ownerTalking({ sending: true, now: T0 }), 'a hold, or its words being sent: a reply waits (or pauses)');
ok(V.ownerTalking({ lastHeardAt: T0 - 1499, now: T0 }) && !V.ownerTalking({ lastHeardAt: T0 - 1500, now: T0 }), 'words within 1.5 s: still talking; 1.5 s quiet: the reply goes on');
ok(!V.ownerTalking({ lastHeardAt: 0, now: T0 }) && !V.ownerTalking({ now: T0 }), 'his utterance sent (lastHeardAt 0): the reply goes on at once');
ok(/if \(vc\.on && !late\) voiceSend\(e\.text, false\);\s*vc\.sentNow = null;\s*vc\.lastHeardAt = 0;/.test(cc), 'an utterance sent marks him finished');
ok(/function replyTick\(\)[\s\S]{0,600}p\.pause\(\); micRelease\(false\)[\s\S]{0,300}micHold\(p\.text\); p\.resume\(\)/.test(cc), 'a reply pauses while he talks and carries on from where it was');
ok(/function ownerSpoke\(\)[\s\S]{0,200}p\.pause\(\);\s*micRelease\(true\)/.test(cc), 'talking over it pauses the reply (not stopped) and sends his words from their start (the pre-roll)');
ok(/function micAct\(\)[\s\S]{0,500}p && !p\.paused[\s\S]{0,120}p\.pause\(\)/.test(cc), 'a click on the mic while Claude talks pauses it too');
ok(/function voiceCancelWords\(\)[\s\S]{0,200}speakStop\(\)/.test(cc) && /function voiceStop\(\)[\s\S]{0,400}speakStop\(\)/.test(cc), 'Cancel, or voice mode off, stops it for good');
ok(cc.includes('ctx.suspend()') && cc.includes('ctx.resume()') && cc.includes('SS.pause()') && cc.includes('SS.resume()'), 'Deepgram\'s voice pauses by suspending its audio; the browser\'s with speechSynthesis.pause/resume');
ok(V.sentenceStart('One. Two three. Four', 10) === 5 && V.sentenceStart('One. Two three. Four', 17) === 16 && V.sentenceStart('No stops here', 8) === 0 && V.sentenceStart('', 3) === 0, 'where pause is not supported, it starts again from the sentence it was in');

// ─── The own-voice gate is crundi-voice.js's ───
ok(V.gateKit({}) === null && V.gateKit({ CrundiVoice: { _test: {} } }) === null, 'without crundi-voice.js (the compact page) there is no gate: listening pauses while Claude talks, as before');
{
  const vw = { addEventListener() {}, location: {}, navigator: {} };
  vm.runInNewContext(read('app', 'vendor', 'crundi-voice.js'), { window: vw, document: { addEventListener() {}, createElement() { return {}; } }, navigator: {}, localStorage: { getItem() { return null; } } });
  const kit = V.gateKit(vw);
  ok(!!kit && typeof kit.makeLoopback === 'function', 'with it, the gate, the pre-roll and the loopback are reused from it');
  const g = kit.createGate();
  g.start();
  let fired = false;
  for (let i = 0; i < 40 && !fired; i++) fired = g.hear(0.3, 0.5, 100, () => 1);   // Claude's own echo, as loud as it plays
  ok(!fired, 'Claude\'s voice coming back through the mic does not count as the owner');
  let at = -1;
  for (let i = 0; i < 10 && at < 0; i++) if (g.hear(0.95, 0.5, 100, () => 1)) at = (i + 1) * 100;
  ok(at >= 300, 'a voice well over the echo, held about 300 ms, is the owner (' + at + ' ms)');
  g.start();
  let unvoiced = false;
  for (let i = 0; i < 10; i++) unvoiced = g.hear(0.95, 0.5, 100, () => 0) || unvoiced;
  ok(!unvoiced, 'a loud noise with no voicing (a cough, a clap) is not');
  const pr = kit.createPreroll(1300);
  for (let i = 0; i < 30; i++) pr.push('c' + i, 100);
  ok(pr.drain(500).join() === 'c25,c26,c27,c28,c29', 'when he cuts in, his words go first from the pre-roll');
}
ok(/m\.held\) \{[\s\S]{0,300}m\.preroll\.push\(out\.buffer, ms\)[\s\S]{0,500}return;/.test(cc), 'while Claude talks the mic is held here, not streamed');
ok(cc.includes("micSend({ type: 'saying', text: text })"), 'the server is told what Claude is saying (its echo filter is the second net)');

ok(V.releaseAction(false) === 'send', 'a hold let go on the mic (or anywhere but cancel) sends');
ok(V.releaseAction(true) === 'cancel', 'a hold let go over cancel is dropped');
ok(V.micClick({ on: false }) === 'on', 'a tap with voice mode off turns it on');
ok(V.micClick({ on: true, interim: 'run the tests' }) === 'send-now', 'a click in voice mode with words heard sends them now (and keeps listening)');
ok(V.micClick({ on: true, interim: '' }) === 'off' && V.micClick({ on: true, interim: '  ' }) === 'off', 'a click in voice mode with nothing heard turns it off');
ok(V.elapsed(0) === '0:00' && V.elapsed(3400) === '0:03' && V.elapsed(65000) === '1:05' && V.elapsed(-5) === '0:00', 'the recording time reads m:ss');
ok(V.alreadySent('Run the tests, please.', 'please') && V.alreadySent('Run the tests, please.', 'run the tests') && !V.alreadySent('Run the tests', 'and deploy') && !V.alreadySent('run the tests', '') && !V.alreadySent('testing', 'test'), 'after "send now", a late utterance of words already sent is dropped, new words are not');

// ─── What is sent ───
ok(V.message('what time is it') === 'voice input: what time is it', 'what was said goes as "voice input: …"');
ok(V.message('  run   the\n tests  ') === 'voice input: run the tests', 'spacing is tidied');
ok(V.message('') === null && V.message('   ') === null && V.message(null) === null && V.message(undefined) === null, 'nothing heard: nothing is sent');
ok(V.join(['open the', ' readme ', '', 'please']) === 'open the readme please' && V.join([]) === '' && V.join(['', '  ']) === '', 'push to talk joins the phrases heard while held');
ok(V.message(V.join(['', ''])) === null, 'a hold with nothing heard sends nothing');

// ─── When Claude's voice is played ───
const say = (o) => Object.assign({ type: 'voice-say', id: 'v1', text: 'Done, Sir.', at: 1 }, o);
const NOW = 1_900_000_000_000;
ok(V.shouldSpeak(say(), { voiceOn: true, now: NOW }), 'in voice mode it is said');
ok(!V.shouldSpeak(say(), { voiceOn: false, lastPttAt: 0, now: NOW }), 'not in voice mode, no push to talk: not said');
ok(V.shouldSpeak(say(), { voiceOn: false, lastPttAt: NOW - 60_000, now: NOW }), 'a minute after push to talk: said');
ok(V.shouldSpeak(say(), { voiceOn: false, lastPttAt: NOW - V.PTT_WINDOW_MS, now: NOW }), 'ten minutes after push to talk: still said');
ok(!V.shouldSpeak(say(), { voiceOn: false, lastPttAt: NOW - V.PTT_WINDOW_MS - 1, now: NOW }), 'later than ten minutes: not said');
ok(!V.shouldSpeak(say({ text: '  ' }), { voiceOn: true, now: NOW }), 'nothing to say: nothing played');
ok(!V.shouldSpeak({ type: 'entry', text: 'x' }, { voiceOn: true, now: NOW }), 'only voice-say events');
ok(!V.shouldSpeak(say(), { voiceOn: true, now: NOW, seen: { v1: 1 } }) && V.shouldSpeak(say({ id: 'v2' }), { voiceOn: true, now: NOW, seen: { v1: 1 } }), 'the same event twice is said once');

// ─── When the mic is offered ───
ok(V.available(200, { ok: true, deepgramKeySet: true }), 'a Deepgram key on the server: the mic shows');
ok(!V.available(200, { ok: true, deepgramKeySet: false }), 'no Deepgram key: no mic');
ok(!V.available(403, { ok: false, error: 'Forbidden' }) && !V.available(404, null) && !V.available(200, {}) && !V.available(200, { ok: false, deepgramKeySet: true }), 'a collaborator, an older server, an error: no mic');

// ─── Which voice ───
ok(V.ttsVoice({ ttsVoice: 'aura-2-draco-en' }) === 'aura-2-draco-en', 'Deepgram\'s voice when one is chosen');
ok(V.ttsVoice({ ttsVoice: '' }) === '' && V.ttsVoice(null) === '' && V.ttsVoice({ ttsVoice: 'aura-2-draco-en', deepgramLimitReached: true }) === '', 'no voice chosen, or the Deepgram limit reached: the browser\'s');
ok(V.pcmRate('audio/L16; rate=24000; channels=1', null) === 24000 && V.pcmRate('audio/L16; rate=16000', null) === 16000 && V.pcmRate('audio/L16', '48000') === 48000 && V.pcmRate('', null) === 24000, 'the streamed voice\'s sample rate');

// ─── The component and its hosts ───
ok(/case 'voice-say': onVoiceSay\(ev\)/.test(cc), 'the view takes voice-say events');
ok(cc.includes("'Voice input (tap: voice mode, hold: push to talk)'"), 'the mic has its accessible label');
ok(/\/api\/voice\/listen\?encoding=linear16&sample_rate=16000&token=/.test(cc), 'it listens as Cru\'s screen does: 16 kHz PCM');
ok(cc.includes("micSend({ type: 'stop' })") && cc.includes("type: p ? 'pause' : 'resume'"), 'push to talk ends with stop; Claude\'s voice pauses the listening');
ok(cc.includes("'Cancel voice input: drop what was heard'") && cc.includes("setPointerCapture(e.pointerId)") && cc.includes("micBtn.addEventListener('pointermove'"), 'cancel has its label; a hold is followed by pointer capture so a slide onto cancel is seen');
ok(/function pttCancel\(\)[\s\S]{0,200}voiceCancelWords\(\)/.test(cc) && /function voiceCancelWords\(\)\s*\{\s*micSend\(\{ type: 'discard' \}\)/.test(cc), 'cancelling tells the server to discard what it heard');
ok(/'\.cc-recording \.cc-sendbtn\{display:none!important\}'/.test(cc) && /'\.cc-rec\{display:none;/.test(cc) && /'\.cc-rec-x\{display:none;/.test(cc), 'not recording: Send shows and the recording parts are hidden');
ok(/typeof opts\.token === 'function'/.test(cc), 'an older host (no token) gets no mic, and no error');
ok(!/[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}]/u.test(cc.slice(cc.indexOf('// ─── Voice mode ───'), cc.indexOf('// ─── Claude\'s voice ───'))), 'flat icons, no emoji');
const wb = read('src', 'webapp-html.js');
const mountAt = wb.indexOf('const view = window.CrundiChat.mount(mountEl, {');
ok(mountAt > 0 && /token: \(\) => token,/.test(wb.slice(mountAt, mountAt + 1200)), 'the workbench hands its chats the token');
const page = read('app', 'vendor', 'crundi-chat-page.js');
const pageAt = page.indexOf("view = window.CrundiChat.mount($('cp-chat'), {");
ok(pageAt > 0 && /token: function \(\) \{ return token; \},/.test(page.slice(pageAt, pageAt + 800)), 'so does the compact chat page');

// ─── Sharing the microphone with Cru, and with other chats ───
ok(V.startChoice({ cruOpen: true }) === 'ask' && V.startChoice({ cruOpen: false }) === 'start', 'voice mode while Cru\'s screen is open asks first ("Cru is listening. Switch to this chat?")');
ok(cc.includes("'Cru is listening. Switch to this chat?'") && cc.includes('>Switch</button>') && cc.includes('>Not now</button>'), 'with Switch and Not now');
ok(!/function micOpen\([^)]*\) \{\s*if \(window\.CrundiVoice && window\.CrundiVoice\.isOpen\)/.test(cc), 'Cru is no longer closed silently when the mic opens');
ok(V.onCruOpen({ on: true }) === 'pause' && V.onCruOpen({ on: true, away: 'cru' }) === 'none' && V.onCruOpen({ on: false }) === 'none', 'Cru opened on purpose: voice mode pauses (it stays on)');
ok(V.onCruOpen({ ptt: true, on: false }) === 'cancel-hold', 'a hold under way when Cru opens is dropped');
ok(cc.includes("voicePauseFor('cru', 'Paused: Cru is listening')"), 'with the note "Paused: Cru is listening"');
ok(V.onCruClose({ on: true, away: 'cru' }) === 'resume' && V.onCruClose({ on: false, away: 'cru' }) === 'none' && V.onCruClose({ on: true, away: 'chat' }) === 'none', 'Cru\'s screen closed: voice mode paused for it carries on by itself');
ok(V.recording({ on: true, away: 'cru' }) === false && V.ownerTalking({ away: true, now: T0 }) === true, 'paused for Cru: no recording view, and Claude\'s replies wait');
ok(cc.includes("window.addEventListener('crundi-voice-open', onCruOpen)") && cc.includes("window.addEventListener('crundi-voice-close', onCruClose)"), 'the chat listens for Cru\'s hand-over events');
const cv = read('app', 'vendor', 'crundi-voice.js');
ok(/function open\(opts\) \{\s*if \(S\) return;\s*opts = opts \|\| \{\};\s*announce\('crundi-voice-open'/.test(cv), 'Cru announces it is opening before it asks for the microphone');
ok(/var s = S; S = null;\s*announce\('crundi-voice-close'/.test(cv), 'and that it closed');
ok(/mouseenter[^\n]*CrundiChat\.voiceActive === true\)\) return;/.test(cv), 'hovering Cru\'s button (not asking) never takes a chat\'s microphone');
{
  const vw2 = { addEventListener() {}, location: {}, navigator: {} };
  vm.runInNewContext(cv, { window: vw2, document: { addEventListener() {}, createElement() { return {}; } }, navigator: {}, localStorage: { getItem() { return null; } } });
  ok(vw2.CrundiVoice.kit && typeof vw2.CrundiVoice.kit.createGate === 'function' && typeof vw2.CrundiVoice.kit.makeLoopback === 'function' && V.gateKit(vw2) === vw2.CrundiVoice.kit, 'Cru shares its own-voice tools as CrundiVoice.kit, and the chat uses them');
}
ok(V.holdsMic({ on: true }) && V.holdsMic({ ptt: true }) && V.holdsMic({ sending: true }) && !V.holdsMic({ on: true, away: 'cru' }) && !V.holdsMic({}), 'a chat holds the microphone in voice mode (not while paused for Cru) or a hold');
ok(win.CrundiChat.voiceActive === false, 'no chat listening: CrundiChat.voiceActive is false');
ok(V.onClaim({ on: true }) === 'off' && V.onClaim({ on: false }) === 'none', 'one chat in voice mode at a time: another turning it on turns this one off');
ok(cc.includes("voiceClaim(voiceSelf);") && cc.includes("new BroadcastChannel('crundi-chat-voice')") && cc.includes("'Voice mode moved to another chat.'"), 'across the workbench\'s cells and other windows (the bubble page), with a note in the one turned off');

// ─── The PC's "Hey Cru" listener ───
const { IS_OPEN_SCRIPT } = await import('../app/voice-wake/controller.js');
const isOpen = (w) => vm.runInNewContext(IS_OPEN_SCRIPT, { window: w });
ok(isOpen({}) === 'closed' && isOpen({ CrundiVoice: { isOpen: false } }) === 'closed', 'nothing listening: Hey Cru listens');
ok(isOpen({ CrundiVoice: { isOpen: true } }) === 'open' && isOpen({ CrundiVoice: { isOpen: true, holdsMic: false } }) === 'asleep', 'Cru\'s screen as before');
ok(isOpen({ CrundiVoice: { isOpen: false }, CrundiChat: { voiceActive: true } }) === 'chat' && isOpen({ CrundiChat: { voiceActive: false } }) === 'closed' && isOpen({ CrundiChat: { mount() {} } }) === 'closed', 'a chat in voice mode takes the microphone: Hey Cru pauses, so "Hey Cru" in a sentence is words for that chat; an older page is as before');
const ctl = read('app', 'voice-wake', 'controller.js');
// 12 Nov: with background listening off, a closed screen is not opened again by a hit heard just after it closed.
ok(/function heard\(keyword, score\) \{[\s\S]{0,900}if \(!on\(\)\) \{[\s\S]{0,400}executeJavaScript\(IS_OPEN_SCRIPT[\s\S]{0,200}if \(st !== 'asleep'\)/.test(ctl), 'with background listening off, a hit wakes only a voice screen that is still open and asleep, asked at that moment');
ok(/setVoiceOpen\(st === 'open' \|\| st === 'chat'\)/.test(ctl) && ctl.includes("'chat-voice': 'Paused: a chat is listening'"), 'the listener pauses while a chat listens, says so, and resumes after');

// ─── Cru's voice screen: chat view by default, typed turns read rather than said (10 Nov) ───
{
  const cvs = read('app', 'vendor', 'crundi-voice.js');
  ok(/function chatPref\(\) \{ try \{ return localStorage\.getItem\(CHAT_KEY\) !== '0'; \} catch \(e\) \{ return true; \} \}/.test(cvs) && /if \(chatPref\(\) && !S\.kid\) setTimeout/.test(cvs), 'the chat view is the default (until the owner goes back to the voice view); kid mode keeps its own screen');
  ok(/if \(say && speaks\(S, 'reply', typed\)\) speak\(say\); else \{ stopCue\(\); afterSpeech\(\); \}/.test(cvs) && /if \(s\.kid\) return true;[\s\S]{0,80}if \(kind === 'reply'\) return !typed;/.test(cvs), 'a typed turn\'s answer is not said (no speech made for it); kid mode still talks');
  ok(/if \(say && speaks\(S, 'unprompted'\)\) speak\(say\); else afterSpeech\(\);/.test(cvs) && /if \(kind === 'unprompted'\) return !s\.lastTyped;/.test(cvs), 'news unasked is said only while the owner\'s last message was spoken');
  ok(/'<button class="cv-mi cv-quiet" role="menuitemcheckbox"/.test(cvs) && /localStorage\.setItem\(QUIET_KEY/.test(cvs) && /localStorage\.setItem\(TYPED_KEY/.test(cvs), 'Cru\'s voice can be muted from the menu; that and the last input mode are kept on the device');
  ok(/class="cv-say" data-say="' \+ i \+ '"/.test(cvs) && /closest\('\.cv-say'\)[\s\S]{0,80}sayAgain\(Number/.test(cvs), 'each reply in the chat view has a speaker that says it');
  ok(/function sayAgain\(i\) \{[\s\S]{0,400}S\.phase !== 'listen' \|\| S\.heard \|\| S\.pending \|\| !speaks\(S, 'tap'\)\) return;[\s\S]{0,60}speak\(m\.speak\)/.test(cvs), 'and only while Cru is just listening');
  ok(/S\.phase === 'speak' && i === speakingIndex\(S\.msgs, S\.phase, S\.lastSaid\)\) \{ cutIn\(\);/.test(cvs), 'tapped while it says that reply: it stops');
  // Thinking and Speaking in the conversation (11 Nov): typing dots at the end, a sound sign by the reply being said.
  ok(/if \(S\.phase === 'think'\) h \+= '<div class="cv-m cru cv-typing"/.test(cvs) && /@keyframes cvdot/.test(cvs), 'while Cru thinks, a typing bubble at the end of the conversation');
  ok(/m\.speak \? \(i === sayingAt\s*\? '<button class="cv-say on"/.test(cvs) && /\.cv-say\.on \.w1\{animation:cvwave/.test(cvs), 'while it speaks, the same icon by the reply being said moves (one icon: tap to hear, and the Speaking sign)');
  ok(/\.cv-say\{position:absolute;right:-40px;bottom:0/.test(cvs) && /\.cv-m\.cru\.say\{margin-right:40px\}/.test(cvs), 'it sits outside the bubble, at its bottom right');
  ok(/var SPEAKER_SVG = '<svg[^']*M4 21v-4\.4C2\.2 15\.2/.test(cvs) && read('android', 'tools', 'icons-extra.json').includes('"speaking-head": "<path d=\\"M4 21v-4.4C2.2 15.2'), 'a speaking head (the owner\'s drawing), the same shape on web and Android');
  {
    const w = { addEventListener() {}, location: {}, navigator: {} };
    vm.runInNewContext(cvs, { window: w, document: { addEventListener() {}, createElement() { return {}; } }, navigator: {}, localStorage: { getItem() { return null; } } });
    const si = w.CrundiVoice._test.speakingIndex;
    const msgs = [{ speak: 'Yes, Sir?' }, { speak: 'Done, Sir.' }, { speak: 'Yes, Sir?' }, { said: 'hi', speak: '' }];
    ok(si(msgs, 'speak', 'Done, Sir.') === 1 && si(msgs, 'speak', ' Yes, Sir? ') === 2, 'the sign goes by the reply with those words (the latest of two alike)');
    // The X on a faded bubble (10 Nov): that phrase goes before it reaches Cru; the answer in progress stays.
    const drop = w.CrundiVoice._test.dropLive, qd = w.CrundiVoice._test.queuedOf, hd = w.CrundiVoice._test.hearingOf;
    const st = { phase: 'think', asked: 'open the board', pending: 'and the logs', heard: 'and the logs please', turn: 3 };
    ok(drop(st, 'queued') === true && qd(st) === '' && hd(st) === 'please' && st.phase === 'think' && st.turn === 3, 'the queued bubble\'s X: never asked, the words still coming in stay, the turn untouched');
    const st2 = { phase: 'think', asked: 'open the board', pending: 'and the logs', heard: 'and the logs please' };
    ok(drop(st2, 'hearing') === 'stream' && hd(st2) === '' && qd(st2) === 'and the logs', 'the heard bubble\'s X: those words go (the stream is told), the queue stays');
    const st3 = { phase: 'listen', heard: 'open dhivo' };
    ok(drop(st3, 'hearing') === 'stream' && st3.heard === '' && drop(st3, 'hearing') === false && drop({ phase: 'listen', heard: '' }, 'queued') === false, 'listening: the words go; nothing there, nothing happens');
    ok(/data-drop="queued"/.test(cvs) && /data-drop="hearing"/.test(cvs) && /closest\('\.cv-drop'\)[\s\S]{0,80}dropTapped\(/.test(cvs), 'the X is on each faded bubble, and wired');
    ok(si(msgs, 'listen', 'Done, Sir.') === -1 && si(msgs, 'speak', 'Could not reach the server') === -1 && si(msgs, 'speak', '') === -1 && si([], 'speak', 'x') === -1, 'and nowhere when Cru is not speaking, or says something not in the list');
  }
  const cc2 = read('android', 'app', 'src', 'main', 'java', 'mv', 'obitech', 'crundi', 'voice', 'VoiceScreen.kt');
  ok(/if \(thinking\) item\(key = "thinking"\) \{ ThinkingBubble\(\) \}/.test(cc2) && /speaking = i == speakingAt/.test(cc2) && /if \(onSay != null\) SpeakerButton\(speaking, onSay/.test(cc2) && /IconPaths\.all\["speaking-head"\]/.test(cc2), 'Android: the same typing bubble and sound sign');
  ok(/key = "queued"\) \{ Bubble\([^\n]*onDrop = \{ more\.onDropLive\(true\) \}/.test(cc2) && /key = "hearing"\) \{ Bubble\([^\n]*onDrop = if \(ui\.state\.cancellable\)/.test(cc2), 'Android: the same X on each faded bubble');
  const ms = read('android', 'app', 'src', 'main', 'java', 'mv', 'obitech', 'crundi', 'voice', 'MicStream.kt');
  ok(ms.includes('getBoolean("voice.chatView", true)'), 'Android: the chat view is the default too');
  const vc = read('android', 'app', 'src', 'main', 'java', 'mv', 'obitech', 'crundi', 'voice', 'VoiceController.kt');
  ok(/silent = typed && VoiceApi\.HISTORY in _ui\.value\.caps\)\)/.test(vc) && /fun sayAgain\(text: String\)/.test(vc), 'Android: a typed turn\'s reply is silent, and the bubble\'s speaker says it');
}

console.log(failed ? `\n${failed} failed` : '\nAll passed');
process.exit(failed ? 1 : 0);
