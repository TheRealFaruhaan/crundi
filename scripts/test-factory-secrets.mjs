#!/usr/bin/env node
/**
 * test-factory-secrets.mjs — a factory's pre-authorised secrets
 * (src/factory-secrets.js, the runner's side of it, and secret_run's
 * decision in authoriseSecretUse()).
 *
 * Covers: settings.secrets normalised to ids only (older factories have
 * none); PINs checked when a run starts, a wrong one refusing before anything
 * is made; the Manager using an unlocked secret at once; an employee's use
 * routed to the Manager, or to the Gatekeeper when there is one, as a
 * "[Factory] Secret request", and a refusal ending it; a rule never covering
 * a secret; a secret not on the list always going to the person; the fixed
 * deny list holding for a command run with a secret; stop, finish and discard
 * forgetting the PINs; after a restart the PIN is asked once on first use and
 * then remembered; and no value or PIN in the journal or the run store.
 *
 * Run: node scripts/test-factory-secrets.mjs
 */

import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const base = mkdtempSync(join(tmpdir(), 'factory-secrets-'));
process.env.DATA_DIR = join(base, 'data');
process.env.FACTORY_WORKTREES_DIR = join(base, 'worktrees');
mkdirSync(process.env.DATA_DIR, { recursive: true });
const { normalizeFactory } = await import('../src/factory-model.js');
const FS = await import('../src/factory-secrets.js');
const P = await import('../src/factory-permissions.js');
const secrets = await import('../src/secrets-store.js');
const { createFactoryRunner } = await import('../src/factory-runner.js');
const { fakeTeam } = await import('./factory-fixtures.mjs');
const { CAPABILITIES } = await import('../src/client-version.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(15); } return null; }
const sh = (a, cwd) => execFileSync('git', a, { cwd, encoding: 'utf-8' }).trim();
function repo(name) {
  const dir = join(base, 'projects', name);
  mkdirSync(dir, { recursive: true });
  sh(['init', '-q', '-b', 'dev'], dir);
  sh(['config', 'user.name', 'Person'], dir); sh(['config', 'user.email', 'p@p'], dir);
  writeFileSync(join(dir, 'a.txt'), 'a\n');
  sh(['add', '.'], dir); sh(['commit', '-q', '-m', 'init'], dir);
  return dir;
}

// Two real secrets in the real (encrypted) store.
const VALUE1 = 'tok-VALUE-one-9f3a', PIN1 = '482913';
const VALUE2 = 'tok-VALUE-two-77c1', PIN2 = '650274';
const S1 = secrets.addSecret({ name: 'Deploy token', value: VALUE1, pin: PIN1 }).id;
const S2 = secrets.addSecret({ name: 'Other token', value: VALUE2, pin: PIN2 }).id;
const meta = (id) => secrets.getSecretMeta(id);
const secretOf = (id) => ({ id, name: meta(id).name });

// ─── The model ───
{
  ok(CAPABILITIES.includes('factory-secrets'), 'handshake: the server says it has factory secrets, so clients show the picker and the start prompts');
  const f = normalizeFactory({ name: 'x', settings: { secrets: [S1, S1, { id: S2, name: 'Other token', value: VALUE2, pin: PIN2 }, 'not an id!', 5, null] } });
  ok(JSON.stringify(f.settings.secrets) === JSON.stringify([S1, S2]), 'model: the list keeps secret ids only, once each');
  ok(!JSON.stringify(f).includes(VALUE2) && !JSON.stringify(f).includes(PIN2), 'model: a value or a PIN sent along is dropped');
  ok(Array.isArray(normalizeFactory({ name: 'old' }).settings.secrets) && normalizeFactory({ name: 'old' }).settings.secrets.length === 0, 'model: an older factory without the field has an empty list');
  ok(FS.normalizeSecretIds(Array.from({ length: 40 }, (_, i) => 'id' + i)).length === FS.MAX_SECRETS, 'model: the list is capped');
  ok(FS.decide({ employeeId: 'manager', listed: true }) === 'direct' && FS.decide({ employeeId: P.GATEKEEPER_ID, listed: true }) === 'direct', 'decide: the Manager and the Gatekeeper use a listed secret directly');
  ok(FS.decide({ employeeId: 'dev', listed: true }) === 'ask-team' && FS.decide({ employeeId: 'manager', listed: false }) === 'person', 'decide: others ask the team; a secret not on the list goes to the person');
  const v = FS.createSecretVault(); v.remember('r', 's', '123456');
  ok(JSON.stringify({ v }) === '{"v":{}}', 'vault: it serialises to nothing, so a stray log cannot print a PIN');
}

function solo(settings = {}) {
  return {
    name: 'Solo', settings: { presets: [], ...settings },
    roster: [{ id: 'manager', name: 'Manager', persona: 'm' }, { id: 'dev', name: 'Developer', persona: 'd' }],
    nodes: [{ id: 'start', type: 'start' }, { id: 'mgr', type: 'manager' }, { id: 'end', type: 'end' }, { id: 'dev', type: 'employee', employeeId: 'dev', label: 'Developer' }],
    edges: [{ source: 'start', target: 'mgr' }, { source: 'mgr', sourceHandle: 'finish', target: 'end' }, { source: 'mgr', target: 'dev' }, { source: 'dev', target: 'mgr', maxRounds: 5 }],
  };
}

/** A Manager that hands to the Developer, who stays on its step (asking things). */
function team({ asks, finishWith = null } = {}) {
  let rounds = 0;
  return fakeTeam({
    manager: async (msg) => {
      if (/^\[Factory\] Secret request/.test(msg)) { asks.manager.push(msg); return null; }
      if (msg.includes('[Factory] Your step')) {
        rounds++;
        if (rounds > 1 && finishWith) return { payload: { summary: 'Done.', finish: true } };
        return { payload: { summary: 'Over to the developer' } };
      }
      return null;
    },
    __gatekeeper: async (msg) => { if (/^\[Factory\] Secret request/.test(msg)) asks.gate.push(msg); return null; },
    dev: async (msg) => {
      if (finishWith && /Your step/.test(msg)) { const w = (msg.match(/^Work in: (.+)$/m) || [])[1]; writeFileSync(join(w, 'hello.txt'), 'hi\n'); return { payload: { summary: 'Wrote hello.txt' } }; }
      return null;
    },
  });
}

function runnerFor(t, dir, f, extra = {}) {
  const r = createFactoryRunner({
    dataDir: process.env.DATA_DIR, agents: t.agents, getProject: () => ({ path: dir }), getFactory: () => ({ factory: f }),
    unlockSecret: (id, pin) => { const d = secrets.decryptSecret(id, pin); return d.ok ? { ok: true, name: d.name } : { ok: false, error: d.error }; },
    secretMeta: meta, settleMs: 10_000, permissionBatchMs: 20, ...extra,
  });
  t.setRunner(r);
  return r;
}

/** secret_run's decision, with the person's PIN prompt scripted. */
function use(r, chatId, id, { command = 'echo "$SECRET" | wc -c', person = null } = {}) {
  const calls = [];
  const askPerson = async (o) => {
    calls.push(o);
    if (!person) return { ok: false, error: 'The user did not approve.' };
    return person(o);
  };
  return FS.authoriseSecretUse({ runner: r, chatId, secret: secretOf(id), command, decrypt: (i, p) => secrets.decryptSecret(i, p), askPerson })
    .then(res => ({ ...res, calls }));
}
const personTypes = (id, pin) => async (o) => { const d = secrets.decryptSecret(id, pin); return d.ok ? { ok: true, value: d.value, ...(o.keepPin ? { pin } : {}) } : d; };

function onDisk(runId) {
  const dir = join(process.env.DATA_DIR, 'factory-runs', runId);
  return readdirSync(dir).filter(n => n.endsWith('.json') || n.endsWith('.jsonl')).map(n => readFileSync(join(dir, n), 'utf-8')).join('\n');
}
const leaks = (text) => [VALUE1, VALUE2, PIN1, PIN2].filter(x => text.includes(x));

// ─── A run with the Manager, an employee and the person ───
let runId = '';
{
  const dir = repo('p1');
  const asks = { manager: [], gate: [] };
  const t = team({ asks });
  const r = runnerFor(t, dir, solo({ secrets: [S1] }));

  const wrong = await r.startRun({ project: 'p1', factoryId: 'x', task: 'Deploy', secretPins: { [S1]: '000000' } });
  ok(!wrong.ok && wrong.field === 'secrets' && wrong.secretId === S1 && /Deploy token/.test(wrong.error), 'start: a wrong PIN is refused, naming the secret');
  ok(!r.activeRunFor('p1') && !r.listRuns().length, 'start: ...before anything is made');

  const st = await r.startRun({ project: 'p1', factoryId: 'x', task: 'Deploy', secretPins: { [S1]: PIN1, [S2]: PIN2 } });
  runId = st.run.id;
  ok(st.ok && st.run.secrets.length === 1 && st.run.secrets[0].name === 'Deploy token' && st.run.secrets[0].unlocked, 'start: the listed secret is unlocked, shown by name');
  ok(JSON.stringify(r.unlockedSecrets(runId)) === JSON.stringify([S1]), 'start: a PIN for a secret not on the list is not kept');
  await until(() => t.of('dev')[0]);
  const mgr = t.of('manager')[0], dev = t.of('dev')[0];

  // The Manager, a listed and unlocked secret: runs at once.
  const m1 = await use(r, mgr.id, S1);
  ok(m1.ok && m1.value === VALUE1 && m1.preauthorised && !m1.calls.length && !asks.manager.length, 'Manager: a pre-authorised secret runs without asking anyone');

  // An employee: the Manager decides, then the unlocked PIN runs it.
  const pending = use(r, dev.id, S1, { command: 'curl -H "Authorization: Bearer $SECRET" https://api.example.com' });
  await until(() => asks.manager.length);
  ok(/^\[Factory\] Secret request/.test(asks.manager[0]) && /Developer wants to run `curl/.test(asks.manager[0]) && /"Deploy token"/.test(asks.manager[0]), 'employee: the Manager gets a "[Factory] Secret request" with the name and the command');
  ok(!leaks(asks.manager[0]).length, 'employee: ...never the value');
  const id1 = (asks.manager[0].match(/- (p\d+) ·/) || [])[1];
  const rulesBefore = r.getRun(runId).rules.length;
  const a1 = await r.answerPermissions(mgr.id, [{ id: id1, allow: true, rule: 'Bash(curl:*)' }]);
  const e1 = await pending;
  ok(a1.ok && e1.ok && e1.value === VALUE1 && e1.preauthorised && JSON.stringify(e1.approvedBy) === '["manager"]' && !e1.calls.length, 'employee: approved by the Manager, the pre-authorised secret runs');
  ok(r.getRun(runId).rules.length === rulesBefore, 'employee: a rule never covers a secret');

  const pending2 = use(r, dev.id, S1);
  await until(() => asks.manager.length === 2);
  const id2 = (asks.manager[1].match(/- (p\d+) ·/) || [])[1];
  await r.answerPermissions(mgr.id, [{ id: id2, allow: false, reason: 'Not needed for this step' }]);
  const e2 = await pending2;
  ok(!e2.ok && /Not needed for this step/.test(e2.error) && !e2.calls.length, 'employee: a refusal is returned, and the person is not asked');

  // Not on the list: the person, whoever asks, and it is not remembered.
  const n1 = await use(r, mgr.id, S2, { person: personTypes(S2, PIN2) });
  ok(n1.ok && n1.value === VALUE2 && !n1.preauthorised && n1.calls.length === 1 && n1.calls[0].keepPin === false, 'not listed: the Manager\'s use goes to the person');
  ok(!r.unlockedSecrets(runId).includes(S2), 'not listed: ...and its PIN is not kept');
  const before = asks.manager.length;
  const n2 = await use(r, dev.id, S2, { person: personTypes(S2, PIN2) });
  ok(n2.ok && n2.calls.length === 1 && asks.manager.length === before, 'not listed: an employee\'s use goes straight to the person');

  // The fixed refusals hold for a command run with a secret.
  const sudo = await use(r, mgr.id, S1, { command: 'sudo curl -d "$SECRET" x' });
  ok(!sudo.ok && /sudo/.test(sudo.error), 'deny list: sudo with a secret is refused, even pre-authorised');

  // Older callers: no context for a chat outside a factory, so the person is asked.
  const outside = await use(r, 'not-a-factory-chat', S1, { person: personTypes(S1, PIN1) });
  ok(outside.ok && outside.calls.length === 1 && outside.calls[0].keepPin === false, 'outside a factory: the person is asked, as always');

  // The journal: name, who, approver, pre-authorised. Never a value or a PIN.
  await sleep(30);
  const used = r.runEvents(runId).events.filter(e => e.type === 'secret-used');
  ok(used.length >= 4 && used.some(e => e.secret === 'Deploy token' && e.employee === 'Manager' && e.preauthorised && !e.approvedBy.length), 'journal: the Manager\'s pre-authorised use is recorded');
  ok(used.some(e => e.employee === 'Developer' && e.preauthorised && e.approvedBy[0] === 'manager'), 'journal: the employee\'s use, approved by the Manager');
  ok(used.some(e => e.secret === 'Other token' && !e.preauthorised && e.approvedBy.includes('person')), 'journal: a use the person approved');
  ok(used.some(e => e.employee === 'Developer' && e.ok === false), 'journal: a refused use');
  ok(!leaks(onDisk(runId)).length, 'store: no value or PIN anywhere in the journal or the run store');
  ok(!leaks(JSON.stringify(r.getRun(runId))).length, 'store: ...nor in what the run shows');

  // Paused still counts as running.
  await r.pause(runId);
  ok(r.unlockedSecrets(runId).includes(S1), 'pause: the PIN is kept while paused');
  await r.resume(runId);
  await r.stopRun(runId);
  ok(!r.unlockedSecrets(runId).length, 'stop: the PINs are forgotten');
  const after = await use(r, mgr.id, S1, { person: personTypes(S1, PIN1) });
  ok(after.calls.length === 1 && after.calls[0].keepPin === false, 'stop: a stopped run\'s chat goes to the person, and nothing is kept');
  r.dispose();
}

// ─── The Gatekeeper answers first ───
{
  const dir = repo('p2');
  const asks = { manager: [], gate: [] };
  const t = team({ asks });
  const r = runnerFor(t, dir, solo({ secrets: [S1], gatekeeper: { enabled: true, model: 'haiku' } }));
  const st = await r.startRun({ project: 'p2', factoryId: 'x', task: 'Deploy', secretPins: { [S1]: PIN1 } });
  await until(() => t.of('dev')[0]);
  const dev = t.of('dev')[0];
  const pending = use(r, dev.id, S1);
  await until(() => asks.gate.length);
  ok(asks.gate.length === 1 && !asks.manager.length && /Secret request/.test(asks.gate[0]), 'gatekeeper: an employee\'s secret request goes to the Gatekeeper, not the Manager');
  const gate = t.of(P.GATEKEEPER_ID)[0];
  const id = (asks.gate[0].match(/- (p\d+) ·/) || [])[1];
  await r.answerPermissions(gate.id, [{ id, allow: true, reason: 'The policy covers deploys' }]);
  const e = await pending;
  ok(e.ok && e.preauthorised && JSON.stringify(e.approvedBy) === '["gatekeeper"]', 'gatekeeper: approved by it, the pre-authorised secret runs');
  const g1 = await use(r, gate.id, S1);
  ok(g1.ok && g1.preauthorised && !g1.calls.length, 'gatekeeper: it uses a listed secret itself without asking');
  await r.stopRun(st.run.id);
  r.dispose();
}

// ─── Approved, but not unlocked: the person's PIN, once ───
{
  const dir = repo('p3');
  const asks = { manager: [], gate: [] };
  const t = team({ asks });
  const r = runnerFor(t, dir, solo({ secrets: [S1] }));
  // As a trigger starts it: nobody there to type PINs.
  const st = await r.startRun({ project: 'p3', factoryId: 'x', task: 'Deploy' });
  ok(st.ok && !st.run.secrets[0].unlocked, 'trigger: a run started without PINs has its secrets locked');
  await until(() => t.of('dev')[0]);
  const mgr = t.of('manager')[0], dev = t.of('dev')[0];
  const pending = use(r, dev.id, S1, { person: personTypes(S1, PIN1) });
  await until(() => asks.manager.length);
  await r.answerPermissions(mgr.id, [{ id: (asks.manager[0].match(/- (p\d+) ·/) || [])[1], allow: true }]);
  const e = await pending;
  ok(e.ok && !e.preauthorised && e.calls.length === 1 && e.calls[0].keepPin && JSON.stringify(e.approvedBy) === '["manager","person"]', 'locked: approved by the Manager, then the person types the PIN');
  const m = await use(r, mgr.id, S1);
  ok(m.ok && m.preauthorised && !m.calls.length, 'locked: ...and from then on it is remembered');
  await r.stopRun(st.run.id);
  r.dispose();
}

// ─── Restart: the memory is empty; asked once, then remembered ───
{
  const dir = repo('p4');
  const asks = { manager: [], gate: [] };
  const t1 = team({ asks });
  const r1 = runnerFor(t1, dir, solo({ secrets: [S1] }));
  const st = await r1.startRun({ project: 'p4', factoryId: 'x', task: 'Deploy', secretPins: { [S1]: PIN1 } });
  ok(r1.unlockedSecrets(st.run.id).length === 1, 'restart: unlocked before');
  await until(() => t1.of('dev')[0]);
  r1.dispose();

  const t2 = team({ asks });
  const r2 = runnerFor(t2, dir, solo({ secrets: [S1] }));
  r2.load();
  ok(r2.getRun(st.run.id).status === 'crashed' && !r2.unlockedSecrets(st.run.id).length, 'restart: the run comes back crashed with nothing unlocked');
  const rec = await r2.recover(st.run.id);
  ok(rec.ok, 'restart: recovered');
  const mgr = await until(() => t2.of('manager')[0]);
  ok(!r2.getRun(st.run.id).secrets[0].unlocked, 'restart: not asked again at once; still locked after recovery');
  const first = await use(r2, mgr.id, S1, { person: personTypes(S1, PIN1) });
  ok(first.ok && first.value === VALUE1 && first.calls.length === 1 && first.calls[0].keepPin, 'restart: the first use asks the person once');
  const second = await use(r2, mgr.id, S1, { person: personTypes(S1, PIN1) });
  ok(second.ok && second.preauthorised && !second.calls.length, 'restart: ...then it is remembered');
  ok(!leaks(onDisk(st.run.id)).length, 'restart: still no value or PIN on disk');
  await r2.stopRun(st.run.id);
  r2.dispose();
}

// ─── Finish and discard forget ───
for (const how of ['merge', 'discard']) {
  const dir = repo('p5' + how);
  const asks = { manager: [], gate: [] };
  const t = team({ asks, finishWith: true });
  const r = runnerFor(t, dir, solo({ secrets: [S1] }));
  const st = await r.startRun({ project: 'p5' + how, factoryId: 'x', task: 'Say hello', secretPins: { [S1]: PIN1 } });
  ok(r.unlockedSecrets(st.run.id).length === 1, `${how}: unlocked while the run goes`);
  await until(() => t.of('manager')[0]?.cards?.[0]);
  const res = await r.finishAction(st.run.id, how);
  ok(res.ok && r.getRun(st.run.id).status === 'finished' && !r.unlockedSecrets(st.run.id).length, `${how}: the PINs are forgotten when the run finishes`);
  r.dispose();
}

rmSync(base, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nAll factory secrets tests passed.');
process.exit(0);
