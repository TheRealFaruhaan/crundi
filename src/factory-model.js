/**
 * factory-model.js — what a factory is, and whether it is allowed to run.
 *
 * A factory is a workflow template: a roster of AI employees and a graph of
 * nodes and edges that routes a task between them. This module is pure (no
 * I/O): it shapes untrusted definitions into a known form (normalize) and
 * proves a definition can always finish (validate). The runner and the editor
 * both lean on the graph helpers exported at the bottom.
 *
 * Spec: specs/factory.md §2 and §3.
 *
 * ─── Shape ───
 *
 *   { id, name, description, version, createdAt, updatedAt,
 *     settings: { managerMode, mergeMode, setupCommand, secrets:[secretId], caps:{ maxHandoffs, maxMinutes, tokenBudget } },
 *     roster:   [ { id, name, persona, model, effort, memory, tools:{ allow, deny }, skills } ],
 *     nodes:    [ { id, type, label, position:{x,y}, employeeId?, decision?, ask?, join? } ],
 *     edges:    [ { id, kind, source, sourceHandle, target, requireNote, noteLabel, maxRounds } ] }
 *
 * Node types and their outputs (sourceHandle of the edges leaving them):
 *
 *   start     out                      — exactly one, into the Manager
 *   manager   out, finish              — finish is wired to End and cannot be removed
 *   employee  out
 *   decision  yes,no | <option ids> | <option ids>,none (pick many)
 *   join      out                      — closes the fan-out named in join.forkId
 *   ask       <reply ids>              — a question for the human
 *   end       (none)
 *
 * Every output takes exactly one flow edge. Note edges (kind 'note') leave
 * from the special handle 'note' of an agent node (manager / employee) to
 * another agent node; they carry a message and never move control.
 *
 * ─── Why it always ends ───
 *
 * validate() refuses to run a factory unless:
 *   - End is reachable from every node, and every node from Start;
 *   - every decision answer and every ask reply is wired;
 *   - every cycle passes through an edge with a round limit (so with all
 *     limited edges removed the graph is acyclic);
 *   - every pick-many fan-out is closed by exactly one Join, and its branches
 *     neither jump out before the Join nor are jumped into from outside.
 *
 * At runtime a used-up limited edge disappears and control escalates to the
 * Manager, whose Finish always reaches End; run caps bound the rest. So a
 * valid factory cannot loop forever.
 */

import { mentionsInFactory } from './factory-skills.js';
import { normalizeTemplate } from './factory-cr.js';

// ─── Constants ───

export const NODE_TYPES = ['start', 'manager', 'employee', 'decision', 'join', 'ask', 'wait', 'end'];
/**
 * Wait kinds: a fixed time, until a clock time, until a command succeeds
 * (polled in the worktree), or until a signal (a secret URL, or the person).
 * Every wait is bounded, so it never breaks the guarantee that a run ends;
 * the last two have a "Timed out" route.
 */
export const WAIT_KINDS = ['time', 'until', 'command', 'signal'];
export const AGENT_TYPES = ['manager', 'employee'];
export const DECISION_KINDS = ['yesno', 'one', 'many'];
export const EDGE_KINDS = ['flow', 'note'];
export const MEMORY_MODES = ['keep', 'fresh'];
export const MANAGER_MODES = ['normal', 'bypass'];
export const MERGE_MODES = ['ask', 'auto'];
export const MANAGER_ID = 'manager';

export const LIMITS = {
  nodes: 200,
  edges: 600,
  roster: 50,
  options: 12,
  name: 80,
  label: 120,
  text: 20_000,        // persona, question, prompt
  short: 500,          // note labels, setup command
  rounds: 100,
};

export const DEFAULT_CAPS = { maxHandoffs: 200, maxMinutes: 8 * 60, tokenBudget: 0 }; // 0 = no budget
export const TEAM_MODES = ['ask', 'bypass'];

/**
 * Permission presets: kinds of request the whole team may make from the
 * start, so they never reach the Manager. 'tests' also grants the project's
 * own test/build/lint scripts, read at run start (factory-permissions.js).
 */
export const PERMISSION_PRESETS = {
  'git-read': { label: 'Read git (status, diff, log)', rules: ['Bash(git status:*)', 'Bash(git diff:*)', 'Bash(git log:*)', 'Bash(git show:*)', 'Bash(git branch --show-current:*)'] },
  tests: { label: 'Tests, builds and linters', rules: ['Bash(npm test:*)', 'Bash(npm run test:*)', 'Bash(npm run build:*)', 'Bash(npm run lint:*)', 'Bash(npx tsc:*)', 'Bash(pytest:*)', 'Bash(python -m pytest:*)', 'Bash(cargo test:*)', 'Bash(cargo build:*)', 'Bash(go test:*)', 'Bash(go build:*)', 'Bash(make test:*)', 'Bash(make build:*)', 'Bash(make lint:*)'] },
  packages: { label: 'Install packages', rules: ['Bash(npm install:*)', 'Bash(npm ci:*)', 'Bash(yarn add:*)', 'Bash(yarn install:*)', 'Bash(pnpm add:*)', 'Bash(pnpm install:*)', 'Bash(pip install:*)', 'Bash(cargo add:*)', 'Bash(go get:*)'] },
  web: { label: 'Search the web and read docs', rules: ['WebSearch', 'WebFetch'] },
};
export const DEFAULT_PRESETS = ['git-read', 'tests'];

export const DEFAULT_MANAGER_PERSONA = [
  'You are the Manager of this factory. You own the task from start to finish.',
  'Understand what is being asked, decide what has to happen, and route the work through the flow by answering its questions honestly.',
  'You do not write the code yourself; your team does. You read, plan, judge and decide.',
  'When employees ask for permission, decide on the merits: allow what the task genuinely needs, deny what is risky or off-task, and say why.',
  'When the work is complete and you are satisfied with it, finish: present the result clearly, with what changed, why, and how to look at it.',
].join('\n');

