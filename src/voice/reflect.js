/**
 * voice/reflect.js — Cru learning its owner (9 Nov 2026, after Hermes Agent).
 *
 * Three small pieces, kept simple on purpose (Cru is the same kind of model
 * as the one that wrote this, and short whole files are easier to keep right
 * than many small edits):
 *
 *  1. Two small files Cru keeps about the owner, with hard size limits so they
 *     are merged and pruned instead of growing: persona.md (how he wants Cru
 *     to behave) and owner.md (facts about him: routines, people, the words he
 *     uses, what he cares about first).
 *  2. Reflection: when the conversation is compacted, the same request that
 *     summarises it also rewrites those two files whole, from what the
 *     conversation taught. Unchanged files are simply left out.
 *  3. Routines: procedures Cru saves under /memories/routines/ when the owner
 *     asks for the same multi-step thing again, listed at the start of every
 *     conversation so Cru knows they exist.
 */

export const LIMITS = { '/memories/persona.md': 3000, '/memories/owner.md': 2500 };
export const ROUTINES_DIR = '/memories/routines/';

/** The size limit of a notebook path, or 0 for none. */
export function limitOf(path) { return LIMITS[String(path || '')] || 0; }

/**
 * Is writing [next] to [path] allowed, given [prev]? A file over its limit may
 * always shrink (that is how it gets back under), never grow.
 */
export function capCheck(path, prev, next) {
  const max = limitOf(path);
  if (!max) return '';
  const n = String(next || '').length, p = String(prev || '').length;
  if (n <= max || n < p) return '';
  return `${path} is limited to ${max} characters (this would make it ${n}). Merge or drop what is outdated, then write it again shorter.`;
}

/** What the compaction request asks for on top of the summary. */
export function reflectionAsk({ persona = '', owner = '' } = {}) {
  return `

Then, before you go on, learn from this conversation. You keep two small files about the owner:
- persona.md: how he wants you to behave (manner, length, habits, what annoys him). At most ${LIMITS['/memories/persona.md']} characters.
- owner.md: facts about him: his routines, the projects and people he cares about, the words and names he uses for things, what he wants first. At most ${LIMITS['/memories/owner.md']} characters.
If this conversation taught you something lasting about either, rewrite that file whole: keep everything still true, merge duplicates, drop what is outdated or contradicted, short bullet points in his terms, never secrets. Leave a file out if nothing should change.

Answer in exactly this form:
<summary>the summary</summary>
<persona>the whole new persona.md, only if it changes</persona>
<owner>the whole new owner.md, only if it changes</owner>

persona.md now:
${persona.trim() || '(empty)'}

owner.md now:
${owner.trim() || '(empty)'}`;
}

/** The parts of a compaction answer. A plain answer (no tags) is all summary. */
export function parseReflection(text) {
  const s = String(text || '');
  const tag = (name) => { const m = s.match(new RegExp(`<${name}>([\\s\\S]*?)</${name}>`)); return m ? m[1].trim() : null; };
  const summary = tag('summary');
  return { summary: summary !== null ? summary : s.replace(/<(persona|owner)>[\s\S]*?<\/\1>/g, '').trim(), persona: tag('persona'), owner: tag('owner') };
}

/**
 * The note at the start of each conversation: how to behave, who the owner
 * is, and which routines exist (name and when to use it, not the steps).
 */
export function selfNote({ persona = '', owner = '', routines = [] } = {}) {
  const parts = [];
  if (persona.trim()) parts.push(`[How the owner wants you to be (/memories/persona.md):\n${persona.trim().slice(0, 4000)}]`);
  else parts.push('[/memories/persona.md is empty: nothing learnt yet about how the owner wants you to be.]');
  if (owner.trim()) parts.push(`[About the owner (/memories/owner.md):\n${owner.trim().slice(0, 3000)}]`);
  if (routines.length) {
    parts.push(`[Your routines (read the file and follow it when one fits):\n${routines.slice(0, 30).map(r => `- ${r.path.slice(ROUTINES_DIR.length).replace(/\.md$/, '')}: ${firstLine(r.text)}`).join('\n')}]`);
  }
  return parts.join('\n') + '\n';
}

function firstLine(t) { return String(t || '').split('\n').map(l => l.replace(/^#+\s*/, '').trim()).find(Boolean)?.slice(0, 160) || ''; }
