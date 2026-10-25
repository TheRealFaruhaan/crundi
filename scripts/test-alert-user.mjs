#!/usr/bin/env node
/**
 * test-alert-user.mjs — the alert_user MCP tool.
 *
 * Checks the push it makes (kind "alert", urgent and downgraded as new
 * optional fields, the old fields as they were), the server's hourly limits
 * (urgent turns into a normal alert past 3, everything stops past 20, and the
 * window rolls), the fallback to the other channels when there is no phone,
 * and that the tool is declared, dispatched and listed where it must be.
 *
 * Run: node scripts/test-alert-user.mjs
 */

import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dataDir = mkdtempSync(join(tmpdir(), 'alert-test-'));
process.env.DATA_DIR = dataDir;
const { createAlerter, URGENT_PER_HOUR, ALERTS_PER_HOUR, WINDOW_MS, HUSH_MS, ACK_MS, ANSWER_GRACE_MS, RING_MAX_MS, describeAnswer } = await import('../src/alert-user.js');
const mp = await import('../src/mobile-push.js');
const { CAPABILITIES } = await import('../src/client-version.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── The payload ───
{
  const chat = { id: 'abc123', title: 'Deploy', project: 'crundi', mark: { initials: 'D', color: '#111111', colorDeep: '#000000' } };
  const urgent = JSON.parse(mp.buildPayload('The site is down', { title: 'Outage', tag: 'alert', url: '/chat/abc123', chat, urgent: true }));
  ok(urgent.kind === 'alert', 'an alert is a push of kind "alert"');
  ok(urgent.urgent === true, 'an urgent alert carries urgent: true');
  ok(!('downgraded' in urgent), 'an alert sent as asked carries no downgraded field');
  ok(urgent.v === 1 && urgent.title === 'Outage' && urgent.body === 'The site is down' && urgent.url === '/chat/abc123' && typeof urgent.at === 'number',
    'the existing fields are unchanged (v, title, body, url, at)');
  ok(urgent.chat && urgent.chat.id === 'abc123' && urgent.chat.title === 'Deploy', 'the chat is attached when known');
  ok(!('silent' in urgent) && !('quiet' in urgent), 'an alert is neither silent nor quiet');

  const normal = JSON.parse(mp.buildPayload('FYI', { title: 'Note', tag: 'alert' }));
  ok(!('urgent' in normal) && !('downgraded' in normal), 'a normal alert has neither new field (an older app sees nothing new)');
  const down = JSON.parse(mp.buildPayload('x', { title: 'y', tag: 'alert', downgraded: true }));
  ok(down.downgraded === true && !('urgent' in down), 'a downgraded alert says so and does not ring');

  const plain = JSON.parse(mp.buildPayload('Service crashed', { title: 'Crundi', tag: 'serviceCrash' }));
  ok(!('urgent' in plain) && !('downgraded' in plain), 'other notifications are untouched');
}

// ─── A fake clock, phone and channels ───
function rig({ phoneOk = true, pushOk = true, channelsOn = ['telegram'] } = {}) {
  let t = 1_800_000_000_000;
  const pushes = [];
  const delivered = [];
  const alerter = createAlerter({
    now: () => t,
    phone: () => (phoneOk ? { ok: true } : { ok: false, reason: 'No phone is paired with this server.' }),
    push: async (text, meta) => { pushes.push({ text, meta }); return pushOk; },
    deliver: async (text, meta, opts) => {
      delivered.push({ text, meta, opts });
      return channelsOn.filter((id) => !(opts && opts.except === id)).map((id) => ({ id, ok: true }));
    },
  });
  return { alerter, pushes, delivered, tick: (ms) => { t += ms; } };
}

// ─── Arguments ───
{
  const { alerter, pushes } = rig();
  const r = await alerter.alert({ body: 'no title' });
  ok(!r.ok && /title/.test(r.error), 'a title is required');
  const r2 = await alerter.alert({ title: 'x', level: 'panic' });
  ok(!r2.ok && /level/.test(r2.error), 'an unknown level is refused');
  ok(pushes.length === 0, 'nothing is sent for a refused call');
  const r3 = await alerter.alert({ title: 'Default level' });
  ok(r3.ok && r3.level === 'urgent' && pushes[0].meta.urgent === true, 'the level defaults to urgent');
  ok(pushes[0].text === 'Default level', 'a title alone is also the body, so the phone has something to show');
  const r4 = await alerter.alert({ title: 'Heads up', body: 'details', level: 'normal' });
  ok(r4.ok && r4.level === 'normal' && !pushes[1].meta.urgent, 'a normal alert does not ring');
  ok(pushes[1].meta.tag === 'alert' && pushes[1].text === 'details', 'the push is tagged alert and carries the body');
}

// ─── The chat it came from ───
{
  const { alerter, pushes } = rig();
  const chat = { id: 'c1', title: 'Night job', project: 'p' };
  await alerter.alert({ title: 'Done' }, { chat });
  ok(pushes[0].meta.chat === chat && pushes[0].meta.url === '/chat/c1', 'an alert from a chat is filed under it and opens it');
  await alerter.alert({ title: 'Done' });
  ok(!pushes[1].meta.chat && pushes[1].meta.url === '/', 'without a chat it opens the app');
}

// ─── The urgent limit: past it, sent as normal; the window rolls ───
{
  const { alerter, pushes, tick } = rig();
  for (let i = 0; i < URGENT_PER_HOUR; i++) {
    const r = await alerter.alert({ title: `u${i}` });
    ok(r.ok && r.level === 'urgent' && !r.downgraded, `urgent alert ${i + 1} of ${URGENT_PER_HOUR} rings`);
    tick(60_000);
  }
  const over = await alerter.alert({ title: 'one too many' });
  ok(over.ok && over.level === 'normal' && over.downgraded === true, 'the next urgent alert in the hour is sent as normal');
  const last = pushes[pushes.length - 1].meta;
  ok(!last.urgent && last.downgraded === true, 'its push does not ring, and says it was downgraded');
  ok(/possible again in \d+ minutes?/.test(over.note), 'the result says when urgent is possible again');
  ok(alerter.usage().urgent === URGENT_PER_HOUR, 'a downgraded alert does not count as a ring');
  // The first ring was at 0; an hour after it, one urgent is possible again.
  tick(WINDOW_MS - 3 * 60_000);
  const again = await alerter.alert({ title: 'after the hour' });
  ok(again.ok && again.level === 'urgent' && !again.downgraded, 'once the oldest ring is an hour old, urgent rings again');
  const again2 = await alerter.alert({ title: 'and one more' });
  ok(again2.downgraded === true, 'but only as many as have rolled out of the window');
}

// ─── The overall limit: past it, refused ───
{
  const { alerter, pushes, tick } = rig();
  for (let i = 0; i < ALERTS_PER_HOUR; i++) { await alerter.alert({ title: `n${i}`, level: 'normal' }); tick(1000); }
  ok(pushes.length === ALERTS_PER_HOUR, `${ALERTS_PER_HOUR} alerts in an hour are sent`);
  const r = await alerter.alert({ title: 'too many', level: 'normal' });
  ok(!r.ok && /limit/.test(r.error) && /send_message_to_user/.test(r.error), 'the next is refused, pointing at send_message_to_user');
  ok(/next one can go in \d+ minutes?/.test(r.error), 'and says when the next can go');
  const u = await alerter.alert({ title: 'urgent too' });
  ok(!u.ok, 'an urgent one is refused too');
  ok(pushes.length === ALERTS_PER_HOUR, 'nothing more went out');
  tick(WINDOW_MS);
  ok((await alerter.alert({ title: 'later' })).ok, 'an hour on, alerts go again');
}

// ─── No phone: the other channels, and the result says why ───
{
  const { alerter, pushes, delivered } = rig({ phoneOk: false });
  const r = await alerter.alert({ title: 'Disk full', body: '/var is at 100%' });
  ok(pushes.length === 0, 'with no phone, nothing is pushed');
  ok(r.ok && r.delivered === 'channels' && r.channels.includes('telegram'), 'the alert goes out through the enabled channels');
  ok(/Could not ring a phone: No phone is paired/.test(r.note), 'the result says it could not ring a phone, and why');
  ok(delivered[0].opts.except === 'mobile', 'the phone channel is left out of the fallback');
  ok(delivered[0].text.includes('Disk full') && delivered[0].text.includes('/var is at 100%'), 'the fallback carries the title and the body');
}
{
  const { alerter, delivered } = rig({ phoneOk: true, pushOk: false });
  const r = await alerter.alert({ title: 'Relay down' });
  ok(r.ok && r.delivered === 'channels' && delivered.length === 1, 'a push the relay did not take falls back to the channels');
  ok(alerter.usage().urgent === 0, 'an alert that did not ring does not use up the urgent allowance');
}
{
  const { alerter } = rig({ phoneOk: false, channelsOn: [] });
  const r = await alerter.alert({ title: 'Nobody' });
  ok(!r.ok && /reached nobody/.test(r.error), 'with no phone and no channel, the tool says it reached nobody');
  ok(alerter.usage().alerts === 0, 'and it does not count against the limit');
}

// ─── notify-channels: except leaves one channel out ───
{
  const channels = await import('../src/notify-channels.js');
  channels._reset();
  const got = [];
  channels.register({ id: 'telegram', enabledByDefault: true, send: async () => { got.push('telegram'); return true; } });
  channels.register({ id: 'mobile', enabledByDefault: true, send: async () => { got.push('mobile'); return true; } });
  await channels.deliver('x', {}, { except: 'mobile' });
  ok(got.join() === 'telegram', 'deliver(..., { except }) skips that channel');
  got.length = 0;
  await channels.deliver('x', {});
  ok(got.join() === 'telegram,mobile', 'deliver() without it is unchanged');
  channels._reset();
}

// ─── Hush: the chat's own notifications do not cut the ringing short ───
{
  const { alerter, tick } = rig();
  const chat = { id: 'c7', title: 'Deploy' };
  ok(!alerter.hushing('c7'), 'hush: nothing rang yet');
  await alerter.alert({ title: 'Down' }, { chat });
  ok(alerter.hushing('c7') && !alerter.hushing('other') && !alerter.hushing(''), 'hush: the chat that rang, and only it');
  tick(HUSH_MS - 1000);
  ok(alerter.hushing('c7'), 'hush: still within the window');
  tick(2000);
  ok(!alerter.hushing('c7'), 'hush: over after HUSH_MS');
  await alerter.alert({ title: 'FYI', level: 'normal' }, { chat: { id: 'c8', title: 'Notes' } });
  ok(!alerter.hushing('c8'), 'hush: a normal alert does not ring, so nothing is hushed');
  const web = readFileSync(join(root, 'src/webapp.js'), 'utf8');
  const fn = web.slice(web.indexOf('function notifyEvent('), web.indexOf('function notifyEvent(') + 1500);
  ok(/alerter\.hushing\(meta\.chat\.id\)/.test(fn) && /quiet: true[\s\S]{0,80}only: \['mobile'\]/.test(fn), 'hush: notifyEvent sends a hushed chat\'s notification to the phone only, quietly');
}

// ─── Waiting for the answer ───
{
  // A fake timer: fire() runs the pending one, as if its time had come.
  let pending = null;
  const timers = { set: (fn, ms) => { pending = { fn, ms }; return pending; }, clear: (t) => { if (pending === t) pending = null; } };
  const mk = () => createAlerter({ phone: () => ({ ok: true }), push: async () => true, deliver: async () => [], timers });
  {
    const a = mk();
    const r = await a.alert({ title: 'Down' });
    ok(r.ok && /^[0-9a-f]{16}$/.test(r.alertId), 'answer: an urgent alert has an alertId to wait on');
    const w = a.waitForAnswer(r.alertId);
    ok(pending && pending.ms === ACK_MS, 'answer: first it waits for the phone to say it is ringing');
    ok(a.answer(r.alertId, { action: 'ringing', ringMs: 5 * 60000 }) && pending.ms === 5 * 60000 + ANSWER_GRACE_MS, 'answer: then for the ring time the phone gives, plus a little');
    ok(a.answer(r.alertId, { action: 'snoozed', minutes: 15 }), 'answer: the phone reports a snooze');
    const got = await w;
    ok(got.answer === 'snoozed' && got.snoozeMinutes === 15 && got.ringMinutes === 5, 'answer: the caller gets the snooze and its minutes');
    ok(!a.answer(r.alertId, { action: 'dismissed' }), 'answer: a second answer is not waited on');
    ok(/snoozed the alert for 15 minutes/.test(describeAnswer(got)), 'answer: said in a sentence');
  }
  {
    const a = mk();
    const r = await a.alert({ title: 'Down' });
    const w = a.waitForAnswer(r.alertId);
    a.answer(r.alertId, { action: 'ringing', ringMs: 99 * 60000 });
    ok(pending.ms === RING_MAX_MS, 'answer: never more than 15 minutes in all, whatever the phone says');
    pending.fn();
    const got = await w;
    ok(got.answer === 'no answer' && /rang for 15 minutes/.test(describeAnswer(got)), 'answer: nobody answered within the ring time');
  }
  {
    const a = mk();
    const r = await a.alert({ title: 'Down' });
    const w = a.waitForAnswer(r.alertId);
    pending.fn();
    ok((await w).answer === 'not confirmed', 'answer: a phone that never says it is ringing (an older app) is "not confirmed"');
  }
  {
    const a = mk();
    const r = await a.alert({ title: 'Down' });
    const w = a.waitForAnswer(r.alertId);
    ok(!a.answer(r.alertId, { action: 'exploded' }) && !a.answer('nope', { action: 'dismissed' }), 'answer: unknown actions and unknown alerts are ignored');
    a.answer(r.alertId, { action: 'dismissed' });
    ok((await w).answer === 'dismissed', 'answer: dismissed before the phone even said it rang still counts');
    const n = await a.alert({ title: 'FYI', level: 'normal' });
    ok(n.ok && !n.alertId, 'answer: a normal alert has nothing to wait for');
  }
  const web = readFileSync(join(root, 'src/webapp.js'), 'utf8');
  ok(/alerter\.waitForAnswer\(alertId\)/.test(web) && /path === '\/api\/alerts\/answer'/.test(web), 'answer: alert_user waits, and the phone has somewhere to report');
  ok(CAPABILITIES.includes('alert-answers'), 'answer: CAPABILITIES lists alert-answers');
  const payload = JSON.parse(mp.buildPayload('x', { tag: 'alert', urgent: true, alertId: 'abc123' }));
  ok(payload.alertId === 'abc123', 'answer: the push carries the alertId');
}

// ─── Wiring ───
{
  const stdio = readFileSync(join(root, 'src/mcp-stdio.js'), 'utf8');
  const web = readFileSync(join(root, 'src/webapp.js'), 'utf8');
  const prompt = readFileSync(join(root, 'src/system-prompt.js'), 'utf8');
  const policy = readFileSync(join(root, 'src/access-policy.js'), 'utf8');
  ok(/name: 'alert_user'/.test(stdio), 'the MCP server declares alert_user');
  const def = stdio.slice(stdio.indexOf("name: 'alert_user'"), stdio.indexOf("name: 'alert_user'") + 4000);
  ok(/send_message_to_user/.test(def) && /required: \['title'\]/.test(def) && /'normal', 'urgent'/.test(def), 'its description points ordinary updates at send_message_to_user; title required; two levels');
  ok(/name === 'alert_user'[\s\S]{0,200}CRUNDI_CHAT_ID/.test(stdio), 'the chat id comes from the bridge environment, not from Claude');
  ok(/body\.tool === 'alert_user'/.test(web) && /alerter\.alert\(/.test(web), 'the dispatcher handles alert_user');
  ok(!/'alert_user'/.test(policy), 'collaborators cannot ring the owner');
  ok(/alert_user/.test(prompt), 'the system prompt mentions alert_user');
  ok(CAPABILITIES.includes('alert-user'), 'CAPABILITIES lists alert-user');
}

rmSync(dataDir, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : '\nAll passed');
process.exit(failed ? 1 : 0);
