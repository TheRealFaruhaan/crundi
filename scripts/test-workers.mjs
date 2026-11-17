#!/usr/bin/env node
/**
 * test-workers.mjs — src/workers.js: PCs joining as workers (phase 1).
 *
 * A real HTTP server on 127.0.0.1:0 and fake workers over real websockets.
 * Covers: join codes (single use, wrong ones counted and limited); the link
 * (key checked, hello -> welcome, info -> labels); a second link replacing
 * the first; the owner's switches reaching the PC and the PC's own pause
 * coming back; the owner's terminal relayed both ways and closed when the PC
 * goes; join-self from the PC client (same device, same worker, new key);
 * revoke and remove; the record surviving a restart; owner only.
 *
 * Run: node scripts/test-workers.mjs
 */
import { createServer } from 'node:http';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import WebSocket from 'ws';
import { createWorkersService, labelsOf, cleanLabels, cleanInfo, makeCode } from '../src/workers.js';

let failed = 0;
const ok = (c, l, extra = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c ? '' : '  ' + String(extra).slice(0, 300))); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 3000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(10); } return null; }

const dir = mkdtempSync(join(tmpdir(), 'workers-'));
let changes = 0;
let svc = createWorkersService({ dataDir: dir, serverVersion: '9.9.9', onChange: () => { changes++; } });

