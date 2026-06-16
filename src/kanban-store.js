/**
 * kanban-store.js — Project-scoped Kanban board for Crundi.
 *
 * Each project alias gets its own board of task cards. A task has a status
 * (column), a description, and a nested checklist of todos. Every mutation is
 * appended to an immutable per-project history log. Deletes are soft — items
 * are flagged `deleted` (and `deletedAt`) and can be restored; they are never
 * physically removed, so the history and the items themselves are recoverable.
 *
 * Stored in <dataDir>/kanban.json as:
 *   {
 *     "<alias>": {
 *       tasks:   [ { id, title, description, status, todos:[...], deleted, ... } ],
 *       history: [ { id, ts, action, taskId?, todoId?, message } ]
 *     }
 *   }
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'kanban.json');

/** Fixed set of columns / statuses, in display order. */
export const STATUSES = ['backlog', 'todo', 'in_progress', 'done'];
const DEFAULT_STATUS = 'todo';

function ensureDataDir() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
}

function loadAll() {
  const file = FILE();
  if (!existsSync(file)) return {};
  try { return JSON.parse(readFileSync(file, 'utf-8')) || {}; } catch { return {}; }
}

function saveAll(map) {
  ensureDataDir();
  writeFileSync(FILE(), JSON.stringify(map, null, 2));
}

function genId() {
  return randomBytes(8).toString('hex');
}

function emptyBoard() {
  return { tasks: [], history: [] };
}

/** Get (and lazily create) the board object for an alias from a loaded map. */
function boardOf(map, alias) {
  const key = String(alias || '').toLowerCase();
  if (!key) return null;
  if (!map[key]) map[key] = emptyBoard();
  return map[key];
}

function record(board, action, message, extra = {}) {
  board.history.push({ id: genId(), ts: new Date().toISOString(), action, message, ...extra });
}

function normalizeStatus(status) {
  return STATUSES.includes(status) ? status : DEFAULT_STATUS;
}

function findTask(board, taskId) {
  return board.tasks.find(t => t.id === taskId) || null;
}

// ─── Reads ───

/**
 * Return the full board for an alias.
 * @param {string} alias
 * @param {{ includeDeleted?: boolean }} [opts]
 */
export function getBoard(alias, { includeDeleted = false } = {}) {
  const map = loadAll();
  const board = map[String(alias || '').toLowerCase()] || emptyBoard();
  const tasks = board.tasks.map(t => ({
    ...t,
    todos: includeDeleted ? t.todos : t.todos.filter(td => !td.deleted),
  }));
  return {
    statuses: STATUSES,
    tasks: includeDeleted ? tasks : tasks.filter(t => !t.deleted),
    deletedTasks: board.tasks.filter(t => t.deleted),
    history: board.history,
  };
}

/**
 * Return a single task (with its todos) by id — cheaper than getBoard() when you
 * only need one card. Todos respect includeDeleted like getBoard.
 */
export function getTask(alias, taskId, { includeDeleted = false } = {}) {
  const map = loadAll();
  const board = map[String(alias || '').toLowerCase()];
  const task = board && board.tasks.find(t => t.id === taskId);
  if (!task) return { ok: false, error: 'Task not found' };
  return { ok: true, task: { ...task, todos: includeDeleted ? task.todos : task.todos.filter(td => !td.deleted) } };
}

/**
 * Return lightweight task summaries for a single status column (no todo text or
 * descriptions) — the cheapest way to see what's in one column. Use getTask for
 * full detail of a specific card.
 */
export function getColumn(alias, status, { includeDeleted = false } = {}) {
  const s = normalizeStatus(status);
  const map = loadAll();
  const board = map[String(alias || '').toLowerCase()];
  const tasks = board ? board.tasks.filter(t => t.status === s && (includeDeleted || !t.deleted)) : [];
  return {
    ok: true,
    status: s,
    tasks: tasks.map(t => {
      const live = (t.todos || []).filter(td => !td.deleted);
      return { id: t.id, title: t.title, status: t.status, todosDone: live.filter(td => td.done).length, todosTotal: live.length };
    }),
  };
}

export function getHistory(alias) {
  const map = loadAll();
  const board = map[String(alias || '').toLowerCase()] || emptyBoard();
  return board.history;
}

// ─── Task mutations ───

export function addTask(alias, { title, description = '', status = DEFAULT_STATUS, todos = [] } = {}) {
  const t = String(title || '').trim();
  if (!t) return { ok: false, error: 'Task title is required' };
  const map = loadAll();
  const board = boardOf(map, alias);
  if (!board) return { ok: false, error: 'Project alias is required' };

  const now = new Date().toISOString();
  const task = {
    id: genId(),
    title: t,
    description: String(description || ''),
    status: normalizeStatus(status),
    todos: (Array.isArray(todos) ? todos : []).map(text => ({
      id: genId(), text: String(text), done: false, deleted: false, createdAt: now, updatedAt: now,
    })),
    deleted: false,
    deletedAt: null,
    createdAt: now,
    updatedAt: now,
  };
  board.tasks.push(task);
  record(board, 'task.add', `Added task "${task.title}"`, { taskId: task.id });
  saveAll(map);
  return { ok: true, task };
}

