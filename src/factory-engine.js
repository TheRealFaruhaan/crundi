/**
 * factory-engine.js — where control goes next. Pure: no I/O, no clocks.
 *
 * The runner owns processes, git and disk; this module owns the rules. Every
 * entry point takes a run state and returns a NEW state plus a list of effects
 * for the runner to carry out in order. A rejected handoff returns ok:false
 * and leaves the state untouched, so an employee can be told what is missing
 * and simply try again.
 *
 * ─── Tokens ───
 *
 * Control is a token sitting somewhere in the graph:
 *   work     at an agent node; that employee is working
 *   answer   at a decision answered by someone other than whoever arrived;
 *            that person is asked just the question(s)
 *   ask      at an Ask me node; waiting on the human
 *   join     parked at a Join until its sibling branches arrive
 *
 * Outside any fan-out there is exactly one token. A pick-many decision
 * replaces it with one token per picked branch (each in its own workspace when
 * there is more than one), and the matching Join merges them back into one.
 *
 * ─── Handoff payload ───
 *
 *   { summary, answers: { <decisionId>: 'yes'|'no'|<optionId>|[<optionIds>] },
 *     notes: { <edgeId or note-target nodeId>: text }, finish?, escalate? }
 *
 * Decisions are answered by the agent in front of them (walking through a
 * chain of decisions), unless decision.answeredBy names someone else. After a
 * Join nobody is "in front", so every decision there is asked separately (the
 * validator insists those name an answerer).
 *
 * ─── Ending ───
 *
 * A limited edge that has used its rounds is not followed: the token escalates
 * to the Manager instead. Crossing it again from a Manager handoff is allowed
 * once more; after that the path is closed and the Manager must Finish or talk
 * to the person. Repeating the same answers on an unchanged tree three times
 * escalates too (no progress), and maxHandoffs stops the run for the human.
 *
 * Spec: specs/factory.md §2.4, §3 (runtime).
 */

import { normalizeFactory, MANAGER_ID } from './factory-model.js';

const NO_PROGRESS_REPEATS = 3;
const MANAGER_REOPEN_LIMIT = 2;
const HISTORY_MAX = 2000;
const TEXT_MAX = 20_000;

// ─── Small helpers ───

const clone = (o) => (typeof structuredClone === 'function' ? structuredClone(o) : JSON.parse(JSON.stringify(o)));
const text = (v) => (typeof v === 'string' ? v : v == null ? '' : String(v)).slice(0, TEXT_MAX);
const stable = (v) => JSON.stringify(v, (k, x) => (x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.entries(x).sort()) : x));

function index(f) {
  const nodes = new Map(f.nodes.map(n => [n.id, n]));
  const roster = new Map(f.roster.map(r => [r.id, r]));
  const outEdge = new Map();
  const notesFrom = new Map();
  for (const e of f.edges) {
    if (e.kind === 'note') {
      if (!notesFrom.has(e.source)) notesFrom.set(e.source, []);
      notesFrom.get(e.source).push(e);
    } else outEdge.set(e.source + '|' + e.sourceHandle, e);
  }
  const manager = f.nodes.find(n => n.type === 'manager');
  const joinOf = new Map(f.nodes.filter(n => n.type === 'join').map(n => [n.join.forkId, n]));
  return { f, nodes, roster, outEdge, notesFrom, manager, joinOf };
}

/** Roster id of the person at an agent node. */
export function rosterOf(node) {
  if (!node) return '';
  if (node.type === 'manager') return MANAGER_ID;
  if (node.type === 'employee') return node.employeeId;
  return '';
}

function nameOf(ix, rosterId) {
  return ix.roster.get(rosterId)?.name || (rosterId === MANAGER_ID ? 'Manager' : rosterId || 'someone');
}

function nodeLabel(ix, node) {
  if (!node) return '?';
  if (node.label) return node.label;
  if (node.type === 'employee' || node.type === 'manager') return nameOf(ix, rosterOf(node));
  return node.type;
}

