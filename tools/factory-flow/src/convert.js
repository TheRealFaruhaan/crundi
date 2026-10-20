// A factory definition <-> what React Flow draws. The factory stays the
// source of truth; nodes and edges are derived from it on every render.
import { outputsOf } from '../../../src/factory-model.js';

export const AGENTS = new Set(['manager', 'employee']);

export function handleLabel(node, h) {
  if (h === 'out') return '';
  if (h === 'finish') return 'Finish';
  if (node.type === 'decision') {
    if (h === 'yes') return 'Yes';
    if (h === 'no') return 'No';
    if (h === 'none') return 'None';
    return node.decision.options.find(o => o.id === h)?.label || '?';
  }
  if (node.type === 'ask') return node.ask.replies.find(r => r.id === h)?.label || '?';
  return h;
}

/** Issues grouped by node and edge id. */
export function issuesBy(validation) {
  const nodes = new Map(), edges = new Map(), employees = new Map();
  const add = (map, id, it) => { if (!id) return; if (!map.has(id)) map.set(id, []); map.get(id).push(it); };
  for (const [list, level] of [[validation.errors, 'error'], [validation.warnings, 'warning']]) {
    for (const it of list) {
      const x = { ...it, level };
      add(nodes, it.nodeId, x);
      for (const id of it.nodeIds || []) add(nodes, id, x);
      add(edges, it.edgeId, x);
      add(employees, it.employeeId, x);
    }
  }
  return { nodes, edges, employees };
}

export function toFlow(f, { validation, selection, run, readOnly }) {
  const by = issuesBy(validation);
  const roster = new Map(f.roster.map(r => [r.id, r]));
  const tokensAt = new Map();
  for (const t of run?.tokens || []) {
    if (!tokensAt.has(t.nodeId)) tokensAt.set(t.nodeId, []);
    tokensAt.get(t.nodeId).push(t);
  }
  const nodes = f.nodes.map(n => ({
    id: n.id,
    type: ['start', 'manager', 'employee', 'decision', 'join', 'ask', 'end'].includes(n.type) ? n.type : 'employee',
    position: n.position,
    selected: selection?.kind === 'node' && selection.id === n.id,
    deletable: !readOnly && !['start', 'manager', 'end'].includes(n.type),
    draggable: !readOnly,
    data: {
      node: n,
      employee: n.type === 'employee' ? roster.get(n.employeeId) : n.type === 'manager' ? roster.get('manager') : null,
      outputs: outputsOf(n),
      issues: by.nodes.get(n.id) || [],
      tokens: tokensAt.get(n.id) || [],
      answeredBy: n.type === 'decision' && n.decision.answeredBy ? roster.get(n.decision.answeredBy)?.name || '' : '',
      join: n.type === 'join' ? f.nodes.find(x => x.id === n.join.forkId) : null,
    },
  }));
  // Loops (connections back up the flow) run in lanes past the right edge of
  // the whole flow, so they never cut through the steps in between.
  const posOf = new Map(f.nodes.map(n => [n.id, n.position]));
  const right = f.nodes.length ? Math.max(...f.nodes.map(n => n.position.x + 230)) : 0;
  const backs = f.edges.filter(e => e.kind === 'flow' && (posOf.get(e.target)?.y ?? 0) < (posOf.get(e.source)?.y ?? 0) - 20)
    .sort((a, b) => (posOf.get(a.source)?.y ?? 0) - (posOf.get(b.source)?.y ?? 0));
  const lane = new Map(backs.map((e, i) => [e.id, right + 40 + i * 24]));
  const edges = f.edges.map(e => {
    const note = e.kind === 'note';
    return {
      id: e.id,
      source: e.source,
      target: e.target,
      sourceHandle: note ? 'note' : e.sourceHandle,
      targetHandle: note ? 'notein' : 'in',
      type: 'factory',
      selected: selection?.kind === 'edge' && selection.id === e.id,
      deletable: !readOnly && !(e.sourceHandle === 'finish'),
      data: { edge: e, issues: by.edges.get(e.id) || [], used: run ? (run.edgeCounts?.[e.id] || 0) : null, laneX: lane.get(e.id) },
    };
  });
  return { nodes, edges, issues: by };
}
