import { useState, useRef, useEffect } from 'react';
import { findMentions } from '../../../src/factory-skills.js';
import { Icon } from './icons.jsx';

export function Field({ label, hint, children, row }) {
  return (
    <label className={'ff-field' + (row ? ' row' : '')}>
      <span className="ff-field-label">{label}</span>
      {children}
      {hint && <span className="ff-hint">{hint}</span>}
    </label>
  );
}

export function Text({ value, onChange, placeholder, mono }) {
  return <input className={'ff-input' + (mono ? ' mono' : '')} value={value || ''} placeholder={placeholder} onChange={e => onChange(e.target.value)} />;
}

export function Num({ value, onChange, min = 0, max = 100000, placeholder }) {
  return <input className="ff-input" type="number" inputMode="numeric" min={min} max={max} value={value === 0 || value ? value : ''} placeholder={placeholder}
    onChange={e => onChange(e.target.value === '' ? 0 : Math.max(min, Math.min(max, Math.round(+e.target.value || 0))))} />;
}

export function Select({ value, onChange, options }) {
  return (
    <select className="ff-input" value={value ?? ''} onChange={e => onChange(e.target.value)}>
      {options.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
    </select>
  );
}

export function Toggle({ checked, onChange, label }) {
  return (
    <label className="ff-toggle">
      <input type="checkbox" checked={!!checked} onChange={e => onChange(e.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

/**
 * A text box where /skill-name can be written: typing "/" offers the skills
 * this machine has, and the skills named are shown underneath, marked when a
 * name is not installed for all projects.
 */
export function SkillText({ value, onChange, skills, rows = 3, placeholder }) {
  const ref = useRef(null);
  const [menu, setMenu] = useState(null);   // { start, query, items, at }
  const known = new Map((skills || []).map(s => [s.name, s]));
  const mentions = findMentions(value || '');

  function look(el) {
    const caret = el.selectionStart;
    const before = el.value.slice(0, caret);
    const m = before.match(/(^|[\s(\[{,;"'`])\/([a-z0-9:-]*)$/);
    if (!m) return setMenu(null);
    const q = m[2];
    const items = (skills || []).filter(s => s.name.startsWith(q)).slice(0, 8);
    if (!items.length) return setMenu(null);
    setMenu({ start: caret - q.length, query: q, items, at: 0 });
  }
  function pick(s) {
    const el = ref.current;
    const v = el.value;
    const caret = el.selectionStart;
    const next = v.slice(0, menu.start) + s.name + ' ' + v.slice(caret);
    onChange(next);
    setMenu(null);
    requestAnimationFrame(() => { el.focus(); const p = menu.start + s.name.length + 1; el.setSelectionRange(p, p); });
  }
  function key(e) {
    if (!menu) return;
    if (e.key === 'ArrowDown') { e.preventDefault(); setMenu({ ...menu, at: (menu.at + 1) % menu.items.length }); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setMenu({ ...menu, at: (menu.at - 1 + menu.items.length) % menu.items.length }); }
    else if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); pick(menu.items[menu.at]); }
    else if (e.key === 'Escape') { e.preventDefault(); setMenu(null); }
  }
  return (
    <div className="ff-skilltext">
      <textarea ref={ref} className="ff-input" rows={rows} value={value || ''} placeholder={placeholder}
        onChange={e => { onChange(e.target.value); look(e.target); }} onKeyDown={key}
        onClick={e => look(e.target)} onBlur={() => setTimeout(() => setMenu(null), 150)} />
      {menu && (
        <div className="ff-menu">
          {menu.items.map((s, i) => (
            <div key={s.name} className={'ff-menu-item' + (i === menu.at ? ' on' : '')} onMouseDown={e => { e.preventDefault(); pick(s); }}>
              <b>/{s.name}</b><span>{s.description}</span>
            </div>
          ))}
        </div>
      )}
      {mentions.length > 0 && (
        <div className="ff-chips">
          {mentions.map(n => (
            <span key={n} className={'ff-chip ' + (known.has(n) ? 'ok' : 'warn')}
              title={known.has(n) ? known.get(n).description : 'Not installed for all projects. It will need to exist in the project you run this in.'}>
              <Icon name={known.has(n) ? 'check' : 'alert'} size={11} />/{n}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

/** A list of labelled choices (decision options, Ask me replies). */
export function ChoiceList({ items, onChange, onRemove, onAdd, addLabel, min = 1 }) {
  return (
    <div className="ff-choices">
      {items.map((it, i) => (
        <div key={it.id} className="ff-choice">
          <input className="ff-input" value={it.label} placeholder={'Choice ' + (i + 1)}
            onChange={e => onChange(items.map(x => x.id === it.id ? { ...x, label: e.target.value } : x))} />
          <button className="ff-iconbtn" title="Remove" disabled={items.length <= min} onClick={() => onRemove(it.id)}><Icon name="x" size={14} /></button>
        </div>
      ))}
      <button className="ff-btn ghost" onClick={onAdd}><Icon name="plus" size={14} />{addLabel}</button>
    </div>
  );
}

export function useDebounced(fn, ms) {
  const t = useRef(null);
  const f = useRef(fn);
  f.current = fn;
  useEffect(() => () => clearTimeout(t.current), []);
  return (...a) => { clearTimeout(t.current); t.current = setTimeout(() => f.current(...a), ms); };
}