const ID_RE = /^[A-Za-z0-9_-]{1,64}$/;
const RESERVED_HANDLES = new Set(['out', 'finish', 'note', 'none', 'yes', 'no', 'done', 'timeout']);

// ─── Small helpers ───

// Web Crypto, not node:crypto: the editor bundles this module and runs the
// same validation in the browser. Node 18+ has globalThis.crypto too.
export function genId(prefix = '') {
  const b = new Uint8Array(6);
  globalThis.crypto.getRandomValues(b);
  return prefix + Array.from(b, x => x.toString(16).padStart(2, '0')).join('');
}

const str = (v, max) => (typeof v === 'string' ? v : v == null ? '' : String(v)).slice(0, max);
const oneOf = (v, list, dflt) => (list.includes(v) ? v : dflt);
const intIn = (v, lo, hi, dflt) => {
  const n = Number(v);
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, Math.round(n))) : dflt;
};
const cleanId = (v, prefix) => (typeof v === 'string' && ID_RE.test(v) ? v : genId(prefix));
const strList = (v, max, each) => (Array.isArray(v) ? v : []).filter(x => typeof x === 'string' && x).slice(0, max).map(x => x.slice(0, each));

// ─── Normalize ───

export const MAX_SECRETS = 20;
const SECRET_ID_RE = /^[A-Za-z0-9_-]{1,64}$/;

/**
 * settings.secrets, shaped: a list of secret ids, nothing else. Entries may
 * come as ids or as { id, name } (a client that sent what it showed); only
 * the id is kept, and anything that looks like a value or a PIN is dropped.
 */
export function normalizeSecretIds(v) {
  const out = [];
  for (const x of Array.isArray(v) ? v : []) {
    const id = typeof x === 'string' ? x : (x && typeof x === 'object' && typeof x.id === 'string' ? x.id : '');
    if (SECRET_ID_RE.test(id) && !out.includes(id)) out.push(id);
    if (out.length >= MAX_SECRETS) break;
  }
  return out;
}

/**
 * Shape an untrusted definition into the known form. It never throws and never
 * "fixes" the graph: wrong wiring is kept as it is so validate() can say what
 * is wrong and where. Only types, lengths and missing ids are corrected.
 */
