/**
 * mindmap-store.js — Global brainstorming mindmap for Crundi.
 *
 * A single global forest of idea nodes. Each node may be free-standing
 * (unlinked) or linked to a Kanban task (project alias + task id) so you can
 * brainstorm a task into finer detail. Nodes form a tree via `parentId`
 * (root nodes have parentId = null).
 *
 * Stored in <dataDir>/mindmap.json as:
 *   { nodes: [ { id, text, note, parentId, linkedTask:{project,taskId}|null,
 *                createdAt, updatedAt } ] }
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';
import { getBoard } from './kanban-store.js';

const FILE = () => join(config.dataDir, 'mindmap.json');

function ensureDataDir() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
}

function load() {
  const file = FILE();
  if (!existsSync(file)) return { nodes: [] };
  try {
    const d = JSON.parse(readFileSync(file, 'utf-8'));
    return { nodes: Array.isArray(d.nodes) ? d.nodes : [] };
  } catch { return { nodes: [] }; }
}

function save(store) {
  ensureDataDir();
  writeFileSync(FILE(), JSON.stringify(store, null, 2));
}

function genId() {
  return randomBytes(8).toString('hex');
}

function findNode(store, id) {
  return store.nodes.find(n => n.id === id) || null;
}

/** Resolve a linked Kanban task's current title/status (or a missing marker). */
function resolveLink(linkedTask) {
  if (!linkedTask || !linkedTask.project || !linkedTask.taskId) return null;
  try {
    const board = getBoard(linkedTask.project, { includeDeleted: true });
    const task = board.tasks.find(t => t.id === linkedTask.taskId);
    if (!task) return { kind: linkedTask.todoId ? 'todo' : 'task', project: linkedTask.project, taskId: linkedTask.taskId, todoId: linkedTask.todoId, missing: true };
    // Link to a specific subtask (todo)
    if (linkedTask.todoId) {
      const todo = (task.todos || []).find(td => td.id === linkedTask.todoId);
      if (!todo) return { kind: 'todo', project: linkedTask.project, taskId: task.id, todoId: linkedTask.todoId, taskTitle: task.title, missing: true };
      return { kind: 'todo', project: linkedTask.project, taskId: task.id, todoId: todo.id, taskTitle: task.title, text: todo.text, done: !!todo.done, deleted: !!todo.deleted };
    }
    const todos = task.todos || [];
    return {
      kind: 'task',
      project: linkedTask.project,
      taskId: task.id,
      title: task.title,
      description: task.description || '',
      status: task.status,
      deleted: !!task.deleted,
      todosDone: todos.filter(td => td.done).length,
      todosTotal: todos.length,
    };
  } catch {
    return { project: linkedTask.project, taskId: linkedTask.taskId, missing: true };
  }
}

// ─── Reads ───

/**
 * Effective project (scope) of a node, by strict priority:
 *   1. If it links to a Kanban task → that task's project (no other choice).
 *   2. Else if it has a parent → inherit the parent's project (children follow
 *      their parent strictly).
 *   3. Else (a root) → its explicit creation scope (set when added via that
 *      project's MCP), or null = general (a user note tied to no project).
 * Derived at read time, so linking or re-parenting re-scopes automatically.
 */
function effectiveProject(store, node) {
  const seen = new Set();
  let cur = node;
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    if (cur.linkedTask && cur.linkedTask.project) return cur.linkedTask.project; // 1
    if (!cur.parentId) return cur.project || null;                               // 3
    const parent = findNode(store, cur.parentId);
    if (!parent) return cur.project || null; // orphan behaves like a root
    cur = parent;                                                                // 2
  }
  return null;
}
function scopeOf(project) { return project ? String(project).toLowerCase() : null; }

/**
 * Return all nodes, each enriched with the linked task's live info.
 * Pass a `project` alias to return ONLY that project's nodes (used by MCP so a
 * project's agent sees just its own mindmap; the web UI passes nothing → all).
 */
