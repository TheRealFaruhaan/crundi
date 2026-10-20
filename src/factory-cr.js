/**
 * factory-cr.js — the change request (CR) that starts a factory run.
 *
 * A run starts from a CR rather than a one-line task: a title plus sections
 * from the factory's CR template (goal, current situation, expected result,
 * "done when", out of scope, constraints), and attachments. The Manager gets
 * the whole CR as its first message; every step carries a brief (title, goal,
 * done when) and the path of the full CR in the shared folder.
 *
 * Pure: used by the runner, the model (template normalising) and the browser.
 * Spec: specs/factory.md §4.1.
 */

export const CR_LIMITS = { title: 120, section: 6000, sections: 12, label: 60, hint: 300, attachments: 10 };

/** The default template. `list` sections are one item per line (rendered as a checklist). */
export const DEFAULT_CR_TEMPLATE = [
  { id: 'goal', label: 'Goal', required: true, list: false,
    hint: 'Why this change, and what it should achieve. One or two sentences.' },
  { id: 'current', label: 'Current situation', required: false, list: false,
    hint: 'What exists or happens today, and what is wrong or missing. For a new project: what there is to start from.' },
  { id: 'expected', label: 'Expected result', required: true, list: false,
    hint: 'What it should do when finished: behaviour, screens, data, APIs. Concrete examples help.' },
  { id: 'done', label: 'Done when', required: true, list: true,
    hint: 'One check per line. The reviewers hold the work to these, and the Manager finishes only when they all pass.' },
  { id: 'scope', label: 'Out of scope', required: false, list: true,
    hint: 'What not to do or touch, one per line.' },
  { id: 'notes', label: 'Constraints and references', required: false, list: false,
    hint: 'Tech to use or avoid, conventions, links, anything the team should know.' },
];

const str = (v) => (typeof v === 'string' ? v : v == null ? '' : String(v));
const slug = (s) => str(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 30);

/** A factory's template, cleaned. Empty or missing means the default. */
export function normalizeTemplate(t) {
  if (!Array.isArray(t) || !t.length) return DEFAULT_CR_TEMPLATE.map(s => ({ ...s }));
  const out = [];
  const seen = new Set();
  for (const raw of t.slice(0, CR_LIMITS.sections)) {
    if (!raw || typeof raw !== 'object') continue;
    const label = str(raw.label).trim().slice(0, CR_LIMITS.label);
    if (!label) continue;
    let id = slug(raw.id) || slug(label) || 's';
    while (seen.has(id) || id === 'title') id += '-2';
    seen.add(id);
    out.push({ id, label, hint: str(raw.hint).trim().slice(0, CR_LIMITS.hint), required: !!raw.required, list: !!raw.list });
  }
  return out.length ? out : DEFAULT_CR_TEMPLATE.map(s => ({ ...s }));
}

const listItems = (v) => str(v).split('\n').map(x => x.replace(/^\s*(?:[-*•]|\d+[.)]|\[[ xX]?\])\s*/, '').trim()).filter(Boolean);

/**
 * Check a CR against its template.
 * @param {object} cr        { title, sections: { id: text }, attachments: [{ path, name }] }
 * @param {Array}  template  normalizeTemplate(...)
 * @returns {{ ok: true, cr } | { ok: false, error, field }}
 */
export function checkCr(cr, template) {
  const t = normalizeTemplate(template);
  const c = cr && typeof cr === 'object' ? cr : {};
  const title = str(c.title).replace(/\s+/g, ' ').trim().slice(0, CR_LIMITS.title);
  if (!title) return { ok: false, field: 'title', error: 'Give the change request a title.' };
  const sections = {};
  for (const s of t) {
    const v = str((c.sections || {})[s.id]).replace(/\r/g, '').trim().slice(0, CR_LIMITS.section);
    if (s.required && !(s.list ? listItems(v).length : v)) return { ok: false, field: s.id, error: `Fill in "${s.label}".` };
    if (v) sections[s.id] = v;
  }
  const attachments = (Array.isArray(c.attachments) ? c.attachments : []).slice(0, CR_LIMITS.attachments)
    .map(a => ({ path: str(a && a.path), name: str(a && a.name).slice(0, 120) || str(a && a.path).split(/[\\/]/).pop() }))
    .filter(a => a.path);
  return { ok: true, cr: { title, sections, attachments } };
}

/**
 * The whole CR as markdown: the file in the shared folder and the Manager's
 * first message. Attachments are listed by the path they were copied to.
 */
export function crMarkdown(cr, template) {
  const t = normalizeTemplate(template);
  const out = [`# Change request: ${cr.title}`, ''];
  for (const s of t) {
    const v = (cr.sections || {})[s.id];
    if (!v) continue;
    out.push(`## ${s.label}`);
    if (s.list) for (const item of listItems(v)) out.push(`- [ ] ${item}`);
    else out.push(v);
    out.push('');
  }
  if ((cr.attachments || []).length) {
    out.push('## Attachments', 'Open each one (images can be read directly):');
    for (const a of cr.attachments) out.push(`- ${a.name}: ${a.path}`);
    out.push('');
  }
  return out.join('\n').trim() + '\n';
}

/**
 * The short form every step carries: the title, the goal (or the first
 * section), and the checks for done. The full CR is a file.
 */
export function crBrief(cr, template, path = '') {
  const t = normalizeTemplate(template);
  const lines = [cr.title];
  const goalSec = t.find(s => s.id === 'goal') || t.find(s => !s.list && (cr.sections || {})[s.id]);
  const goal = goalSec && (cr.sections || {})[goalSec.id];
  if (goal) lines.push(`${goalSec.label}: ${goal.length > 600 ? goal.slice(0, 600) + '…' : goal}`);
  const doneSec = t.find(s => s.id === 'done') || t.find(s => s.list && (cr.sections || {})[s.id]);
  const done = doneSec ? listItems((cr.sections || {})[doneSec.id]) : [];
  if (done.length) { lines.push(`${doneSec.label}:`); for (const d of done.slice(0, 15)) lines.push(`- ${d}`); }
  if (path) lines.push(`The full change request${(cr.attachments || []).length ? `, with ${cr.attachments.length} attachment${cr.attachments.length === 1 ? '' : 's'},` : ''} is in ${path}`);
  return lines.join('\n');
}

/** The text that names skills, for checking /skill-name mentions. */
export function crText(cr) {
  return [cr.title, ...Object.values(cr.sections || {})].join('\n');
}