function handleLabel(node, h) {
  if (node.type === 'decision') {
    if (h === 'none') return 'none of these';
    if (h === 'yes' || h === 'no') return h;
    return node.decision.options.find(o => o.id === h)?.label || h;
  }
  if (node.type === 'ask') return node.ask.replies.find(r => r.id === h)?.label || h;
  return h;
}

// ─── Create ───

/**
 * A new run, with its token at Start. Call start() to get the first effects.
 * The factory is snapshotted into the run: later edits never reach it.
 */
export function createRun({ id, project, factory, task, now = '' }) {
  const f = normalizeFactory(factory);
  const start = f.nodes.find(n => n.type === 'start');
  return {
    id,
    project,
    task: text(task),
    snapshot: f,
    status: 'starting',
    createdAt: now,
    updatedAt: now,
    seq: 0,
    tokens: [{ id: 't0', nodeId: start ? start.id : '', ws: 'main', path: [], status: 'moving' }],
    groups: {},
    workspaces: { main: { id: 'main', parent: null, name: 'main' } },
    edgeCounts: {},
    exhausted: {},
    lastPicks: {},
    inbox: {},
    handoffs: 0,
    fingerprints: {},
    history: [],
    outcome: null,
  };
}

/** Move the token off Start: the Manager gets the task. */
export function start(run) {
  const r = clone(run);
  const ix = index(r.snapshot);
  const ctx = newCtx(r, ix, { from: null, summary: '', payload: {} });
  const t = r.tokens[0];
  const e = t && ix.outEdge.get(t.nodeId + '|out');
  if (!e) return { ok: false, error: 'This factory has no way from Start to the Manager.' };
  traverse(ctx, t, e, null);
  if (ctx.errors.length) return { ok: false, error: ctx.errors.join('\n') };
  r.status = 'running';
  log(r, { type: 'start' });
  return { ok: true, run: r, effects: ctx.effects };
}

// ─── Requirements: what a person must answer to hand off ───

/**
 * What the token's current person has to provide, as a tree the prompt
 * builder can print. Decisions down every branch are included, each marked
 * with the answer that leads to it, because which ones apply depends on the
 * answers given.
 */
export function requirements(run, tokenId) {
  const ix = index(run.snapshot);
  const t = run.tokens.find(x => x.id === tokenId);
  if (!t) return null;
  const node = ix.nodes.get(t.nodeId);
  if (t.status === 'work') {
    const who = rosterOf(node);
    const out = ix.outEdge.get(node.id + '|out');
    return {
      mode: 'work',
      who,
      canFinish: node.type === 'manager',
      route: out ? describeEdge(run, ix, out, who, new Set()) : null,
      noteTargets: (ix.notesFrom.get(node.id) || []).map(e => ({
        key: e.target, edgeId: e.id, label: e.noteLabel || '', to: nameOf(ix, rosterOf(ix.nodes.get(e.target))),
      })),
    };
  }
  if (t.status === 'answer') {
    return { mode: 'answer', who: t.answerer, canFinish: false, route: describeDecision(run, ix, node, t.answerer, new Set()), noteTargets: [] };
  }
  return null;
}

function describeEdge(run, ix, e, who, seen) {
  const target = ix.nodes.get(e.target);
  const item = { edgeId: e.id, requireNote: e.requireNote, noteLabel: e.noteLabel || '', maxRounds: e.maxRounds, used: run.edgeCounts[e.id] || 0 };
  // "None of these" straight into its Join passes through: say where it goes on to.
  if (target && target.type === 'join' && e.sourceHandle === 'none') {
    const on = ix.outEdge.get(target.id + '|out');
    if (on) return { ...describeEdge(run, ix, on, null, seen), requireNote: e.requireNote || on.requireNote };
  }
  if (target && target.type === 'decision' && (target.decision.answeredBy || who) === who && !seen.has(target.id)) {
    item.decision = describeDecision(run, ix, target, who, seen).decision;
  } else {
    item.to = describeTarget(ix, target, who);
  }
  return item;
}

