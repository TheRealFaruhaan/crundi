/**
 * factory-fixtures.mjs — shared test factories.
 *
 * standard(): the hand-drawn flow from the Factory design — planner loop,
 * parallel frontend/backend devs, optional security, parallel reviewers and an
 * approval loop back to the devs. Used by the validator, engine and runner tests.
 */

export function standard() {
  return {
    name: 'Standard',
    roster: [
      { id: 'manager', name: 'Manager', persona: 'Runs it.' },
      { id: 'planner', name: 'Planner', persona: 'Plans.' },
      { id: 'fe', name: 'Frontend dev', persona: 'UI.' },
      { id: 'be', name: 'Backend dev', persona: 'Server.' },
      { id: 'sec', name: 'Security dev', persona: 'Security.' },
      { id: 'uir', name: 'UI reviewer', persona: 'Reviews UI.' },
      { id: 'ber', name: 'Backend reviewer', persona: 'Reviews server.' },
    ],
    nodes: [
      { id: 'start', type: 'start' },
      { id: 'mgr', type: 'manager', label: 'Manager' },
      { id: 'end', type: 'end' },
      { id: 'dPlan', type: 'decision', label: 'Needs plan?', decision: { kind: 'yesno', question: 'Does it need a plan or a plan update?' } },
      { id: 'planner', type: 'employee', employeeId: 'planner', label: 'Planner' },
      { id: 'dWho', type: 'decision', label: 'Which workers?', decision: { kind: 'many', question: 'Which workers are needed?', options: [{ id: 'ui', label: 'UI' }, { id: 'backend', label: 'Backend' }] } },
      { id: 'fe', type: 'employee', employeeId: 'fe', label: 'Frontend dev' },
      { id: 'be', type: 'employee', employeeId: 'be', label: 'Backend dev' },
      { id: 'jDev', type: 'join', label: 'Devs done', join: { forkId: 'dWho' } },
      { id: 'dSec', type: 'decision', label: 'Security?', decision: { kind: 'yesno', question: 'Is a security analysis needed?', answeredBy: 'manager' } },
      { id: 'sec', type: 'employee', employeeId: 'sec', label: 'Security dev' },
      { id: 'dRev', type: 'decision', label: 'Which reviewers?', decision: { kind: 'many', question: 'Which reviews are needed?', answeredBy: 'manager', options: [{ id: 'ui', label: 'UI' }, { id: 'backend', label: 'Backend' }] } },
      { id: 'uir', type: 'employee', employeeId: 'uir', label: 'UI reviewer' },
      { id: 'ber', type: 'employee', employeeId: 'ber', label: 'Backend reviewer' },
      { id: 'jRev', type: 'join', label: 'Reviews done', join: { forkId: 'dRev' } },
      { id: 'dOk', type: 'decision', label: 'Approved?', decision: { kind: 'yesno', question: 'Is the work approved?', answeredBy: 'manager' } },
    ],
    edges: [
      { id: 'e1', source: 'start', target: 'mgr' },
      { id: 'eFin', source: 'mgr', sourceHandle: 'finish', target: 'end' },
      { id: 'e2', source: 'mgr', target: 'dPlan' },
      { id: 'e3', source: 'dPlan', sourceHandle: 'yes', target: 'planner' },
      { id: 'e4', source: 'planner', target: 'mgr', maxRounds: 3 },
      { id: 'e5', source: 'dPlan', sourceHandle: 'no', target: 'dWho' },
      { id: 'e6', source: 'dWho', sourceHandle: 'ui', target: 'fe' },
      { id: 'e7', source: 'dWho', sourceHandle: 'backend', target: 'be' },
      { id: 'e8', source: 'dWho', sourceHandle: 'none', target: 'jDev' },
      { id: 'e9', source: 'fe', target: 'jDev' },
      { id: 'e10', source: 'be', target: 'jDev' },
      { id: 'e11', source: 'jDev', target: 'dSec' },
      { id: 'e12', source: 'dSec', sourceHandle: 'yes', target: 'sec' },
      { id: 'e13', source: 'dSec', sourceHandle: 'no', target: 'dRev' },
      { id: 'e14', source: 'sec', target: 'dRev' },
      { id: 'e15', source: 'dRev', sourceHandle: 'ui', target: 'uir' },
      { id: 'e16', source: 'dRev', sourceHandle: 'backend', target: 'ber' },
      { id: 'e17', source: 'dRev', sourceHandle: 'none', target: 'jRev' },
      { id: 'e18', source: 'uir', target: 'jRev' },
      { id: 'e19', source: 'ber', target: 'jRev' },
      { id: 'e20', source: 'jRev', target: 'dOk' },
      { id: 'e21', source: 'dOk', sourceHandle: 'yes', target: 'mgr', maxRounds: 5 },
      { id: 'e22', source: 'dOk', sourceHandle: 'no', target: 'dWho', maxRounds: 3, requireNote: true, noteLabel: 'What to change' },
      { id: 'n1', kind: 'note', source: 'mgr', target: 'uir', noteLabel: 'Instructions for reviewers' },
      { id: 'n2', kind: 'note', source: 'mgr', target: 'ber', noteLabel: 'Instructions for reviewers' },
    ],
  };
}

