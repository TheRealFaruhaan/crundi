/**
 * schedule-store.js — Scheduled tasks for Crundi.
 *
 * Each schedule belongs to a project (alias) and fires an ACTION when ALL of its
 * CONDITIONS are met (edge-triggered: fires once when conditions transition to
 * true, re-arms when they go false again).
 *
 * Stored in <dataDir>/schedules.json:
 *   { schedules: [ {
 *       id, project, name, enabled,
 *       action: { kind:'agent', agent:'claude', mode:'normal'|'skip',
 *                 prompt, session:'continue'|'new'|'resume', sessionId }
 *             | { kind:'command', command }
 *             | { kind:'service', serviceKey, op:'start'|'stop'|'toggle' },
 *       conditions: [ { type:'time', at:'HH:MM' }
 *                   | { type:'usage', op:'below'|'above'|'equal', value }
 *                   | { type:'kanban', taskId, todoId?, status } ],
 *       createdAt, updatedAt, lastRun, lastMet
 *   } ] }
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'schedules.json');

function ensureDataDir() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
}
function load() {
  const file = FILE();
  if (!existsSync(file)) return { schedules: [] };
  try {
    const d = JSON.parse(readFileSync(file, 'utf-8'));
    return { schedules: Array.isArray(d.schedules) ? d.schedules : [] };
  } catch { return { schedules: [] }; }
}
function save(store) {
  ensureDataDir();
  writeFileSync(FILE(), JSON.stringify(store, null, 2));
}
function genId() { return randomBytes(8).toString('hex'); }

/** All schedules, optionally filtered to one project (alias). */
export function listSchedules(project = null) {
  const all = load().schedules;
  if (!project) return all;
  const k = String(project).toLowerCase();
  return all.filter(s => String(s.project || '').toLowerCase() === k);
}

export function getSchedule(id) {
  return load().schedules.find(s => s.id === id) || null;
}

function sanitize(data = {}) {
  const action = data.action || {};
  // Time lives in `when` (singular, mandatory); only extra conditions stack.
  const conditions = (Array.isArray(data.conditions) ? data.conditions : []).filter(c => c && c.type !== 'time');
  const w = data.when && typeof data.when === 'object' ? data.when : {};
  const when = {
    mode: w.mode === 'once' ? 'once' : 'recurring',
    time: w.time || '09:00',
    date: w.date || '',                                   // for one-time
    days: Array.isArray(w.days) ? w.days.filter(d => d >= 0 && d <= 6) : [], // 0=Sun .. 6=Sat
  };
  return {
    name: (data.name || '').toString().slice(0, 120) || 'Scheduled task',
    project: String(data.project || '').toLowerCase(),
    enabled: data.enabled !== false,
    action,
    when,
    conditions,
  };
}

export function addSchedule(data = {}) {
  if (!data.project) return { ok: false, error: 'project is required' };
  const store = load();
  const now = new Date().toISOString();
  const s = {
    id: genId(),
    ...sanitize(data),
    createdAt: now,
    updatedAt: now,
    lastRun: null,
    lastMet: false,
  };
  store.schedules.push(s);
  save(store);
  return { ok: true, schedule: s };
}

export function updateSchedule(id, updates = {}) {
  const store = load();
  const s = store.schedules.find(x => x.id === id);
  if (!s) return { ok: false, error: 'Schedule not found' };
  const clean = sanitize({ ...s, ...updates });
  s.name = clean.name;
  s.enabled = clean.enabled;
  if (updates.action) s.action = clean.action;
  if (updates.conditions) s.conditions = clean.conditions;
  if (updates.when) s.when = clean.when;
  if (updates.project) s.project = clean.project;
  s.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, schedule: s };
}

/** Toggle enabled without touching the rest. */
export function setEnabled(id, enabled) {
  const store = load();
  const s = store.schedules.find(x => x.id === id);
  if (!s) return { ok: false, error: 'Schedule not found' };
  s.enabled = !!enabled;
  s.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, schedule: s };
}

export function deleteSchedule(id) {
  const store = load();
  const before = store.schedules.length;
  store.schedules = store.schedules.filter(s => s.id !== id);
  if (store.schedules.length === before) return { ok: false, error: 'Schedule not found' };
  save(store);
  return { ok: true };
}

/** Scheduler-internal: persist runtime flags (lastMet / lastRun). */
export function setRuntime(id, { lastMet, lastRun } = {}) {
  const store = load();
  const s = store.schedules.find(x => x.id === id);
  if (!s) return;
  if (lastMet !== undefined) s.lastMet = lastMet;
  if (lastRun !== undefined) s.lastRun = lastRun;
  save(store);
}