function describeDecision(run, ix, node, who, seen) {
  const s = new Set(seen); s.add(node.id);
  const d = node.decision;
  const handles = d.kind === 'yesno' ? ['yes', 'no'] : d.kind === 'many' ? [...d.options.map(o => o.id), 'none'] : d.options.map(o => o.id);
  return {
    decision: {
      id: node.id,
      kind: d.kind,
      question: d.question,
      label: node.label,
      options: d.kind === 'yesno' ? [] : d.options.map(o => ({ id: o.id, label: o.label })),
      lastPicks: d.kind === 'many' ? (run.lastPicks[node.id] || null) : null,
      branches: handles.map(h => {
        const e = ix.outEdge.get(node.id + '|' + h);
        return { answer: h, label: handleLabel(node, h), next: e ? describeEdge(run, ix, e, who, s) : null };
      }),
    },
  };
}

function describeTarget(ix, node, who) {
  if (!node) return { type: 'missing' };
  if (node.type === 'decision') return { type: 'decision', nodeId: node.id, label: nodeLabel(ix, node), answeredBy: nameOf(ix, node.decision.answeredBy || who) };
  if (node.type === 'employee' || node.type === 'manager') return { type: node.type, nodeId: node.id, label: nodeLabel(ix, node), name: nameOf(ix, rosterOf(node)) };
  return { type: node.type, nodeId: node.id, label: nodeLabel(ix, node) };
}

// ─── Handoff ───

/**
 * A person has finished their step.
 * @param {object} run
 * @param {string} tokenId
 * @param {object} payload   see the header
 * @param {{ treeHash?: string, caps?: { maxHandoffs?: number } }} [meta]
 */
export function handoff(run, tokenId, payload, meta = {}) {
  const r = clone(run);
  const ix = index(r.snapshot);
  const t = r.tokens.find(x => x.id === tokenId);
  if (!t || (t.status !== 'work' && t.status !== 'answer')) return { ok: false, error: 'There is nothing waiting on you right now.' };
  const p = payload && typeof payload === 'object' ? payload : {};
  const node = ix.nodes.get(t.nodeId);
  const who = t.status === 'work' ? rosterOf(node) : t.answerer;
  // The Manager deciding, in either mode: it may reopen a used-up loop and
  // cannot escalate to itself. Only a working Manager can finish.
  const isManager = who === MANAGER_ID;
  const summary = text(p.summary).trim();
  if (!summary && !p.escalate) return { ok: false, error: 'Write a short summary of what you did (summary).' };
  if (p.answers != null && (typeof p.answers !== 'object' || Array.isArray(p.answers))) return { ok: false, error: 'answers must be an object keyed by decision id.' };
  if (p.notes != null && (typeof p.notes !== 'object' || Array.isArray(p.notes))) return { ok: false, error: 'notes must be an object keyed by id.' };

  const from = { nodeId: node.id, rosterId: who, name: nameOf(ix, who), label: nodeLabel(ix, node) };
  const ctx = newCtx(r, ix, { from, summary, payload: p, isManager });

  // Escalation: straight to the Manager, with the reason.
  if (p.escalate) {
    if (isManager) return { ok: false, error: 'You are the Manager; there is nobody to escalate to. Finish, or talk to the person in this chat.' };
    r.handoffs++;
    log(r, { type: 'escalate', nodeId: node.id, by: who, reason: text(p.escalate) });
    escalateToken(ctx, t, `${from.name} escalated: ${text(p.escalate)}`);
    return finishCtx(r, ctx);
  }

  // Unknown note keys are a mistake worth pointing out.
  const allowedNotes = new Set(r.snapshot.edges.filter(e => e.requireNote).map(e => e.id));
  if (t.status === 'work') for (const e of ix.notesFrom.get(node.id) || []) { allowedNotes.add(e.id); allowedNotes.add(e.target); }
  for (const k of Object.keys(p.notes || {})) {
    if (!allowedNotes.has(k)) return { ok: false, error: `notes.${k} is not a note you can leave here. Use the keys listed in your instructions.` };
  }

  r.handoffs++;
  const capHandoffs = meta.caps?.maxHandoffs || r.snapshot.settings.caps.maxHandoffs;

  // No progress: same step, same answers, same tree, again and again.
  if (t.status === 'work' && !isManager && meta.treeHash) {
    const fp = node.id + '|' + stable(p.answers || {}) + '|' + meta.treeHash;
    r.fingerprints[fp] = (r.fingerprints[fp] || 0) + 1;
    if (r.fingerprints[fp] >= NO_PROGRESS_REPEATS) {
      log(r, { type: 'no-progress', nodeId: node.id, by: who });
      deliverNotes(ctx, t, node);
      escalateToken(ctx, t, `No progress: "${from.label}" has handed off ${r.fingerprints[fp]} times with the same answers and no change to the code.`);
      return finishCtx(r, ctx);
    }
  }

  log(r, { type: 'handoff', nodeId: node.id, by: who, mode: t.status, summary, answers: p.answers || {}, finish: !!p.finish });

  if (isManager && t.status === 'work' && p.finish) {
    t.status = 'finished';
    r.status = 'finishing';
    ctx.keepStatus = true;
    r.outcome = { summary };
    ctx.effects.push({ type: 'finish', tokenId: t.id, summary });
    return finishCtx(r, ctx);
  }

  if (t.status === 'work') {
    deliverNotes(ctx, t, node);
    const out = ix.outEdge.get(node.id + '|out');
    if (!out) {
      return { ok: false, error: isManager ? 'There is no step after you in this factory. Finish (finish: true) when the work is done.' : 'This step has nowhere to go.' };
    }
    traverse(ctx, t, out, who);
  } else {
    evalDecision(ctx, t, node, who);
  }
  if (ctx.errors.length) return { ok: false, error: ctx.errors.join('\n') };

  if (r.handoffs >= capHandoffs) {
    r.status = 'waiting';
    ctx.keepStatus = true;
    ctx.effects.push({ type: 'cap', which: 'maxHandoffs', limit: capHandoffs });
  }
  return finishCtx(r, ctx);
}

