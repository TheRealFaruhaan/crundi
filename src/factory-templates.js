/**
 * factory-templates.js — the factories Crundi provides, modelled on how real
 * teams work and kept simple. They are read-only: "Use this" copies one into
 * the library, where it can be changed like any other. The point is a good
 * starting place, not a finished process.
 *
 * Built from the persona templates (factory-personas.js); laid out with
 * layoutFactory. Every one validates (scripts/test-factory-templates.mjs).
 */

import { rosterFromPersona } from './factory-personas.js';
import { layoutFactory, normalizeFactory, MANAGER_ID } from './factory-model.js';

const MANAGER = {
  id: MANAGER_ID, name: 'Manager', model: '', effort: '', memory: 'keep', skills: [], presets: [], tools: { allow: [], deny: [] },
  persona: 'You lead the team like a good engineering manager: you make sure the change request is understood before work starts (ask the person when it is not), give each person what they need, keep the work moving, and only finish when every "Done when" check is met and the reviewers are satisfied. When something goes wrong you decide; you do not do the work yourself.',
};

const step = (id, employeeId, label) => ({ id, type: 'employee', employeeId, label });
const yesno = (id, label, question, extra = {}) => ({ id, type: 'decision', label, decision: { kind: 'yesno', question, options: [], answeredBy: null, ...extra } });
const many = (id, label, question, options, extra = {}) => ({ id, type: 'decision', label, decision: { kind: 'many', question, options, answeredBy: null, ...extra } });
const join = (id, forkId, label) => ({ id, type: 'join', label, join: { forkId } });
const ask = (id, label, prompt, replies) => ({ id, type: 'ask', label, ask: { prompt, replies } });
const base = () => [{ id: 'start', type: 'start', label: 'Change request' }, { id: 'mgr', type: 'manager', label: 'Manager' }, { id: 'end', type: 'end', label: 'Result' }];
let ec = 0;
const e = (source, target, extra = {}) => ({ id: 'e' + (++ec), source, target, sourceHandle: extra.h || 'out', ...(extra.max ? { maxRounds: extra.max } : {}), ...(extra.note ? { requireNote: true, noteLabel: extra.note } : {}) });
const noteEdge = (id, source, target, label) => ({ id, kind: 'note', source, target, noteLabel: label });
const frame = () => [e('start', 'mgr'), e('mgr', 'end', { h: 'finish' })];

function people(keys, available) {
  return [MANAGER, ...keys.map(([key, id]) => rosterFromPersona(key, { id, available }))];
}

