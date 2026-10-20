import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { ReactFlow, ReactFlowProvider, Background, Controls, MiniMap, MarkerType } from '@xyflow/react';
import { reduce, asksOf } from '../../../src/factory-journal.js';
import { rosterOf } from '../../../src/factory-engine.js';
import { MANAGER_ID } from '../../../src/factory-model.js';
import { toFlow } from './convert.js';
import { nodeTypes, edgeTypes } from './nodes.jsx';
import { Icon } from './icons.jsx';

const NO_ISSUES = { ok: true, errors: [], warnings: [] };
const POLL_MS = 4000;

function time(at) {
  const d = new Date(at);
  return isNaN(d) ? '' : d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
const first = (s, n = 160) => { const t = String(s || '').split('\n')[0]; return t.length > n ? t.slice(0, n - 1) + '…' : t; };

/** One journal event as a line of the timeline (null: not worth showing). */
function describe(ev, names, nodes) {
  const who = (id) => names.get(id) || id || '';
  switch (ev.type) {
    case 'start': return { k: 'start', main: 'Started', sub: ev.task };
    case 'setup': return { k: ev.status === 'ok' ? 'info' : 'escalate', main: `Setup command ${ev.status === 'ok' ? 'ran' : 'failed'}`, sub: ev.command };
    case 'begin': return { k: 'info', main: 'The Manager got the task' };
    case 'handoff': {
      const p = ev.payload || {};
      if (p.finish) return { k: 'finish', main: `${who(ev.by)} finished`, sub: first(p.summary) };
      if (p.escalate) return { k: 'escalate', main: `${who(ev.by)} escalated`, sub: first(p.escalate) };
      return { k: 'handoff', main: `${who(ev.by)} handed off`, sub: first(p.summary) };
    }
    case 'escalate': return { k: 'escalate', main: 'Sent to the Manager', sub: first(ev.reason) };
    case 'ask-reply': return { k: 'info', main: 'You replied', sub: ev.note || '' };
    case 'perm-decided': return ev.by === 'manager' || ev.by === 'deny-list' ? { k: ev.decision === 'allowed' ? 'info' : 'escalate', main: `${ev.employee}: ${ev.toolName} ${ev.decision}${ev.by === 'deny-list' ? ' (never allowed)' : ' by the Manager'}`, sub: first(ev.input?.command || ev.input?.url || '') } : null;
    case 'rule': return { k: 'info', main: 'Rule granted', sub: ev.rule };
    case 'retire': return { k: 'info', main: `${who(ev.employeeId)} stopped for now`, sub: 'Not needed by the current work; resumed with their memory if needed again.' };
    case 'pause': return { k: 'escalate', main: ev.by === 'usage' ? 'Paused: usage limit' : 'Paused' };
    case 'resume': return { k: 'info', main: 'Resumed' };
    case 'cap': return { k: 'escalate', main: `Limit reached (${ev.which === 'maxMinutes' ? ev.limit + ' minutes' : ev.limit + ' handoffs'})` };
    case 'crash': return { k: 'crash', main: 'Crashed', sub: ev.reason };
    case 'recover': return { k: 'info', main: 'Recovered', sub: ev.note || '' };
    case 'reopen': return { k: 'escalate', main: 'Back to work', sub: first(ev.reason) };
    case 'merged': return { k: 'merged', main: 'Merged', sub: (ev.commit || '').slice(0, 7) };
    case 'kept': return { k: 'info', main: 'Kept on its branch' };
    case 'discarded': return { k: 'info', main: 'Discarded' };
    case 'undone': return { k: 'info', main: 'Merge undone' };
    case 'stop': return { k: 'crash', main: 'Stopped' };
    case 'effect-done': {
      if (ev.result?.conflicts?.length) return { k: 'escalate', main: 'Merging the branches conflicted', sub: ev.result.conflicts.map(c => c.files.join(', ')).join('; ') };
      if (ev.result?.merged?.length) return { k: 'info', main: 'Branches merged back together' };
      return null;
    }
    default: return null;
  }
}

function Inner({ apiFetch, runId, subscribe, openChat, onClose }) {
  const [events, setEvents] = useState([]);
  const [run, setRun] = useState(null);         // the server's summary (live chat ids, status)
  const [at, setAt] = useState(null);           // null = live; else event count shown
  const [sel, setSel] = useState(null);
  const [tab, setTab] = useState('run');
  const [busy, setBusy] = useState('');
  const [note, setNote] = useState('');
  const [err, setErr] = useState('');
  const [sizes, setSizes] = useState({});   // measured node sizes (minimap, fitView)
  const evRef = useRef([]);
  const recs = useRef([]);   // recs[i] = the run after i events

  const fetchAll = useCallback(async () => {
    try {
      const from = evRef.current.length;
      const [e, s] = await Promise.all([
        apiFetch(`/api/factory-runs/events?id=${encodeURIComponent(runId)}&from=${from}`).then(r => r.json()),
        apiFetch(`/api/factory-runs/get?id=${encodeURIComponent(runId)}`).then(r => r.json()),
      ]);
      if (s.ok) setRun(s.run);
      if (e.ok && e.events.length && e.from === evRef.current.length) {
        const all = [...evRef.current, ...e.events];
        for (let i = recs.current.length; i <= all.length; i++) recs.current[i] = i === 0 ? null : reduce(recs.current[i - 1], all[i - 1]);
        evRef.current = all;
        setEvents(all);
      } else if (e.ok && e.total < evRef.current.length) {
        evRef.current = []; recs.current = []; fetchAll();
      }
    } catch { /* next update tries again */ }
  }, [apiFetch, runId]);

  useEffect(() => {
    fetchAll();
    let off = null;
    let poll = null;
    if (subscribe) off = subscribe((d) => { if (!d || d.id === runId) fetchAll(); });
    else poll = setInterval(fetchAll, POLL_MS);
    return () => { if (off) off(); clearInterval(poll); };
  }, [fetchAll, subscribe, runId]);

  const n = at == null ? events.length : at;
  const rec = recs.current[n] || null;
  const prev = n > 0 ? recs.current[n - 1] : null;
  const f = rec?.state?.snapshot;
  const names = useMemo(() => new Map((f?.roster || []).map(r => [r.id, r.name])), [f]);
  const nodesById = useMemo(() => new Map((f?.nodes || []).map(x => [x.id, x])), [f]);

  const flow = useMemo(() => {
    if (!rec) return { nodes: [], edges: [] };
    const out = toFlow(f, { validation: NO_ISSUES, selection: sel ? { kind: 'node', id: sel } : null, run: rec.state, readOnly: true });
    const visited = new Set(rec.state.history.filter(h => h.type === 'handoff').map(h => h.nodeId));
    const skillsAt = new Map();
    for (const h of rec.state.history) if (h.type === 'handoff' && h.skills) skillsAt.set(h.nodeId, [...new Set([...(skillsAt.get(h.nodeId) || []), ...h.skills])]);
    for (const node of out.nodes) {
      const raw = node.data.node;
      node.data.visited = visited.has(node.id);
      node.data.skills = skillsAt.get(node.id) || [];
      const who = rosterOf(raw);
      node.data.retired = !!(who && rec.run.retired[who]);
      const last = [...rec.state.history].reverse().find(h => h.type === 'handoff' && h.nodeId === node.id);
      if (last && last.summary) node.data.live = first(last.summary, 110);
    }
    // The connection(s) just taken flow for a moment. Finishing is not a
    // traversal in the engine, so the Finish connection lights up from the outcome.
    const before = prev?.state?.edgeCounts || {};
    const finished = !!rec.state.outcome;
    for (const e of out.edges) {
      const fin = e.data.edge.sourceHandle === 'finish';
      e.data.pulse = fin ? finished && !prev?.state?.outcome : (rec.state.edgeCounts[e.id] || 0) > (before[e.id] || 0);
      if (fin && finished) e.data.used = 1;
    }
    for (const node of out.nodes) if (sizes[node.id]) node.measured = sizes[node.id];
    return out;
  }, [rec, prev, f, sel, sizes]);
  const onNodesChange = useCallback((changes) => {
    const dims = changes.filter(c => c.type === 'dimensions' && c.dimensions);
    if (dims.length) setSizes(p => { const next = { ...p }; for (const c of dims) next[c.id] = { width: c.dimensions.width, height: c.dimensions.height }; return next; });
  }, []);

  const timeline = useMemo(() => events.map((ev, i) => ({ i: i + 1, at: ev.at, ...(describe(ev, names, nodesById) || {}) })).filter(x => x.main), [events, names, nodesById]);

  const act = async (body, label) => {
    setBusy(label); setErr('');
    try {
      const j = await apiFetch('/api/factory-runs', { method: 'POST', body: JSON.stringify({ id: runId, ...body }) }).then(r => r.json());
      if (!j.ok) setErr(j.error || 'That did not work.');
      else { setNote(''); fetchAll(); }
    } catch (e) { setErr(String(e.message || e)); }
    setBusy('');
  };

  if (!rec) return <div className="ff-loading">Loading the run…</div>;
  const live = at == null;
  const st = run && live ? run.status : rec.state.status;
  const stateLabel = { running: 'Running', waiting: 'Waiting on you', paused: 'Paused', crashed: 'Crashed', finishing: 'Done: your decision', finished: 'Finished', stopped: 'Stopped', starting: 'Starting' }[st] || st;
  const asks = live ? (run?.asks || []) : asksOf(rec);

  // ─── Side panel ───
  const nodeInfo = () => {
    const node = nodesById.get(sel);
    if (!node) return null;
    const who = rosterOf(node) || (node.type === 'decision' && node.decision.answeredBy) || '';
    const person = f.roster.find(r => r.id === who);
    const chatId = live && run ? (who === MANAGER_ID ? run.managerChatId : run.employees?.[who]?.chatId) : '';
    const steps = rec.state.history.filter(h => h.type === 'handoff' && h.nodeId === node.id);
    const tokens = rec.state.tokens.filter(t => t.nodeId === node.id);
    return (
      <div className="ff-pane">
        <div className="ff-pane-title">
          <button className="ff-iconbtn" title="Back" onClick={() => setSel(null)}><Icon name="back" size={15} /></button>
          <span className="ff-kind">{node.label || person?.name || node.type}</span>
        </div>
        <div className="ff-kv">
          {person && <><span>Who</span><span>{person.name}{person.model ? ' · ' + person.model : ''}</span></>}
          <span>Now</span><span>{tokens.length ? { work: 'working on it', answer: 'deciding', ask: 'waiting for your reply', join: 'waiting for the other branches', finished: 'finished' }[tokens[0].status] : steps.length ? 'done' : 'not reached yet'}{who && rec.run.retired[who] ? ' · session stopped for now' : ''}</span>
          {steps.length > 0 && <><span>Visits</span><span>{steps.length}</span></>}
        </div>
        {chatId && openChat && <button className="ff-btn primary" onClick={() => openChat(chatId, person?.name || 'Chat')}><Icon name="ask" size={14} />Open {who === MANAGER_ID ? 'the Manager\'s' : 'their'} chat</button>}
        {!chatId && person && live && <p className="ff-hint">{who && rec.run.retired[who] ? 'Their session is stopped for now; it comes back with its memory when the work needs them.' : 'No live session right now.'}</p>}
        {steps.slice().reverse().map((h, i) => (
          <div key={i} className="ff-step">
            <div className="ff-step-head"><span>{names.get(h.by) || h.by}{h.finish ? ' · finished' : ''}</span><span>#{steps.length - i}</span></div>
            <div>{h.summary}</div>
            {h.answers && Object.keys(h.answers).length > 0 && <div className="ff-hint">Answers: {Object.entries(h.answers).map(([k, v]) => `${nodesById.get(k)?.label || k}: ${Array.isArray(v) ? (v.length ? v.join(', ') : 'none') : v}`).join(' · ')}</div>}
            {h.skills && h.skills.length > 0 && <div className="ff-node-skills">{h.skills.map(k => <span key={k}>/{k}</span>)}</div>}
          </div>
        ))}
      </div>
    );
  };

  const runInfo = () => (
    <div className="ff-pane">
      <div className="ff-kv">
        <span>Task</span><span>{rec.state.task}</span>
        <span>Handoffs</span><span>{rec.state.handoffs}</span>
        {run?.branch && <><span>Branch</span><span>{run.branch}</span></>}
        {run && live && run.liveSessions != null && <><span>Sessions</span><span>{run.liveSessions} live</span></>}
      </div>
      {live && run?.waitingOn === 'manager' && <p className="ff-hint">The Manager is talking to you in its chat.</p>}
      {live && run?.managerChatId && openChat && <button className="ff-btn" onClick={() => openChat(run.managerChatId, 'Manager')}><Icon name="crown" size={14} />Open the Manager's chat</button>}
      {asks.map(a => (
        <div key={a.tokenId} className="ff-actor">
          <div className="ff-actor-head"><Icon name="ask" size={14} />{a.prompt}</div>
          {live && <>
            <textarea className="ff-input" rows={2} placeholder="Optional note" value={note} onChange={e => setNote(e.target.value)} />
            <div className="ff-btnrow">{a.replies.map(r => <button key={r.id} className="ff-btn" disabled={!!busy} onClick={() => act({ action: 'ask', tokenId: a.tokenId, replyId: r.id, note }, 'ask')}>{r.label}</button>)}</div>
          </>}
        </div>
      ))}
      {live && run?.status === 'crashed' && (
        <div className="ff-actor">
          <div className="ff-actor-head"><Icon name="alert" size={14} />It stopped: {run.crash?.reason}</div>
          <textarea className="ff-input" rows={2} placeholder="Anything the team should know (optional)" value={note} onChange={e => setNote(e.target.value)} />
          <div className="ff-btnrow"><button className="ff-btn primary" disabled={!!busy} onClick={() => act({ action: 'recover', note }, 'recover')}>Recover</button></div>
        </div>
      )}
      {live && run?.capHit && <div className="ff-actor"><div className="ff-actor-head"><Icon name="alert" size={14} />It reached its limit ({run.capHit.which === 'maxMinutes' ? run.capHit.limit + ' minutes' : run.capHit.limit + ' handoffs'}).</div><div className="ff-btnrow"><button className="ff-btn primary" disabled={!!busy} onClick={() => act({ action: 'resume' }, 'resume')}>Let it carry on</button></div></div>}
      {err && <div className="ff-issue error"><Icon name="alert" size={13} /><div>{err}</div></div>}
      <p className="ff-hint">Tap a step to see what happened there and open that person's chat.</p>
    </div>
  );

  const timelinePane = () => (
    <div className="ff-pane">
      <div className="ff-timeline">
        {timeline.slice().reverse().map(t => (
          <div key={t.i} className={'ff-tl-item k-' + t.k + (t.i === n ? ' on' : '') + (t.i > n ? ' future' : '')} onClick={() => setAt(t.i === events.length ? null : t.i)}>
            <span className="ff-tl-time">{time(t.at)}</span>
            <div><div className="ff-tl-main">{t.main}</div>{t.sub && <div className="ff-tl-sub">{t.sub}</div>}</div>
          </div>
        ))}
      </div>
    </div>
  );

  const controls = live && run ? [
    ['running', 'waiting'].includes(run.status) && !run.paused && ['pause', 'Pause'],
    run.paused && ['resume', 'Resume'],
    ['running', 'waiting', 'paused', 'crashed', 'finishing'].includes(run.status) && ['stop', 'Stop'],
  ].filter(Boolean) : [];

  return (
    <div className="ff-editor ff-viewer">
      <div className="ff-top">
        {onClose && <button className="ff-iconbtn" title="Back" onClick={onClose}><Icon name="back" size={16} /></button>}
        <span className="ff-name">{f.name}</span>
        <span className={'ff-runstate ' + st}>{live ? stateLabel : 'Replay'}</span>
        <div className="ff-top-sp" />
        {controls.map(([a, label]) => (
          <button key={a} className={'ff-btn' + (a === 'stop' ? ' ghost' : '')} disabled={!!busy}
            onClick={() => { if (a === 'stop' && !window.confirm('Stop this factory? Its team stops; the worktree is kept.')) return; act({ action: a }, a); }}>{label}</button>
        ))}
      </div>
      <div className="ff-body">
        <div className="ff-canvas">
          <ReactFlow nodes={flow.nodes} edges={flow.edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} onNodesChange={onNodesChange}
            nodesDraggable={false} nodesConnectable={false} elementsSelectable
            onNodeClick={(_, x) => { setSel(x.id); setTab('run'); }} onPaneClick={() => setSel(null)}
            fitView fitViewOptions={{ padding: 0.15, maxZoom: 1 }} minZoom={0.15} maxZoom={2} colorMode="dark" deleteKeyCode={null}
            defaultEdgeOptions={{ markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 } }}>
            <Background gap={16} size={1} />
            <Controls showInteractive={false} />
            <MiniMap pannable zoomable nodeColor={(x) => x.data?.tokens?.length ? '#10b981' : x.data?.visited ? '#2f6f5a' : '#4b4b70'} nodeStrokeWidth={0} maskColor="rgba(10,10,15,0.65)" />
          </ReactFlow>
        </div>
        <div className="ff-right">
          <div className="ff-tabs">
            <button className={'ff-tab' + (tab === 'run' ? ' on' : '')} onClick={() => setTab('run')}><Icon name="sliders" size={14} /><span>{sel ? 'Step' : 'Run'}</span></button>
            <button className={'ff-tab' + (tab === 'timeline' ? ' on' : '')} onClick={() => setTab('timeline')}><Icon name="route" size={14} /><span>Timeline</span></button>
          </div>
          <div className="ff-panel">{tab === 'timeline' ? timelinePane() : sel ? nodeInfo() : runInfo()}</div>
        </div>
      </div>
      {events.length > 1 && (
        <div className="ff-replay">
          <button className="ff-iconbtn" title="Step back" disabled={n <= 1} onClick={() => setAt(Math.max(1, n - 1))}><Icon name="undo" size={15} /></button>
          <input type="range" min={1} max={events.length} value={n} onChange={e => { const v = +e.target.value; setAt(v >= events.length ? null : v); }} />
          <button className="ff-iconbtn" title="Step forward" disabled={live} onClick={() => setAt(n + 1 >= events.length ? null : n + 1)}><Icon name="redo" size={15} /></button>
          <span className="ff-replay-label">{live ? 'Live' : `${time(events[n - 1]?.at)} · ${n}/${events.length}`}</span>
          {!live && <button className="ff-btn" onClick={() => setAt(null)}>Back to live</button>}
        </div>
      )}
    </div>
  );
}

export function Viewer(props) {
  return <ReactFlowProvider><Inner {...props} /></ReactFlowProvider>;
}