/** The human answered an Ask me node. */
export function askReply(run, tokenId, replyId, note = '') {
  const r = clone(run);
  const ix = index(r.snapshot);
  const t = r.tokens.find(x => x.id === tokenId);
  if (!t || t.status !== 'ask') return { ok: false, error: 'That question is no longer waiting.' };
  const node = ix.nodes.get(t.nodeId);
  const e = ix.outEdge.get(node.id + '|' + replyId);
  if (!e) return { ok: false, error: 'That is not one of the replies.' };
  const ctx = newCtx(r, ix, { from: { nodeId: node.id, rosterId: '', name: 'You', label: nodeLabel(ix, node) }, summary: text(note), payload: {}, defaultNote: text(note) });
  log(r, { type: 'ask-reply', nodeId: node.id, reply: replyId, note: text(note) });
  traverse(ctx, t, e, null);
  if (ctx.errors.length) return { ok: false, error: ctx.errors.join('\n') };
  return finishCtx(r, ctx);
}

/**
 * Hand a token to the Manager with a reason: a stall, a merge conflict, a
 * crash the runner could not recover from. Aborts any fan-out it was in.
 */
export function escalate(run, tokenId, reason) {
  const r = clone(run);
  const ix = index(r.snapshot);
  const t = r.tokens.find(x => x.id === tokenId);
  if (!t) return { ok: false, error: 'No such token.' };
  const ctx = newCtx(r, ix, { from: null, summary: '', payload: {} });
  log(r, { type: 'escalate', nodeId: t.nodeId, reason: text(reason) });
  escalateToken(ctx, t, text(reason));
  return finishCtx(r, ctx);
}

/** The tokens currently waiting on a given person (work or answer). */
export function tokensFor(run, rosterId) {
  const ix = index(run.snapshot);
  return run.tokens.filter(t => (t.status === 'work' && rosterOf(ix.nodes.get(t.nodeId)) === rosterId) || (t.status === 'answer' && t.answerer === rosterId));
}

// ─── Routing internals ───

function newCtx(r, ix, { from, summary, payload, isManager = false, defaultNote = '' }) {
  return { r, ix, from, summary, payload, isManager, defaultNote, effects: [], errors: [] };
}