export function updateTask(alias, taskId, updates = {}) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  if (!task) return { ok: false, error: 'Task not found' };

  const changes = [];
  if (updates.title !== undefined && String(updates.title).trim() && updates.title !== task.title) {
    task.title = String(updates.title).trim(); changes.push('title');
  }
  if (updates.description !== undefined && updates.description !== task.description) {
    task.description = String(updates.description); changes.push('description');
  }
  if (updates.status !== undefined) {
    const s = normalizeStatus(updates.status);
    if (s !== task.status) { task.status = s; changes.push(`status→${s}`); }
  }
  if (!changes.length) return { ok: true, task, unchanged: true };

  task.updatedAt = new Date().toISOString();
  record(board, 'task.update', `Updated task "${task.title}" (${changes.join(', ')})`, { taskId });
  saveAll(map);
  return { ok: true, task };
}

/** Move a task to a status (column), optionally to a position within the board. */
export function moveTask(alias, taskId, status, index) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  if (!task) return { ok: false, error: 'Task not found' };

  const s = normalizeStatus(status);
  task.status = s;
  task.updatedAt = new Date().toISOString();

  // index = desired position WITHIN the target column (among its tasks).
  if (Number.isInteger(index)) {
    const cur = board.tasks.indexOf(task);
    board.tasks.splice(cur, 1);
    const colIds = board.tasks.filter(t => t.status === s).map(t => t.id);
    const clamped = Math.max(0, Math.min(index, colIds.length));
    if (clamped >= colIds.length) {
      if (colIds.length === 0) board.tasks.push(task);
      else board.tasks.splice(board.tasks.findIndex(t => t.id === colIds[colIds.length - 1]) + 1, 0, task);
    } else {
      board.tasks.splice(board.tasks.findIndex(t => t.id === colIds[clamped]), 0, task);
    }
  }
  record(board, 'task.move', `Moved task "${task.title}" to ${s}`, { taskId });
  saveAll(map);
  return { ok: true, task };
}

export function deleteTask(alias, taskId) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  if (!task) return { ok: false, error: 'Task not found' };
  if (task.deleted) return { ok: true, task };
  task.deleted = true;
  task.deletedAt = new Date().toISOString();
  record(board, 'task.delete', `Deleted task "${task.title}" (soft)`, { taskId });
  saveAll(map);
  return { ok: true, task };
}

export function restoreTask(alias, taskId) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  if (!task) return { ok: false, error: 'Task not found' };
  if (!task.deleted) return { ok: true, task };
  task.deleted = false;
  task.deletedAt = null;
  task.updatedAt = new Date().toISOString();
  record(board, 'task.restore', `Restored task "${task.title}"`, { taskId });
  saveAll(map);
  return { ok: true, task };
}

// ─── Todo mutations ───

function findTodo(task, todoId) {
  return task?.todos.find(td => td.id === todoId) || null;
}

export function addTodo(alias, taskId, text) {
  const txt = String(text || '').trim();
  if (!txt) return { ok: false, error: 'Todo text is required' };
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  if (!task) return { ok: false, error: 'Task not found' };
  const now = new Date().toISOString();
  const todo = { id: genId(), text: txt, done: false, deleted: false, createdAt: now, updatedAt: now };
  task.todos.push(todo);
  task.updatedAt = now;
  record(board, 'todo.add', `Added todo "${txt}" to "${task.title}"`, { taskId, todoId: todo.id });
  saveAll(map);
  return { ok: true, todo };
}

export function updateTodo(alias, taskId, todoId, updates = {}) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  const todo = findTodo(task, todoId);
  if (!todo) return { ok: false, error: 'Todo not found' };

  const changes = [];
  if (updates.text !== undefined && String(updates.text).trim() && updates.text !== todo.text) {
    todo.text = String(updates.text).trim(); changes.push('text');
  }
  if (updates.done !== undefined && !!updates.done !== todo.done) {
    todo.done = !!updates.done; changes.push(todo.done ? 'checked' : 'unchecked');
  }
  if (!changes.length) return { ok: true, todo, unchanged: true };
  const now = new Date().toISOString();
  todo.updatedAt = now;
  task.updatedAt = now;
  record(board, 'todo.update', `Updated todo "${todo.text}" (${changes.join(', ')})`, { taskId, todoId });
  saveAll(map);
  return { ok: true, todo };
}

export function deleteTodo(alias, taskId, todoId) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  const todo = findTodo(task, todoId);
  if (!todo) return { ok: false, error: 'Todo not found' };
  if (todo.deleted) return { ok: true, todo };
  todo.deleted = true;
  todo.deletedAt = new Date().toISOString();
  task.updatedAt = todo.deletedAt;
  record(board, 'todo.delete', `Deleted todo "${todo.text}" (soft)`, { taskId, todoId });
  saveAll(map);
  return { ok: true, todo };
}

export function restoreTodo(alias, taskId, todoId) {
  const map = loadAll();
  const board = boardOf(map, alias);
  const task = board && findTask(board, taskId);
  const todo = findTodo(task, todoId);
  if (!todo) return { ok: false, error: 'Todo not found' };
  if (!todo.deleted) return { ok: true, todo };
  todo.deleted = false;
  todo.deletedAt = null;
  todo.updatedAt = new Date().toISOString();
  task.updatedAt = todo.updatedAt;
  record(board, 'todo.restore', `Restored todo "${todo.text}"`, { taskId, todoId });
  saveAll(map);
  return { ok: true, todo };
}
