import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { ReactFlow, ReactFlowProvider, Background, Controls, MiniMap, MarkerType, useReactFlow } from '@xyflow/react';
import dagre from '@dagrejs/dagre';
import { validateFactory, normalizeFactory, genId, MANAGER_ID, flowGraph, reachable, layoutEdges } from '../../../src/factory-model.js';
import { toFlow, AGENTS } from './convert.js';
import { nodeTypes, edgeTypes } from './nodes.jsx';
import { Icon } from './icons.jsx';
import { NodeInspector, EdgeInspector, FactorySettings, IssuesPane } from './Inspector.jsx';
import { RosterPane } from './Roster.jsx';
import { Palette, DRAG_TYPE } from './Palette.jsx';
import { DryRun } from './DryRun.jsx';

const clone = (o) => JSON.parse(JSON.stringify(o));
const HISTORY_MAX = 100;
const SAVE_DELAY = 1200;

function usePhone() {
  const q = '(max-width: 760px)';
  const [phone, setPhone] = useState(() => typeof matchMedia === 'function' && matchMedia(q).matches);
  useEffect(() => {
    if (typeof matchMedia !== 'function') return;
    const m = matchMedia(q);
    const on = () => setPhone(m.matches);
    m.addEventListener('change', on);
    return () => m.removeEventListener('change', on);
  }, []);
  return phone;
}

function newNode(kind, position) {
  const [type, arg] = kind.split(':');
  const base = { id: genId('n_'), type, label: '', position };
  if (type === 'employee') return { ...base, employeeId: arg || '' };
  if (type === 'decision') {
    const opts = arg === 'yesno' ? [] : [{ id: genId('o_'), label: 'Option 1' }, { id: genId('o_'), label: 'Option 2' }];
    return { ...base, decision: { kind: arg, question: '', options: opts, answeredBy: null } };
  }
  if (type === 'join') return { ...base, join: { forkId: '' } };
  if (type === 'ask') return { ...base, ask: { prompt: '', replies: [{ id: genId('o_'), label: 'Go ahead' }, { id: genId('o_'), label: 'Stop' }] } };
  if (type === 'wait') return { ...base, wait: { kind: 'command', minutes: 10, at: '09:00', command: '', every: 60, timeout: 60, what: '' } };
  return base;
}

