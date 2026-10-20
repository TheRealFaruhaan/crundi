import { Handle, Position, BaseEdge, EdgeLabelRenderer, getBezierPath } from '@xyflow/react';
import { Icon } from './icons.jsx';
import { handleLabel, AGENTS } from './convert.js';

const META = {
  start: { icon: 'play', kind: 'Start' },
  manager: { icon: 'crown', kind: 'Manager' },
  employee: { icon: 'user', kind: 'Employee' },
  decision: { icon: 'branch', kind: 'Decision' },
  join: { icon: 'merge', kind: 'Join' },
  ask: { icon: 'ask', kind: 'Ask me' },
  wait: { icon: 'clock', kind: 'Wait' },
  end: { icon: 'flag', kind: 'End' },
};

function Badge({ issues }) {
  if (!issues.length) return null;
  const errors = issues.filter(i => i.level === 'error');
  const level = errors.length ? 'error' : 'warning';
  const shown = errors.length ? errors : issues;
  return (
    <div className={'ff-badge ' + level} title={shown.map(i => i.message + (i.fix ? '\n  ' + i.fix : '')).join('\n')}>
      <Icon name="alert" size={11} />{shown.length}
    </div>
  );
}

function Tokens({ tokens }) {
  if (!tokens.length) return null;
  const t = tokens[0];
  const label = { work: 'working', answer: 'deciding', ask: 'waiting on you', join: 'waiting', wait: 'waiting', finished: 'finished' }[t.status] || t.status;
  return <div className={'ff-token ' + t.status}>{label}{tokens.length > 1 ? ' ×' + tokens.length : ''}</div>;
}

function body(n, d) {
  switch (n.type) {
    case 'start': return 'The task comes in';
    case 'manager': return d.employee?.model ? 'Model: ' + d.employee.model : 'Leads the team, decides the end';
    case 'employee': return d.employee ? (d.employee.model ? d.employee.model + ' · ' : '') + (d.employee.memory === 'fresh' ? 'fresh each time' : 'keeps memory') : 'Nobody assigned';
    case 'decision': {
      const k = { yesno: 'Yes / no', one: 'Pick one', many: 'Pick any' }[n.decision.kind];
      return (n.decision.question || 'No question yet') + (d.answeredBy ? ' — ' + d.answeredBy + ' answers' : '') + ' · ' + k;
    }
    case 'join': return d.join ? 'Waits for the branches of ' + (d.join.label || 'its decision') : 'Choose which decision it joins';
    case 'ask': return n.ask.prompt || 'No question yet';
    case 'wait': return waitText(n.wait);
    case 'end': return 'The result, presented by the Manager';
    default: return '';
  }
}

/** A wait in a few words: "10 min", "until 07:30", "until `npm test` passes (60 min max)". */
export function waitText(w) {
  if (!w) return '';
  const lim = (m) => (m >= 120 && m % 60 === 0 ? m / 60 + ' h' : m + ' min');
  if (w.kind === 'time') return 'For ' + lim(w.minutes);
  if (w.kind === 'until') return 'Until ' + w.at;
  if (w.kind === 'command') return (w.command ? 'Until `' + w.command + '` succeeds' : 'No command yet') + ' · ' + lim(w.timeout) + ' max';
  return 'For a signal' + (w.what ? ': ' + w.what : '') + ' · ' + lim(w.timeout) + ' max';
}

function title(n, d) {
  if (n.label) return n.label;
  if (n.type === 'employee') return d.employee?.name || 'Employee';
  if (n.type === 'manager') return d.employee?.name || 'Manager';
  return META[n.type]?.kind || n.type;
}

