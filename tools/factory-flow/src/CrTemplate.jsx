import { genId } from '../../../src/factory-model.js';
import { DEFAULT_CR_TEMPLATE, CR_LIMITS } from '../../../src/factory-cr.js';
import { Text, Toggle } from './fields.jsx';
import { Icon } from './icons.jsx';

/**
 * The change request form a run starts from: the factory's sections (after
 * the title, which is always there). Each: a label, a hint shown as the
 * placeholder, required or not, and whether it is a list (one item per line,
 * a checklist in the CR).
 */
export function CrTemplateEditor({ f, mutate }) {
  const t = f.settings.crTemplate || [];
  const set = (fn, key) => mutate(d => { d.settings.crTemplate = d.settings.crTemplate || []; fn(d.settings.crTemplate); }, key);
  const isDefault = JSON.stringify(t.map(({ id, label, hint, required, list }) => ({ id, label, hint, required, list }))) === JSON.stringify(DEFAULT_CR_TEMPLATE);
  const move = (i, by) => set(a => { const j = i + by; if (j < 0 || j >= a.length) return; [a[i], a[j]] = [a[j], a[i]]; });
  return (
    <details className="ff-more">
      <summary>Change request form</summary>
      <p className="ff-hint">What the person fills in to start a run. The Manager gets all of it; every step gets the title, the goal and the "done" checks. The title and attachments are always there.</p>
      <div className="ff-cr-list">
        {t.map((s, i) => (
          <div className="ff-cr-sec" key={s.id}>
            <div className="ff-cr-sec-top">
              <Text value={s.label} placeholder="Section name" onChange={v => set(a => { a[i].label = v.slice(0, CR_LIMITS.label); }, 'crl' + s.id)} />
              <button className="ff-icon-btn" title="Move up" disabled={i === 0} onClick={() => move(i, -1)}><Icon name="chevron-up" size={13} /></button>
              <button className="ff-icon-btn" title="Move down" disabled={i === t.length - 1} onClick={() => move(i, 1)}><Icon name="chevron-down" size={13} /></button>
              <button className="ff-icon-btn" title="Remove" disabled={t.length <= 1} onClick={() => set(a => { a.splice(i, 1); })}><Icon name="trash" size={13} /></button>
            </div>
            <Text value={s.hint} placeholder="Hint: what to write here" onChange={v => set(a => { a[i].hint = v.slice(0, CR_LIMITS.hint); }, 'crh' + s.id)} />
            <div className="ff-cr-sec-flags">
              <Toggle checked={s.required} label="Required" onChange={v => set(a => { a[i].required = v; })} />
              <Toggle checked={s.list} label="A list" onChange={v => set(a => { a[i].list = v; })} />
            </div>
          </div>
        ))}
      </div>
      <div className="ff-btnrow">
        <button className="ff-btn" disabled={t.length >= CR_LIMITS.sections} onClick={() => set(a => { a.push({ id: genId('s'), label: 'New section', hint: '', required: false, list: false }); })}><Icon name="plus" size={13} /> Add a section</button>
        {!isDefault && <button className="ff-btn ghost" onClick={() => mutate(d => { d.settings.crTemplate = DEFAULT_CR_TEMPLATE.map(x => ({ ...x })); })}>Back to the default</button>}
      </div>
    </details>
  );
}
