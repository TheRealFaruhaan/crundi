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
import { validateFactory, MANAGER_ID } from './factory-model.js';
import { systemPromptFor, stepMessage, nudgeMessage, cancelMessage } from './factory-prompts.js';

/** Runs in these states hold their project: no second run can start. */
export const UNFINISHED = new Set(['starting', 'running', 'waiting', 'paused', 'finishing', 'crashed']);

const SETTLE_MS = 20_000;
const SETUP_TIMEOUT_MS = 15 * 60 * 1000;
const SETUP_OUTPUT_MAX = 4000;

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
  settleMs = SETTLE_MS, setupTimeoutMs = SETUP_TIMEOUT_MS,
  now = () => new Date().toISOString(),
}) {
  const RUNS = join(dataDir, 'factory-runs');
  /** runId -> { state, meta, lock, timers, watchers } */
  const runs = new Map();
  /** chatId -> { runId, employeeId } */
  const chats = new Map();

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
        entry.state.status = 'crashed';
        for (const emp of Object.values(entry.meta.employees || {})) { emp.chatId = ''; }
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
      waitingOn: m.capHit ? 'cap' : (m.asks || []).length ? 'ask' : m.managerIdle ? 'manager' : '',
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
      employees: Object.fromEntries(Object.entries(m.employees || {}).map(([k, e]) => [k, { chatId: e.chatId, busy: e.busy || null, queued: (e.queue || []).length, name: roster.get(k)?.name || k }])),
      asks: m.asks || [],
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

  async function ensureSession(entry, employeeId, { fresh = false } = {}) {
    const emp = empOf(entry, employeeId);
    if (emp.chatId && !fresh && agents.isAlive(emp.chatId)) return { ok: true, chatId: emp.chatId };
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
      systemPromptExtra: systemPromptFor({ factory: f, employee: r, project: entry.state.project, paths: paths(entry, employeeId) }),
      model: r.model, effort: r.effort,
      allowedTools: r.tools.allow,
      disallowedTools: r.tools.deny,
      // Until card 3 routes permission requests through the Manager: in
      // bypass mode the whole team runs unprompted; otherwise edits are free
      // and anything else asks in that person's chat.
      skipPermissions: f.settings.managerMode === 'bypass',
      permissionMode: f.settings.managerMode === 'bypass' ? '' : 'acceptEdits',
      background: !isManager,
      resumeId: resume,
    });
    if (!res.ok) return { ok: false, error: res.error || 'could not start' };
    emp.chatId = res.id;
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
      if (!emp || emp.chatId !== chatId) return;
      emp.sessionUuid = emp.sessionUuid || '';
      emp.chatId = '';
      persist(entry);
      if (!UNFINISHED.has(entry.state.status) || entry.state.status === 'crashed') return;
      if (employeeId === MANAGER_ID) {
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

  async function runEffects(entry, effects) {
    // A cap holds new steps back until the person resumes.
    if (effects.some(e => e.type === 'cap')) {
      const cap = effects.find(e => e.type === 'cap');
      entry.meta.capHit = { which: cap.which, limit: cap.limit, at: now() };
      notify('cap', summary(entry), `Factory "${entry.state.snapshot.name}" reached its limit of ${cap.limit} handoffs and is waiting for you.`);
    }
    const skip = new Set();
    for (const ef of effects) {
      if (ef.tokenId && skip.has(ef.tokenId)) continue;
      if (entry.meta.capHit && ef.type === 'activate') { entry.meta.held.push(ef); continue; }
      try {
        switch (ef.type) {
          case 'activate': await activate(entry, ef); break;
          case 'workspace': await makeWorkspace(entry, ef); break;
          case 'merge': {
            const r = await mergeWorkspaces(entry, ef);
            if (!r.ok && !ef.abort) {
              skip.add(ef.contTokenId);
              await escalateToken(entry, ef.contTokenId, r.reason);
            }
            break;
          }
          case 'cancel': await cancelTokens(entry, ef); break;
          case 'ask':
            entry.meta.asks.push({ tokenId: ef.tokenId, nodeId: ef.nodeId, prompt: ef.prompt, replies: ef.replies, context: ef.context, at: now() });
            notify('ask', summary(entry), `Factory "${entry.state.snapshot.name}" asks: ${ef.prompt}`);
            break;
          case 'finish': await onFinish(entry, ef); break;
          case 'cap': break;
          default: break;
        }
      } catch (err) {
        entry.meta.error = `${ef.type}: ${err.message}`;
      }
    }
    persist(entry);
  }

  async function activate(entry, ef) {
    const emp = empOf(entry, ef.employeeId);
    if (emp.busy && emp.busy !== ef.tokenId) { emp.queue.push(ef); return; }
    await startStep(entry, emp, ef);
  }

  async function startStep(entry, emp, ef) {
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
    const msg = stepMessage({
      run: entry.state, effect: ef, req: Engine.requirements(entry.state, ef.tokenId),
      wsPath: wsPath(entry, ef.ws), notice,
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
    appendHandoff(entry, { type: 'escalate', tokenId, reason });
    persist(entry);
    await runEffects(entry, res.effects);
    if (fromEmployee) await dequeue(entry, fromEmployee);
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
      entry.meta.asks = entry.meta.asks.filter(a => a.tokenId !== tokenId);
      appendHandoff(entry, { type: 'ask-reply', tokenId, replyId, note });
      persist(entry);
      await runEffects(entry, res.effects);
      return { ok: true, run: summary(entry) };
    });
  }

  /** Carry on after a cap: grants another maxHandoffs and restarts the clock. */
  async function resume(runId) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (!entry.meta.capHit) return { ok: false, error: 'This run is not held.' };
      if (entry.meta.capHit.which === 'maxHandoffs') entry.meta.capExtra = (entry.meta.capExtra || 0) + entry.state.snapshot.settings.caps.maxHandoffs;
      entry.meta.capHit = null;
      const held = entry.meta.held; entry.meta.held = [];
      entry.state.status = 'running';
      startClock(entry);
      persist(entry);
      await runEffects(entry, held);
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
      if (!UNFINISHED.has(entry.state.status)) return { ok: false, error: `The run is already ${entry.state.status}.` };
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
    for (const e of runs.values()) {
      clearClock(e);
      for (const id of Object.keys(e.watchers)) unwatch(e, id);
    }
  }

  return { load, startRun, handoff, askReply, resume, stopRun, listRuns, getRun, activeRunFor, chatInfo, dispose };
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
      const id = randomBytes(8).toString('hex');
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
    on: (id, fn) => claudeUi.on(id, fn),
    off: (id, fn) => claudeUi.off(id, fn),
  };
}