/**
 * A scripted stand-in for claude-ui, shaped like claudeUiAgents(). Each
 * employee has a brain: (message, session) => { payload } to hand off,
 * { idle: true } to stop without handing off, or null to do nothing.
 * Permission requests are raised with team.ask(chatId, request); answers
 * land in session.responses.
 */
export function fakeTeam(brains) {
  const sessions = new Map();
  let runner = null;
  let permissionCb = null;
  let n = 0;
  let req = 0;
  const emit = (id, ev) => { for (const fn of sessions.get(id)?.listeners || []) fn(ev); };
  const agents = {
    async spawn(o) {
      let id = o.id && !sessions.get(o.id)?.alive ? o.id : '';
      while (!id || (sessions.has(id) && id !== o.id)) id = 'c' + (++n);
      const prev = sessions.get(id);
      sessions.set(id, { ...o, id, listeners: prev ? prev.listeners : new Set(), alive: true, msgs: prev ? prev.msgs : [], results: [], responses: prev ? prev.responses : [], state: 'idle', mode: o.permissionMode, spawns: (prev?.spawns || 0) + 1 });
      return { ok: true, id };
    },
    send(id, text) {
      const s = sessions.get(id);
      if (!s || !s.alive) return { ok: false, error: 'gone' };
      s.msgs.push(text);
      setTimeout(async () => {
        const brain = brains[s.employeeId];
        const out = brain ? await brain(text, s) : null;
        if (out && out.payload) s.results.push(await runner.handoff(id, out.payload));
        else if (out && out.idle) { emit(id, { type: 'state', state: 'working' }); emit(id, { type: 'state', state: 'idle' }); }
      }, 5);
      return { ok: true };
    },
    close(id) { const s = sessions.get(id); if (s) s.alive = false; },
    interrupt() {},
    isAlive: (id) => !!sessions.get(id)?.alive,
    sessionUuid: (id) => 'uuid-' + id,
    state: (id) => sessions.get(id)?.state || '',
    respond(id, r) { const s = sessions.get(id); if (!s) return { ok: false, error: 'gone' }; s.responses.push(r); return { ok: true }; },
    setPermissionMode(id, mode) { const s = sessions.get(id); if (s) s.mode = mode; return { ok: true }; },
    onPermission(cb) { permissionCb = cb; },
    addCard(id, card) { const s = sessions.get(id); if (!s) return { ok: false }; s.cards = s.cards || []; const entryId = 'card' + (s.cards.length + 1); s.cards.push({ ...card, entryId }); return { ok: true, entryId }; },
    patchCard(id, entryId, patch) { const s = sessions.get(id); const c = s?.cards?.find(x => x.entryId === entryId); if (!c) return { ok: false }; Object.assign(c, patch); return { ok: true }; },
    on: (id, fn) => sessions.get(id)?.listeners.add(fn),
    off: (id, fn) => sessions.get(id)?.listeners.delete(fn),
  };
  return {
    agents, sessions, emit,
    setRunner: (r) => { runner = r; },
    of: (emp) => [...sessions.values()].filter(s => s.employeeId === emp),
    /** An employee's CLI asking for permission; returns the request id. */
    ask(chatId, { toolName, input, description = '' }) {
      const requestId = 'req' + (++req);
      permissionCb({ sessionId: chatId, factory: {}, requestId, toolName, displayName: toolName, description, input });
      return requestId;
    },
  };
}
