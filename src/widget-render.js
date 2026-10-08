/**
 * widget-render.js — render a widget the way Crundi will show it, and report
 * what a careful reviewer would: a screenshot per frame, script errors, and
 * the lint findings (overflow, clipped text, tap targets, contrast).
 *
 * This is how Claude checks its own work before a person ever sees it. It
 * always uses the server's headless browser, never an attached desktop app:
 * the result has to be the same wherever the request came from, and only the
 * headless backend can pretend to be a phone.
 */

import { randomBytes } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import * as headless from './browser-headless.js';
import * as store from './widget-store.js';
import { buildDoc, buildHarnessPage, FRAMES } from './widget-doc.js';

const MAX_SHOTS = 8;
const TICKET_TTL_MS = 60_000;

// Harness pages waiting to be fetched by the headless browser. A ticket is the
// only credential: it is random, single-purpose and gone in a minute, so the
// route that serves it needs no session.
const tickets = new Map();

export function takeHarnessPage(ticket) {
  const t = tickets.get(String(ticket || ''));
  if (!t || Date.now() - t.at > TICKET_TTL_MS) { tickets.delete(String(ticket || '')); return null; }
  return t.html;
}

function putHarnessPage(html) {
  const now = Date.now();
  for (const [k, v] of tickets) if (now - v.at > TICKET_TTL_MS) tickets.delete(k);
  const ticket = randomBytes(18).toString('hex');
  tickets.set(ticket, { html, at: now });
  return ticket;
}

export function renderSupported() { return headless.isSupported(); }

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function shootOne({ baseUrl, html, frame }) {
  const f = FRAMES[frame];
  const key = '__widget:' + randomBytes(6).toString('hex');
  const ticket = putHarnessPage(html);
  const url = `${baseUrl}/api/widgets/harness?ticket=${ticket}`;
  const opened = await headless.handle({ type: 'browserOpen', key, url, width: f.width, height: f.height, mobile: !!f.mobile, dpr: f.mobile ? 2 : 1 });
  if (!opened.ok) return { ok: false, error: opened.error };
  try {
    const evalStr = async (code) => { const r = await headless.handle({ type: 'browserEval', key, code }); return r && r.ok ? (r.result ?? '') : ''; };
    const deadline = Date.now() + 10_000;
    // 1. Wait for the widget to start and its first sizes to be applied.
    while (Date.now() < deadline) {
      const s = await evalStr('window.__wgResult ? (window.__wgResult.done ? "done" : (window.__wgResult.readyAt ? String(Date.now() - window.__wgResult.readyAt) : "")) : ""');
      if (s === 'done' || Number(s) >= 600) break;
      await sleep(120);
    }
    // 2. Make the browser draw. A headless page does not repaint by itself,
    //    and a frame only learns it was resized when its page is next drawn:
    //    without this the lint measures the widget in a frame of the wrong size.
    await headless.handle({ type: 'browserScreenshot', key });
    await sleep(150);
    // 3. Now ask for the lint, and wait for it.
    await evalStr('window.__wgAskLint ? String(window.__wgAskLint()) : ""');
    let result = null;
    while (Date.now() < deadline) {
      const raw = await evalStr('window.__wgResult && window.__wgResult.done ? JSON.stringify(window.__wgResult) : ""');
      if (raw) { try { result = JSON.parse(raw); } catch { /* keep waiting */ } }
      if (result) break;
      await sleep(150);
    }
    const shot = await headless.handle({ type: 'browserScreenshot', key });
    const logs = await headless.handle({ type: 'browserConsole', key });
    if (!shot.ok) return { ok: false, error: shot.error };
    return {
      ok: true, png: shot.data,
      result: result || { ready: false, faults: [{ message: 'The harness did not finish in time.' }], lint: null },
      console: (logs.ok ? logs.logs || [] : []).map((l) => String(l.text || '')).filter(Boolean).slice(-15),
    };
  } finally {
    await headless.handle({ type: 'browserClose', key }).catch(() => {});
    tickets.delete(ticket);
  }
}

/**
 * @param {object} o
 * @param {string} o.baseUrl   loopback URL of this server
 * @param {string} o.alias
 * @param {string} o.id
 * @param {string[]} [o.frames]  keys of FRAMES; default: cell + mobile
 * @param {string[]} [o.states]  fixture names; default: live data (or every fixture when `allStates`)
 * @param {object} o.liveData    { source: { value, error } } as the page would get it now
 */
