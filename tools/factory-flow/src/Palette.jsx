import { MANAGER_ID } from '../../../src/factory-model.js';
import { Icon } from './icons.jsx';

export const DRAG_TYPE = 'application/x-crundi-factory-node';

const ITEMS = [
  { kind: 'decision:yesno', icon: 'branch', label: 'Yes / no question' },
  { kind: 'decision:one', icon: 'branch', label: 'Pick one' },
  { kind: 'decision:many', icon: 'branch', label: 'Pick any (parallel)', hint: 'Comes with its Join' },
  { kind: 'join', icon: 'merge', label: 'Join' },
  { kind: 'ask', icon: 'ask', label: 'Ask me' },
  { kind: 'wait', icon: 'clock', label: 'Wait', hint: 'A time, a command, a signal' },
];

/** Drag onto the canvas, or tap to add in the middle of the view. */
export function Palette({ f, onAdd }) {
  const drag = (e, kind) => { e.dataTransfer.setData(DRAG_TYPE, kind); e.dataTransfer.effectAllowed = 'move'; };
  const people = f.roster.filter(r => r.id !== MANAGER_ID);
  return (
    <div className="ff-palette">
      <div className="ff-subtitle">People</div>
      {people.map(r => (
        <div key={r.id} className="ff-pal-item" draggable onDragStart={e => drag(e, 'employee:' + r.id)} onClick={() => onAdd('employee:' + r.id)}>
          <Icon name="user" size={14} /><span>{r.name || 'Unnamed'}</span>
        </div>
      ))}
      <div className="ff-pal-item ghost" draggable onDragStart={e => drag(e, 'employee:')} onClick={() => onAdd('employee:')}>
        <Icon name="plus" size={14} /><span>Someone new</span>
      </div>
      <div className="ff-subtitle">Flow</div>
      {ITEMS.map(it => (
        <div key={it.kind} className="ff-pal-item" draggable onDragStart={e => drag(e, it.kind)} onClick={() => onAdd(it.kind)} title={it.hint || ''}>
          <Icon name={it.icon} size={14} /><span>{it.label}</span>
        </div>
      ))}
      <p className="ff-hint">Drag onto the canvas, or tap to add. Connect an output (bottom) to a step's top. Drag from the side dot of a person to leave a note.</p>
    </div>
  );
}