const readBody = (req) => new Promise(r => { let b = ''; req.on('data', c => b += c); req.on('end', () => r(b)); });
const json = (res, body, code = 200) => { res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(body)); };
let owner = true;
const server = createServer(async (req, res) => {
  const path = new URL(req.url, 'http://x').pathname;
  if (await svc.handlePublic(req, res, path)) return;
  if (await svc.handleApi(req, res, path, { principal: {}, readBody, json, isOwner: () => owner })) return;
  json(res, { error: 'Not found' }, 404);
});
server.on('upgrade', (req, socket, head) => { if (!svc.handleUpgrade(req, socket, head)) socket.destroy(); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const post = async (path, body) => { const r = await fetch(base + path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }); return { status: r.status, ...(await r.json()) }; };
const api = (body) => post('/api/workers', body);
const getList = async () => (await fetch(base + '/api/workers')).json();

/** A fake worker: connects, says hello, records what it gets. */
function fakeWorker(key, { hello = true, info = {} } = {}) {
  const got = [];
  const ws = new WebSocket(base.replace('http', 'ws') + '/api/workers/link', { headers: { Authorization: 'Bearer ' + key } });
  const w = { ws, got, status: null, closed: null };
  w.open = new Promise((resolve) => {
    ws.on('open', () => resolve(true));
    ws.on('error', () => resolve(false));
    // A refused upgrade: ws reports it here and nowhere else.
    ws.on('unexpected-response', (_req, res) => { w.status = res.statusCode; res.resume(); resolve(false); });
  });
  ws.on('message', (raw) => got.push(JSON.parse(raw.toString())));
  ws.on('close', (code) => { w.closed = code; });
  w.send = (m) => ws.send(JSON.stringify(m));
  w.of = (type) => got.filter(m => m.type === type);
  w.ready = (async () => {
    if (!(await w.open)) return false;
    if (hello) w.send({ type: 'hello', proto: 1, version: '0.1.0', kind: 'headless', name: 'x', caps: ['info', 'terminal', 'pause'], info: { os: 'linux', arch: 'x86_64', ramMb: 32768, cpu: { model: 'Ryzen', cores: 8, threads: 16 }, gpus: [{ vendor: 'NVIDIA', name: 'RTX 4070', vramMb: 12282 }], tools: { git: '2.43.0', cargo: '1.83.0', ffmpeg: '6.1' }, disk: { path: '/w', freeMb: 200000, totalMb: 500000 }, workDir: '/w', allow: { chats: true, commands: true }, paused: false, ...info } });
    return true;
  })();
  return w;
}

// ── Helpers ──
ok(/^[A-HJ-NP-Z2-9]{8}$/.test(makeCode()), 'a join code: 8 characters nobody misreads');
{
  const info = cleanInfo({ os: 'linux', arch: 'x86_64', ramMb: 16384 + 500, cpu: { threads: 12 }, gpus: [{ name: 'NVIDIA GeForce RTX 3060' }, { vendor: 'AMD', name: 'Radeon' }], tools: { git: '2', 'bad key!': 'x', cargo: '1' }, evil: 'dropped' });
  ok(!('evil' in info) && !('bad key!' in info.tools), 'info: cleaned to the spec\'s shape, unknown fields dropped');
  const l = labelsOf(info, ['fast', 'os:linux']);
  ok(['os:linux', 'arch:x86_64', 'ram:16g', 'cores:12', 'gpu:nvidia', 'gpu:amd', 'tool:git', 'tool:cargo', 'fast'].every(x => l.includes(x)) && l.filter(x => x === 'os:linux').length === 1, 'labels: made from info, then the owner\'s own, no repeats', l.join(','));
  ok(cleanLabels('Fast, gpu:big  bad label!, ok').join() === 'fast,gpu:big,bad,ok', 'custom labels: cleaned and lowercased', cleanLabels('Fast, gpu:big  bad label!, ok').join());
}

// ── Joining ──
const c1 = await api({ action: 'code' });
ok(c1.ok && /^[A-Z2-9]{8}$/.test(c1.code) && Date.parse(c1.expiresAt) > Date.now(), 'Add a PC: a join code with its expiry');
ok((await getList()).codes.some(c => c.code === c1.code), 'the code waiting shows in the list');
const wrong = await post('/api/workers/join', { code: 'AAAAAAAA', name: 'x' });
ok(wrong.status === 403 && /wrong or has expired/.test(wrong.error), 'a wrong code is refused');
const j1 = await post('/api/workers/join', { code: c1.code.toLowerCase().slice(0, 4) + '-' + c1.code.slice(4), name: 'Desk PC', deviceId: 'dev-desk', kind: 'tray', version: '0.1.0', os: 'windows' });
ok(j1.status === 200 && j1.ok && /^cw_w[0-9a-f]{10}\.[A-Za-z0-9_-]{40,}$/.test(j1.key) && j1.name === 'Desk PC', 'the code joins (case and a dash forgiven): a key, shown once', JSON.stringify(j1));
const again = await post('/api/workers/join', { code: c1.code, name: 'y' });
ok(again.status === 403, 'a code works once');
ok(!JSON.stringify(await getList()).includes(j1.key.split('.')[1]), 'the key is not kept or shown again');

// ── The link ──
{
  const bad = fakeWorker('cw_w0000000000.' + 'x'.repeat(43));
  await bad.open; await until(() => bad.status);
  ok(bad.status === 401, 'a wrong key: 401');
}
let w1 = fakeWorker(j1.key);
await w1.ready;
const welcome = await until(() => w1.of('welcome')[0]);
ok(welcome && welcome.id === j1.id && welcome.name === 'Desk PC' && welcome.caps.includes('terminal') && welcome.settings.allowCommands === true && welcome.server.version === '9.9.9', 'hello -> welcome with its id, name, the server\'s caps and settings', JSON.stringify(welcome));
let L = await until(async () => { const l = await getList(); const w = l.workers[0]; return w && w.online ? w : null; });
ok(L && L.online && L.kind === 'headless' && L.info.cpu.threads === 16 && L.labels.includes('gpu:nvidia') && L.labels.includes('tool:ffmpeg') && L.labels.includes('ram:32g'), 'online, with its info and labels', JSON.stringify(L));
w1.send({ type: 'info', info: { os: 'linux', tools: { git: '2.43.0' }, ramMb: 32768 } });
L = await until(async () => { const w = (await getList()).workers[0]; return w.info && !w.info.tools.cargo ? w : null; });
ok(L && !L.labels.includes('tool:cargo'), 'a fresh info replaces the old');
w1.send({ type: 'log', level: 'warn', text: 'disk is getting full' });
L = await until(async () => { const w = (await getList()).workers[0]; return w.logs.length ? w : null; });
ok(L && L.logs[0].level === 'warn' && /disk/.test(L.logs[0].text), 'the PC\'s log lines show on its page');
w1.send({ type: 'brand-new-thing', x: 1 });
await sleep(30);
ok(w1.ws.readyState === 1, 'an unknown message from a newer worker is ignored');

// A second link replaces the first.
{
  const w2 = fakeWorker(j1.key);
  await w2.ready;
  await until(() => w1.closed !== null);
  ok(w1.of('bye')[0]?.reason === 'replaced' && w1.closed !== null && w2.ws.readyState === 1, 'a second link replaces the first (bye: replaced)');
  w1 = w2;
  await until(() => w1.of('welcome')[0]);
}

// ── Settings both ways ──
{
  const r = await api({ action: 'set', id: j1.id, paused: true, allowChats: false, labels: 'Fast, studio' });
  ok(r.ok && r.worker.paused && r.worker.pausedBy === 'owner' && !r.worker.allowChats && r.worker.labels.includes('fast'), 'the owner pauses it, turns chats off and adds labels');
  const s = await until(() => w1.of('settings').find(m => m.paused === true));
  ok(s && s.allowChats === false, 'the PC is told');
  await api({ action: 'set', id: j1.id, paused: false });
  w1.send({ type: 'paused', paused: true, by: 'pc' });
  L = await until(async () => { const w = (await getList()).workers[0]; return w.paused && w.pausedBy === 'pc' ? w : null; });
  ok(!!L, 'paused from the PC\'s tray: shown as paused by the PC');
  const rn = await api({ action: 'rename', id: j1.id, name: 'Studio PC' });
  ok(rn.ok && rn.worker.name === 'Studio PC' && await until(() => w1.of('settings').find(m => m.name === 'Studio PC')), 'rename: kept and told to the PC');
}

// ── The owner's terminal ──
{
  const toUi = [];
  const t = svc.uiTerminals((m) => toUi.push(m));
  t.open({ ref: 'ui1', worker: j1.id, cols: 100, rows: 30 });
  const open = await until(() => w1.of('term.open')[0]);
  ok(open && open.cols === 100 && open.rows === 30 && open.ref && open.ref !== 'ui1', 'open: the PC is asked for a shell, under a ref of the server\'s');
  ok((await getList()).workers[0].terminals === 1, 'the list counts the open terminal');
  t.input({ ref: 'ui1', data: 'ls\r' });
  const inp = await until(() => w1.of('term.input')[0]);
  ok(inp && Buffer.from(inp.b64, 'base64').toString() === 'ls\r', 'typing reaches the PC');
  t.resize({ ref: 'ui1', cols: 120, rows: 40 });
  ok(!!(await until(() => w1.of('term.resize').find(m => m.cols === 120))), 'a resize reaches the PC');
  w1.send({ type: 'term.data', ref: open.ref, b64: Buffer.from('hello\r\n').toString('base64') });
  ok(!!(await until(() => toUi.find(m => m.type === 'worker-term-data' && m.ref === 'ui1' && m.data === 'hello\r\n'))), 'output comes back to the page, under its own ref');
  w1.send({ type: 'term.data', ref: 'not-mine', b64: 'eA==' });
  w1.send({ type: 'term.exit', ref: open.ref, code: 0 });
  ok(!!(await until(() => toUi.find(m => m.type === 'worker-term-exit' && m.code === 0))) && toUi.filter(m => m.type === 'worker-term-data').length === 1, 'exit reaches the page; a ref it never opened is ignored');
  const hist = await api({ action: 'history', id: j1.id });
  ok(hist.ok && ['joined', 'connected', 'paused', 'renamed', 'terminal-opened', 'terminal-closed'].every(x => hist.history.some(h => h.type === x)), 'history: joined, connected, paused, renamed, terminal opened and closed', hist.history.map(h => h.type).join());
  // The PC goes while a terminal is open.
  t.open({ ref: 'ui2', worker: j1.id, cols: 80, rows: 24 });
  await until(() => w1.of('term.open').length === 2);
  w1.ws.close();
  ok(!!(await until(() => toUi.find(m => m.type === 'worker-term-exit' && m.ref === 'ui2' && /disconnected/.test(m.error || '')))), 'the PC goes: its open terminal tells the page');
  L = await until(async () => { const w = (await getList()).workers[0]; return !w.online ? w : null; });
  ok(L && L.lastSeenAt, 'offline, with when it was last seen');
  t.open({ ref: 'ui3', worker: j1.id, cols: 80, rows: 24 });
  ok(toUi.some(m => m.type === 'worker-term-error' && m.ref === 'ui3' && /not connected/.test(m.error)), 'a terminal on an offline PC: says so');
}

// ── The PC client joins without a code ──
const s1 = await api({ action: 'join-self', name: 'Laptop', deviceId: 'dev-laptop', version: '1.21.8', os: 'linux' });
ok(s1.ok && s1.key && s1.id !== j1.id, 'join-self: the PC client joins with the owner\'s sign-in');
const s2 = await api({ action: 'join-self', name: 'Laptop', deviceId: 'dev-laptop', version: '1.21.8', os: 'linux' });
ok(s2.ok && s2.id === s1.id && s2.key !== s1.key, 'the same device again: the same worker, a new key');
{
  const old = fakeWorker(s1.key, { hello: false });
  await old.open; await until(() => old.status);
  ok(old.status === 401, 'its old key no longer works');
  const nw = fakeWorker(s2.key);
  await nw.ready;
  ok(!!(await until(() => nw.of('welcome')[0])), 'the new one does');
  // Revoke: told, closed, refused after.
  const r = await api({ action: 'revoke', id: s1.id });
  ok(r.ok && r.worker.revoked, 'revoke');
  ok(!!(await until(() => nw.of('bye').find(m => m.reason === 'revoked'))) && !!(await until(() => nw.closed !== null)), 'the PC is told it was revoked and the link closes');
  const back = fakeWorker(s2.key, { hello: false });
  await back.open; await until(() => back.status);
  ok(back.status === 401, 'a revoked key is refused');
  const re = await api({ action: 'reissue', id: s1.id });
  ok(re.ok && re.code, 'reissue: a join code for the same worker');
  const j2 = await post('/api/workers/join', { code: re.code, name: 'ignored' });
  ok(j2.ok && j2.id === s1.id && j2.name === 'Laptop', 'joining with it: the same worker back, name kept');
}

// ── Owner only ──
owner = false;
ok((await fetch(base + '/api/workers')).status === 403, 'not the owner: 403');
owner = true;

// ── A restart keeps the record ──
svc.dispose();
svc = createWorkersService({ dataDir: dir, serverVersion: '9.9.9' });
{
  const l = svc.list().workers;
  ok(l.length === 2 && l.find(w => w.id === j1.id)?.name === 'Studio PC' && l.find(w => w.id === j1.id).customLabels.includes('studio'), 'after a restart: the workers, names and labels are still there');
  const w = fakeWorker(j1.key);
  await w.ready;
  ok(!!(await until(() => w.of('welcome')[0])), 'and the key still works');
  const m = svc.mcpList();
  ok(m.ok && m.workers.some(x => x.name === 'Studio PC' && x.online && x.labels.includes('gpu:nvidia') && x.ramGb === 32), 'worker_list: the PCs with labels and a summary', JSON.stringify(m.workers));
  const rm = svc.act({ action: 'remove', id: j1.id });
  ok(rm.ok && !!(await until(() => w.of('bye').find(x => x.reason === 'removed'))) && svc.list().workers.length === 1, 'remove: told, gone from the list');
}

// Too many wrong codes from one place.
{
  let last;
  for (let i = 0; i < 11; i++) last = await post('/api/workers/join', { code: 'ZZZZZZZZ' });
  ok(last.status === 429, 'ten wrong codes from one address: told to wait');
}
ok(changes >= 3, 'changes are pushed to the owner\'s pages (batched)', changes);

svc.dispose();
server.close();
try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