function finishCtx(r, ctx) {
  if (ctx.errors.length) return { ok: false, error: ctx.errors.join('\n') };
  if (!ctx.keepStatus) {
    r.status = r.tokens.some(t => t.status === 'work' || t.status === 'answer') ? 'running'
      : r.tokens.some(t => t.status === 'ask') ? 'waiting' : r.status;
  }
  return { ok: true, run: r, effects: ctx.effects };
}

function log(r, entry) {
  r.history.push({ seq: ++r.seq, ...entry });
  if (r.history.length > HISTORY_MAX) r.history.splice(0, r.history.length - HISTORY_MAX);
}

function newId(r, prefix) { return prefix + (++r.seq); }

function deliverNotes(ctx, t, node) {
  const { r, ix, payload, from } = ctx;
  for (const e of ix.notesFrom.get(node.id) || []) {
    const body = text((payload.notes || {})[e.id] || (payload.notes || {})[e.target]).trim();
    if (!body) continue;
    const to = rosterOf(ix.nodes.get(e.target));
    if (!r.inbox[to]) r.inbox[to] = [];
    r.inbox[to].push({ from: from.name, label: e.noteLabel || '', text: body });
  }
}

function traverse(ctx, t, e, who) {
  if (ctx.errors.length) return;
  const { r, ix } = ctx;
  if (e.maxRounds > 0) {
    const used = r.edgeCounts[e.id] || 0;
    if (used >= e.maxRounds) {
      const src = ix.nodes.get(e.source), dst = ix.nodes.get(e.target);
      const what = `"${nodeLabel(ix, src)}" → "${nodeLabel(ix, dst)}"`;
      const reopened = r.exhausted[e.id] || 0;
      if (ctx.isManager && reopened >= MANAGER_REOPEN_LIMIT) {
        ctx.errors.push(`The path ${what} has used all ${e.maxRounds} of its rounds and is now closed. Choose another answer, finish, or ask the person.`);
        return;
      }
      r.exhausted[e.id] = reopened + 1;
      if (ctx.isManager) {
        // The Manager itself chose to go round again once more: allowed, counted.
        r.edgeCounts[e.id] = used + 1;
      } else {
        log(r, { type: 'limit', edgeId: e.id });
        escalateToken(ctx, t, `The loop ${what} reached its limit of ${e.maxRounds} round${e.maxRounds === 1 ? '' : 's'}.`);
        return;
      }
    } else {
      r.edgeCounts[e.id] = used + 1;
    }
  } else {
    r.edgeCounts[e.id] = (r.edgeCounts[e.id] || 0) + 1;
  }
  let message = '';
  if (e.requireNote) {
    message = text((ctx.payload.notes || {})[e.id] || ctx.defaultNote).trim();
    if (!message) {
      ctx.errors.push(`This route needs a note: "${e.noteLabel || 'a note'}". Put it in notes.${e.id}.`);
      return;
    }
  }
  arrive(ctx, t, ix.nodes.get(e.target), who, message);
}

function arrive(ctx, t, node, who, message) {
  if (ctx.errors.length) return;
  const { r, ix } = ctx;
  if (!node) { ctx.errors.push('A route points at a node that does not exist.'); return; }
  t.nodeId = node.id;
  switch (node.type) {
    case 'employee':
    case 'manager': {
      t.status = 'work';
      t.answerer = undefined;
      activate(ctx, t, node, 'work', { message });
      return;
    }
    case 'decision': {
      const answerer = node.decision.answeredBy || who;
      if (!answerer || answerer !== who) {
        t.status = 'answer';
        t.answerer = answerer || MANAGER_ID;
        activate(ctx, t, node, 'answer', { message });
        return;
      }
      t.status = 'moving';
      if (message) t.carry = message;
      evalDecision(ctx, t, node, who);
      return;
    }
    case 'join': return handleJoin(ctx, t, node);
    case 'ask': {
      t.status = 'ask';
      ctx.effects.push({ type: 'ask', tokenId: t.id, nodeId: node.id, prompt: node.ask.prompt, replies: node.ask.replies, context: baseContext(ctx, message) });
      return;
    }
    default:
      ctx.errors.push(`The route reached "${nodeLabel(ix, node)}", which cannot take control. (Only the Manager's Finish reaches End.)`);
  }
}

