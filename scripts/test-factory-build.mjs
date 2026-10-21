#!/usr/bin/env node
/**
 * test-factory-build.mjs — building a factory as text, from a chat.
 *
 * Covers: the guide's shapes written the way the guide says, without
 * positions, validating; the layout giving every node its own place, top to
 * bottom, with End last; the store laying out a definition saved without
 * positions and leaving drawn ones alone; and the MCP tool lists (a run's
 * chats get the run tools, every other chat the building tools).
 *
 * Run: node scripts/test-factory-build.mjs
 */
import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const base = mkdtempSync(join(tmpdir(), 'factory-build-'));
process.env.DATA_DIR = base;
const M = await import('../src/factory-model.js');
const S = await import('../src/factory-store.js');
const { FACTORY_GUIDE } = await import('../src/factory-guide.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

ok(FACTORY_GUIDE.length > 3000 && /maxRounds/.test(FACTORY_GUIDE) && /factory_validate/.test(FACTORY_GUIDE) && /"wait"|wait:/.test(FACTORY_GUIDE), 'guide: covers the format, loops, validation and waits');

// "Build and review", as a chat would write it from the guide.
const buildReview = {
  name: 'Build and review',
  roster: [
    { id: 'manager', name: 'Manager', persona: 'Owns the CR.' },
    { id: 'dev', name: 'Developer', persona: 'Builds it.' },
    { id: 'rev', name: 'Reviewer', persona: 'Reviews it.', memory: 'fresh', tools: { deny: ['Edit', 'Write', 'MultiEdit', 'NotebookEdit'] } },
  ],
  nodes: [
    { id: 'start', type: 'start' }, { id: 'mgr', type: 'manager' }, { id: 'end', type: 'end' },
    { id: 'dev', type: 'employee', employeeId: 'dev' }, { id: 'rev', type: 'employee', employeeId: 'rev' },
    { id: 'ok', type: 'decision', label: 'Approved?', decision: { kind: 'yesno', question: 'Is it approved?', answeredBy: 'rev', review: true } },
  ],
  edges: [
    { id: 'e1', source: 'start', target: 'mgr' }, { id: 'e2', source: 'mgr', sourceHandle: 'finish', target: 'end' },
    { id: 'e3', source: 'mgr', sourceHandle: 'out', target: 'dev' }, { id: 'e4', source: 'dev', target: 'rev' },
    { id: 'e5', source: 'rev', target: 'ok' },
    { id: 'e6', source: 'ok', sourceHandle: 'no', target: 'dev', maxRounds: 3, requireNote: true, noteLabel: 'What to change' },
    { id: 'e7', source: 'ok', sourceHandle: 'yes', target: 'mgr', maxRounds: 5 },
  ],
};
const v = M.validateFactory(buildReview);
ok(v.ok, 'guide shape "build and review" validates' + (v.ok ? '' : ': ' + v.errors.map(e => e.message).join('; ')));

// Parallel with a Join and a CI wait.
const parallel = {
  name: 'Parallel',
  roster: [{ id: 'manager', name: 'Manager' }, { id: 'fe', name: 'Frontend' }, { id: 'be', name: 'Backend' }],
  nodes: [
    { id: 'start', type: 'start' }, { id: 'mgr', type: 'manager' }, { id: 'end', type: 'end' },
    { id: 'parts', type: 'decision', decision: { kind: 'many', question: 'Which parts?', options: [{ id: 'ui', label: 'UI' }, { id: 'api', label: 'API' }] } },
    { id: 'fe', type: 'employee', employeeId: 'fe' }, { id: 'be', type: 'employee', employeeId: 'be' },
    { id: 'j', type: 'join', join: { forkId: 'parts' } },
    { id: 'ci', type: 'wait', label: 'CI green', wait: { kind: 'command', command: 'npm test', timeout: 30 } },
  ],
  edges: [
    { id: 'e1', source: 'start', target: 'mgr' }, { id: 'e2', source: 'mgr', sourceHandle: 'finish', target: 'end' },
    { id: 'e3', source: 'mgr', sourceHandle: 'out', target: 'parts' },
    { id: 'e4', source: 'parts', sourceHandle: 'ui', target: 'fe' }, { id: 'e5', source: 'parts', sourceHandle: 'api', target: 'be' },
    { id: 'e6', source: 'parts', sourceHandle: 'none', target: 'j' },
    { id: 'e7', source: 'fe', target: 'j' }, { id: 'e8', source: 'be', target: 'j' },
    { id: 'e9', source: 'j', target: 'ci' },
    { id: 'e10', source: 'ci', sourceHandle: 'done', target: 'mgr', maxRounds: 5 },
    { id: 'e11', source: 'ci', sourceHandle: 'timeout', target: 'mgr', maxRounds: 5 },
  ],
};
const v2 = M.validateFactory(parallel);
ok(v2.ok, 'guide shape "parallel + CI wait" validates' + (v2.ok ? '' : ': ' + v2.errors.map(e => e.message).join('; ')));

// Layout.
ok(M.needsLayout(buildReview) && !M.needsLayout(M.layoutFactory(buildReview)), 'layout: needed without positions, not after');
const L = M.layoutFactory(parallel);
const pos = Object.fromEntries(L.nodes.map(n => [n.id, n.position]));
const spots = new Set(L.nodes.map(n => pos[n.id].x + ',' + pos[n.id].y));
ok(spots.size === L.nodes.length, 'layout: every node has its own place');
ok(pos.start.y < pos.mgr.y && pos.mgr.y < pos.parts.y && pos.parts.y < pos.fe.y && pos.fe.y === pos.be.y && pos.fe.y < pos.j.y && pos.j.y < pos.ci.y && pos.ci.y < pos.end.y, 'layout: top to bottom, parallel branches side by side, End last');

// Store.
const c = S.createFactory({ definition: buildReview });
ok(c.ok && !M.needsLayout(c.factory) && c.validation.ok, 'store: a definition without positions is laid out when saved');
const drawn = M.layoutFactory(parallel); drawn.nodes[0].position = { x: 999, y: -50 };
const c2 = S.createFactory({ definition: drawn });
ok(c2.factory.nodes.find(n => n.id === 'start').position.x === 999, 'store: drawn positions are left alone');

// MCP tool lists.
const mcp = readFileSync(new URL('../src/mcp-stdio.js', import.meta.url), 'utf-8');
ok(/FACTORY_RUN_TOOLS = new Set\(\['factory_handoff', 'factory_permission', 'factory_delegate', 'factory_result', 'factory_status'\]\)/.test(mcp) && /IN_FACTORY \? !FACTORY_LIBRARY_TOOLS\.has\(t\.name\) : !FACTORY_RUN_TOOLS\.has\(t\.name\)/.test(mcp), 'mcp: run tools only in a run, building tools everywhere else');
for (const t of ['factory_guide', 'factory_list', 'factory_get', 'factory_validate', 'factory_save', 'factory_start', 'factory_triggers']) {
  if (!mcp.includes(`name: '${t}'`)) ok(false, 'mcp: tool defined: ' + t);
}
ok(readFileSync(new URL('../skills/crundi-factories/SKILL.md', import.meta.url), 'utf-8').replace(/\r\n/g, '\n').startsWith('---\nname: crundi-factories\n'), 'skill: crundi-factories ships');

try { rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 }); } catch { /* Windows may still hold a file; it is a temp folder */ };
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
