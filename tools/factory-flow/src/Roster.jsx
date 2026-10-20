import { genId, MANAGER_ID, PERMISSION_PRESETS } from '../../../src/factory-model.js';
import { Field, Text, Select, Toggle, SkillText } from './fields.jsx';
import { Icon } from './icons.jsx';
import { PERSONAS } from './personas.js';

const EDIT_TOOLS = ['Edit', 'Write', 'MultiEdit', 'NotebookEdit'];

export function newEmployee(template, f) {
  const t = template ? PERSONAS.find(p => p.key === template) : null;
  let name = t ? t.name : 'New employee';
  let n = 2;
  while (f.roster.some(r => r.name.toLowerCase() === name.toLowerCase())) name = (t ? t.name : 'New employee') + ' ' + n++;
  return { id: genId('emp_'), name, persona: t ? t.persona : '', model: '', effort: '', memory: t ? t.memory : 'keep', tools: { allow: [], deny: t ? [...t.deny] : [] }, skills: [], presets: t && t.presets ? [...t.presets] : [] };
}

export function RosterPane({ f, mutate, selected, setSelected, skills, issuesByEmployee, onRemove, onAddTo }) {
  const r = f.roster.find(x => x.id === selected);
  const set = (fn, key) => mutate(d => { const e = d.roster.find(x => x.id === selected); if (e) fn(e); }, key);
  const used = (id) => f.nodes.filter(n => n.type === 'employee' && n.employeeId === id).length;
  const add = (tpl) => {
    const e = newEmployee(tpl, f);
    mutate(d => {
      d.roster.push(e);
      if (onAddTo) { const n = d.nodes.find(x => x.id === onAddTo); if (n) n.employeeId = e.id; }
    });
    setSelected(e.id);
  };
  if (!r) {
    return (
      <div className="ff-pane">
        <div className="ff-pane-title"><span className="ff-kind">Team</span></div>
        <div className="ff-roster">
          {f.roster.map(x => {
            const iss = issuesByEmployee.get(x.id) || [];
            return (
              <button key={x.id} className="ff-person" onClick={() => setSelected(x.id)}>
                <Icon name={x.id === MANAGER_ID ? 'crown' : 'user'} size={15} />
                <span className="ff-person-name">{x.name || 'Unnamed'}</span>
                <span className="ff-person-meta">{x.id === MANAGER_ID ? 'Manager' : `${used(x.id)} step${used(x.id) === 1 ? '' : 's'}`}{iss.length ? ' · ' + iss.length + ' to check' : ''}</span>
              </button>
            );
          })}
        </div>
        <div className="ff-subtitle">Add someone</div>
        <div className="ff-templates">
          {PERSONAS.map(p => <button key={p.key} className="ff-tpl" onClick={() => add(p.key)} title={p.persona}><Icon name="plus" size={13} />{p.name}</button>)}
          <button className="ff-tpl" onClick={() => add(null)}><Icon name="plus" size={13} />Blank</button>
        </div>
        <p className="ff-hint">Templates are starting points: change the persona however you like.</p>
      </div>
    );
  }
  const isManager = r.id === MANAGER_ID;
  const canEdit = !EDIT_TOOLS.some(t => r.tools.deny.includes(t));
  return (
    <div className="ff-pane">
      <div className="ff-pane-title">
        <button className="ff-iconbtn" title="Back to the team" onClick={() => setSelected(null)}><Icon name="back" size={15} /></button>
        <span className="ff-kind">{isManager ? 'Manager' : 'Employee'}</span>
        {!isManager && <button className="ff-iconbtn danger" title="Remove from the team" onClick={() => onRemove(r.id)}><Icon name="trash" size={15} /></button>}
      </div>
      <Field label="Name"><Text value={r.name} onChange={v => set(e => { e.name = v; }, 'rn:' + r.id)} /></Field>
      <Field label="Persona" hint="Who they are and how they work. They read it and act as it says. /skill-name names a skill they should use.">
        <SkillText skills={skills} rows={9} value={r.persona} onChange={v => set(e => { e.persona = v; }, 'rp:' + r.id)} />
      </Field>
      <div className="ff-grid2">
        <Field label="Model">
          <Select value={r.model} onChange={v => set(e => { e.model = v; })} options={[{ value: '', label: 'Default' }, { value: 'opus', label: 'Opus' }, { value: 'sonnet', label: 'Sonnet' }, { value: 'haiku', label: 'Haiku' }]} />
        </Field>
        <Field label="Effort">
          <Select value={r.effort} onChange={v => set(e => { e.effort = v; })} options={[{ value: '', label: 'Default' }, ...['low', 'medium', 'high', 'xhigh', 'max'].map(x => ({ value: x, label: x }))]} />
        </Field>
      </div>
      {!isManager && (
        <Field label="Memory" hint={r.memory === 'fresh' ? 'A new session each time: useful for reviewers, who should look with fresh eyes.' : 'One session for the whole run: they remember earlier steps.'}>
          <Select value={r.memory} onChange={v => set(e => { e.memory = v; })} options={[{ value: 'keep', label: 'Keeps memory' }, { value: 'fresh', label: 'Fresh each time' }]} />
        </Field>
      )}
      {!isManager && (
        <Toggle checked={canEdit} label="Can edit files" onChange={v => set(e => {
          e.tools.deny = v ? e.tools.deny.filter(t => !EDIT_TOOLS.includes(t)) : [...new Set([...e.tools.deny, ...EDIT_TOOLS])];
        })} />
      )}
      <Field label="Pinned skills" hint="Comma separated. Same as naming them in the persona.">
        <Text value={(r.skills || []).join(', ')} placeholder="frontend-design, run"
          onChange={v => set(e => { e.skills = v.split(',').map(x => x.trim().replace(/^\//, '')).filter(Boolean); }, 'rs:' + r.id)} />
      </Field>
      {!isManager && f.settings.teamMode !== 'bypass' && (
        <Field label="This person may also, without asking" hint="On top of what everyone may (factory settings).">
          <div className="ff-checks">
            {Object.entries(PERMISSION_PRESETS).map(([k, p]) => (
              <Toggle key={k} checked={(r.presets || []).includes(k) || f.settings.presets.includes(k)} label={p.label + (f.settings.presets.includes(k) ? ' (everyone has this)' : '')}
                onChange={v => { if (f.settings.presets.includes(k)) return; set(e => { e.presets = v ? [...new Set([...(e.presets || []), k])] : (e.presets || []).filter(x => x !== k); }); }} />
            ))}
          </div>
        </Field>
      )}
      <details className="ff-more">
        <summary>More tool rules</summary>
        <Field label="Always allowed (this person only)" hint="Claude Code rules, comma separated, e.g. Bash(npm test:*).">
          <Text mono value={r.tools.allow.join(', ')} onChange={v => set(e => { e.tools.allow = v.split(',').map(x => x.trim()).filter(Boolean); }, 'ta:' + r.id)} />
        </Field>
        <Field label="Never allowed">
          <Text mono value={r.tools.deny.join(', ')} onChange={v => set(e => { e.tools.deny = v.split(',').map(x => x.trim()).filter(Boolean); }, 'td:' + r.id)} />
        </Field>
      </details>
      {!isManager && <p className="ff-hint">In {used(r.id)} step{used(r.id) === 1 ? '' : 's'} of the flow.</p>}
    </div>
  );
}
