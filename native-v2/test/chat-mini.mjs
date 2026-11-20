// Mini, the chat mascot (F101; app/vendor/crundi-mini.js): each scene
// started as the web's trigger(name) starts it (`mini-trigger`), Mini
// coming out of the logo (the top bar's head hidden while it is out), what
// sends it away (typing, the queue or the suggestion going: a fall), a tap
// and four taps (it storms off), its eyes on the cursor, the parked chat's
// scene and its hand-over on Resume, a top bar outing, and the frame rules:
// while Mini is not out nothing draws; while it animates only its own layer
// draws (never the log, the composer, the heads, the top bar).
//
// Run as test/lib.mjs says, on a FRESH tools/stub-server.sh <port>:
//   CRUNDI_TEST_SERVER=http://localhost:4885 CRUNDI_TEST_PORT=4886 NATIVE_BIN=… \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/chat-mini.mjs

import * as t from './lib.mjs';

const T = await t.begin('chat-mini');
const A = 'chat-mini';
const M = async () => (await T.state()).mini || {};
const untilM = (what, f, ms = 20000) => T.until(what, (s) => f(s.mini || {}, s), ms);
const renders = async () => (await T.state()).renders || {};
const diff = (a, b) => Object.fromEntries(Object.keys(b).map((k) => [k, (b[k] || 0) - (a[k] || 0)]).filter(([, v]) => v));
const idleCheck = async (what) => {
  await T.settle(1500, 20000);
  const r0 = await renders();
  const m = T.frames.mark();
  await t.sleep(4000);
  const idle = T.frames.since(m);
  const drew = diff(r0, await renders());
  t.check(`${what}: no frames for 4 s`, idle.frames === 0, idle);
  t.check(`${what}: no view rendered`, Object.keys(drew).length === 0, drew);
};
/** Mini has left the chat and folded back into the logo. */
const home = (what) => untilM(what, (m) => !m.scene && !m.actor && !m.logoAway && m.holder == null && m.topActors === 0 && !m.looping, 30000);

await T.ui('new-chat');
await T.until('a chat pane, idle', (s) => s.chat && s.chatState === 'idle' && s.composer?.state === 'idle', 30000);
await t.sleep(4000);
let m = await M();
t.check('Mini is not out, the logo is home', !m.scene && !m.actor && !m.logoAway && m.holder == null, m);
t.check('under the test driver Mini waits to be asked', m.auto === false, m);
await idleCheck('idle, Mini not out');

// ─── nudge: out of the logo, by Send ───
await T.ui('mini-trigger', 'nudge');
m = (await untilM('the logo grows a body and walks off (its head gone from the top bar)', (m) => m.logoAway && m.topRoles?.includes('logo')))?.mini;
m = (await untilM('then Mini walks into the chat: the nudge', (m) => m.scene === 'nudge' && m.actor && m.actor.alpha > 0.9 && m.actor.x < m.layer.w - 20, 30000))?.mini;
t.check('it holds Mini (one at a time)', m?.holder === (await T.state()).chat, m);
t.check('it stands on the composer, in the pane', m?.actor && m.layer && m.actor.x > 0 && m.actor.x < m.layer.w && m.actor.y > m.layer.h * 0.5, m);
await T.snap(A, 'nudge');
// While it animates, only its layer draws.
{
  const r0 = await renders();
  const fm = T.frames.mark();
  await t.sleep(2000);
  const drew = diff(r0, await renders());
  const fr = T.frames.since(fm);
  console.log('views rendered while Mini animates:', JSON.stringify(drew), 'frames', fr.frames);
  t.check('Mini animates (frames, its layer)', fr.frames > 20 && (drew['chat-mini'] || 0) > 20, { drew, frames: fr.frames });
  t.check('…at most 60 a second', (drew['chat-mini'] || 0) <= 2000 / 16 + 5, drew);
  t.check('no log, composer, head, top bar or sidebar redraws for it', !drew.log && !drew.composer && !drew.head && !drew['top-bar'] && !drew.sidebar && !drew['project-bar'], drew);
}
// Its eyes follow the cursor (the pointer over the pane, near it).
m = await M();
await T.mouse('move', m.layer.x + m.actor.x + 80, m.layer.y + m.actor.y - 60);
await untilM('its eyes follow the cursor', (m) => Math.abs(m.actor?.lookX || 0) > 0.3, 5000);
await T.mouse('move', 3, 3);
// Typing sends it away; it goes home along the top bar and folds into the logo.
await T.ui('composer-text', 'hello');
await untilM('someone types: it leaves', (m) => m.scene === 'leaving' || !m.scene, 10000);
await T.ui('composer-text', '');
await home('…and folds back into the logo');
await idleCheck('Mini gone home');

