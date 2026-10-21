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

import { mkdirSync, writeFileSync, readFileSync, existsSync, readdirSync, renameSync, openSync, writeSync, fsyncSync, closeSync, rmdirSync, statSync, copyFileSync, realpathSync } from 'fs';
import { join, basename, resolve, sep } from 'path';
import { spawn } from 'child_process';
import { randomBytes, timingSafeEqual } from 'crypto';
import { fileURLToPath } from 'url';
import * as Engine from './factory-engine.js';
import * as Git from './factory-git.js';
import { validateFactory, normalizeFactory, normalizeLimits, MANAGER_ID, flowGraph, reachable } from './factory-model.js';
import { reduce, replay, asksOf, timeLeft, isHeld, UNFINISHED as UNFINISHED_SET, JOURNAL_VERSION } from './factory-journal.js';
import { systemPromptFor, stepMessage, nudgeMessage, cancelMessage, recoveryOverview } from './factory-prompts.js';
import { hardDeny, ruleMatches, checkRule, batchMessage, denyMessage, DENY_RULES, ALLOW_RULES, GATEKEEPER_ID, gatekeeperPrompt, startRules, scopedRules, rulesFor } from './factory-permissions.js';
import { missingSkills } from './factory-skills.js';
import { checkCr, crMarkdown, crBrief, crText } from './factory-cr.js';

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
 * @param {{ under:(root:string)=>string[], remove:(key:string)=>any }} [o.services]  preview services a run started, by folder
 * @param {(kind:string, run:object, text:string) => void} [o.notify]
 * @param {(run:object) => void} [o.onChange]
 */
