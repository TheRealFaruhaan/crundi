/**
 * voice/kid-skills.js — what the child is learning, measured quietly from
 * every game played (never shown to the child: no scores, stars or streaks).
 *
 *   TAXONOMY        development areas and their skills: the one list games
 *                   (meta.json skills), the levels and the parent's view use.
 *                   THE SOURCE OF TRUTH: kid-games/README.md and the
 *                   baby-games skill follow it. Ids are stable: never rename or
 *                   reuse one; add new ones here first.
 *   createSkills    a level per skill (1-5) and a 0-1 mastery per item,
 *                   updated from each question's FIRST-TRY result: an
 *                   exponential moving average that needs several answers
 *                   before a level moves, rises slowly and falls quickly.
 *                   Kept in <dataDir>/kid-games/skills.json; a daily snapshot
 *                   per skill that was played goes to progress.jsonl.
 *   initFor         what a game is told when it starts: levels, mastery,
 *                   focus (being learnt) and avoid (mastered) per skill.
 *   summary         one short line per area for the kid's Cru.
 *   progress        GET /api/voice/kid/progress: areas over time, play time, notes.
 */

import { existsSync, readFileSync, appendFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { writePrivateFile } from '../private-fs.js';

/** Areas of development, with their skills (ids used everywhere). */
export const TAXONOMY = [
  { id: 'language', label: 'Language and literacy', about: 'Knowing letters, their sounds, big and little letters, and first words.',
    skills: [['letters', 'Letter recognition'], ['letter-sounds', 'Letter sounds (phonics)'], ['upper-lower', 'Big and little letters'], ['vocabulary', 'First words'], ['rhyme', 'Rhymes and sound play'], ['blending', 'Sounding out words']] },
  { id: 'numbers', label: 'Numbers and maths', about: 'Counting things, knowing numbers, more and less, first sums.',
    skills: [['counting', 'Counting'], ['numbers', 'Number recognition'], ['more-less', 'More and less'], ['adding', 'Simple adding']] },
  { id: 'shapes-colours', label: 'Shapes and colours', about: 'Naming and finding shapes and colours.',
    skills: [['shapes', 'Shapes'], ['colours', 'Colours']] },
  { id: 'memory', label: 'Memory and attention', about: 'Remembering where things are, and staying with a game.',
    skills: [['memory', 'Memory pairs'], ['focus', 'Focus time']] },
  { id: 'logic', label: 'Problem solving and logic', about: 'Matching, sorting, patterns and thinking things through.',
    skills: [['matching', 'Matching'], ['sorting', 'Sorting'], ['patterns', 'Patterns'], ['thinking', 'Thinking questions']] },
  { id: 'motor', label: 'Fine motor', about: 'Careful finger movements: tracing letters, drawing.',
    skills: [['writing', 'Tracing letters'], ['drawing', 'Drawing']] },
  { id: 'feelings', label: 'Feelings', about: 'Naming feelings: happy and sad first, then more.',
    skills: [['feelings', 'Naming feelings']] },
  { id: 'creative', label: 'Music and creativity', about: 'Rhythm, songs and making things up.',
    skills: [['music', 'Music and rhythm']] },
  { id: 'world', label: 'Knowledge and the world', about: 'Animals, the body, nature and weather, everyday things, places, fun facts.',
    skills: [['world.animals', 'Animals'], ['world.body', 'The body'], ['world.everyday', 'Everyday things'], ['world.nature', 'Nature and weather'], ['world.places', 'Places'], ['world.facts', 'Fun facts']] },
];
export const SKILL_IDS = TAXONOMY.flatMap((a) => a.skills.map((s) => s[0]));
export const areaOf = (skill) => (TAXONOMY.find((a) => a.skills.some((s) => s[0] === skill)) || {}).id || '';
const skillLabel = (skill) => { for (const a of TAXONOMY) for (const s of a.skills) if (s[0] === skill) return s[1]; return skill; };

/** The model's constants (see the header). */
export const RULES = Object.freeze({
  alpha: 0.25,          // weight of the newest first-try result in the moving average
  itemAlpha: 0.3,       // the same, per item
  upAfter: 10,          // answers at this level before it may rise
  upAt: 0.85,           // ... and the average needed
  downAfter: 4,         // answers at this level before it may fall
  downAt: 0.4,          // ... at or under this average
  restart: 0.6,         // the average a new level starts from
  masteredAt: 0.85, masteredAfter: 3,
});

const dayOf = (t) => new Date(t).toISOString().slice(0, 10);
const r2 = (n) => Math.round(n * 100) / 100;

/** A fresh skill. */
const blank = () => ({ level: 1, ema: RULES.restart, evidence: 0, answers: 0, items: {}, last: 0 });

/**
 * One first-try result for [s] (a skill record, changed in place). Returns
 * 'up', 'down' or '' (the level did not move).
 */
export function applyResult(s, item, ok, now = Date.now()) {
  const x = ok ? 1 : 0;
  s.ema += RULES.alpha * (x - s.ema);
  s.evidence++; s.answers++; s.last = now;
  if (item !== '' && item != null) {
    const k = String(item).slice(0, 40);
    const it = s.items[k] || (s.items[k] = { m: 0.3, n: 0, last: 0 });
    it.m = r2(it.m + RULES.itemAlpha * (x - it.m)); it.n++; it.last = now;
  }
  if (s.level < 5 && s.evidence >= RULES.upAfter && s.ema >= RULES.upAt) { s.level++; s.ema = RULES.restart; s.evidence = 0; return 'up'; }
  if (s.level > 1 && s.evidence >= RULES.downAfter && s.ema <= RULES.downAt) { s.level--; s.ema = RULES.restart; s.evidence = 0; return 'down'; }
  return '';
}

const mastered = (it) => it.n >= RULES.masteredAfter && it.m >= RULES.masteredAt;
/** Items in their natural order (letters A-Z, numbers ascending). */
const order = (a, b) => (Number(a) - Number(b)) || String(a).localeCompare(String(b));
/** "A-F, H" for a sorted list of single letters; otherwise the list. */
export function ranges(items) {
  const xs = [...items].sort(order);
  if (!xs.every((x) => /^[A-Z]$/.test(x) || /^\d+$/.test(x))) return xs.join(', ');
  const v = (x) => (/^\d+$/.test(x) ? Number(x) : x.charCodeAt(0));
  const out = [];
  for (let i = 0; i < xs.length;) {
    let j = i;
    while (j + 1 < xs.length && v(xs[j + 1]) === v(xs[j]) + 1) j++;
    out.push(j - i >= 2 ? `${xs[i]}-${xs[j]}` : xs.slice(i, j + 1).join(', '));
    i = j + 1;
  }
  return out.join(', ');
}

export function createSkills({ file = '', historyFile = '', now = () => Date.now() } = {}) {
  let st = { skills: {}, day: '', today: {}, playSeconds: {} };
  try { if (file && existsSync(file)) st = { ...st, ...JSON.parse(readFileSync(file, 'utf-8')) }; } catch { /* start afresh */ }
  const save = () => { if (!file) return; try { mkdirSync(dirname(file), { recursive: true }); writePrivateFile(file, JSON.stringify(st)); } catch { /* next time */ } };

  const snapshotOf = (skill, d, date) => {
    const s = st.skills[skill] || blank();
    return { date, skill, area: areaOf(skill), level: s.level, mastery: masteryOf(s), plays: d.answers, firstTry: d.answers ? r2(d.ok / d.answers) : null };
  };
  /** A new day: yesterday's snapshots go to progress.jsonl. */
  const roll = (t) => {
    const today = dayOf(t);
    if (st.day && st.day !== today && historyFile) {
      const lines = Object.entries(st.today || {}).map(([skill, d]) => JSON.stringify(snapshotOf(skill, d, st.day)));
      if (lines.length) { try { mkdirSync(dirname(historyFile), { recursive: true }); appendFileSync(historyFile, lines.join('\n') + '\n', { mode: 0o600 }); } catch { /* best effort */ } }
    }
    if (st.day !== today) { st.day = today; st.today = {}; }
  };
  const history = () => {
    let out = [];
    try { out = readFileSync(historyFile, 'utf-8').split('\n').filter(Boolean).map((l) => JSON.parse(l)); } catch { /* none yet */ }
    // Today, not yet rolled over.
    for (const [skill, d] of Object.entries(st.today || {})) out.push(snapshotOf(skill, d, st.day));
    return out;
  };

  const api = {
    /** A question's first-try result from a game. Unknown skills are ignored. */
    record(skill, item, ok, { at = now() } = {}) {
      const k = String(skill || '');
      if (!SKILL_IDS.includes(k)) return '';
      roll(at);
      const s = st.skills[k] || (st.skills[k] = blank());
      const moved = applyResult(s, item, !!ok, at);
      const d = st.today[k] || (st.today[k] = { answers: 0, ok: 0 });
      d.answers++; if (ok) d.ok++;
      save();
      return moved;
    },
    /** A game's time, for the parent's play-time chart. */
    addPlayTime(seconds, at = now()) {
      const sec = Math.max(0, Math.min(4 * 3600, Math.round(Number(seconds) || 0)));
      if (!sec) return;
      roll(at);
      const d = dayOf(at);
      st.playSeconds[d] = (st.playSeconds[d] || 0) + sec;
      save();
    },
    /** For a game starting: { type:'init', levels, mastery, focus, avoid } for [skills] (all when empty). */
    initFor(skills = []) {
      const want = (skills && skills.length ? skills : Object.keys(st.skills)).filter((k) => st.skills[k]);
      const out = { type: 'init', levels: {}, mastery: {}, focus: {}, avoid: {} };
      for (const k of want) {
        const s = st.skills[k];
        out.levels[k] = s.level;
        const items = Object.entries(s.items);
        out.mastery[k] = Object.fromEntries(items.map(([i, it]) => [i, it.m]));
        out.focus[k] = items.filter(([, it]) => !mastered(it)).sort((a, b) => a[1].m - b[1].m).map(([i]) => i).slice(0, 6);
        out.avoid[k] = items.filter(([, it]) => mastered(it)).map(([i]) => i).sort(order);
      }
      return out;
    },
    /** One line per area played, for the kid's Cru: "letters: knows A-F, learning G, H". */
    summary() {
      const lines = [];
      for (const a of TAXONOMY) {
        const parts = [];
        for (const [k, label] of a.skills) {
          const s = st.skills[k];
          if (!s || !s.answers) continue;
          const items = Object.entries(s.items);
          const known = items.filter(([, it]) => mastered(it)).map(([i]) => i);
          const learning = items.filter(([, it]) => !mastered(it)).map(([i]) => i);
          parts.push(`${label.toLowerCase()} level ${s.level}/5${known.length ? `, knows ${ranges(known)}` : ''}${learning.length ? `, learning ${ranges(learning.slice(0, 8))}` : ''}`);
        }
        if (parts.length) lines.push(`${a.label}: ${parts.join('; ')}`);
      }
      return lines.join('\n');
    },
    /**
     * The parent's view: areas over time, play time, notes.
     * @param {{range?: 'week'|'month'|'all', area?: string}} o
     */
    progress({ range = 'month', area = '' } = {}) {
      const t = now();
      const since = range === 'week' ? t - 7 * 86400000 : range === 'all' ? 0 : t - 30 * 86400000;
      const sinceDay = dayOf(since);
      const hist = history().filter((h) => range === 'all' || h.date >= sinceDay);
      const notes = [];
      const areas = TAXONOMY.filter((a) => !area || a.id === area || a.label.toLowerCase().includes(String(area).toLowerCase())).map((a) => {
        const ids = a.skills.map((s) => s[0]);
        const played = ids.filter((k) => st.skills[k] && st.skills[k].answers);
        const byDate = new Map();
        for (const h of hist.filter((x) => ids.includes(x.skill))) {
          const e = byDate.get(h.date) || { date: h.date, levels: [], mastery: [], plays: 0 };
          e.levels.push(h.level); e.mastery.push(h.mastery); e.plays += h.plays || 0;
          byDate.set(h.date, e);
        }
        const series = [...byDate.values()].sort((x, y) => x.date.localeCompare(y.date)).map((e) => ({ date: e.date, level: r2(avg(e.levels)), mastery: r2(avg(e.mastery)), plays: e.plays }));
        const learning = [], masteredList = [];
        for (const k of played) {
          const items = Object.entries(st.skills[k].items);
          const m = items.filter(([, it]) => mastered(it)).map(([i]) => i);
          const l = items.filter(([, it]) => !mastered(it)).map(([i]) => i);
          if (m.length) masteredList.push(`${skillLabel(k)}: ${ranges(m)}`);
          if (l.length) learning.push(`${skillLabel(k)}: ${ranges(l)}`);
        }
        const level = played.length ? r2(avg(played.map((k) => st.skills[k].level))) : 0;
        let trend = 'none';
        if (series.length >= 2) {
          const d = series[series.length - 1].mastery - series[0].mastery + (series[series.length - 1].level - series[0].level) * 0.2;
          trend = d > 0.05 ? 'up' : d < -0.05 ? 'down' : 'steady';
          const when = range === 'week' ? 'this week' : range === 'all' ? 'overall' : 'this month';
          if (d >= 0.15) notes.push(`${a.label} improved a lot ${when}.`);
          else if (d <= -0.15) notes.push(`${a.label} has been harder lately; the games go a little easier there for now.`);
        } else if (series.length === 1) trend = 'steady';
        return { id: a.id, label: a.label, about: a.about, level, trend, series, learning, mastered: masteredList,
          skills: a.skills.map(([k, label]) => ({ id: k, label, level: st.skills[k] ? st.skills[k].level : 0, mastery: st.skills[k] ? masteryOf(st.skills[k]) : 0, last: st.skills[k] && st.skills[k].last ? new Date(st.skills[k].last).toISOString() : null })).filter((s) => s.level) };
      });
      const playTime = Object.entries(st.playSeconds || {}).filter(([d]) => range === 'all' || d >= sinceDay).sort(([x], [y]) => x.localeCompare(y)).map(([date, s]) => ({ date, minutes: Math.round(s / 6) / 10 }));
      if (!hist.length && !playTime.length) notes.push('No games played yet.');
      return { range, areas, playTime, notes };
    },
    reset() { st = { skills: {}, day: '', today: {}, playSeconds: {} }; save(); if (historyFile) { try { writePrivateFile(historyFile, ''); } catch { /* none */ } } },
    get state() { return st; },
  };
  return api;
}

function avg(xs) { return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : 0; }
function masteryOf(s) {
  const items = Object.values(s.items || {});
  return r2(items.length ? avg(items.map((it) => it.m)) : s.ema);
}