export function normalizeFactory(input) {
  const f = input && typeof input === 'object' ? input : {};
  const s = f.settings && typeof f.settings === 'object' ? f.settings : {};
  const caps = s.caps && typeof s.caps === 'object' ? s.caps : {};

  const out = {
    id: typeof f.id === 'string' && ID_RE.test(f.id) ? f.id : '',
    name: str(f.name, LIMITS.name).trim() || 'Untitled factory',
    description: str(f.description, LIMITS.text),
    version: intIn(f.version, 0, Number.MAX_SAFE_INTEGER, 0),
    createdAt: typeof f.createdAt === 'string' ? f.createdAt : '',
    updatedAt: typeof f.updatedAt === 'string' ? f.updatedAt : '',
    settings: {
      managerMode: oneOf(s.managerMode, MANAGER_MODES, 'normal'),
      mergeMode: oneOf(s.mergeMode, MERGE_MODES, 'ask'),
      setupCommand: str(s.setupCommand, LIMITS.short),
      // Pick a crashed run up again by itself (with retries) instead of
      // waiting for the person to press Recover.
      autoRecover: !!s.autoRecover,
      // The team's own permissions: 'ask' sends requests to the Manager (or
      // the Gatekeeper); 'bypass' runs the team unchecked, like a bypassed
      // Manager. The fixed deny list applies either way.
      teamMode: oneOf(s.teamMode, TEAM_MODES, 'ask'),
      presets: Array.isArray(s.presets) ? s.presets.filter(k => Object.prototype.hasOwnProperty.call(PERMISSION_PRESETS, k)) : DEFAULT_PRESETS.slice(),
      // Rules written by hand in the factory, e.g. Bash(docker compose up:*).
      rules: strList(s.rules, 50, 200).map(x => x.trim()).filter(Boolean),
      // A cheap session that answers the team's permission requests by the
      // Manager's policy and passes only the unclear ones to the Manager.
      // May the Manager set standing rules ("allow npm test for the rest of
      // the run") instead of answering every request one by one?
      managerRules: s.managerRules === undefined ? true : !!s.managerRules,
      // The change request form the run starts from (factory-cr.js).
      crTemplate: normalizeTemplate(s.crTemplate),
      // How hard a run may use the person's Claude usage. All the person's
      // choice: 100 / 0 keep today's behaviour (pause only when a window is
      // full; no limit on people at once).
      limits: normalizeLimits(s.limits),
      // The Manager's check-in ticker, in minutes (5 to 1440). 0: the Manager
      // chooses it at the start of each run (factory_tick).
      tickMinutes: (() => { const n = Math.round(Number(s.tickMinutes)); return Number.isFinite(n) && n > 0 ? Math.min(1440, Math.max(5, n)) : 0; })(),
      // Secrets the run may use without the person typing a PIN at each use
      // (factory-secrets.js): ids only, never a value or a PIN. The PINs are
      // asked when a run starts and kept in memory for that run.
      secrets: normalizeSecretIds(s.secrets),
      gatekeeper: {
        enabled: !!(s.gatekeeper && s.gatekeeper.enabled),
        model: str(s.gatekeeper && s.gatekeeper.model, 60) || 'haiku',
      },
      caps: {
        maxHandoffs: intIn(caps.maxHandoffs, 1, 10_000, DEFAULT_CAPS.maxHandoffs),
        maxMinutes: intIn(caps.maxMinutes, 1, 7 * 24 * 60, DEFAULT_CAPS.maxMinutes),
        tokenBudget: intIn(caps.tokenBudget, 0, 1e12, DEFAULT_CAPS.tokenBudget),
      },
    },
    roster: [],
    nodes: [],
    edges: [],
  };

  const seenRoster = new Set();
  for (const r of (Array.isArray(f.roster) ? f.roster : []).slice(0, LIMITS.roster)) {
    if (!r || typeof r !== 'object') continue;
    let id = r.id === MANAGER_ID ? MANAGER_ID : cleanId(r.id, 'emp_');
    while (seenRoster.has(id)) id = genId('emp_');
    seenRoster.add(id);
    const tools = r.tools && typeof r.tools === 'object' ? r.tools : {};
    out.roster.push({
      id,
      name: str(r.name, LIMITS.name).trim(),
      persona: str(r.persona, LIMITS.text),
      model: str(r.model, 60),
      effort: str(r.effort, 20),
      memory: oneOf(r.memory, MEMORY_MODES, 'keep'),
      tools: { allow: strList(tools.allow, 100, 200), deny: strList(tools.deny, 100, 200) },
      skills: strList(r.skills, 50, 100),
      // Permission presets for this person only (the factory's apply to everyone).
      presets: Array.isArray(r.presets) ? r.presets.filter(k => Object.prototype.hasOwnProperty.call(PERMISSION_PRESETS, k)) : [],
    });
  }

  const seenNodes = new Set();
  for (const n of (Array.isArray(f.nodes) ? f.nodes : []).slice(0, LIMITS.nodes)) {
    if (!n || typeof n !== 'object') continue;
    let id = cleanId(n.id, 'n_');
    while (seenNodes.has(id)) id = genId('n_');
    seenNodes.add(id);
    const pos = n.position && typeof n.position === 'object' ? n.position : {};
    const node = {
      id,
      type: typeof n.type === 'string' ? n.type.slice(0, 20) : '',
      label: str(n.label, LIMITS.label),
      position: { x: Number.isFinite(+pos.x) ? +pos.x : 0, y: Number.isFinite(+pos.y) ? +pos.y : 0 },
    };
    if (node.type === 'employee') node.employeeId = typeof n.employeeId === 'string' ? n.employeeId : '';
    if (node.type === 'decision') {
      const d = n.decision && typeof n.decision === 'object' ? n.decision : {};
      node.decision = {
        kind: oneOf(d.kind, DECISION_KINDS, 'yesno'),
        question: str(d.question, LIMITS.text),
        options: normalizeChoices(d.options),
        answeredBy: typeof d.answeredBy === 'string' && d.answeredBy ? d.answeredBy : null,
        // A review gate: "no" needs an open blocking finding, "yes" needs none.
        review: !!d.review,
      };
      if (node.decision.kind === 'yesno') node.decision.options = [];
    }
    if (node.type === 'ask') {
      const a = n.ask && typeof n.ask === 'object' ? n.ask : {};
      node.ask = { prompt: str(a.prompt, LIMITS.text), replies: normalizeChoices(a.replies) };
    }
    if (node.type === 'wait') {
      const w = n.wait && typeof n.wait === 'object' ? n.wait : {};
      node.wait = {
        kind: oneOf(w.kind, WAIT_KINDS, 'time'),
        minutes: intIn(w.minutes, 1, 10080, 10),
        at: /^([01]\d|2[0-3]):[0-5]\d$/.test(String(w.at || '')) ? String(w.at) : '09:00',
        command: str(w.command, 1000),
        every: intIn(w.every, 15, 3600, 60),
        timeout: intIn(w.timeout, 1, 10080, 60),
        what: str(w.what, LIMITS.text),
      };
    }
    if (node.type === 'join') {
      const j = n.join && typeof n.join === 'object' ? n.join : {};
      node.join = { forkId: typeof j.forkId === 'string' ? j.forkId : '' };
    }
    out.nodes.push(node);
  }

  const seenEdges = new Set();
  for (const e of (Array.isArray(f.edges) ? f.edges : []).slice(0, LIMITS.edges)) {
    if (!e || typeof e !== 'object') continue;
    let id = cleanId(e.id, 'e_');
    while (seenEdges.has(id)) id = genId('e_');
    seenEdges.add(id);
    const kind = oneOf(e.kind, EDGE_KINDS, 'flow');
    const edge = {
      id,
      kind,
      source: typeof e.source === 'string' ? e.source : '',
      sourceHandle: kind === 'note' ? 'note' : (typeof e.sourceHandle === 'string' && e.sourceHandle ? e.sourceHandle.slice(0, 64) : 'out'),
      target: typeof e.target === 'string' ? e.target : '',
      requireNote: kind === 'flow' && !!e.requireNote,
      noteLabel: str(e.noteLabel, LIMITS.short),
      maxRounds: kind === 'flow' && e.maxRounds != null && e.maxRounds !== '' && +e.maxRounds > 0
        ? intIn(e.maxRounds, 1, LIMITS.rounds, 1) : 0,
    };
    out.edges.push(edge);
  }

  return out;
}

function normalizeChoices(list) {
  const seen = new Set();
  const out = [];
  for (const o of (Array.isArray(list) ? list : []).slice(0, LIMITS.options)) {
    if (!o || typeof o !== 'object') continue;
    let id = typeof o.id === 'string' && ID_RE.test(o.id) ? o.id : genId('o_');
    while (seen.has(id)) id = genId('o_');
    seen.add(id);
    out.push({ id, label: str(o.label, LIMITS.label).trim() });
  }
  return out;
}

// ─── Skeleton ───