export async function renderWidget({ baseUrl, alias, id, frames, states, liveData = {} }) {
  if (!headless.isSupported()) {
    return { ok: false, error: 'No headless browser on this server, so widgets cannot be rendered for checking. Re-run the Crundi installer to provision one.' };
  }
  const meta = store.get(alias, id);
  if (!meta) return { ok: false, error: `No widget "${id}". Call widget_open first.` };
  const doc = await buildDoc(alias, id, { harness: true });
  // The chip is its own build of the same source: it knows from its first byte
  // that it is the chip.
  const chipDoc = await buildDoc(alias, id, { harness: true, chip: true });
  // A chip is three things to look at: its face in the bar on a desktop and
  // on a phone (where it gets half the width), and the panel it opens.
  const dflt = store.effectiveSlot(meta) === 'chip' ? ['chip', 'mobile-chip', 'mobile'] : [slotFrame(meta), slotFrame(meta, true)];
  const wantFrames = (Array.isArray(frames) && frames.length ? frames : dflt)
    .map(String).filter((f, i, a) => a.indexOf(f) === i);
  const bad = wantFrames.filter((f) => !FRAMES[f]);
  if (bad.length) return { ok: false, error: `Unknown frame "${bad[0]}". Frames: ${Object.keys(FRAMES).join(', ')}` };
  const wantStates = Array.isArray(states) && states.length ? states.map(String) : ['live'];
  const shots = [];
  const kv = store.storeAll(alias, id);
  for (const state of wantStates) {
    let data = liveData;
    if (state !== 'live') {
      const fx = store.readFixture(alias, id, state);
      if (!fx.ok) return { ok: false, error: fx.error };
      data = {};
      for (const [k, v] of Object.entries(fx.data)) {
        // A fixture may give a bare value, or { error: "..." } to show a failed source.
        data[k] = v && typeof v === 'object' && !Array.isArray(v) && typeof v.$error === 'string' ? { value: null, error: v.$error } : { value: v };
      }
    }
    for (const frame of wantFrames) {
      if (shots.length >= MAX_SHOTS) break;
      const html = await buildHarnessPage({ doc: FRAMES[frame].slot === 'chipbar' ? chipDoc.html : doc.html, title: doc.title, frame, data, store: kv, stateName: state === 'live' ? '' : state });
      const r = await shootOne({ baseUrl, html, frame });
      if (!r.ok) return { ok: false, error: `Render failed (${frame}): ${r.error}` };
      if (FRAMES[frame].slot === 'chipbar' && !(r.result.lint && r.result.lint.metrics && r.result.lint.metrics.chipFace)) (r.result.faults = r.result.faults || []).push({ message: 'This panel has no chip face. Mark ONE top-level element in index.html with data-chip (e.g. <div data-chip>…</div>) and fill it from onData; without it the chip shows only the title.' });
      shots.push({ frame, state, width: FRAMES[frame].width, height: FRAMES[frame].height, label: FRAMES[frame].label, png: r.png, faults: r.result.faults || [], lint: r.result.lint, console: r.console, ready: !!r.result.ready });
    }
  }
  // Keep the latest set on disk so the person can see what Claude saw.
  try {
    const dir = join(store.stateDirOf(alias, id), 'renders');
    mkdirSync(dir, { recursive: true });
    for (const s of shots) writeFileSync(join(dir, `${s.frame}--${s.state}.png`), Buffer.from(s.png, 'base64'));
    writeFileSync(join(dir, 'index.json'), JSON.stringify({ at: Date.now(), shots: shots.map(({ png, ...rest }) => rest) }));
  } catch { /* a missing preview must not fail the render */ }
  return { ok: true, shots, problems: doc.problems, fixtures: store.listFixtures(alias, id) };
}

function slotFrame(meta, mobile = false) {
  const slot = store.effectiveSlot(meta);
  if (slot === 'dock') return mobile ? 'mobile-dock' : 'dock';
  if (slot === 'tab') return mobile ? 'mobile-tab' : 'tab';
  if (slot === 'inline') return mobile ? 'mobile' : 'inline';
  // A chip is two things to look at: its face in the bar, and the panel it opens.
  if (slot === 'chip') return mobile ? 'cell' : 'chip';
  return mobile ? 'mobile' : 'cell';
}

/** The findings as text Claude can act on, one block per shot. */
export function describeRender(r) {
  const lines = [];
  let errors = 0, warns = 0;
  for (const s of r.shots) {
    lines.push(`## ${s.frame}${s.state !== 'live' ? ` · fixture "${s.state}"` : ''} — ${s.width}×${s.height}, ${s.label}`);
    const m = s.lint && s.lint.metrics;
    if (m) lines.push(`frame ${m.frameWidth}×${m.frameHeight}px, content ${m.contentWidth}×${m.contentHeight}px, ${m.elements} visible elements`);
    if (!s.ready) { errors++; lines.push('ERROR  The widget did not start.'); }
    for (const f of s.faults) { errors++; lines.push(`ERROR  script: ${f.message}${f.where ? ` (${f.where})` : ''}`); }
    for (const i of (s.lint && s.lint.issues) || []) {
      // A fixture that sets a source's error is showing the error state on purpose.
      if (i.rule === 'source' && s.state !== 'live' && i.level === 'error') i.level = 'info';
      if (i.level === 'error') errors++; else if (i.level === 'warn') warns++;
      lines.push(`${i.level.toUpperCase().padEnd(5)}  ${i.rule}: ${i.message}`);
      for (const n of i.nodes || []) lines.push(`         - ${n}`);
    }
    const noisy = s.console.filter((c) => !/^\[?crundi/i.test(c));
    if (noisy.length) { lines.push('console:'); for (const c of noisy.slice(-6)) lines.push('  ' + c.slice(0, 240)); }
    if (!s.faults.length && !((s.lint && s.lint.issues) || []).length && s.ready) lines.push('clean: no script errors, no lint findings');
    lines.push('');
  }
  if (r.problems && r.problems.length) { lines.push('## Source problems'); for (const p of r.problems) lines.push('- ' + p); lines.push(''); }
  const head = errors ? `${errors} error(s), ${warns} warning(s). Fix the errors and render again.`
    : warns ? `No errors, ${warns} warning(s). Look at each screenshot: lint cannot judge whether it looks good.`
      : 'No errors or warnings. Look at each screenshot before calling it done: lint cannot judge whether it looks good.';
  if (r.fixtures && r.fixtures.length) lines.push(`Fixtures available for states: ${r.fixtures.join(', ')}`);
  return { text: head + '\n\n' + lines.join('\n').trim(), errors, warns };
}
