/**
 * voice/names.js — put Crundi's own names back into what the recogniser heard.
 *
 * Deepgram gets the names as keyterms and still writes the short or made-up
 * ones as English words. On the owner's own recordings (8 Nov 2026): "Hey,
 * crew" for "Hey Cru", "same cv" for the sam-cv service, "Kurony" for Crundi.
 * This runs on the text afterwards, against the names that exist right now
 * (Crundi's vocabulary, the projects, the services).
 *
 * Careful, because a wrong "fix" is worse than none:
 *  - a word that is plain English on its own ("crew", "cloud", "same") is only
 *    changed where the sentence leaves no doubt ("hey crew", "tell cloud to",
 *    "same cv" when a sam-cv exists);
 *  - otherwise a run of words becomes a name only when it sounds like it
 *    (close spelling AND the same consonant skeleton) and the name is not
 *    itself an everyday word ("notes", "api" stay whatever was heard).
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/**
 * About 21,000 English words (the whole words of BERT's uncased vocabulary,
 * Apache-2.0), so that "sent", "snack" or "sentence" are never taken for a
 * project that sounds a bit like them. Read once, on first use.
 */
let english = null;
function isEnglish(w) {
  if (!english) {
    try { english = new Set(readFileSync(fileURLToPath(new URL('./english-words.txt', import.meta.url)), 'utf8').split(/\r?\n/).filter(Boolean)); }
    catch { english = new Set(); }
  }
  const x = String(w).toLowerCase();
  if (COMMON.has(x) || english.has(x)) return true;
  // "crunchy", "snacks", "pushed", "fixing": a known word with an ending.
  for (const [end, put] of [['ies', 'y'], ['es', ''], ['s', ''], ['ed', ''], ['ed', 'e'], ['ing', ''], ['ing', 'e'], ['y', ''], ['er', ''], ['ly', '']]) {
    if (x.length > end.length + 2 && x.endsWith(end) && english.has(x.slice(0, -end.length) + put)) return true;
  }
  return false;
}

/** Everyday words never turned into a name by sound alone. */
const COMMON = new Set(`a an the and or but if then so to of in on at by for with from up down out over under
again about into onto off as is are was were be been being am do does did done have has had having i me my
you your he him his she her it its we us our they them their this that these those what which who whom whose
when where why how all any both each few more most other some such no not only own same than too very can will
just should now new old open close start stop show move make take give get put set run tell ask say send see
look find keep let help turn go come back next last first one two three four five six seven eight nine ten
card cards chat chats project projects service services log logs list task tasks note notes file files branch
dev main push pull build test fix check release play store voice screen mode inbox page app web phone server
please thanks okay ok yes yeah hey hi hello crew cloud crowd crony clued cord code card call cool cue queue
due true through blue clue grew drew few view same some come time line mine fine`.split(/\s+/));

/** Spelling turned into rough sound: c/k/q alike, doubled letters and h after a consonant dropped. */
function sound(w) {
  return String(w).toLowerCase()
    .replace(/[^a-z0-9]/g, '')
    .replace(/ph/g, 'f').replace(/ck/g, 'k').replace(/[cq]/g, 'k').replace(/x/g, 'ks')
    .replace(/([bdgkpstvz])h/g, '$1')
    .replace(/y$/, 'i').replace(/y/g, 'i')
    .replace(/(.)\1+/g, '$1');
}
/** Consonants only, first letter kept: what survives a mumble. */
function skeleton(s) { return s.charAt(0) + s.slice(1).replace(/[aeiou]/g, ''); }

function distance(a, b) {
  if (a === b) return 0;
  const m = a.length, n = b.length;
  let prev = Array.from({ length: n + 1 }, (_, j) => j);
  for (let i = 1; i <= m; i++) {
    const cur = [i];
    for (let j = 1; j <= n; j++) cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    prev = cur;
  }
  return prev[n];
}

/** How a name is said: "sam-cv" is "sam cv". */
function spoken(name) { return String(name || '').replace(/[-_.]+/g, ' ').replace(/\s+/g, ' ').trim(); }

/**
 * The names to look for. Fixed ones first, then projects and services.
 * Each: { text, words, key, skel } where key is its sound.
 */
export function nameList({ projects = [], services = [] } = {}) {
  const out = [];
  const seen = new Set();
  const add = (n, fixed = false) => {
    const text = spoken(n);
    if (!text || text.length < 3 || text.length > 40) return;
    const words = text.split(' ');
    if (words.length > 3) return;
    const lower = text.toLowerCase();
    if (seen.has(lower)) return;
    seen.add(lower);
    // A name made only of everyday words ("notes", "voice assistant") is
    // already written right whenever it is heard right.
    if (!fixed && words.every((w) => isEnglish(w))) return;
    const key = sound(text);
    if (key.length < 4) return;   // too short to match by sound ("cru" has its own rule)
    out.push({ text, words: words.length, key, skel: skeleton(key) });
  };
  ['Crundi', 'Claude', 'kanban', 'mindmap', 'Deepgram'].forEach((n) => add(n, true));
  projects.forEach((n) => add(n));
  services.forEach((n) => add(n));
  return out;
}

