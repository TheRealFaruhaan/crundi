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
 *   - the factory_handoff tool: a call arrives here with the calling chat's id,
 *     the step is committed, the engine routes, and the effects are carried out
 *   - stalls: an employee that stops without handing off is nudged once, then
 *     escalated to the Manager; a Manager that stops is talking to the person
 *   - caps: maxHandoffs (from the engine) and maxMinutes hold the run for the
 *     person; resume() lets it carry on
 *   - one run per project: a second start is refused while one is unfinished
 *
 * Everything about a run is written to <dataDir>/factory-runs/<runId>/run.json
 * after every change, with each handoff appended to handoffs.jsonl. At boot a
 * run that was still going comes back as 'crashed' (its chats died with the
 * process); recovering it is card 4.
 *
 * Work for one run is serialised through a promise chain, so two employees in
 * parallel branches handing off at the same moment are routed one after the
 * other and both arrive.
 *
 * Spec: specs/factory.md §4.
 */

import { mkdirSync, writeFileSync, readFileSync, existsSync, readdirSync, renameSync, appendFileSync } from 'fs';
import { join } from 'path';
import { spawn } from 'child_process';
import { randomBytes } from 'crypto';
import { fileURLToPath } from 'url';
import * as Engine from './factory-engine.js';
import * as Git from './factory-git.js';
import { validateFactory, MANAGER_ID, flowGraph, reachable } from './factory-model.js';
import { systemPromptFor, stepMessage, nudgeMessage, cancelMessage, recoveryOverview } from './factory-prompts.js';
import { hardDeny, ruleMatches, checkRule, batchMessage, denyMessage, DENY_RULES, ALLOW_RULES } from './factory-permissions.js';

/** Runs in these states hold their project: no second run can start. */
export const UNFINISHED = new Set(['starting', 'running', 'waiting', 'paused', 'finishing', 'crashed']);

const SETTLE_MS = 20_000;
const SETUP_TIMEOUT_MS = 15 * 60 * 1000;
const SETUP_OUTPUT_MAX = 4000;
const PERMISSION_BATCH_MS = 1500;   // parallel branches asking at once arrive as one message
const PERMISSION_LOG_MAX = 500;

/**
 * @param {object} o
 * @param {string} o.dataDir
 * @param {object} o.agents         see claudeUiAgents() for the shape
 * @param {(alias:string) => {path:string}|null} o.getProject
 * @param {(id:string) => {factory, validation}|null} o.getFactory
 * @param {object} [o.git]          factory-git.js (tests may wrap it)
 * @param {(kind:string, run:object, text:string) => void} [o.notify]
 * @param {(run:object) => void} [o.onChange]
 */