/** The templates, with skills limited to `available` (a Set of names) when given. */
export function factoryTemplates({ available = null } = {}) {
  ec = 0;
  const list = [];

  // 1. Pair and review: one developer, one reviewer. Most small teams.
  list.push({
    id: 'tpl_pair', name: 'Pair and review',
    goodFor: 'A sizeable feature in one part of the code, built by one developer and held to a proper review.',
    description: 'One developer builds; a reviewer with fresh eyes approves or sends it back with findings. The simplest team that still has a second pair of eyes.',
    roster: people([['fullstack', 'dev'], ['reviewer', 'rev']], available),
    nodes: [...base(), step('dev', 'dev', 'Build'), step('rev', 'rev', 'Review'), yesno('ok', 'Approved?', 'Do you approve the change?', { answeredBy: 'rev', review: true })],
    edges: [...frame(), e('mgr', 'dev'), e('dev', 'rev'), e('rev', 'ok'), e('ok', 'mgr', { h: 'yes', max: 5 }), e('ok', 'dev', { h: 'no', max: 3, note: 'What to change' })],
  });

  // 2. Product squad: plan, build the parts in parallel, test, review.
  list.push({
    id: 'tpl_squad', name: 'Product squad',
    goodFor: 'A large feature across the interface, the server and the data, the way a cross-functional product team builds it.',
    description: 'A planner splits the work; the frontend, backend and database engineers build their parts in parallel branches; a tester checks the "Done when" list; a reviewer approves or sends findings back to whoever needs them.',
    roster: people([['planner', 'plan'], ['frontend', 'fe'], ['backend', 'be'], ['database', 'db'], ['qa', 'qa'], ['reviewer', 'rev']], available),
    nodes: [...base(),
      step('plan', 'plan', 'Plan'),
      many('parts', 'Which parts?', 'Which parts of the plan need work in this round?', [{ id: 'ui', label: 'Interface' }, { id: 'api', label: 'Server' }, { id: 'data', label: 'Data' }]),
      step('fe', 'fe', 'Frontend'), step('be', 'be', 'Backend'), step('db', 'db', 'Database'),
      join('built', 'parts', 'Built'),
      step('qa', 'qa', 'Test'), step('rev', 'rev', 'Review'),
      yesno('ok', 'Approved?', 'Is the change ready to ship?', { answeredBy: 'rev', review: true }),
    ],
    edges: [...frame(), e('mgr', 'plan'), e('plan', 'parts'),
      e('parts', 'fe', { h: 'ui' }), e('parts', 'be', { h: 'api' }), e('parts', 'db', { h: 'data' }), e('parts', 'built', { h: 'none' }),
      e('fe', 'built'), e('be', 'built'), e('db', 'built'),
      e('built', 'qa'), e('qa', 'rev'), e('rev', 'ok'),
      e('ok', 'mgr', { h: 'yes', max: 5 }), e('ok', 'parts', { h: 'no', max: 3, note: 'What to change' }),
      noteEdge('n1', 'mgr', 'rev', 'What to look at closely')],
  });

  // 3. New project kickoff: research, design, the person approves, build, test, document.
  list.push({
    id: 'tpl_kickoff', name: 'New project kickoff',
    goodFor: 'A new project or a new product from nothing to a first working version.',
    description: 'A researcher looks at options, an architect writes the design, you approve it before anything is built, then the team builds the parts in parallel, a tester checks it end to end and a writer leaves it documented.',
    roster: people([['researcher', 'res'], ['architect', 'arch'], ['frontend', 'fe'], ['backend', 'be'], ['database', 'db'], ['qa', 'qa'], ['writer', 'doc']], available),
    nodes: [...base(),
      step('res', 'res', 'Research'), step('arch', 'arch', 'Design'),
      ask('go', 'Approve the design?', 'The design is in the shared folder (design.md). Build it?', [{ id: 'build', label: 'Build it' }, { id: 'change', label: 'Change the design' }]),
      many('parts', 'Which parts?', 'Which parts should be built now?', [{ id: 'ui', label: 'Interface' }, { id: 'api', label: 'Server' }, { id: 'data', label: 'Data' }], { answeredBy: MANAGER_ID }),
      step('fe', 'fe', 'Frontend'), step('be', 'be', 'Backend'), step('db', 'db', 'Database'),
      join('built', 'parts', 'Built'),
      step('qa', 'qa', 'Test end to end'),
      yesno('works', 'Works?', 'Does it meet every "Done when" check?', { answeredBy: 'qa', review: true }),
      step('doc', 'doc', 'Document'),
    ],
    edges: [...frame(), e('mgr', 'res'), e('res', 'arch'), e('arch', 'go'),
      e('go', 'parts', { h: 'build' }), e('go', 'arch', { h: 'change', max: 3, note: 'What to change in the design' }),
      e('parts', 'fe', { h: 'ui' }), e('parts', 'be', { h: 'api' }), e('parts', 'db', { h: 'data' }), e('parts', 'built', { h: 'none' }),
      e('fe', 'built'), e('be', 'built'), e('db', 'built'),
      e('built', 'qa'), e('qa', 'works'),
      e('works', 'doc', { h: 'yes' }), e('works', 'parts', { h: 'no', max: 4, note: 'What failed' }),
      e('doc', 'mgr', { max: 5 })],
  });

  // 4. Design studio: design first, build to it, review it, you sign off.
  list.push({
    id: 'tpl_studio', name: 'Design studio',
    goodFor: 'A new interface or a redesign where how it looks and feels matters as much as what it does.',
    description: 'A designer makes a working mock-up, a frontend developer builds to it, a UI reviewer checks it in a real browser on phone and desktop, and you sign it off before it is finished.',
    roster: people([['designer', 'des'], ['frontend', 'fe'], ['ui-reviewer', 'uir']], available),
    nodes: [...base(),
      step('des', 'des', 'Design'), step('fe', 'fe', 'Build'), step('uir', 'uir', 'UI review'),
      yesno('ok', 'Looks right?', 'Does it match the design and work on phone and desktop?', { answeredBy: 'uir', review: true }),
      ask('sign', 'Sign off?', 'Have a look at it. Happy with it?', [{ id: 'happy', label: 'Happy with it' }, { id: 'more', label: 'Needs changes' }]),
    ],
    edges: [...frame(), e('mgr', 'des'), e('des', 'fe'), e('fe', 'uir'), e('uir', 'ok'),
      e('ok', 'sign', { h: 'yes' }), e('ok', 'fe', { h: 'no', max: 3, note: 'What to change' }),
      e('sign', 'mgr', { h: 'happy', max: 5 }), e('sign', 'des', { h: 'more', max: 3, note: 'What you want changed' })],
  });

  // 5. Platform team: data and server changes with security and testing in parallel.
  list.push({
    id: 'tpl_platform', name: 'Platform team',
    goodFor: 'A large backend change: new services, data model changes, migrations, integrations.',
    description: 'The database engineer changes the schema with migrations, the backend developer builds on it, then security and testing review in parallel; anything blocking goes back to the developers, and DevOps makes sure it builds and runs.',
    roster: people([['database', 'db'], ['backend', 'be'], ['security', 'sec'], ['qa', 'qa'], ['devops', 'ops']], available),
    nodes: [...base(),
      step('db', 'db', 'Data model'), step('be', 'be', 'Backend'),
      many('checks', 'Which checks?', 'Which reviews does this change need?', [{ id: 'sec', label: 'Security' }, { id: 'test', label: 'Testing' }], { answeredBy: MANAGER_ID }),
      step('sec', 'sec', 'Security review'), step('qa', 'qa', 'Test'),
      join('checked', 'checks', 'Checked'),
      yesno('ok', 'Clear to ship?', 'Is it clear to ship, with nothing blocking left open?', { answeredBy: MANAGER_ID, review: true }),
      step('ops', 'ops', 'Build and run'),
    ],
    edges: [...frame(), e('mgr', 'db'), e('db', 'be'), e('be', 'checks'),
      e('checks', 'sec', { h: 'sec' }), e('checks', 'qa', { h: 'test' }), e('checks', 'checked', { h: 'none' }),
      e('sec', 'checked'), e('qa', 'checked'), e('checked', 'ok'),
      e('ok', 'ops', { h: 'yes' }), e('ok', 'be', { h: 'no', max: 3, note: 'What to fix' }),
      e('ops', 'mgr', { max: 5 })],
  });

  return list.map(t => {
    const f = layoutFactory(normalizeFactory({ ...t, settings: { presets: ['git-read', 'tests'] } }));
    return { ...f, id: t.id, template: true, goodFor: t.goodFor };
  });
}
