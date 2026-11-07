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
ok(!!V && typeof win.CrundiChat.mount === 'function', 'the component loads with its voice rules beside mount');

// ─── Tap or hold ───
ok(V.HOLD_MS === 350, 'a hold is 350 ms');
ok(V.pressKind(0) === 'tap' && V.pressKind(120) === 'tap' && V.pressKind(349) === 'tap', 'a short press is a tap (voice mode)');
ok(V.pressKind(350) === 'hold' && V.pressKind(2000) === 'hold', 'a long press is push to talk');

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
const cc = read('app', 'vendor', 'claude-chat.js');
ok(/case 'voice-say': onVoiceSay\(ev\)/.test(cc), 'the view takes voice-say events');
ok(cc.includes("'Voice input (tap: voice mode, hold: push to talk)'"), 'the mic has its accessible label');
ok(/\/api\/voice\/listen\?encoding=linear16&sample_rate=16000&token=/.test(cc), 'it listens as Cru\'s screen does: 16 kHz PCM');
ok(cc.includes("micSend({ type: 'stop' })") && cc.includes("type: p ? 'pause' : 'resume'"), 'push to talk ends with stop; Claude\'s voice pauses the listening');
ok(/typeof opts\.token === 'function'/.test(cc), 'an older host (no token) gets no mic, and no error');
ok(!/[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}]/u.test(cc.slice(cc.indexOf('// ─── Voice mode ───'), cc.indexOf('// ─── Claude\'s voice ───'))), 'flat icons, no emoji');
const wb = read('src', 'webapp-html.js');
const mountAt = wb.indexOf('const view = window.CrundiChat.mount(mountEl, {');
ok(mountAt > 0 && /token: \(\) => token,/.test(wb.slice(mountAt, mountAt + 1200)), 'the workbench hands its chats the token');
const page = read('app', 'vendor', 'crundi-chat-page.js');
const pageAt = page.indexOf("view = window.CrundiChat.mount($('cp-chat'), {");
ok(pageAt > 0 && /token: function \(\) \{ return token; \},/.test(page.slice(pageAt, pageAt + 800)), 'so does the compact chat page');

console.log(failed ? `\n${failed} failed` : '\nAll passed');
process.exit(failed ? 1 : 0);
