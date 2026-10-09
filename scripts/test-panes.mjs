#!/usr/bin/env node
/**
 * test-panes.mjs — the parking rules in src/panes.js, against fake managers
 * and a fake clock.
 *
 * What must hold:
 *   - nothing parks while the timeout is 0
 *   - a pane parks only when it has been idle for the timeout AND nobody has
 *     been at Crundi for the timeout
 *   - working / asking / waiting, an undismissed running background task, or a
 *     message still being handed over all count as busy and restart the clock
 *   - a dismissed background task does not keep a pane open
 *   - shells, scheduler runs, background chats and collaborator chats never
 *     park for idleness
 *   - the timeout is never shorter than 10 minutes
 *   - live panes are written down, and come back parked after a "restart"
 *   - resume asks the manager for the same id and order, and the right session
 *
 * Run: node scripts/test-panes.mjs
 */

import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createPanes } from '../src/panes.js';

let failed = 0;
const ok = (cond, label) => { console.log((cond ? 'PASS  ' : 'FAIL  ') + label); if (!cond) failed++; };

// ─── fake clock ───
let now = 1_000_000_000_000;
const realNow = Date.now;
Date.now = () => now;
const MIN = 60_000;

// ─── fake managers ───
function fakeTerminals() {
  const t = new Map();
  return {
    t,
    add(id, meta) { t.set(id, { id, alias: 'proj', title: 'Terminal', order: 0, sessionId: '', running: true, createdAt: now, shellOnly: false, skipPermissions: false, model: '', effort: '', restorable: true, ...meta }); },
    list() { return [...t.values()].map(x => ({ id: x.id, project: x.alias })); },
    meta(id) { return t.get(id) || null; },
    getScrollback(id) { return t.has(id) ? 'screen of ' + id : ''; },
    close(id) { t.delete(id); return { ok: true }; },
    created: [],
    async create(alias, opts) { this.created.push({ alias, ...opts }); this.add(opts.id, { title: opts.title, order: opts.order }); return { ok: true, id: opts.id }; },
  };
}
function fakeUi() {
  const s = new Map();
  return {
    s,
    add(id, meta) { s.set(id, { id, alias: 'proj', title: 'Chat', order: 1, sessionId: '', model: '', effort: '', permissionMode: 'default', skipPermissions: false, background: false, collaborator: false, cwd: '/p', running: true, state: 'idle', busyTasks: 0, pendingInjections: 0, ...meta }); },
    list() { return [...s.values()].map(x => ({ id: x.id, project: x.alias })); },
    meta(id) { return s.get(id) || null; },
    close(id) { s.delete(id); return { ok: true }; },
    created: [],
    async create(alias, opts) { this.created.push({ alias, ...opts }); this.add(opts.id, { title: opts.title, order: opts.order }); return { ok: true, id: opts.id }; },
  };
}

const dir = mkdtempSync(join(tmpdir(), 'panes-test-'));
let timeout = 0, lastPresent = now, present = false;
const termStates = new Map();
function make(terms, ui) {
  return createPanes({
    dataDir: dir, claudeTerminals: terms, claudeUi: ui,
    readTranscriptHistory: (cwd, uuid) => ({ messages: [{ id: 'm1', kind: 'user', text: 'from ' + uuid }] }),
    getProject: () => ({ path: '/p' }),
    timeoutMinutes: () => timeout,
    awayFor: (ms) => { if (present) { lastPresent = now; return false; } return now - lastPresent >= ms; },
    terminalState: (id) => termStates.get(id) || 'idle',
    onChange: () => {},
  });
}