// ─── sit: on the suggestion's corner; the seat goes, it falls ───
await T.ui('send', 'suggest next please');
await T.until('a suggestion, in the dock', (s) => s.log?.suggestion === 'Tell me more about it' && s.chatState === 'idle', 30000);
await t.sleep(800);
await T.ui('mini-trigger', 'sit');
m = (await untilM('sit: it comes and sits on the suggestion', (m) => m.scene === 'sit' && m.actor?.sit === 1, 30000))?.mini;
await t.sleep(800);
await T.snap(A, 'sit');
// A real click on the robot: annoyed, and it carries on.
m = await M();
if (m.actor && m.layer) {
  await T.mouse('click', m.layer.x + m.actor.x, m.layer.y + m.actor.y - 20);
  await t.sleep(300);
  m = await M();
  t.check('a tap does not send it away', m.scene === 'sit', m);
}
await T.ui('sug-dismiss');
await untilM('the suggestion goes: it falls and runs off', (m) => m.scene === 'leaving', 10000);
await home('…and goes home');

// ─── queue: a held message ───
await T.ui('composer-event', JSON.stringify({ type: 'state', state: 'needs-input' }));
await T.ui('composer-text', 'and also this');
await T.ui('composer-send');
await T.until('the line is held (the queue drawer)', (s) => s.composer?.qHeldN === 1, 10000);
await t.sleep(400);
await T.ui('mini-trigger', 'queue');
m = (await untilM('queue: it walks along the waiting message', (m) => m.scene === 'queue' && m.actor?.alpha > 0.9, 30000))?.mini;
t.check('it stands above the composer\'s queue drawer', m?.actor && m.actor.y < m.layer.h, m);
await t.sleep(1500);
await T.snap(A, 'queue');
// Four taps within 16 s: it storms off.
for (let i = 0; i < 4; i++) { await T.ui('mini-poke'); await t.sleep(250); }
await untilM('four taps: it storms off', (m) => m.scene === 'leaving', 10000);
await home('…and goes home');
await T.ui('composer-event', JSON.stringify({ type: 'state', state: 'idle' }));
await T.until('the held line went out', (s) => s.composer?.qHeldN === 0, 10000);
await T.until('the chat is idle again', (s) => s.chatState === 'idle', 30000);

// ─── wary of the cursor ───
await T.ui('mini-trigger', 'nudge');
await untilM('the nudge again', (m) => m.scene === 'nudge' && m.actor?.alpha > 0.9, 30000);
await T.ui('mini-poke');
await t.sleep(600);
await T.ui('mini-scare');
await t.sleep(300);
m = await M();
t.check('tapped, then the cursor comes close: it watches it', m.scene === 'nudge' && m.actor && Math.abs(m.actor.lookX) > 0.1, m);
await T.ui('mini-act');
await home('the person gets on with things: it goes home');

// ─── a top bar outing; a chat calls it away ───
let r = await T.ui('mini-top', 'party');
t.check('a top bar outing starts (a limit reset: a party)', r.ok, r);
m = (await untilM('Mini is out in the top bar', (m) => m.topBusy && m.holder === 'top' && m.logoAway))?.mini;
await t.sleep(1500);
await T.snap(A, 'top-party');
{
  const r0 = await renders();
  await t.sleep(1000);
  const drew = diff(r0, await renders());
  t.check('its frames draw its layer, not the top bar', (drew['mini-top'] || 0) > 10 && !drew['top-bar'] && !drew.log && !drew.composer, drew);
}
await T.ui('mini-trigger', 'nudge');
await untilM('a chat wants it: it leaves the top bar for the chat', (m) => m.scene === 'nudge' && m.actor && !m.topBusy, 40000);
await T.ui('mini-act');
await home('…and home again');

// ─── a parked chat: on Resume; resumed under its feet ───
const chat = (await T.state()).chat;
const pk = await T.api.http('POST', `/api/panes/${chat}/park`, {});
t.check('the server parks the chat', pk.body?.ok, pk.body);
await T.until('the parked card', (s) => s.log?.parked?.hidesComposer && s.mini?.mode === 'parked', 20000);
await t.sleep(800);
await T.ui('mini-trigger', 'parked');
m = (await untilM('parked: it comes and stands on Resume', (m) => m.scene === 'parked' && m.actor?.alpha > 0.9, 30000))?.mini;
await t.sleep(1500);
await T.snap(A, 'parked');
await T.ui('park-resume', 'resume');
await untilM('resumed: picked up and put on the window\'s layer, it runs off', (m) => m.topRoles?.includes('hand') || m.topRoles?.includes('logo'), 30000);
await home('…and folds back into the logo');
await T.until('the resumed chat is idle', (s) => s.chatState === 'idle' && s.mini?.mode === 'live', 30000);

await idleCheck('at the end');
await T.end();