/** The smallest factory that validates: Start -> Manager -> (finish) End. */
/**
 * A run's usage limits, cleaned.
 *   pauseAt5h, pauseAtWeek  pause when the 5-hour / weekly window reaches this % (50-100)
 *   maxParallel             at most this many people working at once (0: no limit)
 *   oneAtATimeAbove         above this % of the 5-hour window, one at a time (0: off)
 */
export function normalizeLimits(l) {
  const x = l && typeof l === 'object' ? l : {};
  const pct = (v, lo, dflt) => { const n = Math.round(Number(v)); return Number.isFinite(n) && n > 0 ? Math.min(100, Math.max(lo, n)) : dflt; };
  const n = Math.round(Number(x.maxParallel));
  return {
    pauseAt5h: pct(x.pauseAt5h, 50, 100),
    pauseAtWeek: pct(x.pauseAtWeek, 50, 100),
    maxParallel: Number.isFinite(n) && n > 0 ? Math.min(20, n) : 0,
    oneAtATimeAbove: pct(x.oneAtATimeAbove, 10, 0),
  };
}

export function skeletonFactory(name = 'New factory') {
  return normalizeFactory({
    name,
    roster: [{ id: MANAGER_ID, name: 'Manager', persona: DEFAULT_MANAGER_PERSONA, memory: 'keep' }],
    nodes: [
      { id: 'start', type: 'start', label: 'Task', position: { x: 0, y: 0 } },
      { id: 'manager', type: 'manager', label: 'Manager', position: { x: 0, y: 120 } },
      { id: 'end', type: 'end', label: 'Result', position: { x: 0, y: 260 } },
    ],
    edges: [
      { id: 'e_start', kind: 'flow', source: 'start', sourceHandle: 'out', target: 'manager' },
      { id: 'e_finish', kind: 'flow', source: 'manager', sourceHandle: 'finish', target: 'end' },
    ],
  });
}

// ─── Graph helpers (shared with the runner) ───

/** The output handles a node offers; [] for End and for broken nodes. */
export function outputsOf(node) {
  switch (node?.type) {
    case 'start': case 'employee': case 'join': return ['out'];
    case 'manager': return ['out', 'finish'];
    case 'decision': {
      const d = node.decision || {};
      if (d.kind === 'yesno') return ['yes', 'no'];
      const ids = (d.options || []).map(o => o.id);
      return d.kind === 'many' ? [...ids, 'none'] : ids;
    }
    case 'ask': return (node.ask?.replies || []).map(r => r.id);
    case 'wait': return node.wait && (node.wait.kind === 'command' || node.wait.kind === 'signal') ? ['done', 'timeout'] : ['out'];
    default: return [];
  }
}

/** Which outputs must be wired. Manager's 'out' is optional (it may only Finish). */
export function requiredOutputsOf(node) {
  return node?.type === 'manager' ? ['finish'] : outputsOf(node);
}

/** Flow edges whose source and target both exist, as adjacency lists. */
export function flowGraph(f, { exclude } = {}) {
  const ids = new Set(f.nodes.map(n => n.id));
  const out = new Map(), inn = new Map();
  for (const id of ids) { out.set(id, []); inn.set(id, []); }
  for (const e of f.edges) {
    if (e.kind !== 'flow' || !ids.has(e.source) || !ids.has(e.target)) continue;
    if (exclude && exclude(e)) continue;
    out.get(e.source).push(e);
    inn.get(e.target).push(e);
  }
  return { out, inn };
}

/** Node ids reachable from roots by following edges forward (or backward). */
export function reachable(graph, roots, { backward = false, stopAt } = {}) {
  const seen = new Set();
  const stack = [...roots].filter(r => graph.out.has(r));
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    if (stopAt && stopAt.has(id)) continue;
    const list = backward ? graph.inn.get(id) : graph.out.get(id);
    for (const e of list) stack.push(backward ? e.source : e.target);
  }
  return seen;
}

/**
 * Strongly connected components (Tarjan, iterative). Returns only the ones that
 * are real cycles: more than one node, or one node with an edge to itself.
 */
export function cycles(graph) {
  let index = 0;
  const idx = new Map(), low = new Map(), onStack = new Set(), stack = [], result = [];
  for (const start of graph.out.keys()) {
    if (idx.has(start)) continue;
    const work = [[start, 0]];
    idx.set(start, index); low.set(start, index); index++;
    stack.push(start); onStack.add(start);
    while (work.length) {
      const frame = work[work.length - 1];
      const [v, i] = frame;
      const edges = graph.out.get(v);
      if (i < edges.length) {
        frame[1]++;
        const w = edges[i].target;
        if (!idx.has(w)) {
          idx.set(w, index); low.set(w, index); index++;
          stack.push(w); onStack.add(w);
          work.push([w, 0]);
        } else if (onStack.has(w)) {
          low.set(v, Math.min(low.get(v), idx.get(w)));
        }
      } else {
        work.pop();
        if (work.length) {
          const parent = work[work.length - 1][0];
          low.set(parent, Math.min(low.get(parent), low.get(v)));
        }
        if (low.get(v) === idx.get(v)) {
          const comp = [];
          let w;
          do { w = stack.pop(); onStack.delete(w); comp.push(w); } while (w !== v);
          const selfLoop = comp.length === 1 && graph.out.get(v).some(e => e.target === v);
          if (comp.length > 1 || selfLoop) result.push(comp);
        }
      }
    }
  }
  return result;
}

/**
 * The region of a pick-many fan-out: every node reachable from its branch
 * targets without passing through the Join. Used by the validator and, later,
 * by the runner to know which branches a Join is waiting for.
 */
export function forkRegion(f, graph, forkNode, joinId) {
  const branchTargets = graph.out.get(forkNode.id)
    .filter(e => e.sourceHandle !== 'none')
    .map(e => e.target)
    .filter(t => t !== joinId);
  const region = reachable(graph, branchTargets, { stopAt: new Set([joinId]) });
  region.delete(joinId);
  return region;
}

