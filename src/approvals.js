/**
 * approvals.js — the things waiting on a decision from you.
 *
 * Three unrelated mechanisms had grown their own way of asking:
 *
 *   secrets   — an agent wants a credential; you approve with a PIN
 *   chat      — Claude wants to run something; you answer the card in the chat
 *   collab    — an outside collaborator hit a wall and is asking
 *
 * Each was only visible where it happened. A permission card in a chat you are
 * not looking at is invisible; so is a merge request from someone working
 * while you are asleep. This is the one place that knows about all of them, so
 * a single indicator can say "two things need you" from anywhere in the app.
 *
 * It deliberately does NOT replace the existing surfaces. The card still
 * appears in the chat and still works there; this is a second way in, not a
 * migration. Removing the first would mean the indicator becomes load-bearing,
 * and a bug in it would silently strand every approval on the machine.
 *
 * Only the collaborator requests are stored here — secrets and chat cards
 * already have owners that know their own state, and copying them would create
 * two answers to "is this still pending".
 */

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'approvals.json');

/** Collaborator requests: id -> record. */
const requests = new Map();

let notify = null;
/** Called with (record) whenever something new needs attention. */
export function onRequest(cb) { notify = cb; }

function save() {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    // Tool escalations belong to a live process and cannot outlive it — a
    // restored one would be a button that answers a request nobody is waiting
    // on. Merge and note requests are about the repository and do survive.
    const keep = [...requests.values()].filter(r => r.kind !== 'tool' && r.status === 'pending');
    writeFileSync(FILE(), JSON.stringify({ requests: keep }, null, 2), { mode: 0o600 });
  } catch { /* never fail a request over bookkeeping */ }
}

function load() {
  try {
    const d = JSON.parse(readFileSync(FILE(), 'utf8'));
    for (const r of (d?.requests || [])) if (r?.id) requests.set(r.id, r);
  } catch { /* first run */ }
}
load();

/**
 * Record something a collaborator is asking for.
 *
 * @param {object} o
 * @param {'merge'|'tool'|'note'} o.kind
 * @param {string} o.collabId
 * @param {string} o.name      display name of the asker
 * @param {string} o.project
 * @param {string} o.title     one line, shown in the list
 * @param {string} [o.detail]  longer text, shown when opened
 * @param {string} [o.sessionId]  chat this came from (tool escalations)
 * @param {string} [o.requestId]  the CLI's pending control request id
 * @param {Function} [o.resolve]  called with true/false when decided (tool only)
 */
export function add(o) {
  const rec = {
    id: randomBytes(8).toString('hex'),
    kind: o.kind,
    collabId: o.collabId || '',
    collabKey: o.collabKey || '',
    name: o.name || '',
    project: o.project || '',
    title: String(o.title || '').slice(0, 300),
    detail: String(o.detail || '').slice(0, 8000),
    sessionId: o.sessionId || '',
    requestId: o.requestId || '',
    createdAt: Date.now(),
    status: 'pending',
  };
  // The resolver is a live function, so it is kept beside the record rather
  // than in it — records get serialised, and a function does not survive that.
  if (o.resolve) resolvers.set(rec.id, o.resolve);
  requests.set(rec.id, rec);
  save();
  try { notify?.(rec); } catch { /* a failed ping is not a failed request */ }
  return rec;
}

const resolvers = new Map();

export function pending() {
  return [...requests.values()].filter(r => r.status === 'pending')
    .sort((a, b) => a.createdAt - b.createdAt);
}

export function get(id) { return requests.get(id) || null; }

/**
 * Decide a request.
 *
 * `by` is recorded so a collaborator withdrawing their own request reads
 * differently from the owner refusing it — the first is routine, the second
 * is worth them seeing.
 */
export function resolve(id, approved, by = 'owner', note = '') {
  const rec = requests.get(id);
  if (!rec || rec.status !== 'pending') return { ok: false, error: 'That request is no longer pending' };
  rec.status = by === 'collaborator' ? 'cancelled' : (approved ? 'approved' : 'denied');
  rec.decidedAt = Date.now();
  rec.decidedBy = by;
  rec.note = String(note || '').slice(0, 500);
  const fn = resolvers.get(id);
  resolvers.delete(id);
  save();
  if (fn) { try { fn(approved, rec); } catch { /* the caller's problem */ } }
  return { ok: true, request: rec };
}

/** Drop a session's outstanding escalations — its process is gone. */
export function dropSession(sessionId) {
  for (const r of [...requests.values()]) {
    if (r.sessionId === sessionId && r.status === 'pending') {
      r.status = 'cancelled';
      r.decidedAt = Date.now();
      r.decidedBy = 'system';
      resolvers.delete(r.id);
    }
  }
  save();
}

/**
 * Everything a collaborator may see: only their own.
 *
 * Keyed by the PERSON, not the invitation, so someone holding two projects
 * sees both their requests in one list — the same way they see both projects.
 */
export function pendingForKey(collabKey) {
  return pending().filter(r => r.collabKey === collabKey);
}
