// "What's new": the steps shown once after an install or an update.
//
// The content is not here. Each release that has something to show has a
// folder, app/vendor/whatsnew/<version>/, holding steps.json and one small
// animated HTML fragment per step (see docs in that folder's README). This
// module only decides WHICH folders someone should be shown:
//
//   - an update: every edition newer than the one they last saw, oldest first,
//     so jumping several versions misses nothing;
//   - a new install: the latest edition only;
//   - a dev (prerelease) edition: only on an install set to the dev channel.
//     A production install never sees one, even though a production version
//     number sorts above the dev builds that led to it;
//   - a step someone has already seen (by its id) is not shown again, which is
//     what lets a dev edition be re-issued under the production version.

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

/** { core: [1, 19, 24], pre: 'dev.2' | '' } from '1.19.24-dev.2'. Null if it is not a version. */
export function parseVersion(v) {
  const m = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(String(v || '').trim());
  if (!m) return null;
  return { core: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] || '' };
}
export const isPrerelease = (v) => { const p = parseVersion(v); return !!(p && p.pre); };

/** Semver order: 1.19.24-dev.1 < 1.19.24-dev.2 < 1.19.24 < 1.19.25-dev.1. Unparseable sorts lowest. */
export function compareVersions(a, b) {
  const x = parseVersion(a), y = parseVersion(b);
  if (!x || !y) return (x ? 1 : 0) - (y ? 1 : 0);
  for (let i = 0; i < 3; i++) if (x.core[i] !== y.core[i]) return x.core[i] < y.core[i] ? -1 : 1;
  if (!x.pre || !y.pre) return (x.pre ? 0 : 1) - (y.pre ? 0 : 1);       // a release is above its prereleases
  const xs = x.pre.split('.'), ys = y.pre.split('.');
  for (let i = 0; i < Math.max(xs.length, ys.length); i++) {
    if (xs[i] === undefined || ys[i] === undefined) return xs[i] === undefined ? -1 : 1;
    const xn = /^\d+$/.test(xs[i]), yn = /^\d+$/.test(ys[i]);
    if (xn && yn) { if (Number(xs[i]) !== Number(ys[i])) return Number(xs[i]) < Number(ys[i]) ? -1 : 1; }
    else if (xs[i] !== ys[i]) return xs[i] < ys[i] ? -1 : 1;
  }
  return 0;
}

const SAFE = /^[a-z0-9][a-z0-9._-]{0,63}$/i;
const text = (v, max) => String(v == null ? '' : v).slice(0, max);

/**
 * Every edition on disk, oldest first. A folder that is not a version, or
 * whose steps.json does not parse, is skipped rather than breaking the rest.
 * @returns {Array<{version:string, steps:object[]}>}
 */
export function listEditions(dir) {
  let names = [];
  try { names = readdirSync(dir); } catch { return []; }
  const out = [];
  for (const name of names) {
    if (!parseVersion(name)) continue;
    const folder = join(dir, name);
    let raw;
    try { if (!statSync(folder).isDirectory()) continue; raw = JSON.parse(readFileSync(join(folder, 'steps.json'), 'utf8')); } catch { continue; }
    const steps = [];
    for (const s of (Array.isArray(raw && raw.steps) ? raw.steps : [])) {
      if (!s || !SAFE.test(String(s.id || '')) || !String(s.title || '').trim()) continue;
      let art = '';
      if (s.art && SAFE.test(String(s.art))) { try { art = readFileSync(join(folder, String(s.art)), 'utf8'); } catch { art = ''; } }
      steps.push({
        id: String(s.id), version: name,
        tag: text(s.tag, 40), title: text(s.title, 120), body: text(s.body, 600),
        keys: (Array.isArray(s.keys) ? s.keys : []).slice(0, 5).map((k) => ({
          keys: (Array.isArray(k && k.keys) ? k.keys : []).slice(0, 4).map((x) => text(x, 16)),
          text: text(k && k.text, 120),
        })).filter((k) => k.text),
        artTitle: text(s.artTitle || s.tag, 40),
        art: art.length <= 64 * 1024 ? art : '',
      });
    }
    if (steps.length) out.push({ version: name, steps });
  }
  return out.sort((a, b) => compareVersions(a.version, b.version));
}

/**
 * What to show.
 * @param {{editions:object[], current:string, channel:string, state:{seen?:string, ids?:string[], fresh?:boolean}, all?:boolean}} o
 *   all: asked for from Settings; the latest edition, seen or not.
 * @returns {{steps:object[], editions:string[]}}
 */
export function plan({ editions, current, channel, state, all = false }) {
  const st = state || {};
  // Up to what is installed, and no dev edition outside the dev channel.
  const mine = (editions || []).filter((e) => compareVersions(e.version, current) <= 0 && (!isPrerelease(e.version) || channel === 'dev'));
  let pick;
  if (all || st.fresh) pick = mine.slice(-1);
  else pick = mine.filter((e) => compareVersions(e.version, st.seen || '0.0.0') > 0);
  const seen = new Set(all ? [] : (st.ids || []));
  const steps = [];
  for (const e of pick) for (const s of e.steps) { if (seen.has(s.id)) continue; seen.add(s.id); steps.push(s); }
  return { steps, editions: [...new Set(steps.map((s) => s.version))] };
}

// ─── What this install has seen ───

/**
 * The record, made the first time it is asked for. With none on disk, the
 * question is whether this is a new install (latest edition only) or one that
 * was in use before "what's new" existed (everything it has not been shown,
 * which is everything). `usedBefore` is the caller's answer to that.
 */
export function loadState(file, { usedBefore = false } = {}) {
  try { const j = JSON.parse(readFileSync(file, 'utf8')); if (j && typeof j === 'object') return { seen: String(j.seen || '0.0.0'), ids: Array.isArray(j.ids) ? j.ids.map(String) : [], fresh: !!j.fresh }; }
  catch { /* none yet */ }
  const st = { seen: '0.0.0', ids: [], fresh: !usedBefore };
  try { writeFileSync(file, JSON.stringify(st)); } catch { /* decided again next time */ }
  return st;
}

/** Everything up to `current` counts as seen from now on. */
export function markSeen(file, { editions, current, channel }) {
  const prev = loadState(file, { usedBefore: true });
  const ids = new Set(prev.ids);
  for (const e of (editions || [])) {
    if (compareVersions(e.version, current) > 0 || (isPrerelease(e.version) && channel !== 'dev')) continue;
    for (const s of e.steps) ids.add(s.id);
  }
  const st = { seen: current, ids: [...ids].slice(-2000), fresh: false };
  writeFileSync(file, JSON.stringify(st));
  return st;
}

export const hasState = (file) => existsSync(file);