function evalDecision(ctx, t, node, who) {
  const { r, ix } = ctx;
  const d = node.decision;
  const raw = (ctx.payload.answers || {})[node.id];
  const ask = `"${d.question || nodeLabel(ix, node)}"`;
  const follow = (h) => {
    const e = ix.outEdge.get(node.id + '|' + h);
    if (!e) { ctx.errors.push(`The answer "${handleLabel(node, h)}" to ${ask} leads nowhere.`); return; }
    traverse(ctx, t, e, who);
  };
  const matchOption = (v) => {
    if (typeof v !== 'string') return null;
    const s = v.trim().toLowerCase();
    return d.options.find(o => o.id.toLowerCase() === s || o.label.toLowerCase() === s)?.id || null;
  };

  if (d.kind === 'yesno') {
    const v = raw === true ? 'yes' : raw === false ? 'no' : typeof raw === 'string' ? raw.trim().toLowerCase() : '';
    if (v !== 'yes' && v !== 'no') { ctx.errors.push(`Answer ${ask} with "yes" or "no" in answers.${node.id}.`); return; }
    return follow(v);
  }
  if (d.kind === 'one') {
    const id = matchOption(raw);
    if (!id) { ctx.errors.push(`Answer ${ask} with one of: ${d.options.map(o => `"${o.id}"`).join(', ')} in answers.${node.id}.`); return; }
    return follow(id);
  }
  // many
  let picks;
  if (raw === undefined) {
    if (!r.lastPicks[node.id]) { ctx.errors.push(`Answer ${ask} with a list of options (${d.options.map(o => `"${o.id}"`).join(', ')}), or [] for none of these, in answers.${node.id}.`); return; }
    picks = r.lastPicks[node.id].slice();
  } else {
    const list = raw === 'none' ? [] : Array.isArray(raw) ? raw : [raw];
    picks = [];
    for (const v of list) {
      const id = matchOption(v);
      if (!id) { ctx.errors.push(`"${v}" is not an option of ${ask}. Options: ${d.options.map(o => `"${o.id}"`).join(', ')}.`); return; }
      if (!picks.includes(id)) picks.push(id);
    }
  }
  r.lastPicks[node.id] = picks.slice();
  if (!picks.length) return follow('none');
  fanOut(ctx, t, node, picks, who);
}

function fanOut(ctx, t, node, picks, who) {
  const { r, ix } = ctx;
  const join = ix.joinOf.get(node.id);
  if (!join) { ctx.errors.push(`"${nodeLabel(ix, node)}" has no Join.`); return; }
  const gid = newId(r, 'g');
  const group = { id: gid, forkId: node.id, joinId: join.id, parentWs: t.ws, parentPath: t.path.slice(), branches: [] };
  r.groups[gid] = group;
  r.tokens = r.tokens.filter(x => x.id !== t.id);
  const children = [];
  for (const pick of picks) {
    let ws = t.ws;
    if (picks.length > 1) {
      ws = newId(r, 'w');
      const name = handleLabel(node, pick);
      r.workspaces[ws] = { id: ws, parent: t.ws, name };
      ctx.effects.push({ type: 'workspace', wsId: ws, parentWs: t.ws, name });
    }
    const child = { id: newId(r, 't'), nodeId: node.id, ws, path: [...t.path, gid], status: 'moving' };
    if (t.carry) child.carry = t.carry;
    r.tokens.push(child);
    group.branches.push({ tokenId: child.id, pick, ws, done: false, summary: '' });
    children.push([child, pick]);
  }
  for (const [child, pick] of children) {
    const e = ix.outEdge.get(node.id + '|' + pick);
    if (!e) { ctx.errors.push(`The option "${handleLabel(node, pick)}" leads nowhere.`); return; }
    traverse(ctx, child, e, who);
  }
}