function Inner({ apiFetch, factoryId, onClose, initial }) {
  const rf = useReactFlow();
  const phone = usePhone();
  const [f, setF] = useState(initial.factory);
  const [version, setVersion] = useState(initial.factory.version);
  const [save, setSave] = useState('saved');
  const [selection, setSelection] = useState(null);
  const [tab, setTab] = useState(phone ? null : 'edit');
  const [rosterSel, setRosterSel] = useState(null);
  const [rosterAddTo, setRosterAddTo] = useState(null);
  const [skills, setSkills] = useState([]);
  // The machine's secrets, by name (never values), when the server can
  // pre-authorise them for a run (capability 'factory-secrets'). null: hidden.
  const [secretList, setSecretList] = useState(null);
  const [run, setRun] = useState(null);
  const [toast, setToast] = useState('');
  // Measured node sizes. React Flow reports them as 'dimensions' changes; in
  // controlled mode they must be handed back or the minimap, fitView and the
  // layout never learn how big a node is.
  const [sizes, setSizes] = useState({});
  const past = useRef([]), future = useRef([]);
  const last = useRef({ key: '', at: 0 });
  const dragStart = useRef(null);
  const dirty = useRef(false);
  const saveTimer = useRef(null);
  const wrap = useRef(null);
  const [, force] = useState(0);

  useEffect(() => {
    apiFetch('/api/skills').then(r => r.json()).then(j => setSkills((j.skills || []).map(s => ({ name: s.name, description: s.description || '', scope: s.scope })))).catch(() => {});
    apiFetch('/api/handshake').then(r => r.json()).then(hs => {
      if (!((hs && hs.server && hs.server.capabilities) || []).includes('factory-secrets')) return;
      return apiFetch('/api/secrets').then(r => r.json()).then(j => { if (j && j.ok) setSecretList((j.secrets || []).map(x => ({ id: x.id, name: x.name, description: x.description || '' }))); });
    }).catch(() => {});
  }, []);
  const globalSkills = useMemo(() => new Set(skills.filter(s => s.scope === 'global' || s.scope === 'synced').map(s => s.name)), [skills]);
  const uniqueSkills = useMemo(() => { const m = new Map(); for (const s of skills) if (!m.has(s.name)) m.set(s.name, s); return [...m.values()]; }, [skills]);
  const validation = useMemo(() => validateFactory(f, { skills: skills.length ? globalSkills : null }), [f, globalSkills, skills.length]);

  // ─── Changes, undo, redo ───
  const mutate = useCallback((fn, key = '') => {
    if (run) return;
    setF(prev => {
      const now = Date.now();
      const coalesce = key && last.current.key === key && now - last.current.at < 1500;
      last.current = { key, at: now };
      if (!coalesce) { past.current.push(prev); if (past.current.length > HISTORY_MAX) past.current.shift(); future.current = []; }
      const next = clone(prev);
      fn(next);
      return next;
    });
    dirty.current = true;
    setSave('dirty');
  }, [run]);
  const undo = () => { if (!past.current.length || run) return; setF(cur => { future.current.push(cur); return past.current.pop(); }); last.current = { key: '', at: 0 }; dirty.current = true; setSave('dirty'); force(x => x + 1); };
  const redo = () => { if (!future.current.length || run) return; setF(cur => { past.current.push(cur); return future.current.pop(); }); last.current = { key: '', at: 0 }; dirty.current = true; setSave('dirty'); force(x => x + 1); };

  // ─── Saving ───
  const fRef = useRef(f); fRef.current = f;
  const vRef = useRef(version); vRef.current = version;
  const doSave = useCallback(async () => {
    clearTimeout(saveTimer.current);
    if (!dirty.current) return true;
    dirty.current = false;
    setSave('saving');
    try {
      const res = await apiFetch('/api/factories', { method: 'POST', body: JSON.stringify({ action: 'save', id: factoryId, definition: fRef.current, baseVersion: vRef.current }) });
      const j = await res.json();
      if (j.ok) { setVersion(j.factory.version); setSave(dirty.current ? 'dirty' : 'saved'); return true; }
      if (j.conflict) { setSave('conflict'); return false; }
      dirty.current = true; setSave('error'); return false;
    } catch { dirty.current = true; setSave('error'); return false; }
  }, [apiFetch, factoryId]);
  useEffect(() => {
    if (!dirty.current) return;
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(doSave, SAVE_DELAY);
  }, [f, doSave]);
  useEffect(() => () => { clearTimeout(saveTimer.current); if (dirty.current) doSave(); }, []);
  const reload = async () => {
    const j = await apiFetch('/api/factories/get?id=' + encodeURIComponent(factoryId)).then(r => r.json());
    if (j.ok) { setF(j.factory); setVersion(j.factory.version); past.current = []; future.current = []; dirty.current = false; setSave('saved'); }
  };

  // ─── Keyboard ───
  useEffect(() => {
    const onKey = (e) => {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName);
      if (e.key.toLowerCase() === 's') { e.preventDefault(); doSave(); return; }
      if (typing) return;
      if (e.key.toLowerCase() === 'z' && !e.shiftKey) { e.preventDefault(); undo(); }
      else if ((e.key.toLowerCase() === 'z' && e.shiftKey) || e.key.toLowerCase() === 'y') { e.preventDefault(); redo(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ─── Canvas ───
  const flow = useMemo(() => {
    const out = toFlow(f, { validation, selection, run, readOnly: !!run });
    for (const n of out.nodes) if (sizes[n.id]) n.measured = sizes[n.id];
    return out;
  }, [f, validation, selection, run, sizes]);
  const select = (s) => { setSelection(s); if (s) setTab(s.kind === 'node' || s.kind === 'edge' ? 'edit' : tab); };

  const onNodesChange = useCallback((changes) => {
    const moves = changes.filter(c => c.type === 'position' && c.position);
    const removes = changes.filter(c => c.type === 'remove').map(c => c.id);
    const dims = changes.filter(c => c.type === 'dimensions' && c.dimensions);
    if (dims.length) setSizes(prev => { const next = { ...prev }; for (const c of dims) next[c.id] = { width: c.dimensions.width, height: c.dimensions.height }; return next; });
    if (moves.length) {
      const dragging = moves.some(c => c.dragging);
      if (dragging && !dragStart.current) dragStart.current = fRef.current;
      setF(prev => {
        const next = clone(prev);
        for (const c of moves) { const n = next.nodes.find(x => x.id === c.id); if (n) n.position = { x: Math.round(c.position.x), y: Math.round(c.position.y) }; }
        return next;
      });
      if (!dragging && dragStart.current) {
        past.current.push(dragStart.current); future.current = []; dragStart.current = null;
        dirty.current = true; setSave('dirty');
      }
    }
    if (removes.length) {
      mutate(d => {
        const gone = new Set(removes.filter(id => { const n = d.nodes.find(x => x.id === id); return n && !['start', 'manager', 'end'].includes(n.type); }));
        d.nodes = d.nodes.filter(n => !gone.has(n.id));
        d.edges = d.edges.filter(e => !gone.has(e.source) && !gone.has(e.target));
        for (const n of d.nodes) if (n.type === 'join' && gone.has(n.join.forkId)) n.join.forkId = '';
      });
      setSelection(null);
    }
  }, [mutate]);
  const onEdgesChange = useCallback((changes) => {
    const removes = changes.filter(c => c.type === 'remove').map(c => c.id);
    if (!removes.length) return;
    mutate(d => { d.edges = d.edges.filter(e => !removes.includes(e.id) || e.sourceHandle === 'finish'); });
    setSelection(null);
  }, [mutate]);

  const isValidConnection = useCallback((c) => {
    const s = fRef.current.nodes.find(n => n.id === c.source), t = fRef.current.nodes.find(n => n.id === c.target);
    if (!s || !t || c.source === c.target) return false;
    if (c.sourceHandle === 'note') return c.targetHandle === 'notein' && AGENTS.has(s.type) && AGENTS.has(t.type);
    if (c.targetHandle !== 'in' || t.type === 'start') return false;
    if (c.sourceHandle === 'finish') return t.type === 'end';
    if (t.type === 'end') return false;
    return true;
  }, []);
  const onConnect = useCallback((c) => {
    if (!isValidConnection(c)) return;
    let limited = false;
    mutate(d => {
      if (c.sourceHandle === 'note') {
        if (d.edges.some(e => e.kind === 'note' && e.source === c.source && e.target === c.target)) return;
        d.edges.push({ id: genId('e_'), kind: 'note', source: c.source, sourceHandle: 'note', target: c.target, requireNote: false, noteLabel: '', maxRounds: 0 });
        return;
      }
      d.edges = d.edges.filter(e => !(e.kind === 'flow' && e.source === c.source && e.sourceHandle === c.sourceHandle));
      // An edge that closes a loop gets a round limit straight away: a loop
      // without one cannot run, and 3 is a sensible start.
      const g = flowGraph(d);
      const closesLoop = reachable(g, [c.target]).has(c.source);
      limited = closesLoop;
      d.edges.push({ id: genId('e_'), kind: 'flow', source: c.source, sourceHandle: c.sourceHandle, target: c.target, requireNote: false, noteLabel: '', maxRounds: closesLoop ? 3 : 0 });
    });
    if (limited) flash('That closes a loop, so it got a round limit of 3. Change it on the connection.');
  }, [mutate, isValidConnection]);

  const flash = (t) => { setToast(t); setTimeout(() => setToast(x => x === t ? '' : x), 4000); };

  const addAt = useCallback((kind, position) => {
    const pos = position || (() => {
      const r = wrap.current?.getBoundingClientRect();
      return rf.screenToFlowPosition({ x: (r?.left || 0) + (r?.width || 600) / 2, y: (r?.top || 0) + (r?.height || 400) / 2 });
    })();
    const snapped = { x: Math.round(pos.x / 16) * 16, y: Math.round(pos.y / 16) * 16 };
    const n = newNode(kind, snapped);
    mutate(d => {
      d.nodes.push(n);
      if (kind === 'decision:many') d.nodes.push({ ...newNode('join', { x: snapped.x, y: snapped.y + 240 }), label: 'Join', join: { forkId: n.id } });
    });
    setSelection({ kind: 'node', id: n.id });
    if (kind === 'employee:') { setRosterAddTo(n.id); setRosterSel(null); setTab('team'); }
    else setTab(phone ? null : 'edit');
  }, [mutate, rf, phone]);

  const onDrop = useCallback((e) => {
    const kind = e.dataTransfer.getData(DRAG_TYPE);
    if (!kind) return;
    e.preventDefault();
    addAt(kind, rf.screenToFlowPosition({ x: e.clientX, y: e.clientY }));
  }, [addAt, rf]);

  const layout = () => {
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: 'TB', ranksep: 70, nodesep: 50 });
    g.setDefaultEdgeLabel(() => ({}));
    for (const n of rf.getNodes()) g.setNode(n.id, { width: n.measured?.width || 220, height: n.measured?.height || 100 });
    // Back edges (loops) are left out so the flow reads top to bottom.
    for (const [a, b] of layoutEdges(f)) g.setEdge(a, b);
    dagre.layout(g);
    mutate(d => { for (const n of d.nodes) { const p = g.node(n.id); if (p) n.position = { x: Math.round(p.x - p.width / 2), y: Math.round(p.y - p.height / 2) }; } });
    setTimeout(() => rf.fitView({ duration: 300, padding: 0.15 }), 50);
  };

  const pickIssue = (it) => {
    if (it.employeeId) { setRosterSel(it.employeeId); setTab('team'); return; }
    const id = it.nodeId || (it.nodeIds || [])[0];
    if (id) {
      setSelection({ kind: 'node', id }); setTab('edit');
      const n = rf.getNode(id);
      if (n) rf.setCenter(n.position.x + (n.measured?.width || 200) / 2, n.position.y + 40, { zoom: Math.max(rf.getZoom(), 0.9), duration: 300 });
      return;
    }
    if (it.edgeId) { setSelection({ kind: 'edge', id: it.edgeId }); setTab('edit'); }
  };

  const removeEmployee = (id) => {
    const steps = f.nodes.filter(n => n.type === 'employee' && n.employeeId === id).length;
    const r = f.roster.find(x => x.id === id);
    if (!window.confirm(`Remove ${r?.name || 'this person'} from the team${steps ? ` and their ${steps} step${steps === 1 ? '' : 's'} from the flow` : ''}?`)) return;
    mutate(d => {
      const gone = new Set(d.nodes.filter(n => n.type === 'employee' && n.employeeId === id).map(n => n.id));
      d.nodes = d.nodes.filter(n => !gone.has(n.id));
      d.edges = d.edges.filter(e => !gone.has(e.source) && !gone.has(e.target));
      d.roster = d.roster.filter(x => x.id !== id);
      for (const n of d.nodes) if (n.type === 'decision' && n.decision.answeredBy === id) n.decision.answeredBy = null;
    });
    setRosterSel(null);
  };

  // ─── Panels ───
  const selNode = selection?.kind === 'node' ? f.nodes.find(n => n.id === selection.id) : null;
  const selEdge = selection?.kind === 'edge' ? f.edges.find(e => e.id === selection.id) : null;
  const panel = () => {
    if (tab === 'add') return <Palette f={f} onAdd={(k) => { addAt(k); }} />;
    if (tab === 'team') return <RosterPane f={f} mutate={mutate} selected={rosterSel} setSelected={(id) => { setRosterSel(id); if (!id) setRosterAddTo(null); }} skills={uniqueSkills} issuesByEmployee={flow.issues.employees} onRemove={removeEmployee} onAddTo={rosterAddTo} />;
    if (tab === 'issues') return <IssuesPane validation={validation} onPick={pickIssue} />;
    if (tab === 'dry') return <DryRun f={normalizeFactory(f)} run={run} setRun={setRun} valid={validation.ok} />;
    if (selNode) return <NodeInspector f={f} node={selNode} issues={flow.issues.nodes.get(selNode.id) || []} skills={uniqueSkills} mutate={mutate}
      remove={() => onNodesChange([{ type: 'remove', id: selNode.id }])} openRoster={(id, nodeId) => { if (id === 'new') { setRosterAddTo(nodeId); setRosterSel(null); } else setRosterSel(id); setTab('team'); }} />;
    if (selEdge) return <EdgeInspector f={f} edge={selEdge} issues={flow.issues.edges.get(selEdge.id) || []} mutate={mutate} remove={() => onEdgesChange([{ type: 'remove', id: selEdge.id }])} />;
    return <FactorySettings f={f} mutate={mutate} skills={uniqueSkills} secretList={secretList} />;
  };
  const tabs = [
    ...(phone ? [{ id: 'add', icon: 'plus', label: 'Add' }] : []),
    { id: 'edit', icon: 'sliders', label: 'Edit' },
    { id: 'team', icon: 'team', label: 'Team' },
    { id: 'issues', icon: 'alert', label: validation.ok ? 'Issues' : `Fix ${validation.errors.length}` },
    { id: 'dry', icon: 'route', label: 'Dry run' },
  ];
  const saveLabel = { saved: 'Saved', saving: 'Saving…', dirty: 'Unsaved', conflict: 'Changed elsewhere', error: 'Not saved' }[save];

  return (
    <div className={'ff-editor' + (phone ? ' phone' : '') + (run ? ' dry' : '')}>
      <div className="ff-top">
        {onClose && <button className="ff-iconbtn" title="Back" onClick={async () => { await doSave(); onClose(); }}><Icon name="back" size={16} /></button>}
        <button className="ff-name" onClick={() => { setSelection(null); setTab('edit'); }} title="Factory settings">{f.name || 'Untitled factory'}</button>
        <span className={'ff-status ' + save + (validation.ok ? '' : ' draft')} title={validation.ok ? '' : 'Saved as a draft: fix the problems to run it'}>
          {saveLabel}{validation.ok ? '' : ' · draft'}
        </span>
        {save === 'conflict' && <button className="ff-btn" onClick={reload}>Reload</button>}
        {save === 'error' && <button className="ff-btn" onClick={() => { dirty.current = true; doSave(); }}>Retry</button>}
        <div className="ff-top-sp" />
        <button className="ff-iconbtn" title="Undo (Ctrl+Z)" disabled={!past.current.length || !!run} onClick={undo}><Icon name="undo" size={16} /></button>
        <button className="ff-iconbtn" title="Redo (Ctrl+Shift+Z)" disabled={!future.current.length || !!run} onClick={redo}><Icon name="redo" size={16} /></button>
        <button className="ff-iconbtn" title="Tidy the layout" disabled={!!run} onClick={layout}><Icon name="layout" size={16} /></button>
        <button className="ff-iconbtn" title="Fit to screen" onClick={() => rf.fitView({ duration: 300, padding: 0.15 })}><Icon name="fit" size={16} /></button>
      </div>
      <div className="ff-body">
        {!phone && <div className="ff-left"><Palette f={f} onAdd={(k) => addAt(k)} /></div>}
        <div className="ff-canvas" ref={wrap} onDragOver={e => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; }} onDrop={onDrop}>
          <ReactFlow
            nodes={flow.nodes} edges={flow.edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes}
            onNodesChange={onNodesChange} onEdgesChange={onEdgesChange} onConnect={onConnect} isValidConnection={isValidConnection}
            onNodeClick={(_, n) => select({ kind: 'node', id: n.id })} onEdgeClick={(_, e) => select({ kind: 'edge', id: e.id })}
            onPaneClick={() => { setSelection(null); if (phone) setTab(null); }}
            nodesConnectable={!run} elementsSelectable
            snapToGrid snapGrid={[16, 16]} fitView fitViewOptions={{ padding: 0.15, maxZoom: 1 }} minZoom={0.15} maxZoom={2}
            deleteKeyCode={run ? null : ['Delete', 'Backspace']} connectionRadius={30} colorMode="dark"
            defaultEdgeOptions={{ markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 } }}
          >
            <Background gap={16} size={1} />
            <Controls showInteractive={false} position={phone ? 'top-left' : 'bottom-left'} />
            {!phone && <MiniMap pannable zoomable nodeColor={(n) => n.data?.issues?.some(i => i.level === 'error') ? '#ef4444' : n.data?.tokens?.length ? '#10b981' : '#6b6b9a'} nodeStrokeWidth={0} maskColor="rgba(10,10,15,0.65)" />}
          </ReactFlow>
          {toast && <div className="ff-toast">{toast}</div>}
        </div>
        {(!phone || tab) && (
          <div className={'ff-right' + (phone ? ' sheet' : '')}>
            <div className="ff-tabs">
              {tabs.map(t => (
                <button key={t.id} className={'ff-tab' + (tab === t.id || (!tab && t.id === 'edit') ? ' on' : '') + (t.id === 'issues' && !validation.ok ? ' bad' : '')} onClick={() => setTab(t.id)}>
                  <Icon name={t.icon} size={14} /><span>{t.label}</span>
                </button>
              ))}
              {phone && <button className="ff-iconbtn" title="Close" onClick={() => setTab(null)}><Icon name="x" size={16} /></button>}
            </div>
            <div className="ff-panel">{panel()}</div>
          </div>
        )}
      </div>
      {phone && !tab && (
        <div className="ff-phonebar">
          {tabs.map(t => (
            <button key={t.id} className={'ff-tab' + (t.id === 'issues' && !validation.ok ? ' bad' : '')} onClick={() => setTab(t.id)}><Icon name={t.icon} size={16} /><span>{t.label}</span></button>
          ))}
        </div>
      )}
    </div>
  );
}

export function Editor(props) {
  return <ReactFlowProvider><Inner {...props} /></ReactFlowProvider>;
}