export function FactoryNode({ data, selected }) {
  const n = data.node;
  const m = META[n.type] || META.employee;
  const outs = data.outputs;
  const errs = data.issues.some(i => i.level === 'error');
  const active = data.tokens.length > 0;
  return (
    <div className={['ff-node', 'ff-' + n.type, selected ? 'sel' : '', errs ? 'bad' : '', active ? 'active' : '', data.visited && !active ? 'visited' : '', data.retired ? 'retired' : ''].join(' ')}>
      {n.type !== 'start' && <Handle type="target" position={Position.Top} id="in" className="ff-h-in" />}
      {AGENTS.has(n.type) && <Handle type="target" position={Position.Left} id="notein" className="ff-h-note" title="Notes arrive here" />}
      <Badge issues={data.issues} />
      <Tokens tokens={data.tokens} />
      <div className="ff-node-head"><Icon name={m.icon} size={14} /><span className="ff-node-kind">{m.kind}</span></div>
      <div className="ff-node-title">{title(n, data)}</div>
      <div className="ff-node-body">{data.live ? data.live : body(n, data)}</div>
      {data.skills && data.skills.length > 0 && <div className="ff-node-skills">{data.skills.map(k => <span key={k}>/{k}</span>)}</div>}
      {outs.length > 0 && (
        <div className="ff-outs">
          {outs.map((h) => (
            <div key={h} className={'ff-out ' + (h === 'finish' ? 'finish' : '')}>
              {handleLabel(n, h) && <span className="ff-out-label">{handleLabel(n, h)}</span>}
              <Handle type="source" position={Position.Bottom} id={h} className={'ff-h-out ' + (h === 'finish' ? 'finish' : '')} />
            </div>
          ))}
        </div>
      )}
      {AGENTS.has(n.type) && <Handle type="source" position={Position.Right} id="note" className="ff-h-note" title="Drag to leave a note for someone" />}
    </div>
  );
}

export const nodeTypes = Object.fromEntries(Object.keys(META).map(k => [k, FactoryNode]));

export function FactoryEdge(props) {
  const { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, selected, markerEnd } = props;
  const e = data.edge;
  const note = e.kind === 'note';
  // A connection back up the flow (a loop) goes round the side rather than
  // straight through the steps in between, so it reads as a loop.
  const back = !note && data.laneX != null;
  const [path, lx, ly] = back ? loopPath(sourceX, sourceY, targetX, targetY, data.laneX) : getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, curvature: 0.3 });
  const errs = data.issues.some(i => i.level === 'error');
  const bits = [];
  if (note) bits.push(e.noteLabel || 'note');
  else {
    if (e.requireNote) bits.push((e.noteLabel || 'note') + ' required');
    else if (e.noteLabel) bits.push(e.noteLabel);
    if (e.maxRounds) bits.push(data.used != null ? `round ${Math.min(data.used, e.maxRounds)}/${e.maxRounds}` : `max ${e.maxRounds}`);
  }
  const cls = ['ff-edge', note ? 'note' : 'flow', selected ? 'sel' : '', errs ? 'bad' : '', e.maxRounds ? 'loop' : '', data.used ? 'used' : '', data.pulse ? 'pulse' : ''].join(' ');
  return (
    <>
      <BaseEdge id={id} path={path} className={cls} markerEnd={markerEnd} />
      {bits.length > 0 && (
        <EdgeLabelRenderer>
          <div className={'ff-elabel ' + (note ? 'note' : '') + (selected ? ' sel' : '') + (errs ? ' bad' : '')}
            style={{ transform: `translate(-50%, -50%) translate(${lx}px,${ly}px)` }}>
            {e.maxRounds && !note ? <Icon name="loop" size={11} /> : note ? <Icon name="note" size={11} /> : null}
            {bits.join(' · ')}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}

function loopPath(sx, sy, tx, ty, laneX) {
  const side = Math.max(laneX || 0, sx + 40, tx + 40);
  const r = 14, down = sy + 24, up = ty - 24;
  const d = [
    `M ${sx} ${sy}`, `L ${sx} ${down - r}`, `Q ${sx} ${down} ${sx + r} ${down}`,
    `L ${side - r} ${down}`, `Q ${side} ${down} ${side} ${down - r}`,
    `L ${side} ${up + r}`, `Q ${side} ${up} ${side - r} ${up}`,
    `L ${tx + r} ${up}`, `Q ${tx} ${up} ${tx} ${up + r}`, `L ${tx} ${ty}`,
  ].join(' ');
  return [d, side, (down + up) / 2];
}

export const edgeTypes = { factory: FactoryEdge };