function handleJoin(ctx, t, join) {
  const { r, ix } = ctx;
  const gid = t.path[t.path.length - 1];
  const g = gid && r.groups[gid];
  const out = ix.outEdge.get(join.id + '|out');
  if (!g || g.joinId !== join.id) {
    // Arrived by "none of these": nothing to wait for.
    if (!out) { ctx.errors.push(`"${nodeLabel(ix, join)}" leads nowhere.`); return; }
    t.status = 'moving';
    return traverse(ctx, t, out, null);
  }
  const b = g.branches.find(x => x.tokenId === t.id);
  if (b) { b.done = true; b.summary = ctx.summary ? `${ctx.from?.name || ''}: ${ctx.summary}` : b.summary; }
  t.status = 'join';
  if (!g.branches.every(x => x.done)) return;

  // Everyone is here: merge the branch workspaces and go on as one.
  r.tokens = r.tokens.filter(x => !g.branches.some(br => br.tokenId === x.id));
  delete r.groups[gid];
  const cont = { id: newId(r, 't'), nodeId: join.id, ws: g.parentWs, path: g.parentPath.slice(), status: 'moving' };
  r.tokens.push(cont);
  const from = [...new Set(g.branches.map(x => x.ws).filter(w => w !== g.parentWs))];
  if (from.length) ctx.effects.push({ type: 'merge', groupId: gid, into: g.parentWs, from, contTokenId: cont.id });
  for (const w of from) delete r.workspaces[w];
  const joined = g.branches.map(x => x.summary).filter(Boolean).join('\n');
  log(r, { type: 'join', nodeId: join.id, group: gid });
  if (!out) { ctx.errors.push(`"${nodeLabel(ix, join)}" leads nowhere.`); return; }
  const saved = ctx.summary;
  ctx.summary = joined || saved;
  ctx.from = { nodeId: join.id, rosterId: '', name: 'The branches', label: nodeLabel(ix, join) };
  traverse(ctx, cont, out, null);
}

function escalateToken(ctx, t, reason) {
  const { r, ix } = ctx;
  // Leaving a fan-out early: stop the other branches, keep their work.
  if (t.path.length) {
    const outer = r.groups[t.path[0]];
    const inGroup = new Set(Object.values(r.groups).filter(g => g.id === outer.id || g.parentPath.includes(outer.id)).map(g => g.id));
    const others = r.tokens.filter(x => x.id !== t.id && x.path.some(p => inGroup.has(p)));
    if (others.length) ctx.effects.push({ type: 'cancel', tokenIds: others.map(x => x.id), reason });
    const from = [...new Set([t.ws, ...others.map(x => x.ws)].filter(w => w !== outer.parentWs))];
    r.tokens = r.tokens.filter(x => !others.includes(x));
    if (from.length) ctx.effects.push({ type: 'merge', groupId: outer.id, into: outer.parentWs, from, contTokenId: t.id, abort: true });
    for (const w of from) delete r.workspaces[w];
    for (const g of inGroup) delete r.groups[g];
    t.ws = outer.parentWs;
    t.path = outer.parentPath.slice();
  }
  t.nodeId = ix.manager.id;
  t.status = 'work';
  t.answerer = undefined;
  activate(ctx, t, ix.manager, 'work', { escalation: reason });
}

function baseContext(ctx, message) {
  return {
    from: ctx.from ? { name: ctx.from.name, label: ctx.from.label } : null,
    summary: ctx.summary || '',
    message: message || '',
  };
}

function activate(ctx, t, node, mode, { message = '', escalation = '' } = {}) {
  const { r } = ctx;
  const who = mode === 'answer' ? t.answerer : rosterOf(node);
  const carried = t.carry || '';
  delete t.carry;
  const inbox = mode === 'work' ? (r.inbox[who] || []) : [];
  if (mode === 'work') delete r.inbox[who];
  ctx.effects.push({
    type: 'activate',
    tokenId: t.id,
    nodeId: node.id,
    employeeId: who,
    mode,
    ws: t.ws,
    context: { ...baseContext(ctx, message || carried), notes: inbox, escalation },
  });
}