// ─── Validate ───

/**
 * @returns {{ ok:boolean, errors:Issue[], warnings:Issue[] }}
 * Issue: { code, message, fix, nodeId?, edgeId?, nodeIds? }
 *
 * ok means runnable. A factory with errors can still be saved, as a draft.
 * Pass { skills: Set<string> } (the names known on this machine) to be warned
 * about mentioned skills that are not among them. Only a warning: factories
 * are global and a project may bring the skill itself.
 */
export function validateFactory(input, { skills = null } = {}) {
  const f = normalizeFactory(input);
  const errors = [], warnings = [];
  const err = (code, message, fix, at = {}) => errors.push({ code, message, fix, ...at });
  const warn = (code, message, fix, at = {}) => warnings.push({ code, message, fix, ...at });

  const nodes = new Map(f.nodes.map(n => [n.id, n]));
  const roster = new Map(f.roster.map(r => [r.id, r]));
  const label = n => (n ? (n.label || n.type) : '?');

  // ── Roster ──
  const managerEntry = roster.get(MANAGER_ID);
  if (!managerEntry) err('roster-no-manager', 'The roster has no Manager.', 'Add the Manager back to the roster.');
  const names = new Map();
  for (const r of f.roster) {
    if (!r.name) err('roster-unnamed', 'An employee has no name.', 'Give every employee a name.', { employeeId: r.id });
    else {
      const k = r.name.toLowerCase();
      if (names.has(k)) err('roster-duplicate-name', `Two employees are called "${r.name}".`, 'Give each employee a different name.', { employeeId: r.id });
      names.set(k, r.id);
    }
    if (!r.persona.trim()) warn('roster-no-persona', `"${r.name || 'An employee'}" has no persona.`, 'Describe who this employee is and how they work.', { employeeId: r.id });
  }

  // ── Hand-written approval rules ──
  for (const rule of f.settings.rules) {
    const shape = /^[A-Za-z_][\w.-]*(\(.+\))?$/.test(rule);
    const tooBroad = rule === 'Bash' || /^Bash\(\s*\*?\s*:?\s*\*?\s*\)$/.test(rule);
    const forbidden = /(^|[\s(])(sudo|su|doas)\b|git\s+push|git\s+remote/.test(rule);
    if (!shape || tooBroad || forbidden) warn('rule-invalid', `The approval rule "${rule}" ${!shape ? 'is not a rule (write Tool or Tool(spec), e.g. Bash(npm test:*))' : tooBroad ? 'allows every shell command, which is too broad' : 'allows something that is never allowed'}; it will be ignored.`, 'Fix or remove it in the factory settings.', {});
  }

  // ── Nodes ──
  const byType = t => f.nodes.filter(n => n.type === t);
  const starts = byType('start'), managers = byType('manager'), ends = byType('end');
  if (starts.length !== 1) err(starts.length ? 'start-many' : 'start-missing',
    starts.length ? 'There is more than one Start.' : 'There is no Start.',
    starts.length ? 'Keep one Start and delete the others.' : 'Add a Start node and connect it to the Manager.',
    starts.length > 1 ? { nodeIds: starts.map(n => n.id) } : {});
  if (managers.length !== 1) err(managers.length ? 'manager-many' : 'manager-missing',
    managers.length ? 'There is more than one Manager node.' : 'There is no Manager node.',
    managers.length ? 'A factory has exactly one Manager. Delete the extra ones.' : 'Add the Manager node.',
    managers.length > 1 ? { nodeIds: managers.map(n => n.id) } : {});
  if (ends.length !== 1) err(ends.length ? 'end-many' : 'end-missing',
    ends.length ? 'There is more than one End.' : 'There is no End.',
    ends.length ? 'Keep one End and delete the others.' : 'Add an End node and connect the Manager\'s Finish to it.',
    ends.length > 1 ? { nodeIds: ends.map(n => n.id) } : {});

  const usedEmployees = new Set();
  for (const n of f.nodes) {
    if (!NODE_TYPES.includes(n.type)) { err('node-unknown-type', `"${label(n)}" is not a known kind of node.`, 'Delete it and add the node again.', { nodeId: n.id }); continue; }
    if (n.type === 'employee') {
      const r = roster.get(n.employeeId);
      if (!n.employeeId || !r) err('employee-unassigned', `"${label(n)}" has no employee assigned.`, 'Pick someone from the roster for this node.', { nodeId: n.id });
      else if (r.id === MANAGER_ID) err('employee-is-manager', `"${label(n)}" uses the Manager as an employee.`, 'Route to the Manager node instead, or pick another employee.', { nodeId: n.id });
      else usedEmployees.add(r.id);
    }
    if (n.type === 'decision') {
      const d = n.decision;
      if (!d.question.trim()) err('decision-no-question', `A decision has no question.`, 'Write the question this decision asks.', { nodeId: n.id });
      if (d.kind !== 'yesno') {
        if (d.options.length < 2) err('decision-few-options', `"${label(n)}" needs at least two options.`, 'Add another option.', { nodeId: n.id });
        for (const o of d.options) {
          if (!o.label) err('decision-option-unlabelled', `An option of "${label(n)}" has no label.`, 'Label every option.', { nodeId: n.id });
          if (RESERVED_HANDLES.has(o.id)) err('decision-option-reserved', `An option of "${label(n)}" uses a reserved id.`, 'Delete the option and add it again.', { nodeId: n.id });
        }
      }
      if (d.review && d.kind !== 'yesno') warn('review-not-yesno', `"${label(n)}" is marked as a review but is not a yes/no question, so the mark does nothing.`, 'Make it a yes/no question, or unmark it.', { nodeId: n.id });
      if (d.answeredBy && !roster.has(d.answeredBy)) err('decision-answerer-missing', `"${label(n)}" is answered by someone no longer on the roster.`, 'Pick who answers this decision, or leave it to whoever arrives.', { nodeId: n.id });
    }
    if (n.type === 'wait') {
      if (n.wait.kind === 'command' && !n.wait.command.trim()) err('wait-no-command', `"${label(n)}" waits for a command, but has none.`, 'Write the command to run, e.g. gh run watch --exit-status, or curl -fs localhost:4310/health.', { nodeId: n.id });
      if (n.wait.kind === 'command' && n.wait.every * 2 > n.wait.timeout * 60) warn('wait-few-tries', `"${label(n)}" gives up before it can try twice.`, 'Lengthen the time limit or check more often.', { nodeId: n.id });
    }
    if (n.type === 'ask') {
      if (!n.ask.prompt.trim()) err('ask-no-prompt', 'An "Ask me" node has no question.', 'Write what you want to be asked.', { nodeId: n.id });
      if (!n.ask.replies.length) err('ask-no-replies', `"${label(n)}" has no replies to choose from.`, 'Add at least one reply.', { nodeId: n.id });
      for (const r of n.ask.replies) {
        if (!r.label) err('ask-reply-unlabelled', `A reply of "${label(n)}" has no label.`, 'Label every reply.', { nodeId: n.id });
        if (RESERVED_HANDLES.has(r.id)) err('ask-reply-reserved', `A reply of "${label(n)}" uses a reserved id.`, 'Delete the reply and add it again.', { nodeId: n.id });
      }
    }
  }
  for (const r of f.roster) {
    if (r.id !== MANAGER_ID && !usedEmployees.has(r.id)) warn('roster-unused', `"${r.name || 'An employee'}" is on the roster but not in the flow.`, 'Place them on the canvas, or remove them from the roster.', { employeeId: r.id });
  }

  // ── Edges ──
  const handleUse = new Map(); // "node|handle" -> [edges]
  for (const e of f.edges) {
    const s = nodes.get(e.source), t = nodes.get(e.target);
    if (!s || !t) { err('edge-dangling', 'A connection points at a node that no longer exists.', 'Delete the connection.', { edgeId: e.id }); continue; }
    if (e.kind === 'note') {
      if (!AGENT_TYPES.includes(s.type) || !AGENT_TYPES.includes(t.type))
        err('note-not-agents', `A note from "${label(s)}" to "${label(t)}" must go between employees or the Manager.`, 'Connect notes only between people.', { edgeId: e.id });
      if (e.source === e.target) err('note-self', `"${label(s)}" sends a note to itself.`, 'Delete the note.', { edgeId: e.id });
      continue;
    }
    if (t.type === 'start') err('edge-into-start', `"${label(s)}" connects back into Start.`, 'Route it to the Manager instead.', { edgeId: e.id });
    if (s.type === 'end') { err('edge-out-of-end', 'End cannot lead anywhere.', 'Delete this connection.', { edgeId: e.id }); continue; }
    if (!outputsOf(s).includes(e.sourceHandle)) {
      err('edge-bad-handle', `A connection leaves "${label(s)}" from an output it does not have.`, 'Delete it and connect from one of the node\'s outputs.', { edgeId: e.id });
      continue;
    }
    if (s.type === 'start' && t.type !== 'manager') err('start-not-manager', 'Start must lead to the Manager.', 'Connect Start to the Manager node.', { edgeId: e.id });
    if (e.sourceHandle === 'finish' && t.type !== 'end') err('finish-not-end', 'The Manager\'s Finish must lead to End.', 'Connect Finish to End.', { edgeId: e.id });
    if (e.sourceHandle !== 'finish' && t.type === 'end' && s.type !== 'manager')
      err('end-not-via-manager', `"${label(s)}" goes straight to End. The Manager always decides the end.`, 'Route it to the Manager; it finishes from there.', { edgeId: e.id });
    if (e.sourceHandle === 'out' && s.type === 'manager' && t.type === 'end')
      err('end-not-via-finish', 'The Manager reaches End through its Finish output only.', 'Delete this connection; Finish already goes to End.', { edgeId: e.id });
    const k = e.source + '|' + e.sourceHandle;
    if (!handleUse.has(k)) handleUse.set(k, []);
    handleUse.get(k).push(e);
  }
  for (const [k, list] of handleUse) {
    if (list.length > 1) {
      const n = nodes.get(k.split('|')[0]);
      err('handle-many', `One output of "${label(n)}" has ${list.length} connections; only one is allowed.`, 'To branch, add a Decision after this node.', { nodeId: n.id, edgeId: list[1].id });
    }
  }
  for (const n of f.nodes) {
    for (const h of requiredOutputsOf(n)) {
      if (!handleUse.has(n.id + '|' + h)) {
        const what = n.type === 'decision' ? `the answer "${handleLabel(n, h)}"` : n.type === 'ask' ? `the reply "${handleLabel(n, h)}"` : n.type === 'wait' && h !== 'out' ? `its "${handleLabel(n, h)}" route` : h === 'finish' ? 'its Finish' : 'its output';
        err('output-unwired', `"${label(n)}" has nowhere to go for ${what}.`, 'Connect it to the next step.', { nodeId: n.id, handle: h });
      }
    }
  }

  // ── Graph analysis (flow edges only; note edges never count) ──
  const g = flowGraph(f);
  const start = starts[0], end = ends[0];

  if (start) {
    const fromStart = reachable(g, [start.id]);
    for (const n of f.nodes) {
      if (fromStart.has(n.id) || !NODE_TYPES.includes(n.type)) continue;
      const onlyNotes = f.edges.some(e => e.kind === 'note' && e.target === n.id);
      err(onlyNotes ? 'reached-only-by-notes' : 'node-unreachable',
        onlyNotes ? `"${label(n)}" is only reached by notes, so the flow never gets there.` : `"${label(n)}" can never be reached from Start.`,
        onlyNotes ? 'Notes carry messages, not control. Connect a flow edge into it.' : 'Connect a step into it, or delete it.',
        { nodeId: n.id });
    }
  }

  if (end) {
    const toEnd = reachable(g, [end.id], { backward: true });
    const stuck = new Set(f.nodes.filter(n => !toEnd.has(n.id) && NODE_TYPES.includes(n.type)).map(n => n.id));
    if (stuck.size) {
      const loops = cycles(g).filter(c => c.every(id => stuck.has(id)));
      const inLoop = new Set(loops.flat());
      for (const c of loops) {
        err('loop-no-exit', `A loop through ${c.map(id => `"${label(nodes.get(id))}"`).join(', ')} has no way out.`,
          'Add a decision answer inside the loop that leads out of it.', { nodeIds: c });
      }
      for (const id of stuck) {
        if (inLoop.has(id)) continue;
        err('cannot-finish', `There is no way to finish from "${label(nodes.get(id))}".`, 'Connect it onward so the flow can get back to the Manager.', { nodeId: id });
      }
    }
  }

  // Every cycle must pass through a limited edge: drop them and nothing may loop.
  const unlimited = flowGraph(f, { exclude: e => e.maxRounds > 0 });
  for (const c of cycles(unlimited)) {
    err('loop-no-limit', `The loop through ${c.map(id => `"${label(nodes.get(id))}"`).join(', ')} has no round limit.`,
      'Set a round limit on the connection that goes back round this loop.', { nodeIds: c });
  }
  const inSomeCycle = new Set(cycles(g).flat());
  for (const e of f.edges) {
    if (e.kind === 'flow' && e.maxRounds > 0 && !(inSomeCycle.has(e.source) && inSomeCycle.has(e.target) && sameCycle(g, e)))
      warn('limit-not-in-loop', 'A round limit is set on a connection that is not part of a loop.', 'It has no effect; you can remove it.', { edgeId: e.id });
  }

  // ── Fan-out / Join ──
  const forks = f.nodes.filter(n => n.type === 'decision' && n.decision.kind === 'many');
  const joins = byType('join');
  const joinsByFork = new Map();
  for (const j of joins) {
    const fk = nodes.get(j.join.forkId);
    if (!j.join.forkId || !fk || fk.type !== 'decision' || fk.decision.kind !== 'many') {
      err('join-no-fork', `"${label(j)}" does not say which pick-many decision it joins.`, 'Choose the decision whose branches this Join waits for.', { nodeId: j.id });
      continue;
    }
    if (!joinsByFork.has(fk.id)) joinsByFork.set(fk.id, []);
    joinsByFork.get(fk.id).push(j);
  }
  const regions = new Map();
  for (const fk of forks) {
    const js = joinsByFork.get(fk.id) || [];
    if (js.length !== 1) {
      err(js.length ? 'fork-many-joins' : 'fork-no-join',
        js.length ? `"${label(fk)}" is closed by more than one Join.` : `The branches of "${label(fk)}" are never joined back together.`,
        js.length ? 'Keep one Join for this decision.' : 'Add a Join where its branches meet, and point it at this decision.',
        { nodeId: fk.id });
      continue;
    }
    const j = js[0];
    const region = forkRegion(f, g, fk, j.id);
    regions.set(fk.id, { join: j, region });
    if (region.has(fk.id)) {
      err('fork-branch-loops-to-fork', `A branch of "${label(fk)}" loops back to the decision before reaching "${label(j)}".`, 'Loop inside the branch, or go round after the Join.', { nodeId: fk.id });
    }
    for (const id of region) {
      const n = nodes.get(id);
      if (n.type === 'end' || n.type === 'manager' || n.type === 'start') {
        err('fork-branch-escapes', `A branch of "${label(fk)}" reaches "${label(n)}" before "${label(j)}".`, `Every branch must come back to "${label(j)}" first.`, { nodeId: fk.id });
        continue;
      }
      for (const e of g.inn.get(id)) {
        if (e.source !== fk.id && !region.has(e.source))
          err('fork-branch-entered', `"${label(n)}" sits inside a branch of "${label(fk)}" but is also reached from outside it.`, 'Branches may only be entered from their decision.', { nodeId: id, edgeId: e.id });
      }
    }
    for (const e of g.inn.get(j.id)) {
      if (!region.has(e.source) && e.source !== fk.id)
        err('join-entered-outside', `"${label(j)}" is reached from "${label(nodes.get(e.source))}", which is not a branch of "${label(fk)}".`, 'Only branches of its decision may lead into a Join.', { nodeId: j.id, edgeId: e.id });
    }
    const noneEdge = g.out.get(fk.id).find(e => e.sourceHandle === 'none');
    if (noneEdge && region.has(noneEdge.target))
      err('fork-none-into-branch', `"None of these" on "${label(fk)}" leads into one of its own branches.`, `Send it to "${label(j)}" or past it instead.`, { edgeId: noneEdge.id });
    const branchEdges = g.out.get(fk.id).filter(e => e.sourceHandle !== 'none');
    for (const e of branchEdges) {
      if (e.target === j.id) warn('fork-empty-branch', `The answer "${handleLabel(fk, e.sourceHandle)}" of "${label(fk)}" goes straight to the Join and does nothing.`, 'Route it to the employee who should do that work.', { edgeId: e.id });
    }
  }

  // ── Who answers each decision ──
  for (const n of f.nodes) {
    if (n.type !== 'decision' || n.decision.answeredBy) continue;
    const origins = answerOrigins(g, nodes, n.id);
    if (origins.has('?')) {
      err('decision-needs-answerer', `"${label(n)}" does not follow an employee, so nobody is there to answer it.`, 'Choose who answers this decision.', { nodeId: n.id });
    }
  }

  if (skills) {
    for (const [name, where] of mentionsInFactory(f)) {
      if (skills.has(name)) continue;
      const at = where[0] || {};
      warn('skill-unknown', `The skill /${name} is not installed for all projects.`, 'Install it, or make sure the projects you run this in have it.', { ...(at.nodeId ? { nodeId: at.nodeId } : {}), ...(at.employeeId ? { employeeId: at.employeeId } : {}), ...(at.edgeId ? { edgeId: at.edgeId } : {}), skill: name });
    }
  }
  return { ok: errors.length === 0, errors, warnings };
}

/** Whether a limited edge's source and target are in the same cycle. */
function sameCycle(g, e) {
  if (e.source === e.target) return true;
  return reachable(g, [e.target]).has(e.source);
}

/**
 * Who would be answering a decision when control arrives: walk back through
 * decisions (a chain of decisions is answered in one handoff) to the agent
 * nodes in front of it. '?' means some path arrives from a node that cannot
 * answer (Start, a Join, an Ask me).
 */
function answerOrigins(g, nodes, decisionId) {
  const out = new Set(), seen = new Set(), stack = [decisionId];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const e of g.inn.get(id)) {
      const s = nodes.get(e.source);
      if (!s) continue;
      if (AGENT_TYPES.includes(s.type)) out.add(s.id);
      else if (s.type === 'decision' && !s.decision.answeredBy) stack.push(s.id);
      else if (s.type === 'decision') out.add('by:' + s.decision.answeredBy);
      else out.add('?');
    }
  }
  return out;
}