export function getMindmap(project = null) {
  const store = load();
  const p = scopeOf(project);
  const nodes = p ? store.nodes.filter(n => effectiveProject(store, n) === p) : store.nodes;
  return {
    nodes: nodes.map(n => ({
      ...n,
      effectiveProject: effectiveProject(store, n), // resolved scope (link > parent > own)
      linkedTaskInfo: resolveLink(n.linkedTask),
    })),
  };
}

/**
 * Return a node and all of its descendants as a nested tree
 * (node → children → grandchildren → …). Far cheaper than getMindmap() when you
 * only need one branch. Each node carries its linked-task info.
 */
export function getSubtree(nodeId, project = null) {
  const store = load();
  const root = findNode(store, nodeId);
  if (!root) return { ok: false, error: 'Node not found' };
  const p = scopeOf(project);
  if (p && effectiveProject(store, root) !== p) return { ok: false, error: 'Node not in this project' };
  const childrenOf = {};
  for (const n of store.nodes) {
    if (n.parentId) (childrenOf[n.parentId] = childrenOf[n.parentId] || []).push(n);
  }
  const build = (n) => ({
    id: n.id,
    text: n.text,
    note: n.note,
    linkedTaskInfo: resolveLink(n.linkedTask),
    children: (childrenOf[n.id] || [])
      .filter(c => !p || effectiveProject(store, c) === p) // don't leak other projects' branches
      .map(build),
  });
  return { ok: true, tree: build(root) };
}

/**
 * Return only the DIRECT children of a node (one level, no descendants). Pass
 * a null/empty id to get the root nodes. Each child carries a `hasChildren`
 * flag so you can decide whether to drill further. Cheapest way to walk the
 * tree level-by-level.
 */
export function getChildren(nodeId = null, project = null) {
  const store = load();
  const p = scopeOf(project);
  const isRoot = (n) => !n.parentId || !findNode(store, n.parentId);
  if (nodeId && !findNode(store, nodeId)) return { ok: false, error: 'Node not found' };
  let kids = store.nodes.filter(n => nodeId ? n.parentId === nodeId : isRoot(n));
  if (p) kids = kids.filter(n => effectiveProject(store, n) === p);
  return {
    ok: true,
    parentId: nodeId || null,
    children: kids.map(n => ({
      id: n.id,
      text: n.text,
      note: n.note,
      linkedTaskInfo: resolveLink(n.linkedTask),
      hasChildren: store.nodes.some(c => c.parentId === n.id),
    })),
  };
}

/**
 * Reverse lookup: mindmap nodes linked to a given Kanban task. Used to tell the
 * Kanban side which brainstorming nodes extend a task. Lives here (not in
 * kanban-store) to avoid a circular import.
 */
export function getNodesForTask(project, taskId) {
  const p = String(project || '').toLowerCase();
  const tid = String(taskId);
  return load().nodes
    .filter(n => n.linkedTask && n.linkedTask.project === p && n.linkedTask.taskId === tid)
    .map(n => ({ id: n.id, text: n.text, note: n.note, parentId: n.parentId }));
}

// ─── Writes ───

function normalizeLink(linkedTask, project, taskId, todoId) {
  const lt = (linkedTask && linkedTask.project && linkedTask.taskId)
    ? linkedTask
    : (project && taskId ? { project, taskId, todoId } : null);
  if (!lt) return null;
  const out = { project: String(lt.project).toLowerCase(), taskId: String(lt.taskId) };
  if (lt.todoId) out.todoId = String(lt.todoId);
  return out;
}

