/**
 * voice/kid-reflex.js — instant lines for game events, with no model call:
 * "You found B!" the moment it is found, in kid Cru's voice.
 *
 * The phrase bank is kid-games/reflex.json (and the library's
 * reflex.extra.json, which the workshop may add to, checked): per event a mood
 * and many lines with slots ({name}, {item}, {sound}, {word}). pick() chooses
 * uniformly among an event's lines, never one of its last three and skips lines whose slots it
 * cannot fill. gate() lets at most one line through every GAP_MS, coalescing
 * bursts and keeping quiet while kid Cru itself is speaking.
 *
 * Speech is cached on disk by voice + expressivity + text (clipKey), so a line
 * said once is instant afterwards; warm() makes the common lines, and a game's
 * own item lines, in the background.
 */

import { createHash } from 'node:crypto';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { BUILTIN_DIR } from './kid-games.js';

export const GAP_MS = 2500;
/** A line is not said again until this many others have been (for that event). */
export const RECENT = 3;
const LETTER = /^[A-Z]$/;
const SOUNDS = { A: 'ah', B: 'buh', C: 'kuh', D: 'duh', E: 'eh', F: 'fff', G: 'guh', H: 'hhh', I: 'ih', J: 'juh', K: 'kuh', L: 'lll', M: 'mmm', N: 'nnn', O: 'oh', P: 'puh', Q: 'kwuh', R: 'rrr', S: 'sss', T: 'tuh', U: 'uh', V: 'vvv', W: 'wuh', X: 'ks', Y: 'yuh', Z: 'zzz' };
const WORDS = { A: 'apple', B: 'ball', C: 'cake', D: 'drum', E: 'egg', F: 'fish', G: 'grapes', H: 'hat', I: 'igloo', J: 'jar', K: 'kite', L: 'leaf', M: 'moon', N: 'nest', O: 'orange', P: 'pear', Q: 'queen', R: 'rainbow', S: 'sun', T: 'tree', U: 'umbrella', V: 'van', W: 'watermelon', X: 'box', Y: 'yo-yo', Z: 'zebra' };
const MOODS = ['excited', 'happy', 'calm', 'gentle'];
const SLOTS = ['name', 'item', 'sound', 'word', 'parent'];
const UNSAFE = /https?:|www\.|<|>|\b(kill|die|dead|blood|hate|stupid|dumb|bad|wrong|idiot|shut up|scary|gun)\b/i;

/** The lines a bank file holds, checked: kid-safe, short, known slots. */
export function validateReflexBank(raw) {
  const out = {}, problems = [];
  const ev = raw && raw.events && typeof raw.events === 'object' ? raw.events : {};
  for (const [event, v] of Object.entries(ev)) {
    const lines = (Array.isArray(v && v.lines) ? v.lines : []).filter((l) => {
      const ok = typeof l === 'string' && l.trim() && l.length <= 90 && !UNSAFE.test(l) && !/\{(?!name\}|item\}|sound\}|word\}|parent\})[^}]*\}/.test(l);
      if (!ok) problems.push(`${event}: "${String(l).slice(0, 40)}"`);
      return ok;
    });
    if (lines.length) out[event] = { mood: MOODS.includes(v.mood) ? v.mood : 'excited', lines };
  }
  return { events: out, problems };
}

/** Fill the slots; null when one cannot be filled. */
export function fill(line, { name = '', item = '', parent = '' } = {}) {
  const it = String(item || '').trim();
  const L = LETTER.test(it.toUpperCase()) && it.length === 1 ? it.toUpperCase() : '';
  const vals = { name: String(name || '').trim(), item: L || it, sound: L ? SOUNDS[L] : '', word: L ? WORDS[L] : '', parent: String(parent || '').trim() || 'your grown-up' };
  let bad = false;
  let out = line.replace(/\{(name|item|sound|word|parent)\}/g, (_, k) => { if (!vals[k]) { if (k === 'name') return '\u0000'; bad = true; return ''; } return vals[k]; });
  if (bad) return null;
  // No name known: "Great job, {name}!" becomes "Great job!".
  out = out.replace(/,?\s*\u0000/g, '').replace(/\s+([!?.,])/g, '$1').replace(/\s{2,}/g, ' ').trim();
  return out;
}

/** The cache file name for a line said in [voice] at [expressivity]. */
export function clipKey(voice, expressivity, text) {
  return createHash('sha256').update(`${voice}|${expressivity}|${String(text).trim()}`).digest('hex').slice(0, 32);
}

export const MOOD_EXPRESSIVITY = { excited: 2, happy: 1, calm: 0, gentle: -1 };

export function createReflex({ dir = '', now = () => Date.now(), rnd = Math.random } = {}) {
  const load = () => {
    let base = { events: {} };
    try { base = JSON.parse(readFileSync(join(BUILTIN_DIR, 'reflex.json'), 'utf-8')); } catch { /* none */ }
    const merged = validateReflexBank(base).events;
    const extraFile = dir ? join(dir, 'reflex.extra.json') : '';
    if (extraFile && existsSync(extraFile)) {
      try {
        const extra = validateReflexBank(JSON.parse(readFileSync(extraFile, 'utf-8'))).events;
        for (const [k, v] of Object.entries(extra)) merged[k] = { mood: merged[k] ? merged[k].mood : v.mood, lines: [...(merged[k] ? merged[k].lines : []), ...v.lines] };
      } catch { /* a broken extra file is left out */ }
    }
    return merged;
  };
  let bank = load();
  const last = {};
  let lastAt = -Infinity, lastEvent = '';

  /** A line for [event]: { text, mood } or null. */
  function pick(event, slots = {}) {
    const b = bank[event];
    if (!b) return null;
    const cands = b.lines.map((l) => fill(l, slots)).filter((t) => t);
    if (!cands.length) return null;
    // Uniformly among the variants, never one of the last few said for this event.
    const recent = last[event] || [];
    const keep = Math.min(RECENT, cands.length - 1);
    const choices = cands.filter((t) => !recent.slice(-keep).includes(t));
    const pool = choices.length ? choices : cands;
    const text = pool[Math.floor(rnd() * pool.length) % pool.length];
    last[event] = [...recent, text].slice(-RECENT);
    return { text, mood: b.mood, expressivity: MOOD_EXPRESSIVITY[b.mood] };
  }

  /**
   * May a reflex line play now? Not within GAP_MS of the last (a burst of the
   * same event is one line), not while kid Cru speaks; finished always may.
   */
  function gate(event, { speaking = false } = {}) {
    const t = now();
    if (speaking && event !== 'finished') return false;
    if (event !== 'finished' && t - lastAt < GAP_MS) return false;
    lastAt = t; lastEvent = event;
    return true;
  }

  /** The lines worth making ahead of time: every item-free line, and [items]' lines. */
  function warmLines({ name = '', items = [] } = {}) {
    const out = [];
    for (const [event, b] of Object.entries(bank)) {
      for (const l of b.lines) {
        if (!/\{(item|sound|word)\}/.test(l)) { const t = fill(l, { name }); if (t) out.push({ text: t, mood: b.mood }); }
        else for (const it of items.slice(0, 30)) { const t = fill(l, { name, item: it }); if (t) out.push({ text: t, mood: b.mood }); }
      }
    }
    const seen = new Set();
    return out.filter((x) => (seen.has(x.text + x.mood) ? false : (seen.add(x.text + x.mood), true)));
  }

  return { pick, gate, warmLines, reload() { bank = load(); }, get bank() { return bank; }, get lastEvent() { return lastEvent; } };
}