export function createFactoryRunner({
  dataDir, agents, getProject, getFactory, git = Git,
  notify = () => {}, onChange = () => {},
  settleMs = SETTLE_MS, setupTimeoutMs = SETUP_TIMEOUT_MS, permissionBatchMs = PERMISSION_BATCH_MS,
  now = () => new Date().toISOString(),
}) {
  const RUNS = join(dataDir, 'factory-runs');
  /** runId -> { state, meta, lock, timers, watchers } */
  const runs = new Map();
  /** chatId -> { runId, employeeId } */
  const chats = new Map();

  // Employees' permission requests arrive here (see claude-ui onFactoryPermission).
  if (typeof agents.onPermission === 'function') agents.onPermission((req) => onPermissionRequest(req));

  // ─── Persistence ───

  function runDir(id) { return join(RUNS, id); }

  function persist(entry) {
    entry.state.updatedAt = now();
    const dir = runDir(entry.state.id);
    mkdirSync(dir, { recursive: true });
    const tmp = join(dir, 'run.json.tmp');
    writeFileSync(tmp, JSON.stringify({ state: entry.state, meta: entry.meta }, null, 2));
    renameSync(tmp, join(dir, 'run.json'));
    try { onChange(summary(entry)); } catch { /* a listener's problem */ }
  }

  function appendHandoff(entry, rec) {
    try { appendFileSync(join(runDir(entry.state.id), 'handoffs.jsonl'), JSON.stringify({ at: now(), ...rec }) + '\n'); } catch { /* best effort */ }
  }

  /** At boot: read every run; anything that was still going crashed with the process. */
  function load() {
    if (!existsSync(RUNS)) return;
    for (const name of readdirSync(RUNS)) {
      const file = join(RUNS, name, 'run.json');
      if (!existsSync(file)) continue;
      let data;
      try { data = JSON.parse(readFileSync(file, 'utf-8')); } catch { continue; }
      if (!data?.state?.id) continue;
      const entry = newEntry(data.state, data.meta || {});
      if (UNFINISHED.has(entry.state.status) && entry.state.status !== 'crashed') {
        entry.meta.crashedFrom = entry.state.status;
        entry.meta.crashedAt = now();
        entry.meta.crashReason = 'Crundi restarted, and every chat of the run ended with it';
        entry.state.status = 'crashed';
        for (const emp of Object.values(entry.meta.employees || {})) { emp.chatId = ''; }
        // Those requests belonged to processes that are gone; nothing can answer
        // them now. Kept aside so recovery can say what was waiting.
        entry.meta.droppedPermissions = entry.meta.permissions || [];
        entry.meta.permissions = [];
        persist(entry);
      }
      runs.set(entry.state.id, entry);
    }
  }

  function newEntry(state, meta) {
    return { state, meta, lock: Promise.resolve(), timers: {}, watchers: {} };
  }

  /** Serialise everything that touches one run. */
  function locked(entry, fn) {
    const p = entry.lock.then(fn, fn);
    entry.lock = p.catch(() => {});
    return p;
  }

  // ─── Reads ───

  function summary(entry) {
    const { state: s, meta: m } = entry;
    const nodes = new Map(s.snapshot.nodes.map(n => [n.id, n]));
    const roster = new Map(s.snapshot.roster.map(r => [r.id, r]));
    return {
      id: s.id,
      project: s.project,
      factoryId: m.factoryId,
      factoryName: s.snapshot.name,
      factoryVersion: m.factoryVersion,
      task: s.task,
      status: s.status,
      waitingOn: m.paused ? (m.paused.by === 'usage' ? 'usage' : 'paused') : m.capHit ? 'cap' : (m.asks || []).length ? 'ask' : m.managerIdle ? 'manager' : '',
      paused: m.paused || null,
      crash: s.status === 'crashed' ? { reason: m.crashReason || '', at: m.crashedAt || '', from: m.crashedFrom || '' } : null,
      capHit: m.capHit || null,
      createdAt: s.createdAt,
      updatedAt: s.updatedAt,
      handoffs: s.handoffs,
      worktree: m.worktree,
      branch: m.branch,
      parentBranch: m.parentBranch,
      dirtyAtStart: !!m.dirty,
      managerChatId: m.employees?.[MANAGER_ID]?.chatId || '',
      tokens: s.tokens.map(t => {
        const n = nodes.get(t.nodeId);
        const who = t.status === 'answer' ? t.answerer : Engine.rosterOf(n);
        return { id: t.id, nodeId: t.nodeId, label: n?.label || n?.type || '', status: t.status, employeeId: who || '', employee: roster.get(who)?.name || '', ws: t.ws };
      }),
      employees: Object.fromEntries(Object.entries(m.employees || {}).map(([k, e]) => [k, { chatId: e.chatId, busy: e.busy || null, queued: (e.queue || []).length, retired: !!e.retired, name: roster.get(k)?.name || k }])),
      liveSessions: Object.values(m.employees || {}).filter(e => e.chatId).length,
      asks: m.asks || [],
      managerMode: m.managerMode || s.snapshot.settings.managerMode,
      permissions: (m.permissions || []).map(p => ({ id: p.id, employee: p.employee, toolName: p.toolName, input: p.input, at: p.at, sent: p.sent })),
      rules: m.rules || [],
      outcome: s.outcome,
      setup: m.setup || null,
      error: m.error || '',
    };
  }

  function listRuns({ project } = {}) {
    const key = project ? String(project).toLowerCase() : '';
    return [...runs.values()].filter(e => !key || e.state.project === key).map(summary)
      .sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt)));
  }

  function getRun(id) {
    const e = runs.get(id);
    return e ? { ...summary(e), history: e.state.history.slice(-200), snapshot: e.state.snapshot } : null;
  }

  function activeRunFor(project) {
    const key = String(project || '').toLowerCase();
    for (const e of runs.values()) if (e.state.project === key && UNFINISHED.has(e.state.status)) return summary(e);
    return null;
  }

  /** Which run and person a chat belongs to, if any. */
  function chatInfo(chatId) { return chats.get(String(chatId || '')) || null; }

  // ─── Start ───

  async function startRun({ project, factoryId, task, mergeMode } = {}) {
    const key = String(project || '').toLowerCase();
    const proj = getProject(key);
    if (!proj) return { ok: false, error: `Project "${project}" not found.` };
    const existing = activeRunFor(key);
    if (existing) return { ok: false, busy: true, runId: existing.id, error: `A factory is already ${existing.status === 'crashed' ? 'waiting to be recovered' : 'running'} in this project ("${existing.factoryName}"). Finish or stop it first.` };
    const taskText = String(task || '').trim();
    if (!taskText) return { ok: false, error: 'Give the factory a task.' };
    const got = getFactory(factoryId);
    if (!got) return { ok: false, error: 'Factory not found.' };
    const v = validateFactory(got.factory);
    if (!v.ok) return { ok: false, invalid: true, errors: v.errors, error: `This factory cannot run yet: ${v.errors.length} problem${v.errors.length === 1 ? '' : 's'} to fix in the editor.` };

    const id = 'run_' + Date.now().toString(36) + randomBytes(3).toString('hex');
    const state = Engine.createRun({ id, project: key, factory: got.factory, task: taskText, now: now() });
    const meta = {
      factoryId, factoryVersion: got.factory.version,
      mergeMode: mergeMode === 'auto' || mergeMode === 'ask' ? mergeMode : state.snapshot.settings.mergeMode,
      employees: {}, ws: {}, asks: [], notices: {}, held: [], capExtra: 0, startedAt: now(),
      managerMode: state.snapshot.settings.managerMode,
      permissions: [], permSeq: 0, rules: [], permissionLog: [],
    };
    const entry = newEntry(state, meta);
    runs.set(id, entry);   // holds the project from this moment

    const fail = (error) => {
      state.status = 'stopped';
      meta.error = error;
      persist(entry);
      return { ok: false, error };
    };

    const wt = await git.createRunWorktree({ repo: proj.path, project: key, runId: id, name: state.snapshot.name });
    if (!wt.ok) { runs.delete(id); return { ok: false, error: wt.error }; }
    Object.assign(meta, { repo: wt.repo, root: wt.root, worktree: wt.path, branch: wt.branch, parentBranch: wt.parentBranch, baseCommit: wt.baseCommit, dirty: wt.dirty });
    meta.ws.main = { path: wt.path, branch: wt.branch };
    mkdirSync(join(runDir(id), 'shared'), { recursive: true });
    mkdirSync(join(runDir(id), 'notes'), { recursive: true });
    persist(entry);

    const setupCmd = state.snapshot.settings.setupCommand.trim();
    if (setupCmd) {
      meta.setup = { status: 'running', command: setupCmd };
      persist(entry);
      const r = await runSetup(setupCmd, wt.path);
      meta.setup = { status: r.ok ? 'ok' : 'failed', command: setupCmd, output: r.output };
      if (!r.ok) meta.notices.t0 = `The factory's setup command failed (exit ${r.code}):\n$ ${setupCmd}\n${r.output}`;
    }

    // The Manager's chat exists from the start: it is where the person watches.
    const mgr = await ensureSession(entry, MANAGER_ID);
    if (!mgr.ok) return fail(`Could not start the Manager: ${mgr.error}`);

    const s = Engine.start(state);
    if (!s.ok) return fail(s.error);
    entry.state = s.run;
    persist(entry);
    startClock(entry);
    await locked(entry, () => runEffects(entry, s.effects));
    notify('started', summary(entry), `Factory "${state.snapshot.name}" started in ${key}.`);
    return { ok: true, run: summary(entry) };
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
    const dir = runDir(entry.state.id);
    return {
      worktree: entry.meta.worktree,
      branch: entry.meta.branch,
      parentBranch: entry.meta.parentBranch,
      notesFile: join(dir, 'notes', `${employeeId}.md`),
      sharedDir: join(dir, 'shared'),
    };
  }

  function empOf(entry, employeeId) {
    const m = entry.meta;
    if (!m.employees[employeeId]) m.employees[employeeId] = { chatId: '', sessionUuid: '', busy: null, queue: [], nudged: false };
    return m.employees[employeeId];
  }

  async function ensureSession(entry, employeeId, { fresh = false, reuseId = '' } = {}) {
    const emp = empOf(entry, employeeId);
    if (emp.chatId && !fresh && !reuseId && agents.isAlive(emp.chatId)) return { ok: true, chatId: emp.chatId };
    if (emp.chatId) { unwatch(entry, emp.chatId); try { await agents.close(emp.chatId); } catch { /* gone */ } chats.delete(emp.chatId); }
    const f = entry.state.snapshot;
    const r = f.roster.find(x => x.id === employeeId);
    if (!r) return { ok: false, error: `Nobody called "${employeeId}" is on the roster.` };
    const isManager = employeeId === MANAGER_ID;
    const resume = !fresh && r.memory === 'keep' && emp.sessionUuid ? emp.sessionUuid : '';
    const res = await agents.spawn({
      project: entry.state.project,
      runId: entry.state.id,
      role: isManager ? 'manager' : 'employee',
      employeeId,
      title: isManager ? `Factory: ${f.name}` : `${r.name} · ${f.name}`,
      cwd: entry.meta.worktree,
      addDirs: [entry.meta.root, runDir(entry.state.id)].filter(Boolean),
      systemPromptExtra: systemPromptFor({ factory: f, employee: r, project: entry.state.project, paths: paths(entry, employeeId), managerMode: managerModeOf(entry) }),
      model: r.model, effort: r.effort,
      allowedTools: [...r.tools.allow, ...ALLOW_RULES],
      // The fixed deny list rides on every chat: it is all that binds a
      // Manager in bypass mode, which never asks.
      disallowedTools: [...r.tools.deny, ...DENY_RULES],
      // Employees: edits inside the run's folders are free (acceptEdits with
      // --add-dir), anything else asks, and asking goes to the Manager.
      // The Manager: bypass if the person chose it, else its own actions ask
      // the person in its chat as usual.
      skipPermissions: isManager && managerModeOf(entry) === 'bypass',
      permissionMode: isManager ? '' : 'acceptEdits',
      background: !isManager,
      resumeId: resume,
      id: reuseId || '',
    });
    if (!res.ok) return { ok: false, error: res.error || 'could not start' };
    emp.chatId = res.id;
    if (isManager) entry.meta.lastManagerChat = res.id;
    chats.set(res.id, { runId: entry.state.id, employeeId });
    watch(entry, res.id, employeeId);
    persist(entry);
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
        if (employeeId === MANAGER_ID && entry.meta.managerIdle) { entry.meta.managerIdle = false; persist(entry); }
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
      const emp = entry.meta.employees[employeeId];
      if (!emp || emp.chatId !== chatId || !emp.busy || !UNFINISHED.has(entry.state.status)) return;
      // Paused (by the person, or the usage limit cutting a turn short): a
      // stopped turn is expected. resume() picks them up.
      if (entry.meta.paused || entry.state.status === 'crashed') return;
      if (employeeId === MANAGER_ID) {
        entry.meta.managerIdle = true;
        persist(entry);
        return;
      }
      if (!emp.nudged) {
        emp.nudged = true;
        persist(entry);
        agents.send(chatId, nudgeMessage());
        return;
      }
      const name = entry.state.snapshot.roster.find(r => r.id === employeeId)?.name || employeeId;
      await escalateToken(entry, emp.busy, `${name} stopped twice without handing off.`, employeeId);
    });
  }

  function onExit(entry, chatId, employeeId) {
    return locked(entry, async () => {
      const emp = entry.meta.employees[employeeId];
      unwatch(entry, chatId);
      chats.delete(chatId);
      dropPermissions(entry, (p) => p.chatId === chatId, '');
      if (!emp || emp.chatId !== chatId) return;
      emp.sessionUuid = emp.sessionUuid || '';
      emp.chatId = '';
      persist(entry);
      if (!UNFINISHED.has(entry.state.status) || entry.state.status === 'crashed') return;
      if (employeeId === MANAGER_ID) {
        entry.meta.crashedFrom = entry.state.status;
        entry.meta.crashedAt = now();
        entry.meta.crashReason = 'the Manager\'s chat ended';
        entry.state.status = 'crashed';
        entry.meta.error = 'The Manager\'s chat ended.';
        persist(entry);
        notify('crashed', summary(entry), `Factory "${entry.state.snapshot.name}" stopped: the Manager's chat ended.`);
        return;
      }
      if (emp.busy) {
        const name = entry.state.snapshot.roster.find(r => r.id === employeeId)?.name || employeeId;
        await escalateToken(entry, emp.busy, `${name}'s session ended unexpectedly in the middle of a step.`, employeeId);
      }
    });
  }

  // ─── Effects ───

  /**
   * Carry out effects, write-ahead: they are saved as pending BEFORE anything
   * happens and ticked off one by one, so a crash part-way leaves on disk
   * exactly what is still to do (recover() finishes it). Effects raised while
   * draining (an escalation after a merge conflict) go to the front of the
   * queue and the one drain loop runs them next.
   */
  async function runEffects(entry, effects) {
    const cap = effects.find(e => e.type === 'cap');
    if (cap) {
      entry.meta.capHit = { which: cap.which, limit: cap.limit, at: now() };
      notify('cap', summary(entry), `Factory "${entry.state.snapshot.name}" reached its limit of ${cap.limit} handoffs and is waiting for you.`);
    }
    entry.meta.pending = [...effects.filter(e => e.type !== 'cap'), ...(entry.meta.pending || [])];
    persist(entry);
    if (entry.draining) return;
    entry.draining = true;
    try {
      while (entry.meta.pending.length) {
        const ef = entry.meta.pending.shift();
        await runOne(entry, ef);
        persist(entry);
      }
    } finally {
      entry.draining = false;
    }
    collect(entry);
    persist(entry);
  }

  /** Activations wait while the run is held (a cap, or paused). */
  function held(entry) { return !!(entry.meta.capHit || entry.meta.paused); }

  async function runOne(entry, ef) {
    if (held(entry) && ef.type === 'activate') { entry.meta.held.push(ef); return; }
    try {
      switch (ef.type) {
        case 'activate': await activate(entry, ef); break;
        case 'workspace': await makeWorkspace(entry, ef); break;
        case 'merge': {
          const r = await mergeWorkspaces(entry, ef);
          if (!r.ok && !ef.abort) {
            // The continuation must not go on as if merged: drop what was
            // queued for it, and send it to the Manager instead.
            entry.meta.pending = entry.meta.pending.filter(p => p.tokenId !== ef.contTokenId);
            await escalateToken(entry, ef.contTokenId, r.reason);
          }
          break;
        }
        case 'cancel': await cancelTokens(entry, ef); break;
        case 'ask':
          if (!entry.meta.asks.some(a => a.tokenId === ef.tokenId)) entry.meta.asks.push({ tokenId: ef.tokenId, nodeId: ef.nodeId, prompt: ef.prompt, replies: ef.replies, context: ef.context, at: now() });
          notify('ask', summary(entry), `Factory "${entry.state.snapshot.name}" asks: ${ef.prompt}`);
          break;
        case 'finish': await onFinish(entry, ef); break;
        default: break;
      }
    } catch (err) {
      entry.meta.error = `${ef.type}: ${err.message}`;
    }
  }

  /** The engine recomputes status from tokens; the runner's holds win over it. */
  function applyHolds(entry) {
    if (entry.meta.paused && UNFINISHED.has(entry.state.status) && entry.state.status !== 'finishing') entry.state.status = 'paused';
    else if (entry.meta.capHit && entry.state.status === 'running') entry.state.status = 'waiting';
  }

  async function activate(entry, ef) {
    const emp = empOf(entry, ef.employeeId);
    if (emp.busy && emp.busy !== ef.tokenId) { emp.queue.push(ef); return; }
    await startStep(entry, emp, ef);
  }

  async function startStep(entry, emp, ef, { recovery = null } = {}) {
    const r = entry.state.snapshot.roster.find(x => x.id === ef.employeeId);
    const fresh = !!(r && r.memory === 'fresh' && ef.employeeId !== MANAGER_ID && emp.chatId);
    const s = await ensureSession(entry, ef.employeeId, { fresh });
    if (!s.ok) {
      entry.meta.error = s.error;
      if (ef.employeeId !== MANAGER_ID) await escalateToken(entry, ef.tokenId, `Could not start ${r?.name || ef.employeeId}: ${s.error}`);
      return;
    }
    emp.busy = ef.tokenId;
    emp.nudged = false;
    if (ef.employeeId === MANAGER_ID) entry.meta.managerIdle = false;
    const notice = entry.meta.notices[ef.tokenId] || '';
    delete entry.meta.notices[ef.tokenId];
    emp.retired = false;
    const msg = stepMessage({
      run: entry.state, effect: ef, req: Engine.requirements(entry.state, ef.tokenId),
      wsPath: wsPath(entry, ef.ws), notice, recovery,
    });
    const sent = agents.send(s.chatId, msg);
    if (sent && sent.ok === false) entry.meta.error = `Could not message ${r?.name || ef.employeeId}: ${sent.error}`;
    persist(entry);
  }

  function wsPath(entry, wsId) {
    return (entry.meta.ws[wsId] || entry.meta.ws.main || {}).path || entry.meta.worktree;
  }

  async function makeWorkspace(entry, ef) {
    const parent = entry.meta.ws[ef.parentWs] || entry.meta.ws.main;
    const r = await git.createBranchWorktree({ repo: entry.meta.repo, parentPath: parent.path, root: entry.meta.root, name: ef.name, branchPrefix: entry.meta.branch });
    if (r.ok) entry.meta.ws[ef.wsId] = { path: r.path, branch: r.branch, parent: ef.parentWs };
    else entry.meta.ws[ef.wsId] = { path: parent.path, branch: '', parent: ef.parentWs, shared: true, error: r.error };
  }

  async function mergeWorkspaces(entry, ef) {
    const into = entry.meta.ws[ef.into] || entry.meta.ws.main;
    const conflicts = [];
    for (const w of ef.from) {
      const ws = entry.meta.ws[w];
      if (!ws) continue;
      if (ws.shared || !ws.branch) { delete entry.meta.ws[w]; continue; }
      const r = await git.mergeBranch({ parentPath: into.path, childPath: ws.path, childBranch: ws.branch });
      if (r.ok) {
        await git.removeWorktree({ repo: entry.meta.repo, path: ws.path, branch: ws.branch });
        delete entry.meta.ws[w];
      } else {
        conflicts.push({ ws: w, path: ws.path, branch: ws.branch, files: r.files || [], error: r.error });
        ws.orphan = true;   // kept for the Manager to resolve; cleanup knows it
      }
    }
    if (!conflicts.length) return { ok: true };
    const reason = 'Merging the parallel branches back together failed:\n' + conflicts.map(c =>
      `- branch ${c.branch} (folder ${c.path}): ${c.error}`).join('\n') +
      `\nThe branches are kept. Have someone merge them into ${into.path} (git merge <branch>) and resolve the conflicts, then carry on.`;
    if (ef.abort) entry.meta.notices[ef.contTokenId] = (entry.meta.notices[ef.contTokenId] ? entry.meta.notices[ef.contTokenId] + '\n' : '') + reason;
    return { ok: false, reason };
  }

  async function cancelTokens(entry, ef) {
    const ids = new Set(ef.tokenIds);
    for (const [eid, emp] of Object.entries(entry.meta.employees)) {
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
    entry.meta.held = entry.meta.held.filter(h => !ids.has(h.tokenId));
  }

  async function onFinish(entry, ef) {
    entry.meta.finishedSummary = ef.summary;
    dropPermissions(entry, () => true, 'The run has finished.');
    // The team is done; the Manager stays to present the result (card 5).
    for (const [eid, emp] of Object.entries(entry.meta.employees)) {
      if (eid === MANAGER_ID) { emp.busy = null; continue; }
      if (emp.chatId) { unwatch(entry, emp.chatId); chats.delete(emp.chatId); try { await agents.close(emp.chatId); } catch { /* gone */ } emp.chatId = ''; }
      emp.busy = null; emp.queue = [];
    }
    clearClock(entry);
    notify('finished', summary(entry), `Factory "${entry.state.snapshot.name}" is done: ${ef.summary}`);
  }

  async function dequeue(entry, employeeId) {
    const emp = entry.meta.employees[employeeId];
    if (!emp || emp.busy || !emp.queue.length) return;
    const next = emp.queue.shift();
    await startStep(entry, emp, next);
  }

  async function escalateToken(entry, tokenId, reason, fromEmployee) {
    const res = Engine.escalate(entry.state, tokenId, reason);
    if (!res.ok) { entry.meta.error = res.error; persist(entry); return; }
    if (fromEmployee) {
      const emp = entry.meta.employees[fromEmployee];
      if (emp && emp.busy === tokenId) emp.busy = null;
    }
    entry.state = res.run;
    applyHolds(entry);
    appendHandoff(entry, { type: 'escalate', tokenId, reason });
    persist(entry);
    await runEffects(entry, res.effects);
    if (fromEmployee) await dequeue(entry, fromEmployee);
  }

  // ─── Permissions: employees ask, the Manager decides ───

  function managerModeOf(entry) { return entry.meta.managerMode || entry.state.snapshot.settings.managerMode; }

  function logPermission(entry, rec) {
    const log = entry.meta.permissionLog || (entry.meta.permissionLog = []);
    log.push({ at: now(), ...rec });
    if (log.length > PERMISSION_LOG_MAX) log.splice(0, log.length - PERMISSION_LOG_MAX);
  }

  function answer(entry, item, allow, message) {
    let r;
    try { r = agents.respond(item.chatId, { requestId: item.requestId, behavior: allow ? 'allow' : 'deny', message }); }
    catch (err) { r = { ok: false, error: err.message }; }
    return r && r.ok !== false;
  }

  /** Remove queued requests; if a reason is given, the employees are told no. */
  function dropPermissions(entry, pred, reason) {
    const keep = [];
    for (const p of entry.meta.permissions || []) {
      if (!pred(p)) { keep.push(p); continue; }
      if (reason) answer(entry, p, false, denyMessage(reason, 'Crundi'));
      logPermission(entry, { id: p.id, employee: p.employee, toolName: p.toolName, decision: 'dropped', reason });
    }
    entry.meta.permissions = keep;
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
      const roster = entry.state.snapshot.roster.find(r => r.id === info.employeeId);
      const item = {
        id: 'p' + (++entry.meta.permSeq),
        chatId: req.sessionId, employeeId: info.employeeId, employee: roster?.name || info.employeeId,
        tokenId: entry.meta.employees[info.employeeId]?.busy || '',
        requestId: req.requestId, toolName: req.toolName, displayName: req.displayName,
        description: String(req.description || '').slice(0, 1000),
        input: clipInput(req.input), at: now(), sent: false,
      };
      if (!UNFINISHED.has(entry.state.status) || entry.state.status === 'crashed') {
        answer(entry, item, false, denyMessage(`the run is ${entry.state.status}`, 'Crundi'));
        return;
      }
      const why = hardDeny({ toolName: req.toolName, input: req.input }, { writableRoots: [entry.meta.root, runDir(entry.state.id)] });
      if (why) {
        answer(entry, item, false, denyMessage(why, 'Crundi'));
        logPermission(entry, { id: item.id, employee: item.employee, toolName: item.toolName, input: item.input, decision: 'denied', by: 'deny-list', reason: why });
        persist(entry);
        return;
      }
      const rule = (entry.meta.rules || []).find(r => ruleMatches(r, { toolName: req.toolName, input: req.input }));
      if (rule) {
        answer(entry, item, true);
        logPermission(entry, { id: item.id, employee: item.employee, toolName: item.toolName, input: item.input, decision: 'allowed', by: 'rule', rule });
        persist(entry);
        return;
      }
      entry.meta.permissions.push(item);
      persist(entry);
      scheduleDelivery(entry);
    });
  }

  function clipInput(input) {
    const out = {};
    for (const [k, v] of Object.entries(input || {})) out[k] = typeof v === 'string' && v.length > 4000 ? v.slice(0, 4000) + '…' : v;
    return out;
  }

  /** Requests are batched briefly, then sent to the Manager in one message. */
  function scheduleDelivery(entry) {
    if (entry.timers.perm) return;
    entry.timers.perm = setTimeout(() => {
      entry.timers.perm = null;
      locked(entry, async () => {
        const items = (entry.meta.permissions || []).filter(p => !p.sent);
        if (!items.length) return;
        const m = await ensureSession(entry, MANAGER_ID);
        if (!m.ok) { dropPermissions(entry, (p) => !p.sent, `the Manager is unavailable (${m.error})`); persist(entry); return; }
        const sent = agents.send(m.chatId, batchMessage(items, { bypass: managerModeOf(entry) === 'bypass' }));
        if (sent && sent.ok === false) return scheduleDelivery(entry);
        for (const p of items) p.sent = true;
        persist(entry);
      });
    }, permissionBatchMs);
    entry.timers.perm.unref?.();
  }

  /**
   * factory_permission, from the Manager's chat.
   * @param {string} chatId
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
        const item = entry.meta.permissions.find(p => p.id === id);
        if (!item) { errors.push(`${id || '(no id)'}: no such request is waiting`); continue; }
        const allow = d.allow === true || d.allow === 'true';
        const reason = String(d.reason || '').slice(0, 1000);
        if (allow && d.rule) {
          const c = checkRule(d.rule);
          if (!c.ok) { errors.push(`${id}: ${c.error} (the request itself was not answered)`); continue; }
          if (!entry.meta.rules.includes(c.rule)) { entry.meta.rules.push(c.rule); newRules.push(c.rule); }
        }
        const delivered = answer(entry, item, allow, allow ? undefined : denyMessage(reason));
        entry.meta.permissions = entry.meta.permissions.filter(p => p !== item);
        logPermission(entry, { id, employee: item.employee, toolName: item.toolName, input: item.input, decision: allow ? 'allowed' : 'denied', by: 'manager', reason, rule: allow && d.rule ? d.rule : undefined });
        results.push(`${id}: ${allow ? 'allowed' : 'denied'}${delivered ? '' : ' (that request was no longer waiting)'}`);
      }
      // A rule just granted answers anything else waiting that it covers.
      if (newRules.length) {
        for (const item of [...entry.meta.permissions]) {
          const rule = newRules.find(r => ruleMatches(r, item));
          if (!rule) continue;
          answer(entry, item, true);
          entry.meta.permissions = entry.meta.permissions.filter(p => p !== item);
          logPermission(entry, { id: item.id, employee: item.employee, toolName: item.toolName, decision: 'allowed', by: 'rule', rule });
          results.push(`${item.id}: allowed by your new rule ${rule}`);
        }
      }
      persist(entry);
      const left = entry.meta.permissions.map(p => p.id);
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
      if (managerModeOf(entry) === mode) return { ok: true, run: summary(entry) };
      const emp = empOf(entry, MANAGER_ID);
      if (mode === 'normal') {
        entry.meta.managerMode = 'normal';
        if (emp.chatId && agents.isAlive(emp.chatId)) {
          const r = agents.setPermissionMode(emp.chatId, 'default');
          if (r && r.ok === false) return { ok: false, error: r.error };
          agents.send(emp.chatId, '[Factory] The person turned bypass permissions off. Your own actions now ask them first. You still decide your team\'s permission requests.');
        }
        persist(entry);
        return { ok: true, run: summary(entry) };
      }
      if (emp.chatId && agents.isAlive(emp.chatId) && agents.state(emp.chatId) !== 'idle') {
        return { ok: false, error: 'The Manager is in the middle of a turn. Switch to bypass when it has stopped.' };
      }
      entry.meta.managerMode = 'bypass';
      const sameId = emp.chatId;
      if (sameId) emp.sessionUuid = agents.sessionUuid(sameId) || emp.sessionUuid;
      const s = await ensureSession(entry, MANAGER_ID, { reuseId: sameId || '' });
      if (!s.ok) { entry.meta.managerMode = 'normal'; persist(entry); return { ok: false, error: s.error }; }
      if (emp.busy) agents.send(s.chatId, '[Factory] You were relaunched with bypass permissions: the person trusts you. You still owe a handoff for the step above; carry on.');
      persist(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  // ─── Clock ───

  function startClock(entry) {
    clearClock(entry);
    const min = entry.state.snapshot.settings.caps.maxMinutes;
    entry.timers.clock = setTimeout(() => locked(entry, () => {
      if (!UNFINISHED.has(entry.state.status) || entry.state.status === 'finishing') return;
      entry.meta.capHit = { which: 'maxMinutes', limit: min, at: now() };
      entry.state.status = 'waiting';
      persist(entry);
      notify('cap', summary(entry), `Factory "${entry.state.snapshot.name}" has run for ${min} minutes and is waiting for you.`);
    }), min * 60_000);
    entry.timers.clock.unref?.();
  }

  function clearClock(entry) { clearTimeout(entry.timers.clock); }

  // ─── Calls from outside ───

  /**
   * factory_handoff, from the chat that called it. Commits the step, routes
   * it, and answers the employee: where the work went, or what is missing.
   */
  async function handoff(chatId, payload) {
    const info = chatInfo(chatId);
    if (!info) return { ok: false, error: 'This chat is not part of a factory run, so there is nothing to hand off.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    return locked(entry, async () => {
      const emp = entry.meta.employees[info.employeeId];
      if (!UNFINISHED.has(entry.state.status) || entry.state.status === 'crashed') return { ok: false, error: `The run is ${entry.state.status}; nothing is waiting on you.` };
      // The Manager may be asked several things (answers from parallel
      // branches); the one it is on is emp.busy, else the oldest waiting.
      let tokenId = emp && emp.busy;
      if (!tokenId) {
        const mine = Engine.tokensFor(entry.state, info.employeeId);
        tokenId = mine[0]?.id;
      }
      if (!tokenId) return { ok: false, error: 'Nothing is waiting on you right now. Stop here and wait.' };
      const tok = entry.state.tokens.find(t => t.id === tokenId);
      const node = entry.state.snapshot.nodes.find(n => n.id === tok?.nodeId);
      const firstLine = shortSubject(payload?.summary || payload?.escalate || '');
      const c = await git.commitAll(wsPath(entry, tok?.ws), `factory: ${node?.label || node?.type || 'step'}${firstLine ? ' — ' + firstLine : ''}`);
      const caps = { maxHandoffs: entry.state.snapshot.settings.caps.maxHandoffs + (entry.meta.capExtra || 0) };
      const res = Engine.handoff(entry.state, tokenId, payload, { treeHash: c.ok ? c.tree : '', caps });
      if (!res.ok) return { ok: false, error: res.error };
      entry.state = res.run;
      applyHolds(entry);
      if (emp && emp.busy === tokenId) emp.busy = null;
      if (emp && emp.chatId) emp.sessionUuid = agents.sessionUuid(emp.chatId) || emp.sessionUuid;
      appendHandoff(entry, { type: 'handoff', tokenId, by: info.employeeId, nodeId: node?.id, payload, commit: c.ok ? c.head : '' });
      persist(entry);
      const where = describeWhere(entry, res.effects);
      // The next steps are under way before the caller hears back, so the
      // tool's answer can say truthfully where the work went.
      await runEffects(entry, res.effects);
      await dequeue(entry, info.employeeId);
      if (payload?.finish) return { ok: true, message: 'The run is finished. Now present the result to the person in this chat: what was done and why, what changed, and how they can look at it.' };
      return { ok: true, message: `Handed off${where ? ': ' + where : ''}. Your step is over. Stop here and wait; do not reply at length.` };
    });
  }

  function describeWhere(entry, effects) {
    const roster = new Map(entry.state.snapshot.roster.map(r => [r.id, r.name]));
    const parts = [];
    for (const e of effects) {
      if (e.type === 'activate') parts.push(e.mode === 'answer' ? `${roster.get(e.employeeId)} is asked to decide` : `${roster.get(e.employeeId)} takes over`);
      if (e.type === 'ask') parts.push('the person is asked');
      if (e.type === 'cap') parts.push('the run has reached its handoff limit and waits for the person');
    }
    if (!parts.length && entry.state.tokens.some(t => t.status === 'join')) parts.push('waiting for the other branches');
    return parts.join('; ');
  }

  async function askReply(runId, tokenId, replyId, note = '') {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      const res = Engine.askReply(entry.state, tokenId, replyId, note);
      if (!res.ok) return res;
      entry.state = res.run;
      applyHolds(entry);
      entry.meta.asks = entry.meta.asks.filter(a => a.tokenId !== tokenId);
      appendHandoff(entry, { type: 'ask-reply', tokenId, replyId, note });
      persist(entry);
      await runEffects(entry, res.effects);
      return { ok: true, run: summary(entry) };
    });
  }

  /**
   * Carry on after a pause or a cap. A cap grants another maxHandoffs and
   * restarts the clock. Held steps start; anyone whose turn stopped while the
   * run was held (the usage limit cuts turns short) is told to continue.
   */
  async function resume(runId, { reason = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, () => resumeLocked(entry, reason));
  }

  async function resumeLocked(entry, reason = '') {
    if (!entry.meta.capHit && !entry.meta.paused) return { ok: false, error: 'This run is not paused or held.' };
    if (entry.meta.capHit && entry.meta.capHit.which === 'maxHandoffs') entry.meta.capExtra = (entry.meta.capExtra || 0) + entry.state.snapshot.settings.caps.maxHandoffs;
    const why = reason || (entry.meta.paused?.by === 'usage' ? 'the usage limit was reached; it has reset now' : entry.meta.paused ? 'the person paused the run; it is resumed now' : 'the run reached a limit; the person let it carry on');
    entry.meta.capHit = null;
    entry.meta.paused = null;
    const heldSteps = entry.meta.held; entry.meta.held = [];
    entry.state.status = 'running';
    startClock(entry);
    persist(entry);
    // Turns that stopped while held: same continue prompt as a recovery.
    for (const [eid, emp] of Object.entries(entry.meta.employees)) {
      if (!emp.busy || !emp.chatId || !agents.isAlive(emp.chatId)) continue;
      if (agents.state(emp.chatId) !== 'idle') continue;
      const ef = activationFor(entry, emp.busy);
      if (ef) await startStep(entry, emp, ef, { recovery: await recoveryFor(entry, eid, ef, why, '') });
    }
    await runEffects(entry, heldSteps);
    return { ok: true, run: summary(entry) };
  }

  /**
   * Pause: nobody new is started; employees finish the turn they are in and
   * their handoffs are routed, but what follows is held until resume().
   */
  async function pause(runId, { by = 'person', resetsAt = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (!['running', 'waiting'].includes(entry.state.status)) return { ok: false, error: `The run is ${entry.state.status}; it cannot be paused.` };
      entry.meta.paused = { by, at: now(), ...(resetsAt ? { resetsAt } : {}) };
      entry.state.status = 'paused';
      clearClock(entry);
      persist(entry);
      if (by === 'usage') notify('paused', summary(entry), `Factory "${entry.state.snapshot.name}" paused: the usage limit is reached.${resetsAt ? ` It carries on by itself after ${resetsAt}.` : ''}`);
      return { ok: true, run: summary(entry) };
    });
  }

  // ─── Recovery ───

  /** Rebuild the activation a waiting token stands for. */
  function activationFor(entry, tokenId) {
    const t = entry.state.tokens.find(x => x.id === tokenId);
    if (!t || (t.status !== 'work' && t.status !== 'answer')) return null;
    const node = entry.state.snapshot.nodes.find(n => n.id === t.nodeId);
    return {
      type: 'activate', tokenId: t.id, nodeId: t.nodeId,
      employeeId: t.status === 'answer' ? t.answerer : Engine.rosterOf(node),
      mode: t.status, ws: t.ws, context: { from: null, summary: '', message: '', notes: [], escalation: '' },
    };
  }

  async function recoveryFor(entry, employeeId, ef, reason, note) {
    const last = [...entry.state.history].reverse().find(h => h.type === 'handoff' && h.by === employeeId);
    const ws = wsPath(entry, ef.ws);
    let w = { status: '', diffStat: '' };
    try { if (ws && existsSync(ws)) w = await git.workSummary(ws, entry.meta.baseCommit); } catch { /* best effort */ }
    return { reason, lastAction: last ? last.summary : '', status: w.status, diffStat: w.diffStat, note };
  }

  /**
   * Pick a crashed run up again. Unfinished effects are completed, the
   * Manager is relaunched on its conversation and given an overview, and
   * everyone who was mid-step gets a continue prompt with why it stopped,
   * where they were, their last handoff, the worktree now, and the person's note.
   */
  async function recover(runId, { note = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (entry.state.status !== 'crashed') return { ok: false, error: `The run is ${entry.state.status}, not crashed.` };
      if (!entry.meta.worktree || !existsSync(entry.meta.worktree)) return { ok: false, error: 'The run\'s worktree is gone, so it cannot be recovered. Stop the run instead.' };
      const reason = entry.meta.crashReason || 'it was interrupted';
      const from = entry.meta.crashedFrom || 'running';
      const text = String(note || '').slice(0, 5000).trim();

      // Old chats are gone; their queues are rebuilt from the tokens.
      const oldManagerChat = entry.meta.employees[MANAGER_ID]?.chatId || entry.meta.lastManagerChat || '';
      for (const emp of Object.values(entry.meta.employees)) { if (emp.chatId) chats.delete(emp.chatId); emp.chatId = ''; emp.busy = null; emp.queue = []; emp.nudged = false; }
      // Branch worktrees that vanished: their branches carry on in main.
      for (const [id, ws] of Object.entries(entry.meta.ws)) {
        if (id !== 'main' && ws.path && !existsSync(ws.path)) entry.meta.ws[id] = { ...entry.meta.ws.main, parent: ws.parent, shared: true };
      }
      entry.state.status = ['running', 'waiting', 'paused', 'finishing'].includes(from) ? from : 'running';
      if (entry.state.status !== 'paused') entry.meta.paused = null;
      delete entry.meta.crashReason; delete entry.meta.crashedAt; delete entry.meta.crashedFrom;
      entry.meta.error = '';
      appendHandoff(entry, { type: 'recover', reason, note: text });
      entry.state.history.push({ seq: ++entry.state.seq, type: 'recover', reason, note: text });

      const m = await ensureSession(entry, MANAGER_ID, { reuseId: oldManagerChat });
      if (!m.ok) {
        entry.state.status = 'crashed'; entry.meta.crashReason = reason; entry.meta.crashedFrom = from;
        persist(entry);
        return { ok: false, error: `Could not start the Manager again: ${m.error}` };
      }

      // Finish what was half done (not the activations: those are rebuilt below).
      const pending = (entry.meta.pending || []).filter(e => e.type !== 'activate');
      entry.meta.pending = [];
      if (pending.length) await runEffects(entry, pending);

      const roster = new Map(entry.state.snapshot.roster.map(r => [r.id, r.name]));
      const nodes = new Map(entry.state.snapshot.nodes.map(n => [n.id, n]));
      const waiting = entry.state.tokens.filter(t => t.status === 'work' || t.status === 'answer');
      const describe = (t) => {
        const n = nodes.get(t.nodeId);
        if (t.status === 'answer') return { employee: roster.get(t.answerer), what: `was asked "${n?.label || n?.decision?.question || ''}"` };
        if (t.status === 'ask') return { employee: '', what: `the person was asked: ${n?.ask?.prompt || ''}` };
        if (t.status === 'join') return { employee: '', what: `a branch was waiting at "${n?.label || 'Join'}"` };
        return { employee: roster.get(Engine.rosterOf(n)), what: `was working on "${n?.label || n?.type}"` };
      };
      let w = { status: '', diffStat: '' };
      try { w = await git.workSummary(entry.meta.worktree, entry.meta.baseCommit); } catch { /* best effort */ }
      const recent = entry.state.history.filter(h => h.type === 'handoff').slice(-5).map(h => `${roster.get(h.by) || h.by}: ${String(h.summary || '').split('\n')[0].slice(0, 200)}`);
      const dropped = (entry.meta.droppedPermissions || []).map(p => `${p.employee} wanted ${p.toolName} ${JSON.stringify(p.input).slice(0, 160)}`);
      entry.meta.droppedPermissions = [];
      agents.send(m.chatId, recoveryOverview({
        run: entry.state, reason, tokens: entry.state.tokens.map(describe), recent, dropped,
        status: w.status, diffStat: w.diffStat, note: text, wasFinishing: from === 'finishing',
      }));

      if (from !== 'finishing') {
        for (const t of waiting) {
          const ef = activationFor(entry, t.id);
          if (!ef) continue;
          if (held(entry)) { entry.meta.held.push(ef); continue; }
          const emp = empOf(entry, ef.employeeId);
          if (emp.busy) { emp.queue.push(ef); continue; }
          await startStep(entry, emp, ef, { recovery: await recoveryFor(entry, ef.employeeId, ef, reason, text) });
        }
        for (const a of entry.meta.asks) notify('ask', summary(entry), `Factory "${entry.state.snapshot.name}" asks: ${a.prompt}`);
        startClock(entry);
      }
      collect(entry);
      persist(entry);
      notify('recovered', summary(entry), `Factory "${entry.state.snapshot.name}" was recovered.`);
      return { ok: true, run: summary(entry) };
    });
  }

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
      const relevant = [...runs.values()].filter(e => ['running', 'waiting'].includes(e.state.status) || e.meta.paused?.by === 'usage');
      if (!relevant.length) return;
      let u;
      try { u = await getUsage(); } catch { return; }
      if (!u || u.ok === false) return;
      const full = [u.fiveHour, u.week].filter(w => w && w.utilization >= 100);
      const resetsAt = full.map(w => w.resetsAt).filter(Boolean).sort().pop() || '';
      for (const e of relevant) {
        if (full.length && !e.meta.paused && ['running', 'waiting'].includes(e.state.status)) {
          await pause(e.state.id, { by: 'usage', resetsAt });
        } else if (!full.length && e.meta.paused?.by === 'usage') {
          const at = Date.parse(e.meta.paused.resetsAt || '');
          if (Number.isFinite(at) && Date.now() < at) continue;
          await resume(e.state.id);
          notify('resumed', summary(e), `Factory "${e.state.snapshot.name}" carries on: the usage limit has reset.`);
        }
      }
    };
    clearInterval(usageTimer);
    usageTimer = setInterval(tick, tickMs);
    usageTimer.unref?.();
    return { tick, stop: () => clearInterval(usageTimer) };
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
   * Still conservative within the loop: it follows what the graph allows, not
   * what was chosen, so a branch not picked this round stays alive.
   */
  function collect(entry) {
    if (!UNFINISHED.has(entry.state.status) || entry.state.status === 'crashed') return;
    const f = entry.state.snapshot;
    const s = entry.state;
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
    for (const [eid, emp] of Object.entries(entry.meta.employees)) {
      if (needed.has(eid) || !emp.chatId || emp.busy || (emp.queue || []).length) continue;
      if ((entry.meta.permissions || []).some(p => p.employeeId === eid)) continue;
      const chatId = emp.chatId;
      emp.sessionUuid = agents.sessionUuid(chatId) || emp.sessionUuid;
      unwatch(entry, chatId);
      chats.delete(chatId);
      try { agents.close(chatId); } catch { /* gone */ }
      emp.chatId = '';
      emp.retired = true;
      s.history.push({ seq: ++s.seq, type: 'retire', employeeId: eid });
    }
  }

  /**
   * Stop a run: every employee's session is closed, the Manager's chat is
   * left as an ordinary chat. The worktree is kept (cleanup decides).
   */
  async function stopRun(runId) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (!UNFINISHED.has(entry.state.status)) return { ok: false, error: `The run is already ${entry.state.status}.` };
      dropPermissions(entry, () => true, 'The run was stopped.');
      for (const [eid, emp] of Object.entries(entry.meta.employees)) {
        if (emp.chatId) {
          unwatch(entry, emp.chatId);
          chats.delete(emp.chatId);
          if (eid !== MANAGER_ID) { try { await agents.close(emp.chatId); } catch { /* gone */ } }
        }
        emp.busy = null; emp.queue = [];
        if (eid !== MANAGER_ID) emp.chatId = '';
      }
      clearClock(entry);
      entry.state.status = 'stopped';
      entry.meta.stoppedAt = now();
      persist(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  /** Stop timers (tests, shutdown). Runs stay on disk. */
  function dispose() {
    clearInterval(usageTimer);
    for (const e of runs.values()) {
      clearClock(e);
      clearTimeout(e.timers.perm);
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
      // A relaunch (bypass switched on) keeps the chat's id, so the person's
      // tab stays where it is.
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