function childrenIndex(store) {
  const m = {};
  for (const n of store.nodes) if (n.parentId) (m[n.parentId] = m[n.parentId] || []).push(n);
  return m;
}
/** Every distinct project that a node's subtree links to (full depth). */
function subtreeLinkProjects(store, node, childrenOf, skipSelf = false) {
  childrenOf = childrenOf || childrenIndex(store);
  const out = new Set();
  const stack = skipSelf ? [...(childrenOf[node.id] || [])] : [node];
  const seen = new Set();
  while (stack.length) {
    const n = stack.pop();
    if (seen.has(n.id)) continue; seen.add(n.id);
    if (n.linkedTask && n.linkedTask.project) out.add(n.linkedTask.project);
    for (const c of (childrenOf[n.id] || [])) stack.push(c);
  }
  return out;
}
const quoteList = (arr) => arr.map(c => `"${c}"`).join(', ');
/** Scope inherited from the parent chain (ignores the node's own link). */
function parentScope(store, node) {
  if (!node.parentId) return null;
  const parent = findNode(store, node.parentId);
  return parent ? effectiveProject(store, parent) : null;
}

export function addNode({ text, parentId = null, note = '', linkedTask = null, project, taskId, todoId, scope } = {}) {
  const t = String(text || '').trim();
  if (!t) return { ok: false, error: 'Node text is required' };
  const store = load();
  const parent = parentId ? findNode(store, parentId) : null;
  if (parentId && !parent) return { ok: false, error: 'Parent node not found' };
  const link = normalizeLink(linkedTask, project, taskId, todoId);
  // A child linked to a task must match the project its parent branch is scoped to.
  if (parent && link) {
    const ps = effectiveProject(store, parent);
    if (ps && ps !== link.project) {
      return { ok: false, error: `Can't link this idea to a "${link.project}" task — its parent branch is scoped to "${ps}". Children stay in their parent's project.` };
    }
  }
  const now = new Date().toISOString();
  const node = {
    id: genId(),
    text: t,
    note: String(note || ''),
    parentId: parentId || null,
    project: scopeOf(scope) || null, // creation scope (used when this node is a root)
    linkedTask: link,
    createdAt: now,
    updatedAt: now,
  };
  store.nodes.push(node);
  save(store);
  return { ok: true, node };
}

export function updateNode(id, { text, note } = {}) {
  const store = load();
  const node = findNode(store, id);
  if (!node) return { ok: false, error: 'Node not found' };
  if (text !== undefined && String(text).trim()) node.text = String(text).trim();
  if (note !== undefined) node.note = String(note);
  node.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, node };
}

/**
 * Reparent a node and optionally set its position among its new siblings.
 * `index` = desired position among the children of newParentId (or among root
 * nodes when newParentId is null). Rejects moves that would create a cycle.
 */
export function moveNode(id, newParentId, index) {
  const store = load();
  const node = findNode(store, id);
  if (!node) return { ok: false, error: 'Node not found' };
  newParentId = newParentId || null;
  if (newParentId) {
    if (newParentId === id) return { ok: false, error: 'A node cannot be its own parent' };
    if (!findNode(store, newParentId)) return { ok: false, error: 'New parent not found' };
    // Walk up from newParent; if we reach id, it's a cycle.
    let cur = findNode(store, newParentId);
    while (cur) {
      if (cur.id === id) return { ok: false, error: 'Cannot move a node into its own descendant' };
      cur = cur.parentId ? findNode(store, cur.parentId) : null;
    }
    // Block fully if THIS node or any descendant (any depth) links to a project
    // other than the target's — a branch must stay in one project.
    const targetScope = effectiveProject(store, findNode(store, newParentId));
    if (targetScope) {
      const conflicts = [...subtreeLinkProjects(store, node)].filter(p => p !== targetScope);
      if (conflicts.length) {
        const what = conflicts.length === 1 ? 'it or a sub-idea is' : 'sub-ideas are';
        return { ok: false, error: `Can't move this under a "${targetScope}" idea — ${what} linked to ${quoteList(conflicts)}. A whole branch must stay in one project; unlink those Kanban tasks first.` };
      }
    }
  }
  node.parentId = newParentId;
  node.updatedAt = new Date().toISOString();
  // Reorder among siblings (nodes sharing the same parent) if an index is given.
  if (Number.isInteger(index)) {
    store.nodes.splice(store.nodes.indexOf(node), 1);
    const sibIds = store.nodes.filter(n => (n.parentId || null) === newParentId).map(n => n.id);
    const clamped = Math.max(0, Math.min(index, sibIds.length));
    if (clamped >= sibIds.length) {
      if (sibIds.length === 0) store.nodes.push(node);
      else store.nodes.splice(store.nodes.findIndex(n => n.id === sibIds[sibIds.length - 1]) + 1, 0, node);
    } else {
      store.nodes.splice(store.nodes.findIndex(n => n.id === sibIds[clamped]), 0, node);
    }
  }
  save(store);
  return { ok: true, node };
}

