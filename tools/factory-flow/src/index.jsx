// window.CrundiFactoryFlow: the factory editor (and, in card 7, the live
// graph), as a self-contained island. React and React Flow live only in this
// bundle; the page loads it when a factory is opened.
import { createRoot } from 'react-dom/client';
import '@xyflow/react/dist/style.css';
import './styles.css';
import { Editor } from './Editor.jsx';
import { Viewer } from './Viewer.jsx';

function mountEditor(el, { apiFetch, factoryId, onClose }) {
  const root = createRoot(el);
  el.classList.add('ff-host');
  let destroyed = false;
  root.render(<div className="ff-loading">Loading…</div>);
  apiFetch('/api/factories/get?id=' + encodeURIComponent(factoryId)).then(r => r.json()).then(j => {
    if (destroyed) return;
    if (!j.ok) { root.render(<div className="ff-loading">{j.error || 'Could not open this factory.'}</div>); return; }
    root.render(<Editor apiFetch={apiFetch} factoryId={factoryId} onClose={onClose} initial={j} />);
  }).catch(err => { if (!destroyed) root.render(<div className="ff-loading">Could not open this factory: {String(err.message || err)}</div>); });
  return { destroy() { destroyed = true; root.unmount(); el.classList.remove('ff-host'); } };
}

/**
 * The live graph of a run, with its replay.
 * subscribe(cb) -> unsubscribe: the host calls cb({ id }) when a run changes
 * (its factoryRun event); without it the view polls. openChat(chatId, name)
 * opens a person's chat.
 */
function mountViewer(el, { apiFetch, runId, subscribe, openChat, onClose }) {
  const root = createRoot(el);
  el.classList.add('ff-host');
  root.render(<Viewer apiFetch={apiFetch} runId={runId} subscribe={subscribe} openChat={openChat} onClose={onClose} />);
  return { destroy() { root.unmount(); el.classList.remove('ff-host'); } };
}

/* global __FF_SRC_HASH__ */
window.CrundiFactoryFlow = { mountEditor, mountViewer, version: 1, srcHash: __FF_SRC_HASH__ };