export function createFactoryRunner({
  dataDir, agents, getProject, getFactory, git = Git, listSkills = () => [], services = null,
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
      live: { employees: {}, permissions: [], managerIdle: false, error: '', seats: [] },
      lock: Promise.resolve(), timers: {}, watchers: {}, waits: {}, draining: false, skills: null, autoTries: 0,
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
      trigger: r.trigger || null,
      cr: r.cr ? {
        title: r.cr.title,
        sections: s.snapshot.settings.crTemplate.filter(x => r.cr.sections[x.id]).map(x => ({ id: x.id, label: x.label, text: r.cr.sections[x.id] })),
        attachments: r.cr.attachments.map(a => ({ name: a.name })), file: r.cr.file,
      } : null,
      status: s.status,
      waitingOn: r.paused ? (r.paused.by === 'usage' ? 'usage' : 'paused') : r.capHit ? 'cap' : asks.length ? 'ask' : L.managerIdle ? 'manager' : '',
      paused: r.paused || null,
      capHit: r.capHit || null,
      crash: s.status === 'crashed' ? r.crash : null,
      createdAt: s.createdAt,
      updatedAt: s.updatedAt,
      handoffs: s.handoffs,
      worktree: r.worktree,
      root: r.root,
      repo: r.repo,
      branch: r.branch,
      parentBranch: r.parentBranch,
      baseCommit: r.baseCommit || '',
      policy: r.policy || '',
      dirtyAtStart: !!r.dirty,
      managerChatId: L.employees[MANAGER_ID]?.chatId || '',
      tokens: s.tokens.map(t => {
        const n = nodes.get(t.nodeId);
        const who = Engine.personOf(s, t);
        const wp = t.status === 'wait' ? (r.waits || {})[t.id] : null;
        const wl = wp ? entry.waits[t.id] : null;
        return {
          id: t.id, nodeId: t.nodeId, label: t.delegate ? 'Job from the Manager' : n?.label || n?.type || '', status: t.status, employeeId: who || '', employee: roster.get(who)?.name || '', ws: t.ws,
          ...(t.delegate ? { job: t.delegate.task } : {}),
          ...(who && (t.status === 'work' || t.status === 'answer') && L.permissions.some(p => p.employeeId === who) ? { blocked: true } : {}),
          ...(wp ? { wait: { kind: wp.kind, what: n?.wait?.what || '', startedAt: wp.startedAt, deadline: wp.deadline, timeoutAt: wp.timeoutAt, tries: wl?.tries || 0, last: (wl?.last || '').slice(-300), ...(wp.key ? { signalPath: `/api/factory-hook/wait/${s.id}/${t.id}?key=${wp.key}` } : {}) } } : {}),
        };
      }),
      employees: Object.fromEntries(s.snapshot.roster.map(x => {
        const e = L.employees[x.id] || {};
        let activity = null;
        if (e.chatId && agents.activity) { try { activity = agents.activity(e.chatId); } catch { activity = null; } }
        return [x.id, { chatId: e.chatId || '', busy: e.busy || null, queued: (e.queue || []).length, retired: !!r.retired[x.id], name: x.name, ...(activity ? { activity } : {}) }];
      })),
      liveSessions: Object.values(L.employees).filter(e => e.chatId).length,
      usage: usageOf(r),
      limits: limitsOf(entry),
      waitingForSeat: entry.live.seats.map(x => ({ tokenId: x.ef.tokenId, employeeId: x.ef.employeeId, employee: s.snapshot.roster.find(p => p.id === x.ef.employeeId)?.name || x.ef.employeeId })),
      findings: Engine.ledgerOf(s).items.map(i => ({ id: i.id, text: i.text, where: i.where, blocking: i.blocking, status: i.status, by: roster.get(i.by)?.name || i.by, reopened: i.reopened || 0 })),
      asks,
      managerMode: r.managerMode,
      teamMode: r.teamMode || 'ask',
      gatekeeper: !!s.snapshot.settings.gatekeeper?.enabled,
      permissions: L.permissions.map(p => ({ id: p.id, employee: p.employee, toolName: p.toolName, input: p.input, at: p.at, sent: p.sent })),
      rules: scopedRules(r.rules),
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

  async function startRun({ project, factoryId, task, cr, mergeMode, ignoreMissingSkills = false, trigger = null } = {}) {
    const key = String(project || '').toLowerCase();
    const proj = getProject(key);
    if (!proj) return { ok: false, error: `Project "${project}" not found.` };
    const existing = activeRunFor(key);
    if (existing || claiming.has(key)) {
      return { ok: false, busy: true, runId: existing?.id || '', error: existing ? `A factory is already ${existing.status === 'crashed' ? 'waiting to be recovered' : 'running'} in this project ("${existing.factoryName}"). Finish or stop it first.` : 'A factory is being started in this project.' };
    }
    const got = getFactory(factoryId);
    if (!got) return { ok: false, error: 'Factory not found.' };
    // A change request (the start form), or a plain task (older callers).
    let crIn = null;
    if (cr) {
      const c = checkCr(cr, normalizeFactory(got.factory).settings.crTemplate, { lenient: !!trigger });
      if (!c.ok) return { ok: false, field: c.field, error: c.error };
      crIn = c.cr;
      // Attachments come from the project's upload folder (the chat's paperclip), nowhere else.
      const upDir = join(proj.path, 'crundi_attachments');
      for (const a of crIn.attachments) {
        let real = '';
        try { real = realpathSync(a.path); } catch { return { ok: false, field: 'attachments', error: `The attachment "${a.name}" is gone; attach it again.` }; }
        let base = upDir; try { base = realpathSync(upDir); } catch { /* none yet */ }
        if (!real.startsWith(base + sep)) return { ok: false, field: 'attachments', error: `"${a.name}" is not an uploaded attachment.` };
        a.path = real;
      }
    }
    const taskText = crIn ? crIn.title : String(task || '').trim();
    if (!taskText) return { ok: false, error: 'Give the factory a task.' };
    const v = validateFactory(got.factory);
    if (!v.ok) return { ok: false, invalid: true, errors: v.errors, error: `This factory cannot run yet: ${v.errors.length} problem${v.errors.length === 1 ? '' : 's'} to fix in the editor.` };
    let known = [];
    try { known = listSkills(key) || []; } catch { known = []; }
    const factory = normalizeFactory(got.factory);
    const missing = missingSkills(factory, crIn ? crText(crIn) : taskText, new Set(known.map(x => x.name)));
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
      // The change request goes in the shared folder, with copies of its
      // attachments, where every member of the team can read it.
      let crRec = null;
      if (crIn) {
        const dir = join(runDir(id), 'shared', 'change-request');
        mkdirSync(dir, { recursive: true });
        const attachments = crIn.attachments.map((a, i) => {
          const name = (i + 1) + '-' + basename(a.name).replace(/[^\w.\-]+/g, '_').slice(-80);
          copyFileSync(a.path, join(dir, name));
          return { name: a.name, path: join(dir, name) };
        });
        crRec = { ...crIn, attachments, file: join(runDir(id), 'shared', 'CHANGE-REQUEST.md') };
        writeFileSync(crRec.file, crMarkdown(crRec, factory.settings.crTemplate));
      }

      const entry = newEntry(null);
      commit(entry, {
        type: 'start', id, project: key, factory, task: taskText, cr: crRec, trigger: trigger || null, factoryId, factoryVersion: got.factory.version,
        mergeMode: mergeMode === 'auto' || mergeMode === 'ask' ? mergeMode : '',
        wt: { repo: wt.repo, root: wt.root, path: wt.path, branch: wt.branch, parentBranch: wt.parentBranch, baseCommit: wt.baseCommit, dirty: wt.dirty },
        skillExcludes, missingSkills: missing,
        rules: startRules(factory.settings, wt.repo, factory.roster),
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
      // A run starting while usage is already high must not wait a minute
      // for the one-at-a-time rule: read it now (briefly; best effort).
      if (usageSource) {
        try { const u = await Promise.race([usageSource(), new Promise(res => setTimeout(() => res(null), 3000))]); if (u && u.ok !== false) usageNow = u; } catch { /* keep the last reading */ }
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

  /** The Gatekeeper is not on the roster: it is a setting of the factory. */
  function gatekeeperEntry(f) {
    return { id: GATEKEEPER_ID, name: 'Gatekeeper', persona: '', model: f.settings.gatekeeper?.model || 'haiku', effort: '', memory: 'keep',
      tools: { allow: [], deny: ['Edit', 'Write', 'MultiEdit', 'NotebookEdit', 'Bash'] }, skills: [] };
  }

  async function ensureSession(entry, employeeId, { fresh = false, reuseId = '' } = {}) {
    const emp = empOf(entry, employeeId);
    const f = st(entry).snapshot;
    const isManager = employeeId === MANAGER_ID;
    const isGate = employeeId === GATEKEEPER_ID;
    // Bypass is a launch flag: a session started under the other team mode is
    // relaunched on its conversation the next time it is needed.
    const wantSkip = isManager ? rr(entry).managerMode === 'bypass' : !isGate && rr(entry).teamMode === 'bypass';
    if (emp.chatId && !fresh && !reuseId && agents.isAlive(emp.chatId) && (isManager || emp.skip === wantSkip)) return { ok: true, chatId: emp.chatId };
    if (emp.chatId) {
      const uuid = agents.sessionUuid(emp.chatId);
      if (uuid && !fresh) commit(entry, { type: 'session', employeeId, chatId: emp.chatId, sessionUuid: uuid });
      unwatch(entry, emp.chatId); try { await agents.close(emp.chatId); } catch { /* gone */ } chats.delete(emp.chatId); emp.chatId = '';
    }
    const r = isGate ? gatekeeperEntry(f) : f.roster.find(x => x.id === employeeId);
    if (!r) return { ok: false, error: `Nobody called "${employeeId}" is on the roster.` };
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
      systemPromptExtra: isGate
        ? gatekeeperPrompt({ factory: f, task: rr(entry).cr ? crBrief(rr(entry).cr, f.settings.crTemplate) : st(entry).task, project: st(entry).project, policy: rr(entry).policy, team: f.roster })
        : systemPromptFor({ factory: f, employee: r, project: st(entry).project, paths: paths(entry, employeeId), managerMode, teamMode: rr(entry).teamMode, skills: skillsOf(entry), rules: rulesFor(rr(entry).rules, employeeId) }),
      model: r.model, effort: r.effort,
      // The factory's own tools, and what the team may do from the start
      // (presets, the project's scripts, hand-written rules), never ask.
      allowedTools: [...r.tools.allow, ...ALLOW_RULES, ...(isGate ? [] : rulesFor(rr(entry).rules, employeeId))],
      // The fixed deny list rides on every chat: it is all that binds a
      // Manager in bypass mode, which never asks.
      disallowedTools: [...r.tools.deny, ...DENY_RULES],
      // Employees: edits inside the run's folders are free (acceptEdits with
      // --add-dir), anything else asks, and asking goes to the Manager.
      // The Manager: bypass if the person chose it, else its own actions ask
      // the person in its chat as usual.
      skipPermissions: wantSkip,
      permissionMode: isManager ? '' : 'acceptEdits',
      background: !isManager,
      resumeId: resume,
      // The Manager keeps its chat id across relaunches and recoveries.
      id: reuseId || (isManager ? known.chatId || '' : ''),
    });
    if (!res.ok) return { ok: false, error: res.error || 'could not start' };
    emp.chatId = res.id;
    emp.skillsSeen = 0;
    emp.skip = wantSkip;
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
      if (ev.type === 'exit') { clearTimeout(timer); recordUsage(entry, chatId, employeeId); return onExit(entry, chatId, employeeId); }
      if (ev.type !== 'state') return;
      if (ev.state === 'idle') recordUsage(entry, chatId, employeeId);
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

  /** What a chat has cost so far, into the journal when it changed (each turn's end). */
  function recordUsage(entry, chatId, employeeId) {
    if (!agents.usage) return;
    let u;
    try { u = agents.usage(chatId); } catch { return; }
    if (!u) return;
    const prev = (rr(entry).usage || {})[chatId];
    if (prev && prev.costUsd === u.costUsd && prev.turns === u.turns) return;
    commit(entry, { type: 'usage', chatId, employeeId, costUsd: u.costUsd, turns: u.turns, durationMs: u.durationMs || 0 });
  }

  function unwatch(entry, chatId) {
    const w = entry.watchers[chatId];
    if (!w) return;
    const eid = chats.get(chatId)?.employeeId;
    if (eid) recordUsage(entry, chatId, eid);
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
    await releaseSeats(entry);
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
        case 'wait': result = waitPlan(ef); after = () => armWait(entry, ef.tokenId); break;
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
    // The person's limit on people working at once: wait for a free seat.
    if (ef.employeeId !== MANAGER_ID && !emp.busy && seatsFull(entry)) {
      if (!entry.live.seats.some(x => x.ef.tokenId === ef.tokenId)) entry.live.seats.push({ ef, recovery });
      writeCache(entry);
      return;
    }
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
      context: await runContextFor(entry, ef),
      cr: crFor(entry, ef, !!recovery),
    });
    const sent = agents.send(s.chatId, msg);
    if (sent && sent.ok === false) entry.live.error = `Could not message ${r?.name || ef.employeeId}: ${sent.error}`;
    else if (notice) commit(entry, { type: 'notice-clear', tokenId: ef.tokenId });
  }

  // ─── Usage limits (the person's choice, per factory, changeable per run) ───

  /** The limits in force for a run: changed on the run, else the factory's. */
  function limitsOf(entry) {
    return normalizeLimits(rr(entry).limits || st(entry).snapshot.settings.limits);
  }

  /** How many people may work at once right now (Infinity: no limit). */
  function seatLimit(entry) {
    const L = limitsOf(entry);
    let n = L.maxParallel > 0 ? L.maxParallel : Infinity;
    const five = usageNow?.fiveHour?.utilization;
    if (L.oneAtATimeAbove > 0 && Number.isFinite(five) && five >= L.oneAtATimeAbove) n = Math.min(n, 1);
    return n;
  }

  function busyPeople(entry) {
    return Object.entries(entry.live.employees).filter(([id, e]) => id !== MANAGER_ID && id !== GATEKEEPER_ID && e.busy).length;
  }

  function seatsFull(entry) { return busyPeople(entry) >= seatLimit(entry); }

  /** Start whoever waits for a seat, as far as the limit allows. */
  async function releaseSeats(entry) {
    if (isHeld(entry.rec) || !UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') return;
    while (entry.live.seats.length && !seatsFull(entry)) {
      const { ef, recovery } = entry.live.seats.shift();
      const t = st(entry).tokens.find(x => x.id === ef.tokenId);
      if (!t || (t.status !== 'work' && t.status !== 'answer')) continue;
      await startStep(entry, empOf(entry, ef.employeeId), ef, { recovery });
    }
    writeCache(entry);
  }

  /** Change a running run's limits (the Details drawer, or the Manager on the person's word). */
  async function setLimits(runId, limits) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      const next = normalizeLimits({ ...limitsOf(entry), ...(limits || {}) });
      commit(entry, { type: 'limits', limits: next });
      await releaseSeats(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  /**
   * The change request as a step carries it: in full for the Manager's first
   * step (and after a recovery), as a brief with the file's path otherwise.
   */
  function crFor(entry, ef, recovering) {
    const cr = rr(entry).cr;
    if (!cr) return null;
    const t = st(entry).snapshot.settings.crTemplate;
    const first = ef.employeeId === MANAGER_ID && ef.mode === 'work' && !st(entry).history.some(h => h.type === 'handoff');
    return first || (recovering && ef.employeeId === MANAGER_ID) ? { full: true, text: crMarkdown(cr, t) } : { full: false, text: crBrief(cr, t, cr.file) };
  }

  /** A small port range per workspace, so parallel dev servers do not collide. */
  function portRange(entry, wsId) {
    let h = 0;
    for (const ch of st(entry).id + '|' + wsId) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
    const base = 4300 + (h % 60) * 10;   // 4300-4899, clear of 8888 and the low ports
    return [base, base + 9];
  }

  /**
   * What a person needs to orient: where the run started and what it changed,
   * what changed since their own last step, the shared folder, the review
   * findings, where to run commands and which ports to use.
   */
  async function runContextFor(entry, ef) {
    const r = rr(entry), s = st(entry);
    const ws = wsPath(entry, ef.ws);
    const clip = (t, n) => t.split('\n').filter(Boolean).slice(-n).join('\n');
    let diffStat = '', since = '', sinceStat = '';
    try {
      const d = await git.git(['diff', '--stat=100', `${r.baseCommit}..HEAD`, '--', '.', ':(exclude).claude/skills'], ws);
      diffStat = clip(d.stdout, 25);
      const last = [...s.history].reverse().find(h => (h.type === 'handoff' || h.type === 'delegate-done') && h.by === ef.employeeId && h.commit);
      if (last) {
        since = last.commit;
        const d2 = await git.git(['diff', '--stat=100', `${last.commit}..HEAD`, '--', '.', ':(exclude).claude/skills'], ws);
        sinceStat = clip(d2.stdout, 25) || '(nothing changed since)';
      }
    } catch { /* best effort */ }
    let shared = [];
    try {
      const dir = join(runDir(s.id), 'shared');
      shared = readdirSync(dir).filter(n => !n.startsWith('.')).slice(0, 20).map(n => ({ name: n, at: statSync(join(dir, n)).mtimeMs }));
    } catch { shared = []; }
    return {
      base: r.baseCommit, parentBranch: r.parentBranch, diffStat, since, sinceStat, shared,
      cd: ws, branchStep: ef.ws !== 'main' && ws !== r.worktree, ports: portRange(entry, ef.ws),
      ledger: Engine.ledgerOf(s).items,
    };
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
    if (rr(entry).mergeMode === 'auto' && rr(entry).mergeAttempts < 2) await doMerge(entry, { auto: true });
    else await present(entry, { state: 'ask' });
  }

  // ─── The result: present, merge, keep, discard, undo ───

  async function diffOf(entry) {
    try {
      await git.commitAll(rr(entry).worktree, 'factory: final', excl(entry));
      return await git.diffSummary(rr(entry).worktree, rr(entry).baseCommit);
    } catch { return { stat: '', files: [], added: 0, removed: 0 }; }
  }

  /** The card in the Manager's chat: what was done, and the buttons. */
  async function present(entry, { state, error = '', diff = null } = {}) {
    const d = diff || await diffOf(entry);
    const card = {
      kind: 'factory-result', runId: st(entry).id, state, error,
      factory: st(entry).snapshot.name, task: st(entry).task,
      summary: st(entry).outcome?.summary || '',
      branch: rr(entry).branch, parentBranch: rr(entry).parentBranch,
      commit: rr(entry).result.commit || '',
      auto: rr(entry).mergeMode === 'auto',
      diff: { added: d.added, removed: d.removed, count: d.files.length, files: d.files.slice(0, 40) },
      // Review findings not dealt with: nits, and anything the Manager let through.
      followUps: Engine.ledgerOf(st(entry)).items.filter(i => i.status === 'open' || i.status === 'claimed').map(i => ({ id: i.id, text: i.text, where: i.where, blocking: i.blocking })),
      // What it took: measured from the run's own chats.
      usage: (() => { const u = usageOf(rr(entry)); return { costUsd: u.costUsd, turns: u.turns, chats: u.chats, activeMs: u.activeMs, handoffs: st(entry).handoffs }; })(),
    };
    const chatId = entry.live.employees[MANAGER_ID]?.chatId;
    if (!chatId || !agents.addCard) return;
    const old = rr(entry).result.card;
    if (old && agents.patchCard) {
      const r = agents.patchCard(chatId, old, card);
      if (r && r.ok !== false) return;
    }
    const r = agents.addCard(chatId, card);
    if (r && r.ok && r.entryId) commit(entry, { type: 'card', entryId: r.entryId });
  }

  function commitMessage(entry) {
    const s = st(entry);
    const summary = String(s.outcome?.summary || '').trim();
    const subject = shortSubject(summary) || shortSubject(s.task) || `Factory ${s.snapshot.name}`;
    return `${subject}\n\n${summary || s.task}\n\nMade by the Crundi factory "${s.snapshot.name}" (run ${s.id}).`;
  }

  /** Remove the run's worktrees (and branches unless kept) and its preview services. */
  async function cleanup(entry, { keepBranch = false } = {}) {
    const r = rr(entry);
    if (services && r.root) {
      try { for (const key of services.under(r.root)) await services.remove(key); } catch { /* best effort */ }
    }
    for (const [id, w] of Object.entries(r.ws)) {
      if (id === 'main' || w.shared || !w.path) continue;
      try { await git.removeWorktree({ repo: r.repo, path: w.path, branch: w.branch, deleteBranch: true }); } catch { /* gone */ }
    }
    try { await git.removeWorktree({ repo: r.repo, path: r.worktree, branch: r.branch, deleteBranch: !keepBranch }); } catch { /* gone */ }
    // The run's own folder, now empty (rmdir refuses one that is not).
    if (r.root) { try { rmdirSync(r.root); } catch { /* not empty, or gone */ } }
  }

  /**
   * Squash the run onto its parent branch. On a conflict the Manager is sent
   * back to bring the parent in and resolve it (automatic mode tries that
   * twice before asking the person); if the person's checkout is in the way,
   * the card says so and Merge can be pressed again.
   */
  async function doMerge(entry, { auto = false } = {}) {
    const diff = await diffOf(entry);
    const r = rr(entry);
    const res = await git.squashMerge({ repo: r.repo, parentBranch: r.parentBranch, branch: r.branch, message: commitMessage(entry) });
    if (res.ok) {
      await cleanup(entry);
      commit(entry, { type: 'merged', commit: res.commit, before: res.before, after: res.after, message: commitMessage(entry) });
      await present(entry, { state: 'merged', diff });
      notify('merged', summary(entry), `Factory "${st(entry).snapshot.name}" was merged into ${r.parentBranch}${auto ? ' automatically' : ''} (${res.commit.slice(0, 7)}).`);
      return { ok: true, run: summary(entry) };
    }
    if (res.conflict) {
      const reason = `Merging the result into ${r.parentBranch} conflicts in ${res.files.join(', ') || 'some files'}: ${r.parentBranch} has moved on. In ${r.worktree}, merge ${r.parentBranch} into your branch (git merge ${r.parentBranch}), resolve the conflicts or have your team do it, then finish again.`;
      if (auto && r.mergeAttempts >= 1) {
        await present(entry, { state: 'ask', error: res.error + ' Automatic merging gave up; choose what to do.', diff });
        return { ok: false, error: res.error };
      }
      // Ask mode: the Manager needs git to do this, which it is otherwise told not to touch.
      commit(entry, { type: 'reopen', reason, asMessage: false, auto });
      await present(entry, { state: 'conflict', error: res.error, diff });
      await drain(entry);
      return { ok: false, conflict: true, error: res.error };
    }
    await present(entry, { state: 'ask', error: res.error, diff });
    return { ok: false, blocked: !!res.blocked, empty: !!res.empty, error: res.error };
  }

  /**
   * The person's choice on the result card.
   * @param {'merge'|'changes'|'keep'|'discard'|'undo'} action
   */
  async function finishAction(runId, action, { note = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      const s = st(entry), r = rr(entry);
      if (action === 'undo') {
        if (r.result.how !== 'merged') return { ok: false, error: 'Only a merged result can be undone.' };
        const u = await git.undoMerge({ repo: r.repo, parentBranch: r.parentBranch, before: r.result.before, after: r.result.after, restoreBranch: r.branch });
        if (!u.ok) return { ok: false, error: u.error };
        commit(entry, { type: 'undone' });
        await present(entry, { state: 'undone' });
        return { ok: true, run: summary(entry) };
      }
      if (s.status !== 'finishing') return { ok: false, error: `The run is ${s.status}; there is no result waiting for a decision.` };
      if (action === 'merge') return doMerge(entry, { auto: false });
      if (action === 'changes') {
        const text = String(note || '').trim();
        if (!text) return { ok: false, error: 'Say what should change.' };
        commit(entry, { type: 'reopen', reason: `The person looked at the result and asked for changes: ${text.slice(0, 5000)}` });
        await present(entry, { state: 'changes', error: '' });
        startClock(entry);
        await drain(entry);
        return { ok: true, run: summary(entry) };
      }
      if (action === 'keep') {
        const diff = await diffOf(entry);
        await cleanup(entry, { keepBranch: true });
        commit(entry, { type: 'kept' });
        await present(entry, { state: 'kept', diff });
        return { ok: true, run: summary(entry) };
      }
      if (action === 'discard') {
        const diff = await diffOf(entry);
        await cleanup(entry, { keepBranch: false });
        commit(entry, { type: 'discarded' });
        await present(entry, { state: 'discarded', diff });
        return { ok: true, run: summary(entry) };
      }
      return { ok: false, error: `Unknown action "${action}".` };
    });
  }

  /** The full diff of a run, for the person to read before deciding. */
  async function runDiff(runId) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    const r = rr(entry);
    if (!r.worktree || !existsSync(r.worktree)) return { ok: false, error: 'The run\'s worktree is gone.' };
    const d = await git.fullDiff(r.worktree, r.baseCommit);
    return { ok: true, ...d };
  }

  async function dequeue(entry, employeeId) {
    const emp = entry.live.employees[employeeId];
    if (emp && !emp.busy && !emp.queue.length) { await releaseSeats(entry); return; }
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
      const rule = rulesFor(rr(entry).rules, info.employeeId).find(r => ruleMatches(r, { toolName: req.toolName, input: req.input }));
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

  /** Is this factory's Gatekeeper on? It answers first; the Manager gets what it passes on. */
  const gatekeeperOn = (entry) => !!st(entry).snapshot.settings.gatekeeper?.enabled;

  /**
   * Requests are batched briefly, then sent in one message: to the Gatekeeper
   * when the factory has one (unless it already passed them on), else to the
   * Manager.
   */
  function scheduleDelivery(entry) {
    if (entry.timers.perm) return;
    entry.timers.perm = setTimeout(() => {
      entry.timers.perm = null;
      locked(entry, async () => {
        const items = entry.live.permissions.filter(p => !p.sent);
        if (!items.length) return;
        const toGate = gatekeeperOn(entry) ? items.filter(p => !p.escalated) : [];
        const toManager = items.filter(p => !toGate.includes(p));
        if (toGate.length) {
          const g = await ensureSession(entry, GATEKEEPER_ID);
          if (g.ok) {
            const sent = agents.send(g.chatId, batchMessage(toGate, { gatekeeper: true }));
            if (!sent || sent.ok !== false) for (const p of toGate) p.sent = true;
          } else {
            for (const p of toGate) { p.escalated = true; toManager.push(p); }
          }
        }
        if (toManager.length) {
          const m = await ensureSession(entry, MANAGER_ID);
          if (!m.ok) { dropPermissions(entry, (p) => toManager.includes(p), `the Manager is unavailable (${m.error})`); return; }
          const sent = agents.send(m.chatId, batchMessage(toManager, { bypass: rr(entry).managerMode === 'bypass', escalated: gatekeeperOn(entry), rulesAllowed: st(entry).snapshot.settings.managerRules }));
          if (sent && sent.ok === false) return scheduleDelivery(entry);
          for (const p of toManager) p.sent = true;
        }
        writeCache(entry);
      });
    }, permissionBatchMs);
    entry.timers.perm.unref?.();
  }

  /**
   * factory_permission, from the Manager's chat (or the Gatekeeper's).
   * decisions: [{ id, allow, reason, rule?, escalate? }]; the Manager may also
   * grant standing rules up front (rules) and set the policy the Gatekeeper
   * follows (policy), when the factory lets it set rules.
   */
  async function answerPermissions(chatId, decisions, { rules = [], policy } = {}) {
    const info = chatInfo(chatId);
    if (!info) return { ok: false, error: 'This chat is not part of a factory run.' };
    const isGate = info.employeeId === GATEKEEPER_ID;
    if (info.employeeId !== MANAGER_ID && !isGate) return { ok: false, error: 'Only the Manager answers permission requests.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    const list = Array.isArray(decisions) ? decisions : [];
    const pre = Array.isArray(rules) ? rules : rules ? [rules] : [];
    if (!list.length && !pre.length && policy == null) return { ok: false, error: 'Give decisions: a list of { id, allow, reason }' + (isGate ? '.' : ', or rules to grant up front, or a policy.') };
    return locked(entry, async () => {
      const mayRule = !isGate && st(entry).snapshot.settings.managerRules !== false;
      const results = [];
      const errors = [];
      const newRules = [];
      const roster = st(entry).snapshot.roster;
      const whoOf = (v) => {
        if (v == null || v === '' || v === '*' || String(v).toLowerCase() === 'everyone') return '*';
        const e = roster.find(x => x.id === v || x.name.toLowerCase() === String(v).toLowerCase());
        return e ? e.id : null;
      };
      // Rules are per employee unless granted for everyone.
      const grant = (text, label, who) => {
        if (!mayRule) { errors.push(`${label}: standing rules are turned off for this factory; answer requests one by one`); return false; }
        const c = checkRule(text);
        if (!c.ok) { errors.push(`${label}: ${c.error}`); return false; }
        if (who == null) { errors.push(`${label}: nobody by that name is on the team`); return false; }
        if (!scopedRules(rr(entry).rules).some(x => x.rule === c.rule && x.who === who)) { commit(entry, { type: 'rule', rule: c.rule, who }); newRules.push({ rule: c.rule, who }); }
        return true;
      };
      if (policy != null && !isGate) {
        commit(entry, { type: 'policy', text: String(policy) });
        results.push('Policy saved' + (gatekeeperOn(entry) ? ' and sent to the Gatekeeper.' : '.'));
        const g = entry.live.employees[GATEKEEPER_ID];
        if (g && g.chatId) agents.send(g.chatId, '[Factory] The Manager updated the policy:\n' + String(policy).slice(0, 4000));
      }
      for (const r of pre) {
        const text = typeof r === 'object' && r ? String(r.rule || '') : String(r);
        const who = whoOf(typeof r === 'object' && r ? r.employee : '*');
        if (grant(text, 'rule ' + text, who)) results.push(`Rule granted: ${text} for ${who === '*' ? 'everyone' : roster.find(x => x.id === who)?.name}`);
      }
      for (const d of list) {
        const id = String(d?.id || '');
        const item = entry.live.permissions.find(p => p.id === id);
        if (!item) { errors.push(`${id || '(no id)'}: no such request is waiting`); continue; }
        const reason = String(d.reason || '').slice(0, 1000);
        if (isGate && (d.escalate === true || d.escalate === 'true')) {
          item.escalated = true; item.sent = false; item.gateNote = reason;
          decided(entry, item, 'escalated', 'gatekeeper', { reason });
          results.push(`${id}: passed to the Manager`);
          continue;
        }
        const allow = d.allow === true || d.allow === 'true';
        if (allow && d.rule && !grant(String(d.rule), id, d.ruleFor ? whoOf(d.ruleFor) : item.employeeId)) continue;
        const delivered = answer(item, allow, allow ? undefined : denyMessage(reason, isGate ? 'The Gatekeeper' : 'The Manager'));
        entry.live.permissions = entry.live.permissions.filter(p => p !== item);
        decided(entry, item, allow ? 'allowed' : 'denied', isGate ? 'gatekeeper' : 'manager', { reason, ...(allow && d.rule ? { rule: d.rule } : {}) });
        results.push(`${id}: ${allow ? 'allowed' : 'denied'}${delivered ? '' : ' (that request was no longer waiting)'}`);
      }
      // A rule just granted answers anything else waiting that it covers.
      if (newRules.length) {
        for (const item of [...entry.live.permissions]) {
          const hit = newRules.find(r => (r.who === '*' || r.who === item.employeeId) && ruleMatches(r.rule, item));
          if (!hit) continue;
          const rule = hit.rule;
          answer(item, true);
          entry.live.permissions = entry.live.permissions.filter(p => p !== item);
          decided(entry, item, 'allowed', 'rule', { rule });
          results.push(`${item.id}: allowed by your new rule ${rule}`);
        }
      }
      if (entry.live.permissions.some(p => p.escalated && !p.sent)) scheduleDelivery(entry);
      writeCache(entry);
      const left = entry.live.permissions.filter(p => !(isGate && p.escalated)).map(p => p.id);
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
  async function pause(runId, { by = 'person', resetsAt = '', reason = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    return locked(entry, async () => {
      if (!['running', 'waiting'].includes(st(entry).status)) return { ok: false, error: `The run is ${st(entry).status}; it cannot be paused.` };
      commit(entry, { type: 'pause', by, ...(resetsAt ? { resetsAt } : {}), ...(reason ? { reason: String(reason).slice(0, 300) } : {}) });
      clearClock(entry);
      if (by === 'usage') notify('paused', summary(entry), `Factory "${st(entry).snapshot.name}" paused: usage reached the limit you set.${resetsAt ? ` It carries on by itself after ${resetsAt}.` : ''}`);
      if (by === 'manager') notify('paused', summary(entry), `Factory "${st(entry).snapshot.name}" paused by its Manager${reason ? `: ${reason}` : ''}.`);
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
      // Steps nobody is working on: a recovery while paused leaves them for
      // here (their chats ended with the restart), so start them now.
      await startOrphanedSteps(entry, why);
      await drain(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  /**
   * Every step waiting on someone who is not working on it (no busy step, not
   * queued, not waiting for a seat) is started, with a continue prompt.
   */
  async function startOrphanedSteps(entry, why) {
    if (isHeld(entry.rec) || !UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') return;
    for (const t of st(entry).tokens.filter(x => x.status === 'work' || x.status === 'answer')) {
      const who = Engine.personOf(st(entry), t);
      if (!who) continue;
      const emp = empOf(entry, who);
      if (emp.busy === t.id || (emp.queue || []).some(q => q.tokenId === t.id) || entry.live.seats.some(x => x.ef.tokenId === t.id)) continue;
      const ef = activationFor(entry, t.id);
      if (!ef) continue;
      if (emp.busy) { emp.queue.push(ef); continue; }
      await startStep(entry, emp, ef, { recovery: await recoveryFor(entry, who, ef, why, '') });
    }
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
  let usageNow = null;   // the last usage reading, for the one-at-a-time rule
  let usageSource = null; // watchUsage's getUsage, to read it when a run starts
  /**
   * Pause running factories when a usage window is full, and carry them on by
   * themselves once it has reset. Polls getUsage (usage.js shape:
   * { fiveHour:{utilization,resetsAt}, week:{…} }); it only asks while a run
   * is going or waiting on the limit.
   */
  function watchUsage(getUsage, { tickMs = 60_000 } = {}) {
    usageSource = getUsage;
    const tick = async () => {
      const relevant = [...runs.values()].filter(e => ['running', 'waiting'].includes(st(e).status) || rr(e).paused?.by === 'usage');
      if (!relevant.length) return;
      let u;
      try { u = await getUsage(); } catch { return; }
      if (!u || u.ok === false) return;
      usageNow = u;
      for (const e of relevant) {
        // Each run's own thresholds: the person may keep headroom for themselves.
        const L = limitsOf(e);
        const full = [[u.fiveHour, L.pauseAt5h], [u.week, L.pauseAtWeek]].filter(([w, at]) => w && w.utilization >= at).map(([w]) => w);
        const resetsAt = full.map(w => w.resetsAt).filter(Boolean).sort().pop() || '';
        if (!full.length) await locked(e, () => releaseSeats(e));   // the one-at-a-time rule may have lifted
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
    return {
      type: 'activate', tokenId: t.id, nodeId: t.nodeId,
      employeeId: Engine.personOf(st(entry), t),
      mode: t.delegate ? 'delegate' : t.status, ws: t.ws,
      context: { from: null, summary: '', message: t.delegate ? t.delegate.task : '', notes: [], escalation: '' },
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
    entry.live.seats = [];   // rebuilt below from the tokens
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
      if (t.status === 'wait') return { employee: '', what: `the flow is waiting at "${n?.label || 'Wait'}" (it carries on by itself)` };
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
    if (from === 'finishing') await present(entry, { state: 'ask' });
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
      armAllWaits(entry);
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
    for (const t of s.tokens) if (t.delegate) needed.add(t.delegate.employeeId);
    needed.add(GATEKEEPER_ID);
    for (const [eid, emp] of Object.entries(entry.live.employees)) {
      if (needed.has(eid) || !emp.chatId || emp.busy || (emp.queue || []).length) continue;
      // Still finishing the turn it handed off in: closing it now would cut
      // the turn short (and lose what it cost). A later pass retires it.
      if (agents.state && agents.state(emp.chatId) && agents.state(emp.chatId) !== 'idle') continue;
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

  // ─── Waits ───
  //
  // A Wait node holds its token until a time passes, a command succeeds or a
  // signal comes, bounded by a time limit. The plan (deadlines, the signal
  // key) is journaled when the wait starts, so a restart re-arms it from the
  // same deadlines. Polls and timers are live state only.

  /** Deadlines for a wait starting now. */
  function waitPlan(ef) {
    const w = ef.wait, t0 = Date.now();
    let deadline = 0;
    if (w.kind === 'time') deadline = t0 + w.minutes * 60_000;
    if (w.kind === 'until') {
      const [h, m] = w.at.split(':').map(Number);
      const d = new Date(t0); d.setHours(h, m, 0, 0);
      if (d.getTime() <= t0) d.setDate(d.getDate() + 1);
      deadline = d.getTime();
    }
    const timeoutAt = w.kind === 'command' || w.kind === 'signal' ? t0 + w.timeout * 60_000 : deadline;
    return {
      kind: w.kind, nodeId: ef.nodeId, ws: ef.ws, startedAt: new Date(t0).toISOString(),
      deadline: deadline ? new Date(deadline).toISOString() : '', timeoutAt: new Date(timeoutAt).toISOString(),
      key: w.kind === 'signal' ? randomBytes(18).toString('base64url') : '',
    };
  }

  function clearWait(entry, tokenId) {
    const w = entry.waits[tokenId];
    if (!w) return;
    clearTimeout(w.timer);
    if (w.child) { try { w.child.kill('SIGKILL'); } catch { /* gone */ } }
    delete entry.waits[tokenId];
  }

  /** Start (or restart) the timers and polls of one wait. */
  function armWait(entry, tokenId) {
    clearWait(entry, tokenId);
    const plan = rr(entry).waits?.[tokenId];
    const t = st(entry).tokens.find(x => x.id === tokenId);
    if (!plan || !t || t.status !== 'wait') return;
    const node = st(entry).snapshot.nodes.find(n => n.id === plan.nodeId);
    const live = entry.waits[tokenId] = { timer: null, child: null, tries: 0, last: '' };
    const at = (iso) => Math.max(0, Date.parse(iso) - Date.now());
    const later = (fn, ms) => { live.timer = setTimeout(fn, Math.min(ms, 2 ** 31 - 1)); live.timer.unref?.(); };
    if (plan.kind === 'time' || plan.kind === 'until') {
      later(() => finishWait(entry, tokenId, 'done', plan.kind === 'time' ? `${node.wait.minutes} min passed` : `it is ${node.wait.at}`), at(plan.deadline));
      return;
    }
    if (plan.kind === 'signal') {
      later(() => finishWait(entry, tokenId, 'timeout', `no signal within ${node.wait.timeout} min`), at(plan.timeoutAt));
      return;
    }
    // A command: run it now and every `every` seconds until it succeeds or time runs out.
    const poll = async () => {
      if (entry.waits[tokenId] !== live) return;
      if (st(entry).tokens.find(x => x.id === tokenId)?.status !== 'wait') { clearWait(entry, tokenId); return; }
      if (isHeld(entry.rec)) { later(poll, 30_000); return; }   // paused: try again later
      const cwd = wsPath(entry, plan.ws);
      const why = hardDeny({ toolName: 'Bash', input: { command: node.wait.command } }, { writableRoots: [rr(entry).root, runDir(st(entry).id)] });
      if (why) { finishWait(entry, tokenId, 'timeout', `the command is not allowed: ${why}`); return; }
      live.tries++;
      const res = await runCheck(node.wait.command, cwd, Math.min(node.wait.every, 300) * 1000, live);
      if (entry.waits[tokenId] !== live) return;
      live.last = res.output;
      if (res.ok) { finishWait(entry, tokenId, 'done', `the command succeeded after ${live.tries} tr${live.tries === 1 ? 'y' : 'ies'}${res.output ? `; it said: ${res.output.slice(-400)}` : ''}`); return; }
      const left = at(plan.timeoutAt);
      if (left <= 0) { finishWait(entry, tokenId, 'timeout', `the command still failed after ${node.wait.timeout} min and ${live.tries} tries${res.output ? `; last output: ${res.output.slice(-400)}` : ''}`); return; }
      later(poll, Math.min(node.wait.every * 1000, left));
    };
    poll();
  }

  /** Run a wait's check command; ok when it exits 0. */
  function runCheck(command, cwd, timeoutMs, live) {
    return new Promise((done) => {
      let out = '';
      let child;
      try { child = spawn('bash', ['-lc', command], { cwd, env: { ...process.env, CI: '1' }, stdio: ['ignore', 'pipe', 'pipe'] }); } catch (err) { done({ ok: false, output: err.message }); return; }
      live.child = child;
      const add = (b) => { out = (out + b.toString()).slice(-4000); };
      child.stdout.on('data', add); child.stderr.on('data', add);
      const kill = setTimeout(() => { try { child.kill('SIGKILL'); } catch { /* gone */ } }, timeoutMs);
      child.on('close', (code) => { clearTimeout(kill); live.child = null; done({ ok: code === 0, output: out.trim() }); });
      child.on('error', (err) => { clearTimeout(kill); live.child = null; done({ ok: false, output: err.message }); });
    });
  }

  /** A wait is over: route the token on. */
  function finishWait(entry, tokenId, outcome, detail) {
    return locked(entry, async () => {
      clearWait(entry, tokenId);
      const t = st(entry).tokens.find(x => x.id === tokenId);
      if (!t || t.status !== 'wait' || !UNFINISHED.has(st(entry).status) || st(entry).status === 'crashed') return { ok: false, error: 'That wait is no longer waiting.' };
      if (isHeld(entry.rec)) { const l = entry.waits[tokenId] = { timer: setTimeout(() => finishWait(entry, tokenId, outcome, detail), 30_000), child: null }; l.timer.unref?.(); return { ok: false, error: 'The run is paused; the wait ends when it resumes.' }; }
      const dry = Engine.waitDone(st(entry), tokenId, outcome, detail);
      if (!dry.ok) return dry;
      commit(entry, { type: 'wait-done', tokenId, outcome, detail: String(detail || '').slice(0, 1000) });
      await drain(entry);
      collect(entry);
      return { ok: true, run: summary(entry) };
    });
  }

  /** Re-arm every wait of a run (after a recovery). */
  function armAllWaits(entry) {
    for (const t of st(entry).tokens) if (t.status === 'wait') armWait(entry, t.id);
  }

  /**
   * End a wait from outside: the person (from the panel, any kind) or a
   * signal URL (signal waits, with their key).
   */
  async function endWait(runId, tokenId, { key = null, outcome = 'done', note = '' } = {}) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    const plan = rr(entry).waits?.[tokenId];
    if (!plan) return { ok: false, error: 'That wait is no longer waiting.' };
    if (key !== null) {
      const a = Buffer.from(String(key)), b = Buffer.from(plan.key || '');
      if (!plan.key || a.length !== b.length || !timingSafeEqual(a, b)) return { ok: false, forbidden: true, error: 'Wrong key.' };
    }
    const by = key !== null ? 'a signal came' : 'the person ended it';
    return finishWait(entry, tokenId, outcome === 'timeout' ? 'timeout' : 'done', `${by}${note ? `: ${String(note).slice(0, 500)}` : ''}`);
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
      // Working in a branch: the run's main worktree must not have changed
      // (it was committed when the branches were made). Edits that landed
      // there by mistake would be merged as if they were someone else's.
      const here = wsPath(entry, tok?.ws);
      if (tok && here !== rr(entry).worktree && !payload?.escalate) {
        const stray = await git.git(['status', '--porcelain'], rr(entry).worktree);
        const files = stray.stdout.split('\n').map(l => l.slice(3).trim()).filter(f => f && !f.startsWith('.claude/skills') && !f.startsWith('.mcp.json'));
        if (files.length) {
          return { ok: false, error: `You changed files in the run's main worktree (${rr(entry).worktree}): ${files.slice(0, 8).join(', ')}${files.length > 8 ? '…' : ''}. Your workspace for this step is ${here}. Move those changes into your workspace, undo them in the main worktree (git -C ${rr(entry).worktree} checkout -- <file>, or delete new files), then hand off again.` };
        }
      }
      const subject = shortSubject(payload?.summary || payload?.escalate || '');
      const c = await git.commitAll(wsPath(entry, tok?.ws), `factory: ${node?.label || node?.type || 'step'}${subject ? ' — ' + subject : ''}`, excl(entry));
      const caps = { maxHandoffs: st(entry).snapshot.settings.caps.maxHandoffs + rr(entry).capExtra };
      const dry = Engine.handoff(st(entry), tokenId, payload, { treeHash: c.ok ? c.tree : '', caps });
      if (!dry.ok) return { ok: false, error: dry.error };
      // Which skills this person loaded during the step (shown on the live graph).
      let skills = [];
      if (emp?.chatId && agents.skillsUsed) {
        try {
          const all = agents.skillsUsed(emp.chatId) || [];
          skills = [...new Set(all.slice(emp.skillsSeen || 0))];
          emp.skillsSeen = all.length;
        } catch { skills = []; }
      }
      commit(entry, {
        type: 'handoff', tokenId, payload, treeHash: c.ok ? c.tree : '', by: info.employeeId,
        commit: c.ok ? c.head : '', sessionUuid: emp?.chatId ? agents.sessionUuid(emp.chatId) || '' : '',
        ...(skills.length ? { skills } : {}),
      });
      if (emp && emp.busy === tokenId) emp.busy = null;
      const where = describeWhere(entry, dry.effects);
      await drain(entry);
      await dequeue(entry, info.employeeId);
      if (payload?.finish) {
        return {
          ok: true,
          message: [
            'The run is finished. A card with the diff and the Merge / Request changes / Keep / Discard buttons is now in this chat for the person.',
            'Present the result to them here: what was done and why, what changed, and anything they should check.',
            `If it is something to look at (a page, an app), let them see it: start it from ${rr(entry).worktree} as a Crundi service (register_service with that folder as cwd, then start_service) on a free high port (4000-9999, never 8888), and show screenshots with show_image. Crundi removes that service when the run ends.`,
            'Do not merge or commit yourself: the person decides on the card.',
          ].join('\n'),
        };
      }
      return { ok: true, message: `Handed off${where ? ': ' + where : ''}. Your step is over. Stop here and wait; do not reply at length.` };
    });
  }

  function describeWhere(entry, effects) {
    const roster = new Map(st(entry).snapshot.roster.map(r => [r.id, r.name]));
    const parts = [];
    for (const e of effects) {
      if (e.type === 'activate') parts.push(e.mode === 'answer' ? `${roster.get(e.employeeId)} is asked to decide` : `${roster.get(e.employeeId)} takes over`);
      if (e.type === 'ask') parts.push('the person is asked');
      if (e.type === 'wait') parts.push(`the flow waits at "${st(entry).snapshot.nodes.find(n => n.id === e.nodeId)?.label || 'Wait'}" and then carries on`);
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
      for (const id of Object.keys(entry.waits)) clearWait(entry, id);
      entry.live.seats = [];
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
      for (const id of Object.keys(e.waits)) clearWait(e, id);
      for (const id of Object.keys(e.watchers)) unwatch(e, id);
    }
  }

  /**
   * factory_delegate, from the Manager's chat: a one-off job for someone on
   * the team, outside the drawn flow. It comes back to the Manager.
   */
  async function delegateJob(chatId, employee, task) {
    const info = chatInfo(chatId);
    if (!info || info.employeeId !== MANAGER_ID) return { ok: false, error: 'Only the Manager of a factory run hands out jobs.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    return locked(entry, async () => {
      if (!['running', 'waiting'].includes(st(entry).status)) return { ok: false, error: `The run is ${st(entry).status}; jobs can be handed out while it runs.` };
      const emp = entry.live.employees[MANAGER_ID];
      const tokenId = (emp && emp.busy) || Engine.tokensFor(st(entry), MANAGER_ID)[0]?.id;
      if (!tokenId) return { ok: false, error: 'It is not your turn: hand out jobs while a step or decision is yours.' };
      const caps = { maxHandoffs: st(entry).snapshot.settings.caps.maxHandoffs + rr(entry).capExtra };
      const dry = Engine.delegate(st(entry), tokenId, employee, task, { caps });
      if (!dry.ok) return dry;
      const job = dry.run.tokens.find(t => t.delegatedBy === tokenId);
      commit(entry, { type: 'delegate', tokenId, employeeId: job.delegate.employeeId, task: job.delegate.task });
      if (emp && emp.busy === tokenId) emp.busy = null;
      await drain(entry);
      const name = st(entry).snapshot.roster.find(r => r.id === job.delegate.employeeId)?.name;
      return { ok: true, message: `Handed to ${name}. Stop here and wait: their result comes back to you as a message, and then it is your turn again.` };
    });
  }

  /**
   * factory_result, from the Manager's chat: carry out the person's decision
   * on the result when they gave it in the chat ("merge it").
   */
  async function resultFromManager(chatId, action, note = '') {
    const info = chatInfo(chatId);
    if (!info || info.employeeId !== MANAGER_ID) return { ok: false, error: 'Only the Manager of a factory run can do this.' };
    const map = { merge: 'merge', keep: 'keep', discard: 'discard', changes: 'changes', reopen: 'changes', undo: 'undo' };
    const a = map[String(action || '').toLowerCase()];
    if (!a) return { ok: false, error: 'action is merge, keep, discard, changes or undo.' };
    const r = await finishAction(info.runId, a, { note });
    if (!r.ok) return r;
    const done = { merge: 'Merged.', keep: 'Kept on its branch.', discard: 'Discarded.', changes: 'Reopened: the run is back to work, and it is your turn.', undo: 'The merge is undone.' }[a];
    return { ok: true, message: done + ' Tell the person in a sentence.' };
  }

  /**
   * factory_status: where the run stands, in one block, for whoever in the
   * run asks (the Manager after a long conversation or a compaction).
   */
  async function statusFor(chatId) {
    const info = chatInfo(chatId);
    if (!info) return { ok: false, error: 'This chat is not part of a factory run.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    const s = st(entry), r = rr(entry), f = s.snapshot;
    const names = new Map(f.roster.map(x => [x.id, x.name]));
    const nodes = new Map(f.nodes.map(n => [n.id, n]));
    const lines = [`[Factory] Status of "${f.name}" (${s.status}), ${s.handoffs} handoffs so far.`];
    if (r.cr) lines.push('', 'Change request:', crBrief(r.cr, f.settings.crTemplate, r.cr.file));
    else lines.push('', `Task: ${s.task}`);
    lines.push('', 'Now:');
    for (const t of s.tokens) {
      const n = nodes.get(t.nodeId);
      const who = names.get(Engine.personOf(s, t)) || '';
      const what = t.delegate ? `${who}: a job you handed out (${t.delegate.task.slice(0, 120)})`
        : t.status === 'work' ? `${who} is working on "${n?.label || who}"`
        : t.status === 'answer' ? `${who} is asked "${n?.label || n?.decision?.question || ''}"`
        : t.status === 'ask' ? `the person is asked: ${n?.ask?.prompt || ''}`
        : t.status === 'join' ? `a branch waits at "${n?.label || 'Join'}"`
        : t.status === 'wait' ? `the flow waits at "${n?.label || 'Wait'}"`
        : t.status === 'delegated' ? 'you are waiting for a job you handed out'
        : t.status;
      lines.push(`- ${what}${t.ws && t.ws !== 'main' ? ` (branch ${t.ws})` : ''}`);
    }
    if (!s.tokens.length) lines.push('- nothing is moving');
    const loops = f.edges.filter(e => e.maxRounds > 0 && (s.edgeCounts[e.id] || 0) > 0);
    if (loops.length) {
      lines.push('', 'Loops used:');
      for (const e of loops) lines.push(`- "${nodes.get(e.source)?.label || e.source}" -> "${nodes.get(e.target)?.label || e.target}": ${s.edgeCounts[e.id]} of ${e.maxRounds}${s.exhausted[e.id] ? ` (reopened ${s.exhausted[e.id]}x)` : ''}`);
    }
    const L = Engine.ledgerOf(s).items;
    if (L.length) {
      const open = L.filter(i => i.status === 'open' || i.status === 'claimed');
      lines.push('', `Review findings: ${open.length} open of ${L.length}.`);
      for (const i of open) lines.push(`- ${i.id} [${i.blocking ? 'blocking' : 'nit'}${i.status === 'claimed' ? ', said to be fixed' : ''}] ${i.text}`);
    }
    if (entry.live.permissions.length) lines.push('', `Permission requests waiting: ${entry.live.permissions.map(p => `${p.id} (${p.employee}: ${p.toolName})`).join(', ')}`);
    try {
      const d = await git.git(['diff', '--stat=100', `${r.baseCommit}..HEAD`, '--', '.', ':(exclude).claude/skills'], r.worktree);
      const stat = d.stdout.split('\n').filter(Boolean).slice(-15).join('\n');
      lines.push('', `Changes since the run started (${String(r.baseCommit).slice(0, 12)}):`, stat ? stat.split('\n').map(x => '  ' + x).join('\n') : '  (none yet)');
    } catch { /* best effort */ }
    const recent = s.history.filter(h => h.type === 'handoff').slice(-6).map(h => `- ${names.get(h.by) || h.by}: ${String(h.summary || '').split('\n')[0].slice(0, 160)}`);
    if (recent.length) lines.push('', 'Recent handoffs:', ...recent);
    const u = usageOf(r);
    lines.push('', `Spent so far: ${u.turns} turns in ${u.chats} chats${u.costUsd ? `, about $${u.costUsd.toFixed(2)}` : ''}; ${Math.round(timeLeft(entry.rec, Date.now()) / 60000)} min of the time limit left.`);
    const Lm = limitsOf(entry);
    lines.push(`Usage limits the person set: pause at ${Lm.pauseAt5h}% of the 5-hour window and ${Lm.pauseAtWeek}% of the week${usageNow?.fiveHour ? ` (now ${Math.round(usageNow.fiveHour.utilization)}% and ${Math.round(usageNow.week?.utilization || 0)}%)` : ''}; ${Lm.maxParallel ? `at most ${Lm.maxParallel} at once` : 'no limit on people at once'}${Lm.oneAtATimeAbove ? `, one at a time above ${Lm.oneAtATimeAbove}%` : ''}.${entry.live.seats.length ? ` Waiting for a seat: ${entry.live.seats.length}.` : ''}`);
    return { ok: true, content: [{ type: 'text', text: lines.join('\n') }] };
  }

  /**
   * factory_control, from the Manager's chat: pause or resume the run, or
   * change its usage limits, when the person asks (or to stay inside them).
   */
  async function controlFromManager(chatId, { action, reason = '', limits = null } = {}) {
    const info = chatInfo(chatId);
    if (!info || info.employeeId !== MANAGER_ID) return { ok: false, error: 'Only the Manager of a factory run can do this.' };
    if (action === 'pause') {
      const r = await pause(info.runId, { by: 'manager', reason });
      return r.ok ? { ok: true, message: 'Paused. Nobody new starts a step; whoever is mid-turn finishes it. Resume with action "resume" when the person says so.' } : r;
    }
    if (action === 'resume') {
      const r = await resume(info.runId, { reason: reason ? 'the Manager resumed it: ' + reason : '' });
      return r.ok ? { ok: true, message: 'Resumed.' } : r;
    }
    if (action === 'limits') {
      const r = await setLimits(info.runId, limits || {});
      return r.ok ? { ok: true, message: 'Limits now: ' + JSON.stringify(r.run.limits) } : r;
    }
    return { ok: false, error: 'action is pause, resume or limits.' };
  }

  /**
   * factory_message, from the Manager's chat: a message into the chat of
   * someone who is mid-step (a correction, a heads-up), delivered like the
   * person typing into it. Only while that person holds a step: otherwise
   * there is nobody to tell, and factory_delegate or the next step's notes
   * are the way.
   */
  async function messageFromManager(chatId, employee, text) {
    const info = chatInfo(chatId);
    if (!info || info.employeeId !== MANAGER_ID) return { ok: false, error: 'Only the Manager of a factory run can do this.' };
    const entry = runs.get(info.runId);
    if (!entry) return { ok: false, error: 'That factory run no longer exists.' };
    const body = String(text || '').trim().slice(0, 6000);
    if (!body) return { ok: false, error: 'Say what to tell them (text).' };
    return locked(entry, async () => {
      const roster = st(entry).snapshot.roster;
      const want = String(employee || '').trim().toLowerCase();
      const r = roster.find(x => x.id !== MANAGER_ID && (x.id.toLowerCase() === want || x.name.toLowerCase() === want));
      if (!r) return { ok: false, error: `Nobody called "${employee}" is on your team. The team: ${roster.filter(x => x.id !== MANAGER_ID).map(x => x.name).join(', ')}.` };
      const emp = entry.live.employees[r.id];
      if (!emp || !emp.busy || !emp.chatId || !agents.isAlive(emp.chatId)) {
        const busy = Object.entries(entry.live.employees).filter(([id, e]) => id !== MANAGER_ID && e.busy && e.chatId).map(([id]) => roster.find(x => x.id === id)?.name).filter(Boolean);
        return { ok: false, error: `${r.name} is not working on a step right now, so there is nobody to tell. Hand them a job with factory_delegate when it is your turn, or leave a note for their next step.${busy.length ? ` Working now: ${busy.join(', ')}.` : ''}` };
      }
      const t = st(entry).tokens.find(x => x.id === emp.busy);
      const node = st(entry).snapshot.nodes.find(n => n.id === t?.nodeId);
      const sent = agents.send(emp.chatId, `[Factory] A message from the Manager while you work on "${node?.label || (t?.delegate ? 'the job it gave you' : 'your step')}":\n\n${body}\n\nCarry on with your step; this message does not end it.`);
      if (sent && sent.ok === false) return { ok: false, error: `Could not reach ${r.name}: ${sent.error}` };
      commit(entry, { type: 'manager-message', employeeId: r.id, text: body });
      return { ok: true, message: `Told ${r.name}. They read it in their running step; it does not end it.` };
    });
  }

  /** Switch the team between asking (the Manager or Gatekeeper) and bypass. */
  async function setTeamMode(runId, mode) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    if (mode !== 'bypass' && mode !== 'ask') return { ok: false, error: 'mode is "ask" or "bypass".' };
    return locked(entry, async () => {
      if (rr(entry).teamMode === mode) return { ok: true, run: summary(entry) };
      commit(entry, { type: 'team-mode', mode });
      return { ok: true, run: summary(entry), message: 'Each employee switches the next time they start a step.' };
    });
  }

  /** The run's journal from `from` on, for the live graph and its replay. */
  function runEvents(runId, from = 0) {
    const entry = runs.get(runId);
    if (!entry) return { ok: false, error: 'Run not found.' };
    const all = readJournal(runId);
    const start = Math.max(0, Math.min(all.length, Number(from) || 0));
    return { ok: true, from: start, total: all.length, events: all.slice(start) };
  }

  /** Does this chat belong to a run that has not finished? (The close guard.) */
  function guardsChat(chatId) {
    const info = chatInfo(chatId);
    if (!info) return null;
    const entry = runs.get(info.runId);
    if (!entry || !UNFINISHED.has(st(entry).status)) return null;
    return summary(entry);
  }

  return { load, startRun, handoff, askReply, endWait, resume, pause, recover, stopRun, finishAction, runDiff, runEvents, guardsChat, delegateJob, resultFromManager, setTeamMode, statusFor, setLimits, controlFromManager, messageFromManager, listRuns, getRun, activeRunFor, chatInfo, answerPermissions, setManagerMode, watchUsage, dispose };
}

/**
 * What the phone's live card says about a run, carried by the Manager's chat:
 * working while anyone on the team works, "needs you" for a question, a
 * limit, a crash, the result or the Manager talking to the person, waiting
 * while paused, idle (the card goes) once the run is over.
 * @param {object} run           a run summary (status, waitingOn, tokens, factoryName)
 * @param {string} managerState  the Manager chat's own state
 */
export function factoryLiveCard(run, managerState = 'idle') {
  const active = ['starting', 'running', 'waiting', 'paused', 'finishing', 'crashed'].includes(run.status);
  const busy = (run.tokens || []).find(t => t.status === 'work' || t.status === 'answer');
  let state;
  if (!active) state = 'idle';
  else if (run.status === 'crashed' || run.status === 'finishing' || run.waitingOn === 'ask' || run.waitingOn === 'cap' || run.waitingOn === 'manager' || managerState === 'needs-input') state = 'needs-input';
  else if (run.status === 'paused') state = 'waiting';
  else state = busy || managerState === 'working' ? 'working' : 'waiting';
  const doing = state === 'needs-input' ? (run.status === 'finishing' ? 'done, your decision' : run.status === 'crashed' ? 'crashed' : 'needs you')
    : run.status === 'paused' ? 'paused'
    : !active ? run.status
    : busy ? `${busy.employee || 'someone'} ${busy.status === 'answer' ? 'deciding' : 'working'}`
    : (run.tokens || []).find(t => t.status === 'wait') ? `waiting: ${(run.tokens || []).find(t => t.status === 'wait').label}` : 'between steps';
  return { state, title: `${run.factoryName}: ${doing}` };
}

/** A run's spend, from its journaled usage: totals and per person. */
export function usageOf(r) {
  const out = { costUsd: 0, turns: 0, chats: 0, byEmployee: {} };
  for (const u of Object.values(r.usage || {})) {
    out.costUsd += u.costUsd || 0; out.turns += u.turns || 0; out.chats++;
    const b = out.byEmployee[u.employeeId] || (out.byEmployee[u.employeeId] = { costUsd: 0, turns: 0 });
    b.costUsd += u.costUsd || 0; b.turns += u.turns || 0;
  }
  out.costUsd = Math.round(out.costUsd * 10000) / 10000;
  out.activeMs = r.time ? r.time.activeMs + (r.time.since ? Math.max(0, Date.now() - Date.parse(r.time.since)) : 0) : 0;
  return out;
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
    addCard: (id, card) => claudeUi.addCard(id, card),
    // Every skill this chat has loaded so far, in order (Skill tool calls).
    skillsUsed: (id) => ((claudeUi.history(id) || {}).messages || [])
      .filter(m => m.kind === 'tool' && m.name === 'Skill')
      .map(m => String(m.input?.skill || m.input?.command || m.input?.name || '').replace(/^\//, ''))
      .filter(Boolean),
    patchCard: (id, entryId, patch) => claudeUi.patchCard(id, entryId, patch),
    // What this person is doing right now, in a few words, for the live graph:
    // the tool in flight (or the last one), thinking, or writing.
    activity: (id) => {
      const h = claudeUi.history(id);
      if (!h) return null;
      const msgs = h.messages || [];
      const base = (p) => String(p || '').split(/[\\/]/).pop();
      const clip = (t, n = 60) => { t = String(t || '').replace(/\s+/g, ' ').trim(); return t.length > n ? t.slice(0, n - 1) + '…' : t; };
      let text = '';
      for (let i = msgs.length - 1; i >= 0 && i >= msgs.length - 40; i--) {
        const m = msgs[i];
        if (m.kind === 'tool') {
          const n = String(m.name || ''), inp = m.input || {};
          text = /^(Edit|Write|MultiEdit|NotebookEdit)$/.test(n) ? 'Editing ' + base(inp.file_path || inp.notebook_path)
            : n === 'Read' ? 'Reading ' + base(inp.file_path)
            : n === 'Bash' ? 'Running ' + clip(inp.command, 50)
            : /^(Grep|Glob)$/.test(n) ? 'Searching ' + clip(inp.pattern, 40)
            : /^Web/.test(n) ? 'Reading the web'
            : /^(Task|Agent)$/.test(n) ? 'Delegating: ' + clip(inp.description, 40)
            : n === 'Skill' ? 'Using /' + clip(inp.skill || inp.command, 30)
            : /factory_handoff$/.test(n) ? 'Handing off'
            : /factory_permission$/.test(n) ? 'Answering permissions'
            : /factory_delegate$/.test(n) ? 'Handing out a job'
            : /factory_status$/.test(n) ? 'Checking where the run stands'
            : /factory_result$/.test(n) ? 'Acting on your decision'
            : /rename_chat$/.test(n) ? 'Naming its chat'
            : n === 'ToolSearch' ? 'Loading its tools'
            : n === 'TodoWrite' ? 'Updating its to-do list'
            : n.replace(/^mcp__\w+__/, '').replace(/_/g, ' ');
          break;
        }
        if (m.kind === 'thinking') { text = 'Thinking'; break; }
        if (m.kind === 'assistant-text') { text = 'Writing: ' + clip(m.text, 50); break; }
        if (m.kind === 'user') { text = 'Reading its instructions'; break; }
      }
      const state = h.state || '';
      return { state, text: state === 'idle' ? (text ? 'Idle · last: ' + text : 'Idle') : (text || 'Starting'), at: msgs.length ? (msgs[msgs.length - 1].at || '') : '' };
    },
    // Cost and turns so far. The CLI's cost is cumulative per process, so a
    // relaunch on the same chat id (bypass, recovery) starts a new segment.
    usage: (id) => {
      let cost = 0, seg = 0, turns = 0, ms = 0;
      for (const m of ((claudeUi.history(id) || {}).messages || [])) {
        if (m.kind !== 'result') continue;
        const c = Number(m.costUsd) || 0;
        if (c < seg) { cost += seg; }
        seg = c;
        turns += Number(m.numTurns) || 0;
        ms += Number(m.durationMs) || 0;
      }
      return { costUsd: Math.round((cost + seg) * 1e6) / 1e6, turns, durationMs: ms };
    },
    on: (id, fn) => claudeUi.on(id, fn),
    off: (id, fn) => claudeUi.off(id, fn),
  };
}