export function linkNode(id, { project, taskId, todoId, linkedTask } = {}) {
  const store = load();
  const node = findNode(store, id);
  if (!node) return { ok: false, error: 'Node not found' };
  const link = normalizeLink(linkedTask, project, taskId, todoId);
  if (!link) return { ok: false, error: 'project and taskId are required' };
  // Linking strictly scopes the node to the task's project — so it must agree
  // with the branch it sits in and with any task links in its own subtree.
  const ps = parentScope(store, node);
  if (ps && ps !== link.project) {
    return { ok: false, error: `This idea is in a "${ps}" branch (linked to that project), so it can't be linked to a task in "${link.project}". Move it out of that branch first.` };
  }
  // Block fully if any descendant (any depth) links to a different project.
  const descConflicts = [...subtreeLinkProjects(store, node, null, true)].filter(p => p !== link.project);
  if (descConflicts.length) {
    const what = descConflicts.length === 1 ? 'a sub-idea is' : 'sub-ideas are';
    return { ok: false, error: `Can't link this idea to "${link.project}" — ${what} linked to ${quoteList(descConflicts)}. The whole branch must stay in one project; unlink those first.` };
  }
  node.linkedTask = link;
  node.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, node };
}

export function unlinkNode(id) {
  const store = load();
  const node = findNode(store, id);
  if (!node) return { ok: false, error: 'Node not found' };
  node.linkedTask = null;
  node.project = null; // also drop its own scope → general (or inherits parent)
  node.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, node };
}

/**
 * Scope a node to a project WITHOUT linking it to a specific task (a general
 * project note). Pass a falsy project to clear the node's own scope. Subject to
 * the same single-project-per-branch rule as task linking.
 */
export function setNodeProject(id, project) {
  const store = load();
  const node = findNode(store, id);
  if (!node) return { ok: false, error: 'Node not found' };
  const p = scopeOf(project);
  if (p) {
    if (node.linkedTask && node.linkedTask.project !== p) {
      return { ok: false, error: `This idea is linked to a "${node.linkedTask.project}" task — unlink it before scoping it to "${p}".` };
    }
    const ps = parentScope(store, node);
    if (ps && ps !== p) {
      return { ok: false, error: `This idea is in a "${ps}" branch, so it can't be scoped to "${p}". Children stay in their parent's project.` };
    }
    const descConflicts = [...subtreeLinkProjects(store, node, null, true)].filter(x => x !== p);
    if (descConflicts.length) {
      const what = descConflicts.length === 1 ? 'a sub-idea is' : 'sub-ideas are';
      return { ok: false, error: `Can't scope this idea to "${p}" — ${what} linked to ${quoteList(descConflicts)}. The whole branch must stay in one project.` };
    }
  }
  node.project = p;
  node.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, node };
}

/** Delete a node and all of its descendants. Returns the number removed. */
export function deleteNode(id) {
  const store = load();
  if (!findNode(store, id)) return { ok: false, error: 'Node not found' };
  // Collect the subtree rooted at id.
  const toRemove = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const n of store.nodes) {
      if (n.parentId && toRemove.has(n.parentId) && !toRemove.has(n.id)) {
        toRemove.add(n.id); grew = true;
      }
    }
  }
  store.nodes = store.nodes.filter(n => !toRemove.has(n.id));
  save(store);
  return { ok: true, removed: toRemove.size };
}