try {
  const T = fakeTerminals(), U = fakeUi();
  T.add('a000000000000001', { title: 'Claude term' });
  T.add('a000000000000002', { title: 'Shell', shellOnly: true });
  T.add('a000000000000003', { title: 'Scheduled run', restorable: false });
  U.add('b000000000000001', { title: 'Idle chat', sessionId: '11111111-1111-1111-1111-111111111111' });
  U.add('b000000000000002', { title: 'Working chat', state: 'working' });
  U.add('b000000000000003', { title: 'Task running', busyTasks: 1 });
  U.add('b000000000000004', { title: 'Background job', background: true });
  U.add('b000000000000005', { title: 'Collaborator chat', collaborator: true });
  let P = make(T, U);
  P.start();

  // 1) timeout 0: nothing ever parks
  now += 600 * MIN; P.tick();
  ok(P.listParked().length === 0, 'timeout 0 parks nothing, however long everything idles');

  // 2) the minimum: a 1-minute setting still waits 10 minutes
  timeout = 1; lastPresent = now; P.tick();
  now += 5 * MIN; P.tick();
  ok(P.listParked().length === 0, 'a timeout below 10 minutes is treated as 10 (nothing parks at 5 min)');

  // 3) you are here: nothing parks even when idle long enough
  timeout = 30; present = true; now += 60 * MIN; P.tick();
  ok(P.listParked().length === 0, 'nothing parks while someone is at Crundi');

  // 4) away, but not for the timeout yet
  present = false; now += 20 * MIN; P.tick();
  ok(P.listParked().length === 0, 'away for less than the timeout: nothing parks');

  // 5) away for the timeout: only the idle, eligible panes park
  now += 11 * MIN; P.tick();
  const parked = P.listParked().map(p => p.title).sort();
  ok(JSON.stringify(parked) === JSON.stringify(['Claude term', 'Idle chat']), 'away + idle for the timeout parks exactly the idle chat and Claude terminal: ' + JSON.stringify(parked));
  ok(!T.t.has('a000000000000001') && !U.s.has('b000000000000001'), 'parked panes had their processes closed');
  ok(T.t.has('a000000000000002'), 'a shell is never parked for idleness');
  ok(T.t.has('a000000000000003'), 'a scheduler run is never parked');
  ok(U.s.has('b000000000000002'), 'a working chat stays open');
  ok(U.s.has('b000000000000003'), 'a chat with a running, undismissed background task stays open');
  ok(U.s.has('b000000000000004') && U.s.has('b000000000000005'), 'background and collaborator chats stay open');

  // 6) the busy chat finishes: its clock starts THEN, not from when it was opened
  U.s.get('b000000000000002').state = 'idle';
  now += 1 * MIN; P.tick();
  now += 20 * MIN; P.tick();
  ok(U.s.has('b000000000000002'), 'a chat that just finished is not parked before it has been idle for the timeout');
  now += 11 * MIN; P.tick();
  ok(!U.s.has('b000000000000002'), 'and is parked once it has');

  // 7) dismissing the background task frees that chat
  U.s.get('b000000000000003').busyTasks = 0;   // dismissed (meta only counts undismissed running ones)
  P.tick(); now += 31 * MIN; P.tick();
  ok(!U.s.has('b000000000000003'), 'a chat whose only task was dismissed parks like any idle one');

  // 8) preview
  const pv = P.preview('b000000000000001');
  ok(pv.ok && pv.kind === 'ui' && pv.messages[0].text.includes('11111111'), 'a parked chat previews its own transcript (by session id)');
  const pt = P.preview('a000000000000001');
  ok(pt.ok && pt.kind === 'terminal' && pt.screen === 'screen of a000000000000001', 'a parked terminal previews its last screen');

  // 9) resume: same id, same order, the right session
  const r = await P.resume('b000000000000001');
  const c = U.created.at(-1);
  ok(r.ok && r.id === 'b000000000000001' && c.id === 'b000000000000001' && c.order === 1, 'resume asks for the same id and order');
  ok(c.sessionMode === 'resume' && c.resumeId === '11111111-1111-1111-1111-111111111111', 'resume reattaches to the same conversation');
  ok(!P.listParked().some(p => p.id === 'b000000000000001'), 'a resumed pane is no longer parked');
  await P.resume('a000000000000001');
  const tc = T.created.at(-1);
  ok(tc.id === 'a000000000000001' && tc.sessionMode === 'new', 'a terminal with no known conversation resumes as a new session');

  // 10) restart: live panes are written down, and come back parked
  P.snapshot();
  const file = JSON.parse(readFileSync(join(dir, 'panes.json'), 'utf-8'));
  ok(file.panes['a000000000000002'] && file.panes['a000000000000002'].status === 'live', 'live panes (including shells) are on disk before any shutdown');
  P.stop();
  const P2 = make(fakeTerminals(), fakeUi());   // a fresh process: nothing running
  P2.start();
  const back = P2.listParked();
  ok(back.some(p => p.id === 'a000000000000002' && p.reason === 'restart' && p.shellOnly), 'after a restart, an open shell comes back parked');
  ok(back.some(p => p.id === 'b000000000000001' && p.reason === 'restart'), 'after a restart, a resumed chat comes back parked');
  ok(!back.some(p => p.id === 'a000000000000003'), 'a scheduler run does not come back');
  P2.forget('a000000000000002');
  ok(!P2.listParked().some(p => p.id === 'a000000000000002'), 'closing a parked pane forgets it');
  P2.stop();

  // 11) "nobody here" is asked per project: being busy in one project does
  //     not keep another project's idle chat awake.
  {
    const seen = new Map();   // project -> when someone was last at it
    const asked = [];
    const U3 = fakeUi();
    U3.add('c000000000000001', { alias: 'here', title: 'Where I am' });
    U3.add('c000000000000002', { alias: 'elsewhere', title: 'Not visited' });
    U3.add('c000000000000003', { alias: 'elsewhere', title: 'Not visited, working', state: 'working' });
    const P3 = createPanes({
      dataDir: mkdtempSync(join(tmpdir(), 'panes-test-')), claudeTerminals: fakeTerminals(), claudeUi: U3,
      readTranscriptHistory: () => ({ messages: [] }), getProject: () => ({ path: '/p' }),
      timeoutMinutes: () => 30,
      awayFor: (ms, project) => { asked.push(project); return now - (seen.get(project) || 0) >= ms; },
      terminalState: () => 'idle', onChange: () => {},
    });
    seen.set('here', now); seen.set('elsewhere', now);
    P3.tick();
    // Half an hour and more in "here", touching it throughout; "elsewhere" never visited.
    for (let i = 0; i < 4; i++) { now += 10 * MIN; seen.set('here', now); P3.tick(); }
    const parked3 = P3.listParked().map(p => p.title);
    ok(JSON.stringify(parked3) === JSON.stringify(['Not visited']), 'a chat in a project you have not been at parks, though you are at Crundi: ' + JSON.stringify(parked3));
    ok(U3.s.has('c000000000000001'), 'the chat in the project you are at stays');
    ok(U3.s.has('c000000000000003'), 'a working chat stays, visited or not');
    ok(asked.includes('here') && asked.includes('elsewhere'), 'the away question names the project');
    // Then you leave "here" as well.
    now += 31 * MIN; P3.tick();
    ok(!U3.s.has('c000000000000001'), 'and it parks too once its own project has gone unvisited for the timeout');
    P3.stop();
  }
} finally {
  Date.now = realNow;
  rmSync(dir, { recursive: true, force: true });
}

if (failed) { console.log(`\n${failed} pane check(s) failed`); process.exit(1); }
console.log('\nAll pane checks passed.');