function handleLabel(n, h) {
  if (n.type === 'decision') {
    if (h === 'yes' || h === 'no' || h === 'none') return h === 'none' ? 'none of these' : h;
    return n.decision.options.find(o => o.id === h)?.label || h;
  }
  if (n.type === 'ask') return n.ask.replies.find(r => r.id === h)?.label || h;
  if (n.type === 'wait') return h === 'done' ? 'Done' : h === 'timeout' ? 'Timed out' : '';
  return h;
}

// ─── Layout for factories written as text (a chat, an import) ───

/** True when the nodes have no real positions (missing, or piled on one spot). */
export function needsLayout(input) {
  const nodes = Array.isArray(input?.nodes) ? input.nodes : [];
  if (nodes.length < 2) return false;
  const seen = new Set();
  let missing = 0;
  for (const n of nodes) {
    const p = n && n.position;
    if (!p || !Number.isFinite(+p.x) || !Number.isFinite(+p.y)) { missing++; continue; }
    const k = Math.round(p.x / 40) + ',' + Math.round(p.y / 40);
    if (seen.has(k)) return true;
    seen.add(k);
  }
  return missing > 0;
}

/**
 * The flow edges that read top to bottom: every flow edge except the ones
 * that go back up a loop (found walking from Start, so the Manager sits under
 * Start and a review's way back counts as the loop). [[source, target]].
 */
