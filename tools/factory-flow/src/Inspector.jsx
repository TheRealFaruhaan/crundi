import { genId, MANAGER_ID } from '../../../src/factory-model.js';
import { Field, Text, Num, Select, Toggle, SkillText, ChoiceList } from './fields.jsx';
import { Icon } from './icons.jsx';
import { handleLabel } from './convert.js';

const KIND_LABEL = { start: 'Start', manager: 'Manager', employee: 'Employee', decision: 'Decision', join: 'Join', ask: 'Ask me', end: 'End' };

function IssueList({ issues, onPick }) {
  if (!issues.length) return null;
  return (
    <div className="ff-issues-inline">
      {issues.map((it, i) => (
        <div key={i} className={'ff-issue ' + it.level} onClick={onPick ? () => onPick(it) : undefined}>
          <Icon name="alert" size={13} />
          <div><div>{it.message}</div>{it.fix && <div className="ff-hint">{it.fix}</div>}</div>
        </div>
      ))}
    </div>
  );
}

export function NodeInspector({ f, node, issues, skills, mutate, remove, openRoster }) {
  const set = (fn, key) => mutate(d => { const n = d.nodes.find(x => x.id === node.id); if (n) fn(n, d); }, key);
  const employees = f.roster.filter(r => r.id !== MANAGER_ID);
  const forks = f.nodes.filter(n => n.type === 'decision' && n.decision.kind === 'many');
  const dropHandles = (d, keep) => { d.edges = d.edges.filter(e => !(e.source === node.id && e.kind === 'flow' && !keep.includes(e.sourceHandle))); };
  return (
    <div className="ff-pane">
      <div className="ff-pane-title"><span className="ff-kind">{KIND_LABEL[node.type]}</span>
        {!['start', 'manager', 'end'].includes(node.type) && <button className="ff-iconbtn danger" title="Delete" onClick={remove}><Icon name="trash" size={15} /></button>}
      </div>
      <IssueList issues={issues} />
      <Field label="Label" hint="Shown on the canvas; also how the team refers to this step.">
        <Text value={node.label} placeholder={KIND_LABEL[node.type]} onChange={v => set(n => { n.label = v; }, 'label:' + node.id)} />
      </Field>

      {node.type === 'employee' && (
        <Field label="Who does this step">
          <Select value={node.employeeId || ''} onChange={v => { if (v === '__new') return openRoster('new', node.id); set(n => { n.employeeId = v; }); }}
            options={[{ value: '', label: 'Choose someone…' }, ...employees.map(r => ({ value: r.id, label: r.name || 'Unnamed' })), { value: '__new', label: '+ Add someone to the team…' }]} />
        </Field>
      )}
      {node.type === 'employee' && node.employeeId && <button className="ff-btn ghost" onClick={() => openRoster(node.employeeId)}><Icon name="user" size={14} />Edit their persona</button>}
      {node.type === 'manager' && <button className="ff-btn ghost" onClick={() => openRoster(MANAGER_ID)}><Icon name="crown" size={14} />Edit the Manager's persona</button>}

      {node.type === 'decision' && (
        <>
          <Field label="Question" hint="Answered at handoff. You can name skills with /skill-name.">
            <SkillText skills={skills} rows={2} value={node.decision.question} onChange={v => set(n => { n.decision.question = v; }, 'q:' + node.id)} placeholder="Does it need a UI change?" />
          </Field>
          <Field label="Kind of answer">
            <Select value={node.decision.kind} options={[{ value: 'yesno', label: 'Yes or no' }, { value: 'one', label: 'Pick one' }, { value: 'many', label: 'Pick any (runs them in parallel)' }]}
              onChange={k => set((n, d) => {
                n.decision.kind = k;
                if (k === 'yesno') { n.decision.options = []; dropHandles(d, ['yes', 'no']); }
                else {
                  if (n.decision.options.length < 2) n.decision.options = [{ id: genId('o_'), label: 'Option 1' }, { id: genId('o_'), label: 'Option 2' }];
                  dropHandles(d, [...n.decision.options.map(o => o.id), ...(k === 'many' ? ['none'] : [])]);
                }
              })} />
          </Field>
          {node.decision.kind !== 'yesno' && (
            <Field label="Options">
              <ChoiceList items={node.decision.options} min={2} addLabel="Add option"
                onChange={items => set(n => { n.decision.options = items; }, 'opts:' + node.id)}
                onRemove={id => set((n, d) => { n.decision.options = n.decision.options.filter(o => o.id !== id); d.edges = d.edges.filter(e => !(e.source === node.id && e.sourceHandle === id)); })}
                onAdd={() => set(n => { n.decision.options.push({ id: genId('o_'), label: 'Option ' + (n.decision.options.length + 1) }); })} />
            </Field>
          )}
          <Field label="Who answers" hint="Usually whoever arrives here. After a Join or an Ask me, someone has to be named.">
            <Select value={node.decision.answeredBy || ''} onChange={v => set(n => { n.decision.answeredBy = v || null; })}
              options={[{ value: '', label: 'Whoever arrives' }, ...f.roster.map(r => ({ value: r.id, label: r.name || r.id }))]} />
          </Field>
        </>
      )}

      {node.type === 'ask' && (
        <>
          <Field label="What you are asked" hint="You get a notification and the run waits for your reply.">
            <SkillText skills={skills} rows={2} value={node.ask.prompt} onChange={v => set(n => { n.ask.prompt = v; }, 'ask:' + node.id)} placeholder="Ship it?" />
          </Field>
          <Field label="Replies">
            <ChoiceList items={node.ask.replies} min={1} addLabel="Add reply"
              onChange={items => set(n => { n.ask.replies = items; }, 'rep:' + node.id)}
              onRemove={id => set((n, d) => { n.ask.replies = n.ask.replies.filter(o => o.id !== id); d.edges = d.edges.filter(e => !(e.source === node.id && e.sourceHandle === id)); })}
              onAdd={() => set(n => { n.ask.replies.push({ id: genId('o_'), label: 'Reply ' + (n.ask.replies.length + 1) }); })} />
          </Field>
        </>
      )}

      {node.type === 'join' && (
        <Field label="Joins the branches of" hint="The pick-any decision whose parallel branches meet here.">
          <Select value={node.join.forkId || ''} onChange={v => set(n => { n.join.forkId = v; })}
            options={[{ value: '', label: 'Choose a decision…' }, ...forks.map(x => ({ value: x.id, label: x.label || x.decision.question || 'Decision' }))]} />
        </Field>
      )}

      {node.type === 'start' && <p className="ff-hint">The task you give when starting the factory enters here and goes to the Manager.</p>}
      {node.type === 'end' && <p className="ff-hint">Only the Manager's Finish reaches here. The Manager then presents the result and you decide whether to merge it.</p>}
    </div>
  );
}

export function EdgeInspector({ f, edge, issues, mutate, remove }) {
  const set = (fn, key) => mutate(d => { const e = d.edges.find(x => x.id === edge.id); if (e) fn(e); }, key);
  const src = f.nodes.find(n => n.id === edge.source), dst = f.nodes.find(n => n.id === edge.target);
  const name = n => n ? (n.label || (n.type === 'employee' ? f.roster.find(r => r.id === n.employeeId)?.name : '') || KIND_LABEL[n.type]) : '?';
  const note = edge.kind === 'note';
  return (
    <div className="ff-pane">
      <div className="ff-pane-title"><span className="ff-kind">{note ? 'Note' : 'Connection'}</span>
        {edge.sourceHandle !== 'finish' && <button className="ff-iconbtn danger" title="Delete" onClick={remove}><Icon name="trash" size={15} /></button>}
      </div>
      <p className="ff-hint">{name(src)}{!note && src && edge.sourceHandle !== 'out' ? ` (${handleLabel(src, edge.sourceHandle)})` : ''} → {name(dst)}</p>
      <IssueList issues={issues} />
      {note ? (
        <>
          <p className="ff-hint">A note carries a message, not control: it waits in {name(dst)}'s inbox until the flow reaches them.</p>
          <Field label="What the note is for"><Text value={edge.noteLabel} placeholder="Instructions for reviewers" onChange={v => set(e => { e.noteLabel = v; }, 'nl:' + edge.id)} /></Field>
        </>
      ) : (
        <>
          <Field label="Round limit" hint="Set it on the connection that goes back round a loop. When it runs out, the Manager takes over. Empty means no limit.">
            <Num value={edge.maxRounds} min={0} max={100} placeholder="No limit" onChange={v => set(e => { e.maxRounds = v; }, 'mr:' + edge.id)} />
          </Field>
          <Toggle checked={edge.requireNote} label="Require a note on this route" onChange={v => set(e => { e.requireNote = v; })} />
          {edge.requireNote && <Field label="What the note says"><Text value={edge.noteLabel} placeholder="What to change" onChange={v => set(e => { e.noteLabel = v; }, 'nl:' + edge.id)} /></Field>}
        </>
      )}
    </div>
  );
}

export function FactorySettings({ f, mutate, skills }) {
  const s = f.settings;
  const set = (fn, key) => mutate(d => fn(d), key);
  return (
    <div className="ff-pane">
      <div className="ff-pane-title"><span className="ff-kind">Factory</span></div>
      <Field label="Name"><Text value={f.name} onChange={v => set(d => { d.name = v; }, 'name')} /></Field>
      <Field label="Description" hint="What this factory is for. /skill-name works here too.">
        <SkillText skills={skills} rows={3} value={f.description} onChange={v => set(d => { d.description = v; }, 'desc')} />
      </Field>
      <Field label="Manager's permissions" hint={s.managerMode === 'bypass' ? 'The Manager runs unchecked and approves its team\'s requests itself. Choose this only if you trust it. Some things stay refused: sudo, restarting Crundi, pushing, the server\'s ports.' : 'The Manager\'s own actions ask you. It still decides its team\'s permission requests.'}>
        <Select value={s.managerMode} onChange={v => set(d => { d.settings.managerMode = v; })} options={[{ value: 'normal', label: 'Normal: ask me for the Manager\'s own actions' }, { value: 'bypass', label: 'Bypass: I trust the Manager' }]} />
      </Field>
      <Field label="When the work is done" hint="Either way it is a single squash commit, and nothing is ever pushed.">
        <Select value={s.mergeMode} onChange={v => set(d => { d.settings.mergeMode = v; })} options={[{ value: 'ask', label: 'Show me first, I decide' }, { value: 'auto', label: 'Merge automatically (with Undo)' }]} />
      </Field>
      <Toggle checked={s.autoRecover} label="Recover by itself after a crash" onChange={v => set(d => { d.settings.autoRecover = v; })} />
      <Field label="Setup command" hint="Runs in each new worktree before the team starts, e.g. npm ci.">
        <Text mono value={s.setupCommand} placeholder="npm ci" onChange={v => set(d => { d.settings.setupCommand = v; }, 'setup')} />
      </Field>
      <div className="ff-grid2">
        <Field label="Max handoffs"><Num value={s.caps.maxHandoffs} min={1} max={10000} onChange={v => set(d => { d.settings.caps.maxHandoffs = v || 1; }, 'caps1')} /></Field>
        <Field label="Max minutes"><Num value={s.caps.maxMinutes} min={1} max={10080} onChange={v => set(d => { d.settings.caps.maxMinutes = v || 1; }, 'caps2')} /></Field>
      </div>
      <p className="ff-hint">When a limit is reached the run waits for you; you can let it carry on.</p>
    </div>
  );
}

export function IssuesPane({ validation, onPick }) {
  const all = [...validation.errors.map(x => ({ ...x, level: 'error' })), ...validation.warnings.map(x => ({ ...x, level: 'warning' }))];
  return (
    <div className="ff-pane">
      <div className="ff-pane-title"><span className="ff-kind">{validation.ok ? 'Ready to run' : `${validation.errors.length} to fix`}</span></div>
      {validation.ok && !all.length && <p className="ff-ok"><Icon name="check" size={14} /> Every path can finish, every loop has a limit, every answer is wired.</p>}
      {validation.ok && all.length > 0 && <p className="ff-hint">It can run. The warnings below are worth a look.</p>}
      {!validation.ok && <p className="ff-hint">It is saved as a draft and cannot run until these are fixed. Tap one to go to it.</p>}
      <IssueList issues={all} onPick={onPick} />
    </div>
  );
}
