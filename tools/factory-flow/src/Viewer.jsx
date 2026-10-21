import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { ReactFlow, ReactFlowProvider, Background, Controls, MiniMap, MarkerType, useReactFlow } from '@xyflow/react';
import { reduce, asksOf } from '../../../src/factory-journal.js';
import { rosterOf } from '../../../src/factory-engine.js';
import { MANAGER_ID } from '../../../src/factory-model.js';
import { toFlow } from './convert.js';
import { nodeTypes, edgeTypes } from './nodes.jsx';
import { Icon } from './icons.jsx';

const NO_ISSUES = { ok: true, errors: [], warnings: [] };
const POLL_MS = 4000;
const ACTIVITY_MS = 3000;

function dur(ms) {
  const m = Math.max(0, Math.round(ms / 60000));
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`;
}
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
    case 'wait-done': return { k: ev.outcome === 'timeout' ? 'escalate' : 'info', main: `${nodes.get(ev.nodeId)?.label || 'Wait'}: ${ev.outcome === 'timeout' ? 'gave up waiting' : 'over'}`, sub: first(ev.detail) };
    case 'perm-decided': return ev.by === 'manager' || ev.by === 'deny-list' || ev.by === 'gatekeeper' ? { k: ev.decision === 'allowed' ? 'info' : 'escalate', main: `${ev.employee}: ${ev.toolName} ${ev.decision === 'escalated' ? 'passed to the Manager' : ev.decision}${ev.by === 'deny-list' ? ' (never allowed)' : ev.decision === 'escalated' ? ' by the Gatekeeper' : ev.by === 'gatekeeper' ? ' by the Gatekeeper' : ' by the Manager'}`, sub: first(ev.reason || ev.input?.command || ev.input?.url || '') } : null;
    case 'delegate': return { k: 'handoff', main: `The Manager handed ${who(ev.employeeId)} a job`, sub: first(ev.task) };
    case 'team-mode': return { k: 'info', main: ev.mode === 'bypass' ? 'The team now runs with bypass' : 'The team asks again' };
    case 'policy': return { k: 'info', main: 'The Manager set its permission policy', sub: first(ev.text) };
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

/** A Wait in progress: what it waits for, how long is left, end it early. */
function WaitCard({ t, busy, act }) {
  const [, tick] = useState(0);
  useEffect(() => { const i = setInterval(() => tick(x => x + 1), 1000); return () => clearInterval(i); }, []);
  const w = t.wait;
  const left = (iso) => { const ms = Date.parse(iso) - Date.now(); if (!(ms > 0)) return 'any moment'; const m = Math.floor(ms / 60000); return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : m >= 1 ? `${m} min` : `${Math.ceil(ms / 1000)} s`; };
  const url = w.signalPath ? location.origin + w.signalPath : '';
  const two = w.kind === 'command' || w.kind === 'signal';
  return (
    <div className="ff-actor">
      <div className="ff-actor-head"><Icon name="clock" size={14} />{t.label}: {w.kind === 'time' || w.kind === 'until' ? `${left(w.deadline)} left` : w.kind === 'signal' ? (w.what || 'waiting for a signal') : `trying (${w.tries} so far)`}</div>
      {two && <p className="ff-hint">Gives up in {left(w.timeoutAt)}.</p>}
      {w.kind === 'command' && w.last && <pre className="ff-pre">{w.last}</pre>}
      {url && (
        <div className="ff-field"><span className="ff-field-label">Signal URL (POST)</span>
          <div className="ff-btnrow"><input className="ff-input mono" readOnly value={url} onFocus={e => e.target.select()} /><button className="ff-btn" onClick={() => navigator.clipboard?.writeText(`curl -X POST '${url}'`)}><Icon name="copy" size={13} />curl</button></div>
          <p className="ff-hint">Body optional: {'{"note": "…"}'}, or {'{"outcome": "failed"}'} for Timed out.</p>
        </div>
      )}
      <div className="ff-btnrow">
        <button className="ff-btn" disabled={!!busy} onClick={() => act({ action: 'endWait', tokenId: t.id, outcome: 'done' }, 'wait')}>End the wait now</button>
        {two && <button className="ff-btn ghost" disabled={!!busy} onClick={() => act({ action: 'endWait', tokenId: t.id, outcome: 'timeout' }, 'wait')}>Treat as timed out</button>}
      </div>
    </div>
  );
}

function Inner({ apiFetch, runId, subscribe, openChat, onClose, mode = 'full' }) {
  const panel = mode === 'panel';
  const [chat, setChat] = useState(null);      // panel: { chatId, name } open over the graph
  const [drawer, setDrawer] = useState(false); // panel: the run / timeline pane, on demand
  const [follow, setFollow] = useState(mode === 'panel'); // the camera follows where people are working
  const rf = useReactFlow();
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
  // Who is doing what changes all the time without changing the run: the
  // summary is fetched every few seconds while anyone is at work.
  useEffect(() => {
    // New journal events too (only what is new): a missed push must not
    // leave the graph behind the run.
    const t = setInterval(() => { if (!document.hidden) fetchAll(); }, ACTIVITY_MS);
    return () => clearInterval(t);
  }, [fetchAll]);

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
      // Someone here is waiting for a permission answer.
      node.data.blocked = !!(who && node.data.tokens.length && (rec.run.perms?.outstanding || []).some(p => p.employeeId === who));
      const last = [...rec.state.history].reverse().find(h => h.type === 'handoff' && h.nodeId === node.id);
      if (last && last.summary) node.data.live = first(last.summary, 110);
      // Live: who is here now and what they are doing this second.
      if (at == null && run && !['crashed', 'paused', 'stopped', 'finished'].includes(run.status)) {
        const tok = (run.tokens || []).find(t => t.nodeId === node.id && (t.status === 'work' || t.status === 'answer'));
        const emp = tok && run.employees?.[tok.employeeId];
        if (tok) node.data.doing = { who: tok.employee || names.get(tok.employeeId) || '', text: emp?.activity?.text || (tok.status === 'answer' ? 'Deciding' : 'Starting'), state: emp?.activity?.state || '', chatId: emp?.chatId || '' };
      }
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
  }, [rec, prev, f, sel, sizes, run, at, names]);
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

  // Follow the work: zoom to the steps where someone is working, and move
  // with it. Panning or zooming by hand stops following.
  const activeKey = at == null && run ? (run.tokens || []).filter(t => t.nodeId && ['work', 'answer', 'ask', 'wait', 'join'].includes(t.status)).map(t => t.nodeId).sort().join(',') : '';
  useEffect(() => {
    if (!follow || !rec) return;
    const ids = activeKey ? [...new Set(activeKey.split(','))] : [];
    const t = setTimeout(() => {
      try { rf.fitView(ids.length ? { nodes: ids.map(id => ({ id })), padding: 0.45, maxZoom: 0.95, duration: 450 } : { padding: 0.15, maxZoom: 1, duration: 450 }); } catch { /* not measured yet */ }
    }, 120);
    return () => clearTimeout(t);
  }, [follow, activeKey, rf, !!rec, Object.keys(sizes).length]);
  // The embedded chat page's back arrow asks to be closed.
  useEffect(() => {
    if (!panel) return undefined;
    const onMsg = (e) => { if (e.origin === location.origin && e.data && e.data.type === 'crundi-chat-close') setChat(null); };
    window.addEventListener('message', onMsg);
    return () => window.removeEventListener('message', onMsg);
  }, [panel]);
  if (!rec) return <div className="ff-loading">Loading the run…</div>;
  const live = at == null;
  const st = run && live ? run.status : rec.state.status;
  const stateLabel = { running: 'Running', waiting: 'Waiting on you', paused: 'Paused', crashed: 'Crashed', finishing: 'Done: your decision', finished: 'Finished', stopped: 'Stopped', starting: 'Starting' }[st] || st;
  const asks = live ? (run?.asks || []) : asksOf(rec);

  // A person's transcript: inside the panel, or the host's chat window.
  const openPerson = (chatId, name) => {
    if (!chatId) return;
    if (panel) { setChat({ chatId, name }); return; }
    if (openChat) openChat(chatId, name);
  };
  // Everyone at work right now, with what they are doing.
  const working = live && run && !['crashed', 'paused', 'stopped', 'finished'].includes(run.status) ? (run.tokens || []).filter(t => t.status === 'work' || t.status === 'answer').map(t => {
    const e = run.employees?.[t.employeeId] || {};
    return { id: t.id, nodeId: t.nodeId, name: t.employee || names.get(t.employeeId) || 'Someone', what: t.job ? 'Job: ' + first(t.job, 50) : (e.activity?.text || (t.status === 'answer' ? 'Deciding' : 'Starting')), state: e.activity?.state || '', chatId: e.chatId || '', blocked: !!t.blocked };
  }) : [];
  const strip = working.length > 0 && (
    <div className="ff-working">
      {working.map(w => (
        <button key={w.id} className={'ff-worker' + (w.state === 'working' ? ' busy' : '') + (w.blocked ? ' blocked' : '')} disabled={!w.chatId}
          title={w.chatId ? 'Open ' + w.name + "'s transcript" : ''} onClick={() => openPerson(w.chatId, w.name)}>
          <span className="ff-pulse" /><b>{w.name}</b><span>{w.blocked ? 'Needs permission' : w.what}</span>
        </button>
      ))}
    </div>
  );
  // What needs the person, said once, at the top, with the one action that answers it.
  const needs = live && run ? (
    run.status === 'crashed' ? { k: 'bad', text: 'It stopped: ' + (run.crash?.reason || 'it was interrupted') + '. Recover picks it up where it was.', act: ['Recover', () => act({ action: 'recover', note: '' }, 'recover')] }
    : run.status === 'finishing' ? { k: 'you', text: 'Done. The result is in the Manager\'s chat: merge it, ask for changes, keep the branch or discard it.', act: run.managerChatId ? ['Open the result', () => openPerson(run.managerChatId, 'Manager')] : null }
    : run.capHit ? { k: 'you', text: 'It reached its limit (' + (run.capHit.which === 'maxMinutes' ? run.capHit.limit + ' minutes' : run.capHit.limit + ' handoffs') + ').', act: ['Let it carry on', () => act({ action: 'resume' }, 'resume')] }
    : (run.asks || []).length ? { k: 'you', text: 'A question for you: ' + run.asks[0].prompt, act: ['Answer', () => { setSel(null); setTab('run'); setDrawer(true); }] }
    : run.waitingOn === 'manager' ? { k: 'you', text: 'The Manager is talking to you in its chat.', act: run.managerChatId ? ['Open its chat', () => openPerson(run.managerChatId, 'Manager')] : null }
    : run.paused ? { k: 'info', text: run.paused.by === 'usage' ? 'Paused: the usage limit is full; it carries on when it resets.' : 'Paused.', act: ['Resume', () => act({ action: 'resume' }, 'resume')] }
    : null) : null;
  const needsBar = needs && (
    <div className={'ff-needs ' + needs.k}>
      <Icon name="alert" size={14} /><span>{needs.text}</span>
      {needs.act && <button className="ff-btn primary" disabled={!!busy} onClick={needs.act[1]}>{needs.act[0]}</button>}
    </div>
  );
  const progress = run ? [
    `${run.handoffs} handoff${run.handoffs === 1 ? '' : 's'}`,
    run.createdAt ? dur(Date.now() - Date.parse(run.createdAt)) : '',
    run.usage?.turns ? `${run.usage.turns} turns${run.usage.costUsd ? ' · $' + run.usage.costUsd.toFixed(2) : ''}` : '',
    (run.findings || []).length ? `${run.findings.filter(i => i.status === 'open' || i.status === 'claimed').length} open findings` : '',
  ].filter(Boolean).join(' · ') : '';

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
          <span>Now</span><span>{tokens.length ? { work: 'working on it', answer: 'deciding', ask: 'waiting for your reply', join: 'waiting for the other branches', wait: 'waiting', finished: 'finished' }[tokens[0].status] : steps.length ? 'done' : 'not reached yet'}{who && rec.run.retired[who] ? ' · session stopped for now' : ''}</span>
          {steps.length > 0 && <><span>Visits</span><span>{steps.length}</span></>}
        </div>
        {chatId && (openChat || panel) && <button className="ff-btn primary" onClick={() => openPerson(chatId, person?.name || 'Chat')}><Icon name="ask" size={14} />Open {who === MANAGER_ID ? 'the Manager\'s' : 'their'} transcript</button>}
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
        {run?.usage?.turns > 0 && <><span>Spent</span><span>{run.usage.costUsd ? '$' + run.usage.costUsd.toFixed(2) + ' · ' : ''}{run.usage.turns} turns in {run.usage.chats} chats</span></>}
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
      {live && (run?.tokens || []).filter(t => t.wait).map(t => <WaitCard key={t.id} t={t} busy={busy} act={act} />)}
      {(run?.findings || []).length > 0 && (
        <details className="ff-more">
          <summary>Review findings: {run.findings.filter(i => i.status === 'open' || i.status === 'claimed').length} open of {run.findings.length}</summary>
          {run.findings.map(i => (
            <div key={i.id} className="ff-hint"><b>{i.id}</b> {({ open: 'open', claimed: 'said fixed', closed: 'verified', dismissed: 'dismissed' })[i.status]}{i.blocking ? ' · blocking' : ' · nit'}{i.reopened ? ` · reopened ${i.reopened}x` : ''} · {i.by}: {i.text}{i.where ? ` (${i.where})` : ''}</div>
          ))}
        </details>
      )}
      {live && run?.status === 'crashed' && (
        <div className="ff-actor">
          <div className="ff-actor-head"><Icon name="alert" size={14} />It stopped: {run.crash?.reason}</div>
          <textarea className="ff-input" rows={2} placeholder="Anything the team should know (optional)" value={note} onChange={e => setNote(e.target.value)} />
          <div className="ff-btnrow"><button className="ff-btn primary" disabled={!!busy} onClick={() => act({ action: 'recover', note }, 'recover')}>Recover</button></div>
        </div>
      )}
      {live && run?.capHit && <div className="ff-actor"><div className="ff-actor-head"><Icon name="alert" size={14} />It reached its limit ({run.capHit.which === 'maxMinutes' ? run.capHit.limit + ' minutes' : run.capHit.limit + ' handoffs'}).</div><div className="ff-btnrow"><button className="ff-btn primary" disabled={!!busy} onClick={() => act({ action: 'resume' }, 'resume')}>Let it carry on</button></div></div>}
      {err && <div className="ff-issue error"><Icon name="alert" size={13} /><div>{err}</div></div>}
      <p className="ff-hint">{panel ? 'Tap someone at work to read their transcript live; tap a finished step to see what happened there.' : "Tap a step to see what happened there and open that person's transcript."}</p>
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
    <div className={'ff-editor ff-viewer' + (panel ? ' ff-panelmode' : '')}>
      <div className="ff-top">
        {onClose && <button className="ff-iconbtn" title="Back" onClick={onClose}><Icon name="back" size={16} /></button>}
        <span className="ff-name">{f.name}</span>
        <span className={'ff-runstate ' + st}>{live ? stateLabel : 'Replay'}</span>
        {progress && <span className="ff-progress">{progress}</span>}
        <div className="ff-top-sp" />
        {panel && <button className={'ff-btn' + (follow ? ' primary' : '')} title="Keep the view on where people are working" onClick={() => setFollow(v => !v)}><Icon name="route" size={14} />Follow</button>}
        {panel && <button className={'ff-btn' + (drawer ? ' primary' : '')} onClick={() => setDrawer(v => !v)}><Icon name="sliders" size={14} />Details</button>}
        {controls.map(([a, label]) => (
          <button key={a} className={'ff-btn' + (a === 'stop' ? ' ghost' : '')} disabled={!!busy}
            onClick={() => { if (a === 'stop' && !window.confirm('Stop this factory? Its team stops; the worktree is kept.')) return; act({ action: a }, a); }}>{label}</button>
        ))}
      </div>
      {needsBar}
      {strip}
      <div className="ff-body">
        <div className="ff-canvas">
          <ReactFlow nodes={flow.nodes} edges={flow.edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} onNodesChange={onNodesChange}
            nodesDraggable={false} nodesConnectable={false} elementsSelectable
            onNodeClick={(_, x) => {
              const d = x.data?.doing;
              const who = rosterOf(x.data?.node);
              const chatId = d?.chatId || (live && run && who ? (who === MANAGER_ID ? run.managerChatId : run.employees?.[who]?.chatId) : '');
              if (panel && chatId) { openPerson(chatId, d?.who || names.get(who) || 'Chat'); return; }
              setSel(x.id); setTab('run'); if (panel) setDrawer(true);
            }} onPaneClick={() => setSel(null)}
            fitView fitViewOptions={{ padding: 0.15, maxZoom: 1 }} minZoom={0.15} maxZoom={2} colorMode="dark" deleteKeyCode={null}
            onMoveStart={(e) => { if (e && follow) setFollow(false); }}
            defaultEdgeOptions={{ markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 } }}>
            <Background gap={16} size={1} />
            <Controls showInteractive={false} />
            {!panel && <MiniMap pannable zoomable nodeColor={(x) => x.data?.tokens?.length ? '#10b981' : x.data?.visited ? '#2f6f5a' : '#4b4b70'} nodeStrokeWidth={0} maskColor="rgba(10,10,15,0.65)" />}
          </ReactFlow>
        </div>
        {panel && chat && (
          <div className="ff-chatover">
            {/* The chat page brings its own header; its back arrow comes back here. */}
            <iframe key={chat.chatId} src={'/chat/' + encodeURIComponent(chat.chatId) + '?in=graph'} title={chat.name} />
          </div>
        )}
        <div className={'ff-right' + (panel ? ' ff-drawer' + (drawer ? ' open' : '') : '')}>
          <div className="ff-tabs">
            <button className={'ff-tab' + (tab === 'run' ? ' on' : '')} onClick={() => setTab('run')}><Icon name="sliders" size={14} /><span>{sel ? 'Step' : 'Run'}</span></button>
            <button className={'ff-tab' + (tab === 'timeline' ? ' on' : '')} onClick={() => setTab('timeline')}><Icon name="route" size={14} /><span>Timeline</span></button>
          </div>
          <div className="ff-panel">{tab === 'timeline' ? timelinePane() : sel ? nodeInfo() : runInfo()}</div>
        </div>
      </div>
      {events.length > 1 && !panel && (
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