export function layoutEdges(input) {
  const f = input;
  const out = new Map(f.nodes.map(n => [n.id, []]));
  for (const e of f.edges) if (e.kind !== 'note' && out.has(e.source) && out.has(e.target)) out.get(e.source).push(e);
  // Forward edges first, so a loop's limited edge is the one left out.
  for (const list of out.values()) list.sort((a, b) => (a.maxRounds > 0) - (b.maxRounds > 0));
  const state = new Map();
  const keep = [];
  const visit = (id) => {
    state.set(id, 1);
    for (const e of out.get(id)) {
      if (state.get(e.target) === 1) continue;   // back up the path: a loop
      keep.push([id, e.target]);
      if (!state.has(e.target)) visit(e.target);
    }
    state.set(id, 2);
  };
  const start = f.nodes.find(n => n.type === 'start');
  if (start) visit(start.id);
  for (const n of f.nodes) if (!state.has(n.id)) visit(n.id);
  return keep;
}

/**
 * Lay a factory out top to bottom: layers by longest path from Start over
 * layoutEdges; each layer centred. Returns a copy.
 */
export function layoutFactory(input) {
  const f = normalizeFactory(input);
  const W = 240, H = 130, GAP = 40;
  const dag = new Map(f.nodes.map(n => [n.id, []]));
  for (const [a, b] of layoutEdges(f)) dag.get(a).push(b);
  // Longest path layering.
  const layer = new Map();
  const order = [];
  const seen = new Set();
  const topo = (id) => { if (seen.has(id)) return; seen.add(id); for (const t of dag.get(id)) topo(t); order.push(id); };
  for (const n of f.nodes) topo(n.id);
  order.reverse();
  for (const id of order) if (!layer.has(id)) layer.set(id, 0);
  for (const id of order) for (const t of dag.get(id)) layer.set(t, Math.max(layer.get(t) || 0, layer.get(id) + 1));
  // End goes last, under everything.
  const end = f.nodes.find(n => n.type === 'end');
  if (end) layer.set(end.id, Math.max(...[...layer.values()]) + (layer.get(end.id) === Math.max(...[...layer.values()]) ? 0 : 1));
  const rows = new Map();
  for (const n of f.nodes) { const l = layer.get(n.id) || 0; if (!rows.has(l)) rows.set(l, []); rows.get(l).push(n.id); }
  const pos = new Map();
  for (const [l, ids] of rows) {
    const width = ids.length * W + (ids.length - 1) * GAP;
    ids.forEach((id, i) => pos.set(id, { x: Math.round(-width / 2 + i * (W + GAP)), y: l * (H + GAP) }));
  }
  return { ...f, nodes: f.nodes.map(n => ({ ...n, position: pos.get(n.id) || { x: 0, y: 0 } })) };
}
