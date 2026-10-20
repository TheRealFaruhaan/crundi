/**
 * factory-skills.js — skills in a factory. Pure.
 *
 * Employees are full Claude sessions and use skills like any chat. A factory
 * can also name skills on purpose: pinned on a roster entry, or written as
 * /skill-name anywhere a prompt is written (persona, the task, a decision's
 * question, a note label, an Ask me prompt, a note from the person). Each
 * mention becomes a line in the prompt naming the skill and its description,
 * with "use it via the Skill tool"; the skill's body is not inlined, Claude
 * loads it when it invokes it.
 *
 * A mention is "/" + a skill name (lowercase letters, digits, hyphens, with an
 * optional "plugin:" prefix), at the start of the text or after a space or
 * punctuation, and NOT followed by another "/", or a "." then a letter — so /home/crundi,
 * src/a.js, https://x.io/a and /api/x are never taken for skills.
 *
 * Spec: specs/factory.md §2.5.
 */

const MENTION = /(^|[\s(\[{,;"'`])\/([a-z0-9][a-z0-9-]*(?::[a-z0-9][a-z0-9-]*)?)(?![\w/:-]|\.\w)/g;

/** Skill names mentioned in a piece of text, in order, without repeats. */
export function findMentions(text) {
  const out = [];
  const s = String(text || '');
  for (const m of s.matchAll(MENTION)) if (!out.includes(m[2])) out.push(m[2]);
  return out;
}

/**
 * Every skill a factory names, and where. Used for the start check, and by
 * the editor for chips and warnings.
 * @returns {Map<string, Array<{ where: string, nodeId?: string, employeeId?: string, edgeId?: string }>>}
 */
export function mentionsInFactory(f) {
  const out = new Map();
  const add = (name, at) => { if (!out.has(name)) out.set(name, []); out.get(name).push(at); };
  for (const n of findMentions(f.description)) add(n, { where: 'description' });
  for (const r of f.roster || []) {
    for (const n of findMentions(r.persona)) add(n, { where: 'persona', employeeId: r.id });
    for (const n of r.skills || []) add(String(n).replace(/^\//, ''), { where: 'pinned', employeeId: r.id });
  }
  for (const node of f.nodes || []) {
    if (node.type === 'decision') for (const n of findMentions(node.decision?.question)) add(n, { where: 'question', nodeId: node.id });
    if (node.type === 'ask') for (const n of findMentions(node.ask?.prompt)) add(n, { where: 'ask', nodeId: node.id });
  }
  for (const e of f.edges || []) for (const n of findMentions(e.noteLabel)) add(n, { where: 'note', edgeId: e.id });
  return out;
}

/** Names mentioned anywhere in the factory or the task that the project cannot see. */
export function missingSkills(f, task, known) {
  const names = new Set([...mentionsInFactory(f).keys(), ...findMentions(task)]);
  return [...names].filter(n => !known.has(n));
}

/**
 * The prompt lines for a set of mentioned skills.
 * @param {string[]} names
 * @param {Map<string,string>} known   name -> description, for what the project can see
 */
export function skillLines(names, known) {
  if (!names.length) return [];
  const lines = ['Skills to use for this (load each with the Skill tool when you reach the part it covers):'];
  for (const n of names) {
    const d = known.get(n);
    lines.push(d ? `- /${n}: ${oneLine(d, 300)}` : `- /${n} (not found on this machine; carry on without it)`);
  }
  return lines;
}

function oneLine(s, max) {
  const t = String(s || '').replace(/\s+/g, ' ').trim();
  return t.length > max ? t.slice(0, max - 1) + '…' : t;
}
