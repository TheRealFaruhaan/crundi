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

/** Return all nodes, each enriched with the linked task's live info. */
export function getMindmap() {
  const store = load();
  return {
    nodes: store.nodes.map(n => ({
      ...n,
      linkedTaskInfo: resolveLink(n.linkedTask),
    })),
  };
}

/**
 * Return a node and all of its descendants as a nested tree
 * (node → children → grandchildren → …). Far cheaper than getMindmap() when you
 * only need one branch. Each node carries its linked-task info.
 */
export function getSubtree(nodeId) {
  const store = load();
  const root = findNode(store, nodeId);
  if (!root) return { ok: false, error: 'Node not found' };
  const childrenOf = {};
  for (const n of store.nodes) {
    if (n.parentId) (childrenOf[n.parentId] = childrenOf[n.parentId] || []).push(n);
  }
  const build = (n) => ({
    id: n.id,
    text: n.text,
    note: n.note,
    linkedTaskInfo: resolveLink(n.linkedTask),
    children: (childrenOf[n.id] || []).map(build),
  });
  return { ok: true, tree: build(root) };
}

/**
 * Return only the DIRECT children of a node (one level, no descendants). Pass
 * a null/empty id to get the root nodes. Each child carries a `hasChildren`
 * flag so you can decide whether to drill further. Cheapest way to walk the
 * tree level-by-level.
 */
export function getChildren(nodeId = null) {
  const store = load();
  const isRoot = (n) => !n.parentId || !findNode(store, n.parentId);
  if (nodeId && !findNode(store, nodeId)) return { ok: false, error: 'Node not found' };
  const kids = store.nodes.filter(n => nodeId ? n.parentId === nodeId : isRoot(n));
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

export function addNode({ text, parentId = null, note = '', linkedTask = null, project, taskId, todoId } = {}) {
  const t = String(text || '').trim();
  if (!t) return { ok: false, error: 'Node text is required' };
  const store = load();
  if (parentId && !findNode(store, parentId)) return { ok: false, error: 'Parent node not found' };
  const now = new Date().toISOString();
  const node = {
    id: genId(),
    text: t,
    note: String(note || ''),
    parentId: parentId || null,
    linkedTask: normalizeLink(linkedTask, project, taskId, todoId),
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
