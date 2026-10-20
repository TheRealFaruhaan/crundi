/**
 * factory-runner.js — runs factories: worktrees, sessions, handoffs.
 *
 * The engine (factory-engine.js) decides where control goes; this module
 * makes it happen. It owns:
 *
 *   - the run worktree and the branch worktrees (factory-git.js)
 *   - one chat per person: the Manager is an ordinary visible chat, employees
 *     are background chats kept out of the workbench (claude-ui.js, through
 *     the `agents` adapter so tests can stand in a fake team)
 *   - the factory_handoff and factory_permission tools
 *   - stalls (nudge once, then escalate), caps, pause and the usage limit
 *   - recovery after a crash, by hand or automatically
 *   - the reachability collector
 *   - one run per project
 *
 * ─── Durable by journal ───
 *
 * Everything that changes a run is an event, appended to
 * <dataDir>/factory-runs/<runId>/events.jsonl and flushed to disk BEFORE it
 * takes effect (commit()). The run is the fold of its events
 * (factory-journal.js); run.json is a cache of it. At boot a run is read from
 * the cache if it is whole and up to date, else rebuilt by replaying the
 * journal; a half-written last line from a crash is ignored.
 *
 * Effects are recorded as pending when the event that caused them is
 * written, and ticked off by an `effect-done` event carrying their result.
 * Each one is safe to repeat (a branch worktree is found again by its id, a
 * merge already made is a no-op), so after a crash the run finishes exactly
 * what was left.
 *
 * Not journaled, because it dies with the process and is rebuilt on recovery:
 * which chat belongs to whom, who is busy, queued steps, permission requests
 * in flight (`live`).
 *
 * Work for one run is serialised through a promise chain, so two employees in
 * parallel branches handing off at the same moment are routed one after the
 * other and both arrive.
 *
 * Spec: specs/factory.md §4.
 */

import { mkdirSync, writeFileSync, readFileSync, existsSync, readdirSync, renameSync, openSync, writeSync, fsyncSync, closeSync } from 'fs';
import { join } from 'path';
import { spawn } from 'child_process';
import { randomBytes } from 'crypto';
import { fileURLToPath } from 'url';
import * as Engine from './factory-engine.js';
import * as Git from './factory-git.js';
import { validateFactory, normalizeFactory, MANAGER_ID, flowGraph, reachable } from './factory-model.js';
import { reduce, replay, asksOf, timeLeft, isHeld, UNFINISHED as UNFINISHED_SET, JOURNAL_VERSION } from './factory-journal.js';
import { systemPromptFor, stepMessage, nudgeMessage, cancelMessage, recoveryOverview } from './factory-prompts.js';
import { hardDeny, ruleMatches, checkRule, batchMessage, denyMessage, DENY_RULES, ALLOW_RULES } from './factory-permissions.js';
import { missingSkills } from './factory-skills.js';

/** Runs in these states hold their project: no second run can start. */
export const UNFINISHED = UNFINISHED_SET;

const SETTLE_MS = 20_000;
const SETUP_TIMEOUT_MS = 15 * 60 * 1000;
const SETUP_OUTPUT_MAX = 4000;
const PERMISSION_BATCH_MS = 1500;   // parallel branches asking at once arrive as one message
const AUTO_RECOVER_DELAYS = [30_000, 2 * 60_000, 10 * 60_000];

/**
 * @param {object} o
 * @param {string} o.dataDir
 * @param {object} o.agents         see claudeUiAgents() for the shape
 * @param {(alias:string) => {path:string}|null} o.getProject
 * @param {(id:string) => {factory, validation}|null} o.getFactory
 * @param {(project:string) => Array<{name:string, description:string}>} [o.listSkills]  skills a project can see
 * @param {object} [o.git]          factory-git.js (tests may wrap it)
 * @param {(kind:string, run:object, text:string) => void} [o.notify]
 * @param {(run:object) => void} [o.onChange]
 */
