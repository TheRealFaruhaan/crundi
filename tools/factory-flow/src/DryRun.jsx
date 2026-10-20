import { waitText } from './nodes.jsx';
import { useState } from 'react';
import * as E from '../../../src/factory-engine.js';
import { Icon } from './icons.jsx';

let fakeTree = 0;

/**
 * Walk the flow without Claude: the real engine, with you answering for
 * whoever has the work. Shows what each step would be asked, rejects a
 * handoff the way an employee's would be rejected, counts loop rounds on the
 * canvas, and logs what would happen (who starts, workspaces, merges).
 */
export function DryRun({ f, run, setRun, valid }) {
  const [answers, setAnswers] = useState({});
  const [notes, setNotes] = useState({});
  const [error, setError] = useState('');
  const [log, setLog] = useState([]);
  const [noChange, setNoChange] = useState(false);
  const roster = new Map(f.roster.map(r => [r.id, r.name]));
  const nodes = new Map(f.nodes.map(n => [n.id, n]));

  const describe = (effects) => effects.map(e => {
    if (e.type === 'activate') return e.mode === 'answer' ? `${roster.get(e.employeeId)} is asked to decide` : `${roster.get(e.employeeId)} gets the work`;
    if (e.type === 'workspace') return `a separate worktree is made for "${e.name}"`;
    if (e.type === 'merge') return e.abort ? 'the branches are stopped and their work merged back' : 'the branches are merged back together';
    if (e.type === 'cancel') return 'the other branches are stopped';
    if (e.type === 'ask') return 'you are asked: ' + e.prompt;
    if (e.type === 'wait') return 'the flow waits';
    if (e.type === 'finish') return 'the Manager finishes and presents the result';
    if (e.type === 'cap') return `the run reaches its limit of ${e.limit} handoffs and waits for you`;
    return e.type;
  });
  const apply = (res, what) => {
    if (!res.ok) { setError(res.error); return; }
    setError('');
    setRun(res.run);
    setAnswers({}); setNotes({});
    setLog(l => [...l, what, ...describe(res.effects).map(x => '  → ' + x)].slice(-80));
  };

  if (!run) {
    return (
      <div className="ff-pane">
        <div className="ff-pane-title"><span className="ff-kind">Dry run</span></div>
        <p className="ff-hint">Walk through the flow by hand, answering for each person, before spending anything on Claude. The canvas shows where the work is and how many rounds each loop has used.</p>
        {!valid && <p className="ff-issue error"><Icon name="alert" size={13} /> Fix the problems first: a factory that cannot run cannot be walked either.</p>}
        <button className="ff-btn primary" disabled={!valid} onClick={() => {
          const r = E.createRun({ id: 'dry', project: 'dry', factory: f, task: 'A dry run' });
          const s = E.start(r);
          setLog([]);
          apply(s, 'Started');
        }}><Icon name="play" size={14} />Start a dry run</button>
      </div>
    );
  }

  const active = run.tokens.filter(t => ['work', 'answer', 'ask', 'wait'].includes(t.status));
  const finished = run.status === 'finishing';

  return (
    <div className="ff-pane">
      <div className="ff-pane-title"><span className="ff-kind">Dry run</span>
        <button className="ff-btn ghost" onClick={() => { setRun(null); setLog([]); setError(''); }}><Icon name="x" size={14} />End</button>
      </div>
      <p className="ff-hint">Handoffs: {run.handoffs}{run.status === 'waiting' ? ' · waiting for you' : ''}</p>
      {finished && <p className="ff-ok"><Icon name="check" size={14} /> Finished: the Manager would now present the result.</p>}
      {active.map(t => <div key={t.id}>{Actor({ t })}</div>)}
      {error && <div className="ff-issue error"><Icon name="alert" size={13} /><div>{error}</div></div>}
      {log.length > 0 && <pre className="ff-log">{log.join('\n')}</pre>}
    </div>
  );

  function Actor({ t }) {
    const node = nodes.get(t.nodeId);
    if (t.status === 'wait') {
      const two = node.wait.kind === 'command' || node.wait.kind === 'signal';
      return (
        <div key={t.id} className="ff-actor">
          <div className="ff-actor-head"><Icon name="clock" size={14} />{node.label || 'Wait'}: {waitText(node.wait)}</div>
          <div className="ff-btnrow">
            <button className="ff-btn" onClick={() => apply(E.waitDone(run, t.id, 'done', 'in the dry run'), two ? 'The wait was done' : 'The time passed')}>{two ? 'Done' : 'Time passes'}</button>
            {two && <button className="ff-btn" onClick={() => apply(E.waitDone(run, t.id, 'timeout', 'in the dry run'), 'The wait timed out')}>Timed out</button>}
          </div>
        </div>
      );
    }
    if (t.status === 'ask') {
      return (
        <div className="ff-actor">
          <div className="ff-actor-head"><Icon name="ask" size={14} />You are asked: {node.ask.prompt}</div>
          <textarea className="ff-input" rows={2} placeholder="Optional note" value={notes.__ask || ''} onChange={e => setNotes({ ...notes, __ask: e.target.value })} />
          <div className="ff-btnrow">{node.ask.replies.map(r => <button key={r.id} className="ff-btn" onClick={() => apply(E.askReply(run, t.id, r.id, notes.__ask || ''), `You replied "${r.label}"`)}>{r.label}</button>)}</div>
        </div>
      );
    }
    const req = E.requirements(run, t.id);
    const who = roster.get(req.who) || req.who;
    const send = (extra = {}) => {
      fakeTree++;
      const payload = { summary: 'Dry run step', answers, notes, ...extra };
      apply(E.handoff(run, t.id, payload, { treeHash: noChange ? 'same' : 'tree' + fakeTree }), `${who} handed off${extra.finish ? ' (finish)' : extra.escalate ? ' (escalated)' : ''}`);
    };
    return (
      <div className="ff-actor">
        <div className="ff-actor-head"><Icon name={req.who === 'manager' ? 'crown' : 'user'} size={14} />{who} {t.status === 'answer' ? 'is asked to decide' : 'has the work'}{node?.label && node.label !== who ? ' · ' + node.label : ''}</div>
        {req.route && Route({ item: req.route })}
        {!req.route && <p className="ff-hint">No decisions here.</p>}
        {req.noteTargets.map(n => (
          <input key={n.key} className="ff-input" placeholder={`Optional note for ${n.to}${n.label ? ' (' + n.label + ')' : ''}`} value={notes[n.key] || ''} onChange={e => setNotes({ ...notes, [n.key]: e.target.value })} />
        ))}
        <div className="ff-btnrow">
          <button className="ff-btn primary" onClick={() => send()}>Hand off</button>
          {req.canFinish && <button className="ff-btn" onClick={() => send({ finish: true })}>Finish</button>}
          {req.mode === 'work' && req.who !== 'manager' && <button className="ff-btn ghost" onClick={() => send({ escalate: 'Stuck in the dry run' })}>Escalate</button>}
        </div>
        {req.who !== 'manager' && t.status === 'work' && <label className="ff-toggle small"><input type="checkbox" checked={noChange} onChange={e => setNoChange(e.target.checked)} /><span>Pretend nothing changed in the code (tests the no-progress check)</span></label>}
      </div>
    );
  }

  function Route({ item }) {
    if (!item) return null;
    // A route can need a note AND lead on to another question.
    const note = item.requireNote ? NoteBox({ item }) : null;
    if (!item.decision) {
      const to = item.to || {};
      return (
        <>
          {note}
          <p className="ff-hint">Then: {to.name || to.answeredBy ? (to.type === 'decision' ? `${to.answeredBy} is asked "${to.label}"` : to.name) : to.label || to.type}{item.maxRounds ? ` (round ${Math.min(item.used + 1, item.maxRounds)} of ${item.maxRounds})` : ''}</p>
        </>
      );
    }
    const d = item.decision;
    const cur = answers[d.id];
    const handles = d.kind === 'yesno' ? ['yes', 'no'] : d.options.map(o => o.id);
    const label = (h) => d.kind === 'yesno' ? (h === 'yes' ? 'Yes' : 'No') : d.options.find(o => o.id === h)?.label;
    const picked = d.kind === 'many' ? (Array.isArray(cur) ? cur : (cur === undefined && d.lastPicks ? d.lastPicks : [])) : cur;
    const toggle = (h) => {
      if (d.kind !== 'many') return setAnswers({ ...answers, [d.id]: h });
      const s = new Set(picked); s.has(h) ? s.delete(h) : s.add(h);
      setAnswers({ ...answers, [d.id]: [...s] });
    };
    const chosen = d.kind === 'many' ? (picked.length ? picked : ['none']) : picked ? [picked] : [];
    return (
      <div className="ff-q">
        {note}
        <div className="ff-q-text">{d.question || d.label}</div>
        <div className="ff-btnrow">
          {handles.map(h => <button key={h} className={'ff-chipbtn' + ((d.kind === 'many' ? picked.includes(h) : cur === h) ? ' on' : '')} onClick={() => toggle(h)}>{label(h)}</button>)}
          {d.kind === 'many' && <button className={'ff-chipbtn' + (Array.isArray(cur) && !cur.length ? ' on' : '')} onClick={() => setAnswers({ ...answers, [d.id]: [] })}>None</button>}
        </div>
        {d.kind === 'many' && d.lastPicks && cur === undefined && <p className="ff-hint">Last time's picks are used unless you change them.</p>}
        {chosen.map(h => {
          const b = d.branches.find(x => x.answer === h);
          return b && b.next ? <div key={h} className="ff-q-next">{Route({ item: b.next })}</div> : null;
        })}
      </div>
    );
  }

  function NoteBox({ item }) {
    return <input className="ff-input" placeholder={`Required: ${item.noteLabel || 'a note'}`} value={notes[item.edgeId] || ''} onChange={e => setNotes({ ...notes, [item.edgeId]: e.target.value })} />;
  }
}
