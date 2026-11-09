/**
 * voice/jobs.js — Cru carrying a card to done (9 Nov 2026).
 *
 * A job is a kanban card Cru has taken on: it starts a Claude chat in the
 * card's project (bypass mode, so the chat does not stall on permissions),
 * gives it the card, follows it turn by turn, checks the result, sends the
 * next step when more is needed, and finishes the job: the card to Done, or
 * left where it is as stuck, with a word to the owner. The heartbeat lists the
 * jobs still open, so a quiet one is noticed.
 *
 * Stored in <dataDir>/voice-jobs.json. Finished jobs are kept a week.
 */

import { readFileSync, existsSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { writePrivateFile } from '../private-fs.js';

export const MAX_OPEN_JOBS = 3;
const KEEP_MS = 7 * 86400000;
const OPEN = new Set(['running', 'paused']);

export function createJobStore({ file = '', now = () => Date.now(), onChange = () => {} } = {}) {
  let jobs = [];
  try { if (file && existsSync(file)) jobs = JSON.parse(readFileSync(file, 'utf-8')).jobs || []; } catch { jobs = []; }
  const save = () => {
    const t = now();
    jobs = jobs.filter((j) => OPEN.has(j.status) || t - (j.updatedAt || 0) < KEEP_MS);
    if (file) { try { writePrivateFile(file, JSON.stringify({ jobs }, null, 2)); } catch (e) { console.warn('[voice] could not save the jobs:', e.message); } }
    onChange();
  };
  const find = (ref) => { const r = String(ref || ''); return jobs.find((j) => j.id === r) || jobs.find((j) => j.taskId === r && OPEN.has(j.status)) || null; };
  return {
    list: (all = false) => jobs.filter((j) => all || OPEN.has(j.status)).map((j) => ({ ...j })),
    open: () => jobs.filter((j) => j.status === 'running'),
    find,
    forChat: (chatId) => jobs.find((j) => j.chatId === chatId && OPEN.has(j.status)) || null,
    add({ project, taskId, title, chatId }) {
      if (jobs.filter((j) => j.status === 'running').length >= MAX_OPEN_JOBS) throw new Error(`You already have ${MAX_OPEN_JOBS} jobs running; finish one first`);
      if (jobs.some((j) => j.taskId === taskId && j.project === project && OPEN.has(j.status))) throw new Error('You already have a job on that card');
      const j = { id: `job_${randomBytes(3).toString('hex')}`, project, taskId, title, chatId, status: 'running', startedAt: now(), updatedAt: now(), notes: [] };
      jobs.push(j); save();
      return { ...j };
    },
    note(ref, text) {
      const j = find(ref); if (!j) throw new Error(`No job "${ref}"`);
      j.notes = [...(j.notes || []), { at: now(), text: String(text || '').slice(0, 300) }].slice(-12);
      j.updatedAt = now(); save();
      return { ...j };
    },
    set(ref, patch) {
      const j = find(ref); if (!j) throw new Error(`No job "${ref}"`);
      Object.assign(j, patch, { updatedAt: now() }); save();
      return { ...j };
    },
    /** Stop everything: open jobs wait until Cru is told to carry on. */
    pauseAll() { let n = 0; for (const j of jobs) if (j.status === 'running') { j.status = 'paused'; j.updatedAt = now(); n++; } if (n) save(); return n; },
  };
}

/** What a job's chat is told first. */
export function jobPrompt(card = {}, extra = '') {
  const todos = (card.todos || []).filter((t) => !t.done).map((t) => `- ${t.text}`).join('\n');
  return [
    `Work on this kanban card until it is done: "${card.title || ''}".`,
    card.description ? `\n${card.description}` : '',
    todos ? `\nTodos:\n${todos}` : '',
    extra ? `\n${extra}` : '',
    '\nFollow the project\'s own instructions (CLAUDE.md) for how to work, test and commit. When you have finished, end with a short summary: what you changed, how you checked it, and anything left for the owner. If you are blocked, say exactly what you need.',
  ].join('');
}

const str = (description, extra = {}) => ({ type: 'string', description, ...extra });
const obj = (properties = {}, required = []) => ({ type: 'object', properties, required, additionalProperties: false });

/**
 * Cru's job tools, as action defs for actions.js (gated there).
 * @param deps { jobs, kanban, project(alias), createChat, sendMessage, watch, chats(), log(line) }
 */
export function jobToolDefs(deps) {
  const card = (alias, id) => { const t = deps.kanban.getTask(alias, String(id)); const c = t && (t.task || t); if (!c || t.ok === false || !c.title) throw new Error('No such card'); return c; };
  return [
    {
      name: 'start_job',
      description: 'Take on a kanban card and see it through: a new Claude chat in its project, in bypass mode, is given the card (title, description, open todos, and your extra instructions); the card moves to In progress; you are told each time the chat finishes a turn, and check what it did, send the next step with send_to_chat, and end with finish_job. At most 3 at a time. intent explicit when the owner asked you to take the card on.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Card id'), instructions: str('Optional extra instructions for the chat'), intent: str('explicit or inferred', { enum: ['explicit', 'inferred'] }) }, ['project', 'taskId']),
      check: (i) => { const p = deps.project(i.project); card(p.alias, i.taskId); },
      confirm: (i) => i.intent === 'explicit' ? null : `Take on "${card(deps.project(i.project).alias, i.taskId).title}" in ${i.project}: a Claude chat in bypass mode works on it until it's done?`,
      run: async (i) => {
        const p = deps.project(i.project);
        const c = card(p.alias, i.taskId);
        const r = await deps.createChat(p.alias, { title: c.title.slice(0, 60), skipPermissions: true });
        if (!r || !r.ok) return r || { ok: false, error: 'The chat could not be started' };
        let job;
        try { job = deps.jobs.add({ project: p.alias, taskId: String(i.taskId), title: c.title, chatId: r.id }); }
        catch (e) { return { ok: false, error: e.message, chat: r.id }; }
        deps.sendMessage(r.id, jobPrompt(c, String(i.instructions || '')));
        deps.watch(r.id, c.title.slice(0, 60), p.alias, { follow: true });
        try { if (c.status !== 'in_progress') deps.kanban.moveTask(p.alias, String(i.taskId), 'in_progress'); } catch { /* best effort */ }
        deps.log?.(`Took on "${c.title}" (${p.alias}) as ${job.id}`);
        return { ok: true, job: job.id, chat: r.id, project: p.alias, card: c.title, note: 'You will be told when the chat finishes each turn. Check its work, then send_to_chat the next step or finish_job.' };
      },
    },
    {
      name: 'list_jobs',
      description: 'Your jobs (cards you took on): their chat, status, how long, and your notes. all: include the finished ones of the last week.',
      input_schema: obj({ all: { type: 'boolean', description: 'Include finished ones' } }),
      read: true,
      run: (i = {}) => {
        const chats = new Map((deps.chats() || []).map((c) => [c.id, c]));
        return { ok: true, jobs: deps.jobs.list(!!i.all).map((j) => { const c = chats.get(j.chatId); return { ...j, chat: c ? { id: c.id, status: c.status || 'open', title: c.title } : { id: j.chatId, status: 'gone' }, minutes: Math.round((Date.now() - j.startedAt) / 60000) }; }) };
      },
    },
    {
      name: 'job_note',
      description: 'Keep a short note on a job (what it is doing, what you sent next), for later turns and the heartbeat.',
      input_schema: obj({ job: str('Job id or card id'), note: str('The note') }, ['job', 'note']),
      safe: true,
      run: (i) => ({ ok: true, job: deps.jobs.note(i.job, i.note).id }),
    },
    {
      name: 'finish_job',
      description: 'End a job. done: you checked the work is finished (the card moves to Done). stuck: it needs the owner (the card stays; tell him what is needed). dropped: no longer wanted. Also resume: carry on a paused job. Give a one-line summary.',
      input_schema: obj({ job: str('Job id or card id'), outcome: str('done, stuck, dropped or resume', { enum: ['done', 'stuck', 'dropped', 'resume'] }), summary: str('One line: what was done, or what is needed') }, ['job', 'outcome', 'summary']),
      check: (i) => { if (!deps.jobs.find(i.job)) throw new Error(`No job "${i.job}"`); },
      run: (i) => {
        const j = deps.jobs.find(i.job);
        if (i.outcome === 'resume') { deps.jobs.set(j.id, { status: 'running' }); deps.log?.(`Resumed ${j.id} "${j.title}"`); return { ok: true, job: j.id, status: 'running' }; }
        deps.jobs.set(j.id, { status: i.outcome, summary: String(i.summary || '').slice(0, 300) });
        if (i.outcome === 'done') { try { deps.kanban.moveTask(j.project, j.taskId, 'done'); } catch { /* best effort */ } }
        deps.unwatch?.(j.chatId);
        deps.log?.(`Job ${j.id} "${j.title}" ${i.outcome}: ${String(i.summary || '').slice(0, 200)}`);
        return { ok: true, job: j.id, status: i.outcome, card: i.outcome === 'done' ? 'moved to Done' : 'left where it is' };
      },
    },
  ];
}
export const JOB_TOOL_NAMES = ['start_job', 'list_jobs', 'job_note', 'finish_job'];