/** True when [heard] (one or more words) sounds like [name] closely enough. */
function soundsLikeName(heard, name) {
  const k = sound(heard);
  if (!k || k === name.key) return k === name.key;
  // Same opening sound, and the consonants nearly the same: "kurony" ~
  // "crundi" passes, "country" does not.
  if (k.charAt(0) !== name.key.charAt(0)) return false;
  const sk = skeleton(k);
  if (distance(sk, name.skel) > 1) return false;
  const d = distance(k, name.key);
  if (d <= (name.key.length <= 5 ? 1 : 2)) return true;
  // A vowel moved ("kurony"): allowed one more when the first two consonants agree.
  return name.key.length >= 6 && d <= 3 && sk.slice(0, 2) === name.skel.slice(0, 2);
}

const WORD = /[A-Za-z0-9][A-Za-z0-9'-]*/g;

/**
 * The text with Crundi's names put back. Punctuation and everything else
 * is kept as heard.
 * @param {string} text
 * @param {{projects?:string[], services?:string[], names?:object[]}} opts
 */
export function fixNames(text, opts = {}) {
  let s = String(text || '');
  if (!s.trim()) return s;
  const names = opts.names || nameList(opts);

  // Cru, by the sentence around it: a greeting, or a call at the very start.
  s = s.replace(/\b(hey|hi|hello|okay|ok|oi)([,.!]?\s+)(crew|crue|crow|cru|crewe|true|clue|q)\b/gi, (_, g, sp) => `${g}${sp}Cru`);
  s = s.replace(/^(\s*)(crew|crue|crewe)(?=[,.!?])/i, '$1Cru');
  s = s.replace(/\b(hey|hi|hello|okay|ok)(\s*)(crundy|krundi|crunchy)\b/gi, (_, g, sp) => `${g}${sp}Crundi`);
  // Claude, when Claude is being told or asked something.
  s = s.replace(/\b(tell|ask|let|get|have|make)(\s+)(cloud|clawed|clod|claud|clued|plod)\b(?=\s+(to|about|if|whether|for|what|why|how|that)\b)/gi, (_, v, sp) => `${v}${sp}Claude`);

  // Everything else by sound, longest names first so "sam cv" beats "sam".
  const spans = [];
  for (const m of s.matchAll(WORD)) spans.push({ w: m[0], at: m.index, end: m.index + m[0].length });
  if (!spans.length || !names.length) return s;
  const sorted = [...names].sort((a, b) => b.words - a.words || b.key.length - a.key.length);
  const edits = [];
  const taken = new Array(spans.length).fill(false);
  // Words next to each other, with nothing but spaces between.
  const joined = (i, n) => {
    for (let j = i + 1; j < i + n; j++) if (!/^\s+$/.test(s.slice(spans[j - 1].end, spans[j].at))) return false;
    return true;
  };
  // A name already said right is left as it is, before anything is matched by sound.
  for (const name of sorted) {
    for (let i = 0; i + name.words <= spans.length; i++) {
      const heard = spans.slice(i, i + name.words).map((x) => x.w).join(' ');
      if (joined(i, name.words) && heard.toLowerCase() === name.text.toLowerCase()) taken.fill(true, i, i + name.words);
    }
  }
  for (const name of sorted) {
    // The longest run of words first: "Senti Q" is one name, not "Senti" plus a Q.
    for (let n = name.words + 1; n >= name.words; n--) {
      for (let i = 0; i + n <= spans.length; i++) {
        if (taken.slice(i, i + n).some(Boolean) || !joined(i, n)) continue;
        const words = spans.slice(i, i + n).map((x) => x.w);
        const heard = words.join(' ');
        // Plain English is left alone: one English word never becomes a name,
        // and English words together only when they spell it exactly ("deep
        // gram", "jelly fin").
        if (n === 1 && isEnglish(heard)) continue;
        if (n > 1 && words.every((w) => isEnglish(w) && w.length > 2)) {
          if (sound(heard) !== name.key) continue;
        } else if (!soundsLikeName(heard, name)) continue;
        taken.fill(true, i, i + n);
        // Written as the name is, capitalised where the heard word was.
        const cap = /^[A-Z]/.test(words[0]) ? name.text.charAt(0).toUpperCase() + name.text.slice(1) : name.text;
        edits.push({ at: spans[i].at, end: spans[i + n - 1].end, text: cap });
      }
    }
  }
  edits.sort((a, b) => b.at - a.at);
  for (const e of edits) s = s.slice(0, e.at) + e.text + s.slice(e.end);
  return s;
}