export function createFactoryRunner({
  dataDir, agents, getProject, getFactory, git = Git, listSkills = () => [],
  notify = () => {}, onChange = () => {},
  settleMs = SETTLE_MS, setupTimeoutMs = SETUP_TIMEOUT_MS, permissionBatchMs = PERMISSION_BATCH_MS,
  autoRecoverDelays = AUTO_RECOVER_DELAYS,
  now = () => new Date().toISOString(),
}) {
  const RUNS = join(dataDir, 'factory-runs');
  /** runId -> entry */
  const runs = new Map();
  /** chatId -> { runId, employeeId } */
  const chats = new Map();
  /** Projects claimed by a run that is still being set up (before its first event). */
  const claiming = new Set();

  if (typeof agents.onPermission === 'function') agents.onPermission((req) => onPermissionRequest(req));

  // ─── Journal and cache ───

  const runDir = (id) => join(RUNS, id);
  const journalFile = (id) => join(runDir(id), 'events.jsonl');
  const cacheFile = (id) => join(runDir(id), 'run.json');

  function newEntry(rec) {
    return {
      rec,
      live: { employees: {}, permissions: [], managerIdle: false, error: '' },
      lock: Promise.resolve(), timers: {}, watchers: {}, draining: false, skills: null, autoTries: 0,
    };
  }

  const st = (entry) => entry.rec.state;
  const rr = (entry) => entry.rec.run;

  /** Append an event (flushed to disk), fold it in, refresh the cache. */
  function commit(entry, ev) {
    const e = { ...ev, at: ev.at || now() };
    const id = entry.rec?.state?.id || ev.id;
    mkdirSync(runDir(id), { recursive: true });
    const fd = openSync(journalFile(id), 'a');
    try { writeSync(fd, JSON.stringify(e) + '\n'); fsyncSync(fd); } finally { closeSync(fd); }
    entry.rec = reduce(entry.rec, e);
    writeCache(entry);
    return entry.rec;
  }

  function writeCache(entry) {
    const id = st(entry).id;
    const tmp = cacheFile(id) + '.tmp';
    writeFileSync(tmp, JSON.stringify({ rec: entry.rec, error: entry.live.error || '' }));
    renameSync(tmp, cacheFile(id));
    try { onChange(summary(entry)); } catch { /* a listener's problem */ }
  }

  /** The journal's events; a damaged line ends it (a crash mid-append). */
  function readJournal(id) {
    const f = journalFile(id);
    if (!existsSync(f)) return [];
    const out = [];
    for (const line of readFileSync(f, 'utf-8').split('\n')) {
      if (!line.trim()) continue;
      try { out.push(JSON.parse(line)); } catch { break; }
    }
    return out;
  }

  /** At boot: every run, from its cache if whole and current, else by replay. */
  function load() {
    if (!existsSync(RUNS)) return;
    for (const name of readdirSync(RUNS)) {
      const events = readJournal(name);
      if (!events.length) continue;
      let rec = null;
      try {
        const cached = JSON.parse(readFileSync(cacheFile(name), 'utf-8'));
        if (cached?.rec?.v === JOURNAL_VERSION && cached.rec.events === events.length) rec = cached.rec;
      } catch { rec = null; }
      let rebuilt = false;
      if (!rec) { rec = replay(events); rebuilt = true; }
      if (!rec?.state?.id) continue;
      const entry = newEntry(rec);
      entry.rebuilt = rebuilt;
      runs.set(rec.state.id, entry);
      if (rebuilt) writeCache(entry);
      if (UNFINISHED.has(rec.state.status) && rec.state.status !== 'crashed') {
        commit(entry, { type: 'crash', reason: 'Crundi restarted, and every chat of the run ended with it', diedAt: events[events.length - 1].at });
      }
      if (st(entry).status === 'crashed') scheduleAutoRecover(entry);
    }
  }

  /** Serialise everything that touches one run. */
  function locked(entry, fn) {
    const p = entry.lock.then(fn, fn);
    entry.lock = p.catch(() => {});
    return p;
  }

  // ─── Reads ───

  function summary(entry) {
    const s = st(entry), r = rr(entry), L = entry.live;
    const nodes = new Map(s.snapshot.nodes.map(n => [n.id, n]));
    const roster = new Map(s.snapshot.roster.map(x => [x.id, x]));
    const asks = asksOf(entry.rec);
    return {
      id: s.id,
      project: s.project,
      factoryId: r.factoryId,
      factoryName: s.snapshot.name,
      factoryVersion: r.factoryVersion,
      task: s.task,
      status: s.status,
      waitingOn: r.paused ? (r.paused.by === 'usage' ? 'usage' : 'paused') : r.capHit ? 'cap' : asks.length ? 'ask' : L.managerIdle ? 'manager' : '',
      paused: r.paused || null,
      capHit: r.capHit || null,
      crash: s.status === 'crashed' ? r.crash : null,
      createdAt: s.createdAt,
      updatedAt: s.updatedAt,
      handoffs: s.handoffs,
      worktree: r.worktree,
      branch: r.branch,
      parentBranch: r.parentBranch,
      dirtyAtStart: !!r.dirty,
      managerChatId: L.employees[MANAGER_ID]?.chatId || '',
      tokens: s.tokens.map(t => {
        const n = nodes.get(t.nodeId);
        const who = t.status === 'answer' ? t.answerer : Engine.rosterOf(n);
        return { id: t.id, nodeId: t.nodeId, label: n?.label || n?.type || '', status: t.status, employeeId: who || '', employee: roster.get(who)?.name || '', ws: t.ws };
      }),
      employees: Object.fromEntries(s.snapshot.roster.map(x => {
        const e = L.employees[x.id] || {};
        return [x.id, { chatId: e.chatId || '', busy: e.busy || null, queued: (e.queue || []).length, retired: !!r.retired[x.id], name: x.name }];
      })),
      liveSessions: Object.values(L.employees).filter(e => e.chatId).length,
      asks,
      managerMode: r.managerMode,
      permissions: L.permissions.map(p => ({ id: p.id, employee: p.employee, toolName: p.toolName, input: p.input, at: p.at, sent: p.sent })),
      rules: r.rules,
      outcome: s.outcome,
      setup: r.setup,
      missingSkills: r.missingSkills,
      timeLeftMs: Math.max(0, timeLeft(entry.rec, Date.now())),
      autoRecover: !!s.snapshot.settings.autoRecover,
      error: L.error || '',
    };
  }

  function listRuns({ project } = {}) {
    const key = project ? String(project).toLowerCase() : '';
    return [...runs.values()].filter(e => !key || st(e).project === key).map(summary)
      .sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt)));
  }

  function getRun(id) {
    const e = runs.get(id);
    return e ? { ...summary(e), history: st(e).history.slice(-200), snapshot: st(e).snapshot, permissionLog: rr(e).perms.log.slice(-200) } : null;
  }

  function activeRunFor(project) {
    const key = String(project || '').toLowerCase();
    for (const e of runs.values()) if (st(e).project === key && UNFINISHED.has(st(e).status)) return summary(e);
    return null;
  }

  /** Which run and person a chat belongs to, if any. */
  function chatInfo(chatId) { return chats.get(String(chatId || '')) || null; }

  /** name -> description of the skills a project can see (cached per run). */
  function skillsOf(entry) {
    if (!entry.skills) {
      let list = [];
      try { list = listSkills(st(entry).project) || []; } catch { list = []; }
      entry.skills = new Map(list.map(x => [x.name, x.description || '']));
    }
    return entry.skills;
  }

  /** Commit options for a run: its linked skills are never committed. */
  const excl = (entry) => ({ exclude: rr(entry).skillExcludes || [] });

  // ─── Start ───

  async function startRun({ project, factoryId, task, mergeMode, ignoreMissingSkills = false } = {}) {
    const key = String(project || '').toLowerCase();
    const proj = getProject(key);
    if (!proj) return { ok: false, error: `Project "${project}" not found.` };
    const existing = activeRunFor(key);
    if (existing || claiming.has(key)) {
      return { ok: false, busy: true, runId: existing?.id || '', error: existing ? `A factory is already ${existing.status === 'crashed' ? 'waiting to be recovered' : 'running'} in this project ("${existing.factoryName}"). Finish or stop it first.` : 'A factory is being started in this project.' };
    }
    const taskText = String(task || '').trim();
    if (!taskText) return { ok: false, error: 'Give the factory a task.' };
    const got = getFactory(factoryId);
    if (!got) return { ok: false, error: 'Factory not found.' };
    const v = validateFactory(got.factory);
    if (!v.ok) return { ok: false, invalid: true, errors: v.errors, error: `This factory cannot run yet: ${v.errors.length} problem${v.errors.length === 1 ? '' : 's'} to fix in the editor.` };
    let known = [];
    try { known = listSkills(key) || []; } catch { known = []; }
    const factory = normalizeFactory(got.factory);
    const missing = missingSkills(factory, taskText, new Set(known.map(x => x.name)));
    if (missing.length && !ignoreMissingSkills) {
      return { ok: false, missingSkills: missing, error: `This factory names skill${missing.length === 1 ? '' : 's'} this project does not have: ${missing.map(n => '/' + n).join(', ')}. Install ${missing.length === 1 ? 'it' : 'them'}, or start anyway.` };
    }

    claiming.add(key);
    try {
      const id = 'run_' + Date.now().toString(36) + randomBytes(3).toString('hex');
      const wt = await git.createRunWorktree({ repo: proj.path, project: key, runId: id, name: factory.name });
      if (!wt.ok) return { ok: false, error: wt.error };
      const skillExcludes = git.linkProjectSkills ? git.linkProjectSkills(wt.repo, wt.path) : [];
      mkdirSync(join(runDir(id), 'shared'), { recursive: true });
      mkdirSync(join(runDir(id), 'notes'), { recursive: true });

      const entry = newEntry(null);
      commit(entry, {
        type: 'start', id, project: key, factory, task: taskText, factoryId, factoryVersion: got.factory.version,
        mergeMode: mergeMode === 'auto' || mergeMode === 'ask' ? mergeMode : '',
        wt: { repo: wt.repo, root: wt.root, path: wt.path, branch: wt.branch, parentBranch: wt.parentBranch, baseCommit: wt.baseCommit, dirty: wt.dirty },
        skillExcludes, missingSkills: missing,
      });
      runs.set(id, entry);
      claiming.delete(key);

      const setupCmd = factory.settings.setupCommand.trim();
      if (setupCmd) {
        const res = await runSetup(setupCmd, wt.path);
        commit(entry, { type: 'setup', status: res.ok ? 'ok' : 'failed', code: res.code, command: setupCmd, output: res.output });
      }

      // The Manager's chat exists from the start: it is where the person watches.
      const mgr = await ensureSession(entry, MANAGER_ID);
      if (!mgr.ok) {
        entry.live.error = `Could not start the Manager: ${mgr.error}`;
        commit(entry, { type: 'stop' });
        return { ok: false, error: entry.live.error };
      }
      await locked(entry, async () => {
        commit(entry, { type: 'begin' });
        startClock(entry);
        await drain(entry);
      });
      notify('started', summary(entry), `Factory "${factory.name}" started in ${key}.`);
      return { ok: true, run: summary(entry) };
    } finally {
      claiming.delete(key);
    }
  }

  function runSetup(command, cwd) {
    return new Promise((done) => {
      let out = '';
      let child;
      try { child = spawn('bash', ['-lc', command], { cwd, env: process.env }); }
      catch (err) { return done({ ok: false, code: -1, output: err.message }); }
      const keep = (b) => { out = (out + b.toString()).slice(-SETUP_OUTPUT_MAX); };
      child.stdout.on('data', keep);
      child.stderr.on('data', keep);
      const t = setTimeout(() => { try { child.kill('SIGKILL'); } catch { /* gone */ } }, setupTimeoutMs);
      child.on('error', (err) => { clearTimeout(t); done({ ok: false, code: -1, output: err.message }); });
      child.on('close', (code) => { clearTimeout(t); done({ ok: code === 0, code, output: out.trim() }); });
    });
  }

  // ─── Sessions ───

  function paths(entry, employeeId) {
    const dir = runDir(st(entry).id);
    return {
      worktree: rr(entry).worktree,
      branch: rr(entry).branch,
      parentBranch: rr(entry).parentBranch,
      notesFile: join(dir, 'notes', `${employeeId}.md`),
      sharedDir: join(dir, 'shared'),
    };
  }

  function empOf(entry, employeeId) {
    const L = entry.live.employees;
    if (!L[employeeId]) L[employeeId] = { chatId: '', busy: null, queue: [], nudged: false };
    return L[employeeId];
  }

  async function ensureSession(entry, employeeId, { fresh = false, reuseId = '' } = {}) {
    const emp = empOf(entry, employeeId);
    if (emp.chatId && !fresh && !reuseId && agents.isAlive(emp.chatId)) return { ok: true, chatId: emp.chatId };
    if (emp.chatId) { unwatch(entry, emp.chatId); try { await agents.close(emp.chatId); } catch { /* gone */ } chats.delete(emp.chatId); emp.chatId = ''; }
    const f = st(entry).snapshot;
    const r = f.roster.find(x => x.id === employeeId);
    if (!r) return { ok: false, error: `Nobody called "${employeeId}" is on the roster.` };
    const isManager = employeeId === MANAGER_ID;
    const known = rr(entry).sessions[employeeId] || {};
    const resume = !fresh && r.memory === 'keep' && known.sessionUuid ? known.sessionUuid : '';
    const managerMode = rr(entry).managerMode;
    const res = await agents.spawn({
      project: st(entry).project,
      runId: st(entry).id,
      role: isManager ? 'manager' : 'employee',
      employeeId,
      title: isManager ? `Factory: ${f.name}` : `${r.name} · ${f.name}`,
      cwd: rr(entry).worktree,
      addDirs: [rr(entry).root, runDir(st(entry).id)].filter(Boolean),
      systemPromptExtra: systemPromptFor({ factory: f, employee: r, project: st(entry).project, paths: paths(entry, employeeId), managerMode, skills: skillsOf(entry) }),
      model: r.model, effort: r.effort,
      allowedTools: [...r.tools.allow, ...ALLOW_RULES],
      // The fixed deny list rides on every chat: it is all that binds a
      // Manager in bypass mode, which never asks.
      disallowedTools: [...r.tools.deny, ...DENY_RULES],
      // Employees: edits inside the run's folders are free (acceptEdits with
      // --add-dir), anything else asks, and asking goes to the Manager.
      // The Manager: bypass if the person chose it, else its own actions ask
      // the person in its chat as usual.
      skipPermissions: isManager && managerMode === 'bypass',
      permissionMode: isManager ? '' : 'acceptEdits',
      background: !isManager,
      resumeId: resume,
      // The Manager keeps its chat id across relaunches and recoveries.
      id: reuseId || (isManager ? known.chatId || '' : ''),
    });
    if (!res.ok) return { ok: false, error: res.error || 'could not start' };
    emp.chatId = res.id;
    chats.set(res.id, { runId: st(entry).id, employeeId });
    watch(entry, res.id, employeeId);
    commit(entry, { type: 'session', employeeId, chatId: res.id, sessionUuid: resume || known.sessionUuid || '' });
    return { ok: true, chatId: res.id };
  }

  /**
   * Watch a person's chat for the end of a turn. Idle has to hold for
   * settleMs (the CLI dips through idle between turns). Then: an employee who
   * still owes a handoff is nudged once and escalated the second time; the
   * Manager is simply talking to the person.
   */
  function watch(entry, chatId, employeeId) {
    let timer = null;
    const fn = (ev) => {
      if (!ev) return;
      if (ev.type === 'exit') { clearTimeout(timer); return onExit(entry, chatId, employeeId); }
      if (ev.type !== 'state') return;
      if (ev.state !== 'idle') {
        clearTimeout(timer); timer = null;
        if (employeeId === MANAGER_ID && entry.live.managerIdle) { entry.live.managerIdle = false; writeCache(entry); }
        return;
      }
      if (timer) return;
      timer = setTimeout(() => { timer = null; onSettled(entry, chatId, employeeId); }, settleMs);
    };
    entry.watchers[chatId] = { fn, clear: () => clearTimeout(timer) };
    agents.on(chatId, fn);
  }

  function unwatch(entry, chatId) {
    const w = entry.watchers[chatId];
    if (!w) return;
    w.clear();
    try { agents.off(chatId, w.fn); } catch { /* gone */ }
    delete entry.watchers[chatId];
  }

  function onSettled(entry, chatId, employeeId) {
    return locked(entry, async () => {
      const emp = entry.live.employees[employeeId];
      if (!emp || emp.chatId !== chatId || !emp.busy || !UNFINISHED.has(st(entry).status)) return;
      // Paused (by the person, or the usage limit cutting a turn short): a
      // stopped turn is expected. resume() picks them up.
      if (rr(entry).paused || st(entry).status === 'crashed') return;
      if (employeeId === MANAGER_ID) {
        entry.live.managerIdle = true;
        writeCache(entry);
        return;
      }
      if (!emp.nudged) {
        emp.nudged = true;
        agents.send(chatId, nudgeMessage());
        return;
      }
      const name = st(entry).snapshot.roster.find(r => r.id === employeeId)?.name || employeeId;
      await escalateToken(entry, emp.busy, `${name} stopped twice without handing off.`, employeeId);
    });
  }

  function onExit(entry, chatId, employeeId) {
    return locked(entry, async () => {
      const emp = entry.live.employees[employeeId];
      unwatch(entry, chatId);
      chats.delete(chatId);
      dropPermissions(entry, (p) => p.chatId === chatId, '');
      if (!emp || emp.chatId !== chatId) return;
      emp.chatId = '';
      if (!UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') return;
      if (employeeId === MANAGER_ID) {
        entry.live.error = 'The Manager\'s chat ended.';
        commit(entry, { type: 'crash', reason: 'the Manager\'s chat ended' });
        notify('crashed', summary(entry), `Factory "${st(entry).snapshot.name}" stopped: the Manager's chat ended.`);
        scheduleAutoRecover(entry);
        return;
      }
      if (emp.busy) {
        const name = st(entry).snapshot.roster.find(r => r.id === employeeId)?.name || employeeId;
        await escalateToken(entry, emp.busy, `${name}'s session ended unexpectedly in the middle of a step.`, employeeId);
      }
    });
  }

  // ─── Effects ───

  /**
   * Carry out pending effects, oldest first, each ticked off with an
   * effect-done event that carries its result. Activations wait while the run
   * is held. Events committed while draining (an escalation after a merge
   * conflict) add effects that the same loop picks up next.
   */
  async function drain(entry) {
    if (entry.draining) return;
    entry.draining = true;
    try {
      for (;;) {
        if (!UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') break;
        const held = isHeld(entry.rec);
        const ef = rr(entry).pending.find(e => !(held && e.type === 'activate'));
        if (!ef) break;
        await runOne(entry, ef);
      }
    } finally {
      entry.draining = false;
    }
    collect(entry);
  }

  async function runOne(entry, ef) {
    let result;
    let after = null;
    try {
      switch (ef.type) {
        case 'activate': await activate(entry, ef); break;
        case 'workspace': result = await makeWorkspace(entry, ef); break;
        case 'merge': {
          result = await mergeWorkspaces(entry, ef);
          if (result.conflicts.length && !ef.abort) after = () => escalateToken(entry, ef.contTokenId, result.reason);
          break;
        }
        case 'cancel': await cancelTokens(entry, ef); break;
        case 'ask': notify('ask', summary(entry), `Factory "${st(entry).snapshot.name}" asks: ${ef.prompt}`); break;
        case 'finish': await onFinish(entry, ef); break;
        default: break;
      }
    } catch (err) {
      entry.live.error = `${ef.type}: ${err.message}`;
    }
    commit(entry, { type: 'effect-done', effectId: ef.id, ...(result ? { result } : {}) });
    if (after) await after();
  }

  async function activate(entry, ef) {
    // The token may have moved on since this was queued (a cancelled branch).
    const t = st(entry).tokens.find(x => x.id === ef.tokenId);
    if (!t || (t.status !== 'work' && t.status !== 'answer')) return;
    const emp = empOf(entry, ef.employeeId);
    if (emp.busy && emp.busy !== ef.tokenId) { emp.queue.push(ef); return; }
    await startStep(entry, emp, ef);
  }

  async function startStep(entry, emp, ef, { recovery = null } = {}) {
    const r = st(entry).snapshot.roster.find(x => x.id === ef.employeeId);
    const fresh = !!(r && r.memory === 'fresh' && ef.employeeId !== MANAGER_ID && emp.chatId);
    const s = await ensureSession(entry, ef.employeeId, { fresh });
    if (!s.ok) {
      entry.live.error = s.error;
      if (ef.employeeId !== MANAGER_ID) await escalateToken(entry, ef.tokenId, `Could not start ${r?.name || ef.employeeId}: ${s.error}`);
      return;
    }
    emp.busy = ef.tokenId;
    emp.nudged = false;
    if (ef.employeeId === MANAGER_ID) entry.live.managerIdle = false;
    const notice = rr(entry).notices[ef.tokenId] || '';
    const msg = stepMessage({
      run: st(entry), effect: ef, req: Engine.requirements(st(entry), ef.tokenId),
      wsPath: wsPath(entry, ef.ws), notice, recovery, skills: skillsOf(entry),
    });
    const sent = agents.send(s.chatId, msg);
    if (sent && sent.ok === false) entry.live.error = `Could not message ${r?.name || ef.employeeId}: ${sent.error}`;
    else if (notice) commit(entry, { type: 'notice-clear', tokenId: ef.tokenId });
  }

  function wsPath(entry, wsId) {
    const ws = rr(entry).ws;
    return (ws[wsId] || ws.main || {}).path || rr(entry).worktree;
  }

  async function makeWorkspace(entry, ef) {
    const ws = rr(entry).ws;
    const parent = ws[ef.parentWs] || ws.main;
    const r = await git.createBranchWorktree({ repo: rr(entry).repo, parentPath: parent.path, root: rr(entry).root, name: ef.name, branchPrefix: rr(entry).branch, key: ef.wsId, ...excl(entry) });
    if (!r.ok) { entry.live.error = r.error; return { shared: true, error: r.error }; }
    if (git.linkProjectSkills) git.linkProjectSkills(rr(entry).repo, r.path);
    return { path: r.path, branch: r.branch };
  }

  async function mergeWorkspaces(entry, ef) {
    const ws = rr(entry).ws;
    const into = ws[ef.into] || ws.main;
    const merged = [];
    const conflicts = [];
    for (const w of ef.from) {
      const x = ws[w];
      if (!x || x.shared || !x.branch) { merged.push(w); continue; }
      const r = await git.mergeBranch({ parentPath: into.path, childPath: existsSync(x.path) ? x.path : '', childBranch: x.branch, ...excl(entry) });
      if (r.ok) {
        await git.removeWorktree({ repo: rr(entry).repo, path: x.path, branch: x.branch });
        merged.push(w);
      } else {
        conflicts.push({ ws: w, path: x.path, branch: x.branch, files: r.files || [], error: r.error });
      }
    }
    const reason = conflicts.length
      ? 'Merging the parallel branches back together failed:\n' + conflicts.map(c => `- branch ${c.branch} (folder ${c.path}): ${c.error}`).join('\n') +
        `\nThe branches are kept. Have someone merge them into ${into.path} (git merge <branch>) and resolve the conflicts, then carry on.`
      : '';
    return { merged, conflicts, reason };
  }

  async function cancelTokens(entry, ef) {
    const ids = new Set(ef.tokenIds);
    for (const [eid, emp] of Object.entries(entry.live.employees)) {
      emp.queue = (emp.queue || []).filter(q => !ids.has(q.tokenId));
      if (emp.busy && ids.has(emp.busy)) {
        emp.busy = null;
        dropPermissions(entry, (p) => p.chatId === emp.chatId, 'Your step was cancelled.');
        if (emp.chatId) {
          try { agents.interrupt(emp.chatId); } catch { /* not mid-turn */ }
          agents.send(emp.chatId, cancelMessage(ef.reason || 'the flow moved on'));
        }
        await dequeue(entry, eid);
      }
    }
  }

  async function onFinish(entry, ef) {
    dropPermissions(entry, () => true, 'The run has finished.');
    // The team is done; the Manager stays to present the result (card 5).
    for (const [eid, emp] of Object.entries(entry.live.employees)) {
      if (eid === MANAGER_ID) { emp.busy = null; continue; }
      if (emp.chatId) { unwatch(entry, emp.chatId); chats.delete(emp.chatId); try { await agents.close(emp.chatId); } catch { /* gone */ } emp.chatId = ''; }
      emp.busy = null; emp.queue = [];
    }
    clearClock(entry);
    notify('finished', summary(entry), `Factory "${st(entry).snapshot.name}" is done: ${ef.summary}`);
  }

  async function dequeue(entry, employeeId) {
    const emp = entry.live.employees[employeeId];
    if (!emp || emp.busy || !emp.queue.length) return;
    const next = emp.queue.shift();
    await activate(entry, next);
  }

  async function escalateToken(entry, tokenId, reason, fromEmployee) {
    const dry = Engine.escalate(st(entry), tokenId, reason);
    if (!dry.ok) { entry.live.error = dry.error; writeCache(entry); return; }
    if (fromEmployee) {
      const emp = entry.live.employees[fromEmployee];
      if (emp && emp.busy === tokenId) emp.busy = null;
    }
    commit(entry, { type: 'escalate', tokenId, reason });
    await drain(entry);
    if (fromEmployee) await dequeue(entry, fromEmployee);
  }

  // ─── Permissions: employees ask, the Manager decides ───

  function answer(item, allow, message) {
    let r;
    try { r = agents.respond(item.chatId, { requestId: item.requestId, behavior: allow ? 'allow' : 'deny', message }); }
    catch (err) { r = { ok: false, error: err.message }; }
    return r && r.ok !== false;
  }

  function decided(entry, item, decision, by, extra = {}) {
    commit(entry, { type: 'perm-decided', id: item.id, employee: item.employee, toolName: item.toolName, input: item.input, decision, by, ...extra });
  }

  /** Remove requests in flight; if a reason is given, the employees are told no. */
  function dropPermissions(entry, pred, reason) {
    const keep = [];
    for (const p of entry.live.permissions) {
      if (!pred(p)) { keep.push(p); continue; }
      if (reason) answer(p, false, denyMessage(reason, 'Crundi'));
      decided(entry, p, 'dropped', 'crundi', { reason });
    }
    entry.live.permissions = keep;
  }

  function clipInput(input) {
    const out = {};
    for (const [k, v] of Object.entries(input || {})) out[k] = typeof v === 'string' && v.length > 4000 ? v.slice(0, 4000) + '…' : v;
    return out;
  }

  /** From claude-ui: an employee wants to do something its mode does not allow. */
  function onPermissionRequest(req) {
    const info = chatInfo(req.sessionId);
    const entry = info && runs.get(info.runId);
    if (!entry) {
      try { agents.respond(req.sessionId, { requestId: req.requestId, behavior: 'deny', message: 'This chat is not part of a running factory.' }); } catch { /* gone */ }
      return;
    }
    return locked(entry, async () => {
      const roster = st(entry).snapshot.roster.find(r => r.id === info.employeeId);
      const item = {
        id: 'p' + (rr(entry).perms.seq + 1),
        chatId: req.sessionId, employeeId: info.employeeId, employee: roster?.name || info.employeeId,
        requestId: req.requestId, toolName: req.toolName, displayName: req.displayName,
        description: String(req.description || '').slice(0, 1000),
        input: clipInput(req.input), at: now(), sent: false,
      };
      commit(entry, { type: 'perm-asked', id: item.id, employeeId: item.employeeId, employee: item.employee, toolName: item.toolName, input: item.input });
      if (!UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') {
        answer(item, false, denyMessage(`the run is ${st(entry).status}`, 'Crundi'));
        decided(entry, item, 'denied', 'crundi', { reason: 'run not active' });
        return;
      }
      const why = hardDeny({ toolName: req.toolName, input: req.input }, { writableRoots: [rr(entry).root, runDir(st(entry).id)] });
      if (why) {
        answer(item, false, denyMessage(why, 'Crundi'));
        decided(entry, item, 'denied', 'deny-list', { reason: why });
        return;
      }
      const rule = rr(entry).rules.find(r => ruleMatches(r, { toolName: req.toolName, input: req.input }));
      if (rule) {
        answer(item, true);
        decided(entry, item, 'allowed', 'rule', { rule });
        return;
      }
      entry.live.permissions.push(item);
      writeCache(entry);
      scheduleDelivery(entry);
    });
  }

  /** Requests are batched briefly, then sent to the Manager in one message. */
  function scheduleDelivery(entry) {
    if (entry.timers.perm) return;
    entry.timers.perm = setTimeout(() => {
      entry.timers.perm = null;
      locked(entry, async () => {
        const items = entry.live.permissions.filter(p => !p.sent);
        if (!items.length) return;
        const m = await ensureSession(entry, MANAGER_ID);
        if (!m.ok) { dropPermissions(entry, (p) => !p.sent, `the Manager is unavailable (${m.error})`); return; }
        const sent = agents.send(m.chatId, batchMessage(items, { bypass: rr(entry).managerMode === 'bypass' }));
        if (sent && sent.ok === false) return scheduleDelivery(entry);
        for (const p of items) p.sent = true;
        writeCache(entry);
      });
    }, permissionBatchMs);
    entry.timers.perm.unref?.();
  }

  /**
   * factory_permission, from the Manager's chat.
   * @param {Array<{id:string, allow:boolean, reason?:string, rule?:string}>} decisions
   */
  async function answerPermissions(chatId, decisions) {
    const info = chatInfo(chatId);
    if (!info) return { ok: false, error: 'This chat is not part of a factory run.' };
    if (info.employeeId !== MANAGER_ID) return { ok: false, error: 'Only the Manager answers permission requests.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    if (!Array.isArray(decisions) || !decisions.length) return { ok: false, error: 'Give decisions: a list of { id, allow, reason }.' };
    return locked(entry, async () => {
      const results = [];
      const errors = [];
      const newRules = [];
      for (const d of decisions) {
        const id = String(d?.id || '');
        const item = entry.live.permissions.find(p => p.id === id);
        if (!item) { errors.push(`${id || '(no id)'}: no such request is waiting`); continue; }
        const allow = d.allow === true || d.allow === 'true';
        const reason = String(d.reason || '').slice(0, 1000);
        if (allow && d.rule) {
          const c = checkRule(d.rule);
          if (!c.ok) { errors.push(`${id}: ${c.error} (the request itself was not answered)`); continue; }
          if (!rr(entry).rules.includes(c.rule)) { commit(entry, { type: 'rule', rule: c.rule }); newRules.push(c.rule); }
        }
        const delivered = answer(item, allow, allow ? undefined : denyMessage(reason));
        entry.live.permissions = entry.live.permissions.filter(p => p !== item);
        decided(entry, item, allow ? 'allowed' : 'denied', 'manager', { reason, ...(allow && d.rule ? { rule: d.rule } : {}) });
        results.push(`${id}: ${allow ? 'allowed' : 'denied'}${delivered ? '' : ' (that request was no longer waiting)'}`);
      }
      // A rule just granted answers anything else waiting that it covers.
      if (newRules.length) {
        for (const item of [...entry.live.permissions]) {
          const rule = newRules.find(r => ruleMatches(r, item));
          if (!rule) continue;
          answer(item, true);
          entry.live.permissions = entry.live.permissions.filter(p => p !== item);
          decided(entry, item, 'allowed', 'rule', { rule });
          results.push(`${item.id}: allowed by your new rule ${rule}`);
        }
      }
      writeCache(entry);
      const left = entry.live.permissions.map(p => p.id);
      return {
        ok: errors.length === 0 || results.length > 0,
        message: [...results, ...errors.map(e => 'not done: ' + e)].join('\n') + (left.length ? `\nStill waiting: ${left.join(', ')}` : '\nNothing else is waiting.'),
        ...(errors.length && !results.length ? { error: errors.join('\n') } : {}),
      };
    });
  }

  /**
   * Switch the Manager between normal and bypass. Bypass can only be set when
   * a CLI starts, so turning it on relaunches the Manager on the same
   * conversation (--resume) under the same chat id; turning it off is live.
   */
  async function setManagerMode(runId, mode) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    if (mode !== 'bypass' && mode !== 'normal') return { ok: false, error: 'mode is "normal" or "bypass".' };
    return locked(entry, async () => {
      if (rr(entry).managerMode === mode) return { ok: true, run: summary(entry) };
      const emp = empOf(entry, MANAGER_ID);
      if (mode === 'normal') {
        if (emp.chatId && agents.isAlive(emp.chatId)) {
          const r = agents.setPermissionMode(emp.chatId, 'default');
          if (r && r.ok === false) return { ok: false, error: r.error };
          agents.send(emp.chatId, '[Factory] The person turned bypass permissions off. Your own actions now ask them first. You still decide your team\'s permission requests.');
        }
        commit(entry, { type: 'manager-mode', mode: 'normal' });
        return { ok: true, run: summary(entry) };
      }
      if (emp.chatId && agents.isAlive(emp.chatId) && agents.state(emp.chatId) !== 'idle') {
        return { ok: false, error: 'The Manager is in the middle of a turn. Switch to bypass when it has stopped.' };
      }
      const sameId = emp.chatId;
      if (sameId) commit(entry, { type: 'session', employeeId: MANAGER_ID, chatId: sameId, sessionUuid: agents.sessionUuid(sameId) || '' });
      commit(entry, { type: 'manager-mode', mode: 'bypass' });
      const s = await ensureSession(entry, MANAGER_ID, { reuseId: sameId || '' });
      if (!s.ok) { commit(entry, { type: 'manager-mode', mode: 'normal' }); return { ok: false, error: s.error }; }
      if (emp.busy) agents.send(s.chatId, '[Factory] You were relaunched with bypass permissions: the person trusts you. You still owe a handoff for the step above; carry on.');
      return { ok: true, run: summary(entry) };
    });
  }

  // ─── Pause, resume, caps ───

  /**
   * Pause: nobody new is started; employees finish the turn they are in and
   * their handoffs are routed, but what follows is held until resume().
   */
  async function pause(runId, { by = 'person', resetsAt = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (!['running', 'waiting'].includes(st(entry).status)) return { ok: false, error: `The run is ${st(entry).status}; it cannot be paused.` };
      commit(entry, { type: 'pause', by, ...(resetsAt ? { resetsAt } : {}) });
      clearClock(entry);
      if (by === 'usage') notify('paused', summary(entry), `Factory "${st(entry).snapshot.name}" paused: the usage limit is reached.${resetsAt ? ` It carries on by itself after ${resetsAt}.` : ''}`);
      return { ok: true, run: summary(entry) };
    });
  }

  /**
   * Carry on after a pause or a cap (a cap grants another maxHandoffs or
   * maxMinutes). Held steps start; anyone whose turn stopped while the run was
   * held (the usage limit cuts turns short) is told to continue.
   */
  async function resume(runId, { reason = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      const r = rr(entry);
      if (!r.capHit && !r.paused) return { ok: false, error: 'This run is not paused or held.' };
      const why = reason || (r.paused?.by === 'usage' ? 'the usage limit was reached; it has reset now' : r.paused ? 'the person paused the run; it is resumed now' : 'the run reached a limit; the person let it carry on');
      commit(entry, { type: 'resume' });
      startClock(entry);
      for (const [eid, emp] of Object.entries(entry.live.employees)) {
        if (!emp.busy || !emp.chatId || !agents.isAlive(emp.chatId)) continue;
        if (agents.state(emp.chatId) !== 'idle') continue;
        const ef = activationFor(entry, emp.busy);
        if (ef) await startStep(entry, emp, ef, { recovery: await recoveryFor(entry, eid, ef, why, '') });
      }
      await drain(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  /**
   * The time limit counts the run's active time, from its own events, so a
   * restart or a pause does not reset it. Checked when the run starts,
   * resumes or is recovered, and again when the remaining time runs out.
   */
  function startClock(entry) {
    clearClock(entry);
    if (!['running', 'waiting'].includes(st(entry).status) || rr(entry).capHit) return;
    const left = timeLeft(entry.rec, Date.now());
    const fire = () => locked(entry, () => {
      if (!['running', 'waiting'].includes(st(entry).status) || rr(entry).capHit) return;
      if (timeLeft(entry.rec, Date.now()) > 1000) return startClock(entry);
      const limit = rr(entry).time.maxMinutes + rr(entry).capMinutesExtra;
      commit(entry, { type: 'cap', which: 'maxMinutes', limit });
      notify('cap', summary(entry), `Factory "${st(entry).snapshot.name}" has run for ${limit} minutes and is waiting for you.`);
    });
    if (left <= 0) { fire(); return; }
    entry.timers.clock = setTimeout(fire, Math.min(left, 2 ** 31 - 1));
    entry.timers.clock.unref?.();
  }

  function clearClock(entry) { clearTimeout(entry.timers.clock); }

  // ─── Usage limit ───

  let usageTimer = null;
  /**
   * Pause running factories when a usage window is full, and carry them on by
   * themselves once it has reset. Polls getUsage (usage.js shape:
   * { fiveHour:{utilization,resetsAt}, week:{…} }); it only asks while a run
   * is going or waiting on the limit.
   */
  function watchUsage(getUsage, { tickMs = 60_000 } = {}) {
    const tick = async () => {
      const relevant = [...runs.values()].filter(e => ['running', 'waiting'].includes(st(e).status) || rr(e).paused?.by === 'usage');
      if (!relevant.length) return;
      let u;
      try { u = await getUsage(); } catch { return; }
      if (!u || u.ok === false) return;
      const full = [u.fiveHour, u.week].filter(w => w && w.utilization >= 100);
      const resetsAt = full.map(w => w.resetsAt).filter(Boolean).sort().pop() || '';
      for (const e of relevant) {
        if (full.length && !rr(e).paused && ['running', 'waiting'].includes(st(e).status)) {
          await pause(st(e).id, { by: 'usage', resetsAt });
        } else if (!full.length && rr(e).paused?.by === 'usage') {
          const at = Date.parse(rr(e).paused.resetsAt || '');
          if (Number.isFinite(at) && Date.now() < at) continue;
          await resume(st(e).id);
          notify('resumed', summary(e), `Factory "${st(e).snapshot.name}" carries on: the usage limit has reset.`);
        }
      }
    };
    clearInterval(usageTimer);
    usageTimer = setInterval(tick, tickMs);
    usageTimer.unref?.();
    return { tick, stop: () => clearInterval(usageTimer) };
  }

  // ─── Recovery ───

  /** Rebuild the activation a waiting token stands for. */
  function activationFor(entry, tokenId) {
    const t = st(entry).tokens.find(x => x.id === tokenId);
    if (!t || (t.status !== 'work' && t.status !== 'answer')) return null;
    const node = st(entry).snapshot.nodes.find(n => n.id === t.nodeId);
    return {
      type: 'activate', tokenId: t.id, nodeId: t.nodeId,
      employeeId: t.status === 'answer' ? t.answerer : Engine.rosterOf(node),
      mode: t.status, ws: t.ws, context: { from: null, summary: '', message: '', notes: [], escalation: '' },
    };
  }

  async function recoveryFor(entry, employeeId, ef, reason, note) {
    const last = [...st(entry).history].reverse().find(h => h.type === 'handoff' && h.by === employeeId);
    const ws = wsPath(entry, ef.ws);
    let w = { status: '', diffStat: '' };
    try { if (ws && existsSync(ws)) w = await git.workSummary(ws, rr(entry).baseCommit); } catch { /* best effort */ }
    return { reason, lastAction: last ? last.summary : '', status: w.status, diffStat: w.diffStat, note };
  }

  /**
   * Pick a crashed run up again. Unfinished effects are completed (each is
   * safe to repeat), the Manager is relaunched on its chat and conversation
   * and given an overview, and everyone who was mid-step gets a continue
   * prompt: why it stopped, their last handoff, the worktree now, the note.
   */
  async function recover(runId, { note = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, () => recoverLocked(entry, note));
  }

  async function recoverLocked(entry, note) {
    if (st(entry).status !== 'crashed') return { ok: false, error: `The run is ${st(entry).status}, not crashed.` };
    const r = rr(entry);
    if (!r.worktree || !existsSync(r.worktree)) return { ok: false, error: 'The run\'s worktree is gone, so it cannot be recovered. Stop the run instead.' };
    const reason = r.crash?.reason || 'it was interrupted';
    const from = r.crash?.from || 'running';
    const text = String(note || '').slice(0, 5000).trim();
    const dropped = r.perms.outstanding.map(p => `${p.employee} wanted ${p.toolName} ${JSON.stringify(p.input).slice(0, 160)}`);

    for (const emp of Object.values(entry.live.employees)) {
      if (emp.chatId) { unwatch(entry, emp.chatId); chats.delete(emp.chatId); }
      emp.chatId = ''; emp.busy = null; emp.queue = []; emp.nudged = false;
    }
    entry.live.permissions = [];
    entry.live.error = '';
    entry.skills = null;   // skills may have been installed since
    const missingWs = Object.entries(r.ws).filter(([id, w]) => id !== 'main' && w.path && !existsSync(w.path)).map(([id]) => id);
    const skillExcludes = git.linkProjectSkills && r.repo ? git.linkProjectSkills(r.repo, r.worktree) : [];
    commit(entry, { type: 'recover', reason, note: text, missingWs, skillExcludes });

    const m = await ensureSession(entry, MANAGER_ID);
    if (!m.ok) {
      commit(entry, { type: 'crash', reason });
      return { ok: false, error: `Could not start the Manager again: ${m.error}` };
    }

    const roster = new Map(st(entry).snapshot.roster.map(x => [x.id, x.name]));
    const nodes = new Map(st(entry).snapshot.nodes.map(n => [n.id, n]));
    const describe = (t) => {
      const n = nodes.get(t.nodeId);
      if (t.status === 'answer') return { employee: roster.get(t.answerer), what: `was asked "${n?.label || n?.decision?.question || ''}"` };
      if (t.status === 'ask') return { employee: '', what: `the person was asked: ${n?.ask?.prompt || ''}` };
      if (t.status === 'join') return { employee: '', what: `a branch was waiting at "${n?.label || 'Join'}"` };
      return { employee: roster.get(Engine.rosterOf(n)), what: `was working on "${n?.label || n?.type}"` };
    };
    let w = { status: '', diffStat: '' };
    try { w = await git.workSummary(r.worktree, r.baseCommit); } catch { /* best effort */ }
    const recent = st(entry).history.filter(h => h.type === 'handoff').slice(-5).map(h => `${roster.get(h.by) || h.by}: ${String(h.summary || '').split('\n')[0].slice(0, 200)}`);
    agents.send(m.chatId, recoveryOverview({
      run: st(entry), reason, tokens: st(entry).tokens.map(describe), recent, dropped,
      status: w.status, diffStat: w.diffStat, note: text, wasFinishing: from === 'finishing',
    }));

    // Finish what was half done; activations were dropped and are rebuilt here.
    await drain(entry);
    if (from !== 'finishing') {
      for (const t of st(entry).tokens.filter(x => x.status === 'work' || x.status === 'answer')) {
        const ef = activationFor(entry, t.id);
        if (!ef) continue;
        if (isHeld(entry.rec)) continue;   // resume() picks them up
        const emp = empOf(entry, ef.employeeId);
        if (emp.busy) { emp.queue.push(ef); continue; }
        await startStep(entry, emp, ef, { recovery: await recoveryFor(entry, ef.employeeId, ef, reason, text) });
      }
      for (const a of asksOf(entry.rec)) notify('ask', summary(entry), `Factory "${st(entry).snapshot.name}" asks: ${a.prompt}`);
      startClock(entry);
    }
    collect(entry);
    entry.autoTries = 0;
    notify('recovered', summary(entry), `Factory "${st(entry).snapshot.name}" was recovered.`);
    return { ok: true, run: summary(entry) };
  }

  /**
   * A factory set to recover by itself tries a few times, backing off; if it
   * still cannot, it stays crashed for the person to Recover by hand.
   */
  function scheduleAutoRecover(entry) {
    if (!st(entry).snapshot.settings.autoRecover || st(entry).status !== 'crashed') return;
    if (entry.autoTries >= autoRecoverDelays.length) return;
    clearTimeout(entry.timers.auto);
    const delay = autoRecoverDelays[entry.autoTries];
    entry.timers.auto = setTimeout(() => locked(entry, async () => {
      if (st(entry).status !== 'crashed') return;
      entry.autoTries++;
      const res = await recoverLocked(entry, '(recovered automatically)');
      if (res.ok) return;
      entry.live.error = res.error;
      writeCache(entry);
      if (entry.autoTries < autoRecoverDelays.length) scheduleAutoRecover(entry);
      else notify('crashed', summary(entry), `Factory "${st(entry).snapshot.name}" could not recover by itself (${res.error}). Open it and press Recover.`);
    }), delay);
    entry.timers.auto.unref?.();
  }

  // ─── Reachability collector ───

  /**
   * Stop the sessions of employees the current work can no longer reach.
   *
   * Roots are wherever the flow is (every token). Reachable is what flow
   * edges lead to from there, minus loops closed for good, and NOT onward
   * through the Manager unless the Manager is the one acting. That boundary
   * is what makes this worth having: Start only leads to the Manager, so
   * everything in a valid factory is reachable from the Manager, and with the
   * Manager always counted nobody would ever be collected. Employees inside
   * the loop being worked (a review loop back to the devs) stay alive;
   * someone only the Manager could send work to again is stopped until it does.
   *
   * An employee is needed if any of its nodes is reachable, or it answers a
   * reachable decision. Collected sessions keep their conversation id, and
   * when the flow reaches them again they are resumed with their memory.
   */
  function collect(entry) {
    if (!['running', 'waiting', 'paused'].includes(st(entry).status)) return;
    const f = st(entry).snapshot;
    const s = st(entry);
    const closed = (e) => e.maxRounds > 0 && (s.edgeCounts[e.id] || 0) >= e.maxRounds && (s.exhausted[e.id] || 0) >= 2;
    const g = flowGraph(f, { exclude: closed });
    const managerNode = f.nodes.find(n => n.type === 'manager');
    const managerActing = s.tokens.some(t => t.nodeId === managerNode?.id || (t.status === 'answer' && t.answerer === MANAGER_ID));
    const roots = s.tokens.map(t => t.nodeId).filter(Boolean);
    const seen = reachable(g, roots, managerActing ? {} : { stopAt: new Set([managerNode?.id]) });
    const needed = new Set([MANAGER_ID]);
    for (const n of f.nodes) {
      if (!seen.has(n.id)) continue;
      if (n.type === 'employee') needed.add(n.employeeId);
      if (n.type === 'decision' && n.decision.answeredBy) needed.add(n.decision.answeredBy);
    }
    for (const t of s.tokens) if (t.status === 'answer' && t.answerer) needed.add(t.answerer);
    for (const [eid, emp] of Object.entries(entry.live.employees)) {
      if (needed.has(eid) || !emp.chatId || emp.busy || (emp.queue || []).length) continue;
      if (entry.live.permissions.some(p => p.employeeId === eid)) continue;
      const chatId = emp.chatId;
      const uuid = agents.sessionUuid(chatId) || '';
      unwatch(entry, chatId);
      chats.delete(chatId);
      try { agents.close(chatId); } catch { /* gone */ }
      emp.chatId = '';
      commit(entry, { type: 'retire', employeeId: eid, sessionUuid: uuid });
    }
  }

  // ─── Calls from outside ───

  /**
   * factory_handoff, from the chat that called it. Commits the step's files,
   * checks the handoff, writes it to the journal, routes it, and answers the
   * employee: where the work went, or what is missing.
   */
  async function handoff(chatId, payload) {
    const info = chatInfo(chatId);
    if (!info) return { ok: false, error: 'This chat is not part of a factory run, so there is nothing to hand off.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    return locked(entry, async () => {
      const emp = entry.live.employees[info.employeeId];
      if (!UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') return { ok: false, error: `The run is ${st(entry).status}; nothing is waiting on you.` };
      // The Manager may be asked several things (answers from parallel
      // branches); the one it is on is emp.busy, else the oldest waiting.
      let tokenId = emp && emp.busy;
      if (!tokenId) tokenId = Engine.tokensFor(st(entry), info.employeeId)[0]?.id;
      if (!tokenId) return { ok: false, error: 'Nothing is waiting on you right now. Stop here and wait.' };
      const tok = st(entry).tokens.find(t => t.id === tokenId);
      const node = st(entry).snapshot.nodes.find(n => n.id === tok?.nodeId);
      const subject = shortSubject(payload?.summary || payload?.escalate || '');
      const c = await git.commitAll(wsPath(entry, tok?.ws), `factory: ${node?.label || node?.type || 'step'}${subject ? ' — ' + subject : ''}`, excl(entry));
      const caps = { maxHandoffs: st(entry).snapshot.settings.caps.maxHandoffs + rr(entry).capExtra };
      const dry = Engine.handoff(st(entry), tokenId, payload, { treeHash: c.ok ? c.tree : '', caps });
      if (!dry.ok) return { ok: false, error: dry.error };
      commit(entry, {
        type: 'handoff', tokenId, payload, treeHash: c.ok ? c.tree : '', by: info.employeeId,
        commit: c.ok ? c.head : '', sessionUuid: emp?.chatId ? agents.sessionUuid(emp.chatId) || '' : '',
      });
      if (emp && emp.busy === tokenId) emp.busy = null;
      const where = describeWhere(entry, dry.effects);
      await drain(entry);
      await dequeue(entry, info.employeeId);
      if (payload?.finish) return { ok: true, message: 'The run is finished. Now present the result to the person in this chat: what was done and why, what changed, and how they can look at it.' };
      return { ok: true, message: `Handed off${where ? ': ' + where : ''}. Your step is over. Stop here and wait; do not reply at length.` };
    });
  }

  function describeWhere(entry, effects) {
    const roster = new Map(st(entry).snapshot.roster.map(r => [r.id, r.name]));
    const parts = [];
    for (const e of effects) {
      if (e.type === 'activate') parts.push(e.mode === 'answer' ? `${roster.get(e.employeeId)} is asked to decide` : `${roster.get(e.employeeId)} takes over`);
      if (e.type === 'ask') parts.push('the person is asked');
      if (e.type === 'cap') parts.push('the run has reached its handoff limit and waits for the person');
    }
    if (!parts.length && st(entry).tokens.some(t => t.status === 'join')) parts.push('waiting for the other branches');
    return parts.join('; ');
  }

  async function askReply(runId, tokenId, replyId, note = '') {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      const dry = Engine.askReply(st(entry), tokenId, replyId, note);
      if (!dry.ok) return dry;
      commit(entry, { type: 'ask-reply', tokenId, replyId, note: String(note || '') });
      await drain(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  /**
   * Stop a run: every employee's session is closed, the Manager's chat is
   * left as an ordinary chat. The worktree is kept (cleanup decides).
   */
  async function stopRun(runId) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (!UNFINISHED.has(st(entry).status)) return { ok: false, error: `The run is already ${st(entry).status}.` };
      dropPermissions(entry, () => true, 'The run was stopped.');
      for (const [eid, emp] of Object.entries(entry.live.employees)) {
        if (emp.chatId) {
          unwatch(entry, emp.chatId);
          chats.delete(emp.chatId);
          if (eid !== MANAGER_ID) { try { await agents.close(emp.chatId); } catch { /* gone */ } emp.chatId = ''; }
        }
        emp.busy = null; emp.queue = [];
      }
      clearClock(entry);
      clearTimeout(entry.timers.auto);
      commit(entry, { type: 'stop' });
      return { ok: true, run: summary(entry) };
    });
  }

  /** Stop timers (tests, shutdown). Runs stay on disk. */
  function dispose() {
    clearInterval(usageTimer);
    for (const e of runs.values()) {
      clearClock(e);
      clearTimeout(e.timers.perm);
      clearTimeout(e.timers.auto);
      for (const id of Object.keys(e.watchers)) unwatch(e, id);
    }
  }

  return { load, startRun, handoff, askReply, resume, pause, recover, stopRun, listRuns, getRun, activeRunFor, chatInfo, answerPermissions, setManagerMode, watchUsage, dispose };
}

/** A commit subject from a summary: its first sentence, kept short. */
function shortSubject(text) {
  const first = String(text || '').split('\n')[0].trim();
  const sentence = (first.match(/^.+?[.!?](\s|$)/) || [first])[0].trim().replace(/[.!?]$/, '');
  return sentence.length > 72 ? sentence.slice(0, 71).trimEnd() + '…' : sentence;
}

/**
 * The real team: claude-ui chats. Each gets the Crundi tools through its own
 * --mcp-config file (in the data dir, never in the worktree), carrying the
 * chat id and the run id so factory_handoff is offered and lands in the right
 * place.
 */
export function claudeUiAgents(claudeUi, { dataDir }) {
  const dir = join(dataDir, 'factory-mcp');
  return {
    async spawn(o) {
      // A relaunch (bypass switched on, a recovery) keeps the chat's id, so
      // the person's tab stays where it is.
      const id = o.id && /^[0-9a-f]{16}$/.test(o.id) && !claudeUi.has(o.id) ? o.id : randomBytes(8).toString('hex');
      let mcpConfigFile = '';
      try {
        mkdirSync(dir, { recursive: true });
        mcpConfigFile = join(dir, `${id}.json`);
        writeFileSync(mcpConfigFile, JSON.stringify({
          mcpServers: {
            crundi: {
              command: 'node',
              args: [fileURLToPath(new URL('./mcp-stdio.js', import.meta.url))],
              env: {
                CRUNDI_API_URL: claudeUi.apiUrl || '',
                CRUNDI_API_KEY: claudeUi.apiKey || '',
                CRUNDI_PROJECT: o.project,
                CRUNDI_CHAT_ID: id,
                CRUNDI_FACTORY_RUN: o.runId,
              },
            },
          },
        }, null, 2), { mode: 0o600 });
      } catch { mcpConfigFile = ''; }
      return claudeUi.create(o.project, {
        id,
        title: o.title,
        cwd: o.cwd,
        model: o.model || '',
        effort: o.effort || '',
        skipPermissions: !!o.skipPermissions,
        permissionMode: o.permissionMode || '',
        sessionMode: o.resumeId ? 'resume' : 'new',
        resumeId: o.resumeId || '',
        background: !!o.background,
        systemPromptExtra: o.systemPromptExtra,
        factory: { runId: o.runId, role: o.role, employeeId: o.employeeId },
        addDirs: o.addDirs,
        extraEnv: { CRUNDI_FACTORY_RUN: o.runId },
        allowedTools: o.allowedTools,
        disallowedTools: o.disallowedTools,
        mcpConfigFile,
      });
    },
    send: (id, text) => claudeUi.sendMessage(id, text),
    close: (id) => claudeUi.close(id),
    interrupt: (id) => claudeUi.interrupt(id),
    isAlive: (id) => !!(claudeUi.has(id) && claudeUi.meta(id)?.running),
    sessionUuid: (id) => claudeUi.meta(id)?.sessionId || '',
    state: (id) => claudeUi.meta(id)?.state || '',
    respond: (id, r) => claudeUi.respond(id, r),
    setPermissionMode: (id, mode) => claudeUi.setPermissionMode(id, mode),
    onPermission: (cb) => claudeUi.onFactoryPermission(cb),
    on: (id, fn) => claudeUi.on(id, fn),
    off: (id, fn) => claudeUi.off(id, fn),
  };
}
