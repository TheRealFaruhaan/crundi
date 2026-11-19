#!/usr/bin/env node
/**
 * test-workers-chats.mjs — whole chats on a worker PC (src/workers-chats.js,
 * phase 4 of specs/workers-protocol.md), with claude-ui's real chat machinery
 * driving a fake worker over a real websocket.
 *
 * Covers: welcome offers chats; a PC without claude, an older worker, chats
 * switched off, an offline PC and a collaborator are refused (the first with
 * the install text); the places offered; chat.spawn carries claude's args,
 * the git source (https remote, the server's commit) and the chat's id but no
 * API key, and no .mcp.json lands in the server's checkout; stdin waits for
 * chat.started; stream-json from the PC becomes the chat's messages; the PC
 * goes away (header: offline), stdin sent meanwhile is held and delivered
 * after chat.attach from the right byte, nothing doubled; mcp.list gives an
 * owner chat's tools; mcp.call reaches /api/mcp/call with the chat's own
 * args (alias, session id, chat token) and the internal key; at a turn end
 * the work comes back as a real git bundle + patch into a worktree on
 * pc/<pc>/<chat>, uncommitted work uncommitted, the main checkout untouched;
 * "Bring changes back"; pullChat into a given worktree and branch (phase 5);
 * a factory chat is not pulled at turn end; a copy-source chat (no git);
 * close kills on the PC; a refusal from the PC ends the chat with the reason;
 * a chat the server does not know is stopped on hello; workersApi.
 *
 * Run: node scripts/test-workers-chats.mjs
 */
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, mkdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import WebSocket from 'ws';

let failed = 0;
const ok = (c, l, extra = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c ? '' : '  ' + String(extra).slice(0, 500))); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 6000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(10); } return null; }

const dir = mkdtempSync(join(tmpdir(), 'workers-chats-'));
const projects = join(dir, 'projects');
mkdirSync(projects, { recursive: true });
process.env.DATA_DIR = join(dir, 'data');
process.env.PROJECTS_DIR = projects;
const { createWorkersService } = await import('../src/workers.js');
const { createClaudeUiSessions } = await import('../src/claude-ui.js');
const { chatToken } = await import('../src/chat-token.js');
const { unsafeTar } = await import('../src/workers-chats.js');

// ── A PC's files are checked before they are unpacked on the server ──
{
  const t = join(dir, 'tars'); mkdirSync(join(t, 'in', 'sub'), { recursive: true });
  writeFileSync(join(t, 'in', 'ok.txt'), 'ok');
  writeFileSync(join(t, 'in', 'sub', 'b.txt'), 'b');
  const sh = (...a) => execFileSync('tar', a, { cwd: t });
  sh('-cf', 'good.tar', '-C', 'in', 'ok.txt', 'sub');
  // Symlinks need ln, which a Windows runner lacks: those two cases run elsewhere.
  const links = process.platform !== 'win32';
  if (links) {
    execFileSync('ln', ['-s', '../../etc/passwd', join(t, 'in', 'evil')]);
    sh('-cf', 'link.tar', '-C', 'in', 'ok.txt', 'evil');
    execFileSync('ln', ['-s', 'sub/b.txt', join(t, 'in', 'inside')]);
    sh('-cf', 'inlink.tar', '-C', 'in', 'inside', 'sub');
  }
  // GNU tar strips "../" when it writes; ours keeps the name as given.
  { const { writeTar } = await import('../src/tar-lite.js'); const parts = []; await writeTar(join(t, 'in', 'sub'), ['../ok.txt'], (b) => { parts.push(Buffer.from(b)); }); writeFileSync(join(t, 'dots.tar'), Buffer.concat(parts)); }
  ok(await unsafeTar(join(t, 'good.tar')) === '', 'a PC\'s tar with plain files: unpacked');
  if (links) {
    ok(/link that points outside/.test(await unsafeTar(join(t, 'link.tar'))), 'a symlink pointing outside the folder: refused');
    ok(await unsafeTar(join(t, 'inlink.tar')) === '', 'a symlink inside the folder: fine');
  }
  const d = await unsafeTar(join(t, 'dots.tar'));
  ok(/leaves the folder/.test(d), 'a "../" path: refused', d);
}
const { CLAUDE_MISSING, pullBranch } = await import('../src/workers-chats.js');

// A project: a git repo whose origin is GitHub (never contacted: the PC is fake).
const proj = join(projects, 'proj');
mkdirSync(proj, { recursive: true });
const gitIn = (cwd, ...a) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...a], { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
const g = (...a) => gitIn(proj, ...a);
g('init', '-q', '-b', 'main');
g('config', 'core.autocrlf', 'false');
writeFileSync(join(proj, 'a.txt'), 'one\n');
writeFileSync(join(proj, 'b.txt'), 'two\n');
g('add', '.'); g('commit', '-qm', 'first');
const headSha = g('rev-parse', 'HEAD');
g('remote', 'add', 'origin', 'git@github.com:someone/proj.git');
// A project without git (copy source).
const plain = join(projects, 'plain');
mkdirSync(plain, { recursive: true });
writeFileSync(join(plain, 'notes.txt'), 'plain\n');

// The server: the worker link, plus a stub /api/mcp/call that says what it got.
const INTERNAL = 'k-internal-test';
const mcpCalls = [];
let svc;
const server = createServer(async (req, res) => {
  const path = new URL(req.url, 'http://x').pathname;
  if (path === '/api/mcp/call' && req.method === 'POST') {
    const parts = []; for await (const c of req) parts.push(c);
    const body = JSON.parse(Buffer.concat(parts).toString('utf8'));
    mcpCalls.push({ key: req.headers['x-api-key'], ...body });
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(req.headers['x-api-key'] === INTERNAL ? { ok: true, tool: body.tool, echo: body.args } : { error: 'Invalid API key' }));
    return;
  }
  if (!(await svc.handlePublic(req, res, path))) { res.writeHead(404); res.end(); }
});
server.on('upgrade', (req, socket, head) => { if (!svc.handleUpgrade(req, socket, head)) socket.destroy(); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;
svc = createWorkersService({ dataDir: join(dir, 'data'), serverVersion: 't', mcpApi: () => ({ url: base, key: INTERNAL }) });
const ui = createClaudeUiSessions({ apiUrl: base, apiKey: INTERNAL });
ui.setRemote({ launch: (w, o) => svc.chats.launch(w, o), status: (w, id) => svc.chats.status(w, id) });
ui.onTurnEnd((id) => svc.chats.onTurnEnd(id));

function fakeWorker(key, { caps = ['info', 'terminal', 'pause', 'jobs', 'chats'], chats = [], tools = { git: '2.4', claude: '2.1.300' } } = {}) {
  const got = [];
  const ws = new WebSocket(base.replace('http', 'ws') + '/api/workers/link', { headers: { Authorization: 'Bearer ' + key } });
  const w = { ws, got, closed: false };
  ws.on('message', (raw) => got.push(JSON.parse(raw.toString())));
  ws.on('close', () => { w.closed = true; });
  w.send = (m) => ws.send(JSON.stringify(m));
  w.of = (type, chat) => got.filter(m => m.type === type && (!chat || m.chat === chat));
  w.ready = new Promise((resolve) => ws.on('open', () => {
    w.send({ type: 'hello', proto: 1, version: '0.4.0', kind: 'headless', name: 'x', caps, chats, info: { os: 'linux', arch: 'x86_64', tools, allow: { chats: true, commands: true } } });
    resolve();
  }));
  return w;
}
const joinPc = (name, deviceId) => svc.act({ action: 'join-self', name, deviceId, kind: 'native', version: '1.30', os: 'linux' });
const out = (w, chat, off, obj) => { const b = Buffer.from(JSON.stringify(obj) + '\n'); w.send({ type: 'chat.out', chat, stream: 'stdout', off, b64: b.toString('base64') }); return off + b.length; };
const stdinOf = (w, chat) => w.of('chat.in', chat).map(m => Buffer.from(m.b64, 'base64').toString('utf8')).join('');

const presence = [];
svc.workersApi.onChange((id, online) => presence.push([id, online]));

const k1 = joinPc('Big PC', 'dev-big');
const k2 = joinPc('No Claude', 'dev-nc');
const k3 = joinPc('Old PC', 'dev-old');
let big = fakeWorker(k1.key);
const nc = fakeWorker(k2.key, { tools: { git: '2.4' } });
const old = fakeWorker(k3.key, { caps: ['info', 'terminal', 'pause', 'jobs'] });
await big.ready; await nc.ready; await old.ready;
await until(() => big.of('welcome')[0] && nc.of('welcome')[0] && old.of('welcome')[0]);
ok(big.of('welcome')[0].caps.includes('chats'), 'welcome: the server offers chats');

// ── Refusals and the places offered ──
{
  const r1 = await ui.create('proj', { where: `worker:${k2.id}` });
  ok(!r1.ok && r1.error === CLAUDE_MISSING('No Claude') && /curl -fsSL https:\/\/claude\.ai\/install\.sh \| bash/.test(r1.error), 'a PC without claude: refused with how to install it', r1.error);
  const r2 = await ui.create('proj', { where: `worker:${k3.id}` });
  ok(!r2.ok && /older worker/.test(r2.error), 'an older worker (no chats cap) is never used', r2.error);
  svc.act({ action: 'set', id: k1.id, allowChats: false });
  const r3 = await ui.create('proj', { where: `worker:${k1.id}` });
  ok(!r3.ok && /does not allow chats/.test(r3.error), 'chats switched off for that PC: refused', r3.error);
  const pl = svc.act({ action: 'chat-places' });
  ok(pl.ok && pl.places.length === 0, 'and nothing is offered then', JSON.stringify(pl));
  svc.act({ action: 'set', id: k1.id, allowChats: true });
  const pl2 = svc.act({ action: 'chat-places' });
  ok(pl2.places.length === 1 && pl2.places[0].name === 'Big PC' && pl2.places[0].where === `worker:${k1.id}`, 'places: only the PC that is online, allows chats and has claude', JSON.stringify(pl2));
  const r4 = await ui.create('proj', { where: 'worker:wffffffffff' });
  ok(!r4.ok && /not joined/.test(r4.error), 'an unknown PC: refused', r4.error);
  const r5 = await ui.create('proj', { where: `worker:${k1.id}`, collaborator: { id: 'c1', name: 'Sam', key: 'x' } });
  ok(!r5.ok && /owner/.test(r5.error), 'a collaborator cannot run a chat on a PC', r5.error);
  const v = svc.list().workers.find(w => w.id === k1.id);
  ok(v.canRunChats === true && v.hasClaude === true && svc.list().workers.find(w => w.id === k3.id).canRunChats === false, 'the Workers view says which PCs can run chats');
}

// ── A chat on the PC, end to end ──
const created = await ui.create('proj', { where: `worker:${k1.id}`, title: 'On the PC' });
ok(created.ok, 'a chat on the PC is created', JSON.stringify(created));
const C = created.id;
const spawn = await until(() => big.of('chat.spawn', C)[0]);
ok(spawn && spawn.project === 'proj' && spawn.source === 'git' && spawn.git.url === 'https://github.com/someone/proj.git' && spawn.git.commit === headSha && spawn.git.branch === 'main', 'chat.spawn: the git source (https remote, the server\'s commit and branch)', JSON.stringify(spawn && spawn.git));
ok(spawn && spawn.args.includes('--input-format') && spawn.args.includes('stream-json') && spawn.args.includes('--permission-prompt-tool') && !spawn.args.includes('--mcp-config') && !spawn.args.includes('--add-dir'), 'chat.spawn: claude\'s stream-json args, no server-side --mcp-config', JSON.stringify(spawn && spawn.args));
ok(spawn && spawn.env.CRUNDI_CHAT_ID === C && !spawn.env.CRUNDI_API_KEY && !spawn.env.CRUNDI_CHAT_TOKEN && !spawn.env.CRUNDI_API_URL, 'chat.spawn env: the chat\'s id, never the API key or the chat token', JSON.stringify(spawn && spawn.env));
ok(!existsSync(join(proj, '.mcp.json')), 'no .mcp.json is written into the server\'s checkout for it');
await sleep(100);
ok(big.of('chat.in', C).length === 0, 'stdin waits until the PC says the chat started');
big.send({ type: 'chat.started', chat: C, pid: 4242 });
const init = await until(() => { const t = stdinOf(big, C); return t.includes('"subtype":"initialize"') ? t : null; });
ok(!!init, 'then the initialize request reaches the PC\'s claude', stdinOf(big, C));
const initReq = JSON.parse(init.split('\n')[0]).request_id;
let off = 0;
off = out(big, C, off, { type: 'control_response', response: { subtype: 'success', request_id: initReq, response: { commands: [], models: [] } } });
off = out(big, C, off, { type: 'system', subtype: 'init', session_id: '11111111-2222-3333-4444-555555555555', model: 'opus', tools: [] });
off = out(big, C, off, { type: 'assistant', message: { id: 'm1', role: 'assistant', content: [{ type: 'text', text: 'Hello from the PC' }] }, parent_tool_use_id: null });
const said = await until(() => (ui.history(C)?.messages || []).find(m => m.kind === 'assistant-text' && /Hello from the PC/.test(m.text)));
ok(!!said, 'the PC\'s stream-json becomes the chat\'s messages');
ok(ui.history(C).sessionId === '11111111-2222-3333-4444-555555555555', 'its session id comes from the PC\'s claude');
{
  const l = ui.list().find(x => x.id === C);
  ok(l && l.where === `worker:${k1.id}` && l.pc && l.pc.name === 'Big PC' && l.pc.online === true && l.status === 'running', 'the chat list says where it runs (PC name, online)', JSON.stringify(l && l.pc));
  ok(ui.meta(C).where === `worker:${k1.id}`, 'and so does its meta');
}
ui.sendMessage(C, 'make the thing');
ok(!!(await until(() => /make the thing/.test(stdinOf(big, C)))), 'a message goes to the PC\'s claude');

// ── The PC goes away and comes back ──
const metaEvents = [];
ui.on(C, (e) => { if (e.type === 'meta' && e.pc) metaEvents.push(e.pc); });
big.ws.close();
await until(() => ui.list().find(x => x.id === C).pc.online === false);
ok(ui.list().find(x => x.id === C).pc.online === false && ui.list().find(x => x.id === C).status === 'running', 'the PC goes away: the chat shows its PC offline, still running there');
ui.remoteChanged();
ok(metaEvents.some(p => p.online === false), 'the open chat is told (meta: pc offline)');
ok(presence.some(([id, o]) => id === k1.id && o === false), 'workersApi.onChange hears the PC go');
const inBefore = big.of('chat.in', C).length;
ui.sendMessage(C, 'while you were away');
await sleep(50);
big = fakeWorker(k1.key, { chats: [{ chat: C, running: true, outBytes: off + 40 }, { chat: 'feedfacefeedface', running: true, outBytes: 0 }] });
await big.ready;
const att = await until(() => big.of('chat.attach', C)[0]);
ok(att && att.from === off, 'back: the server asks for stdout from the byte it stopped at', JSON.stringify(att));
ok(!!(await until(() => /while you were away/.test(stdinOf(big, C)))), 'what was sent while it was away is delivered now', String(inBefore));
ok(!!(await until(() => big.of('chat.kill', 'feedfacefeedface')[0])), 'a chat the server does not know is stopped on the PC');
ok(presence.some(([id, o]) => id === k1.id && o === true), 'workersApi.onChange hears it come back');
// The PC resends from an earlier byte: nothing is doubled.
{
  const before = ui.history(C).messages.filter(m => m.kind === 'assistant-text').length;
  const line = Buffer.from(JSON.stringify({ type: 'assistant', message: { id: 'm2', role: 'assistant', content: [{ type: 'text', text: 'Second, after the gap' }] }, parent_tool_use_id: null }) + '\n');
  const prevLine = Buffer.from(JSON.stringify({ type: 'assistant', message: { id: 'm1', role: 'assistant', content: [{ type: 'text', text: 'Hello from the PC' }] }, parent_tool_use_id: null }) + '\n');
  const both = Buffer.concat([prevLine, line]);
  big.send({ type: 'chat.out', chat: C, stream: 'stdout', off: off - prevLine.length, b64: both.toString('base64') });
  off += line.length;
  await until(() => ui.history(C).messages.some(m => /Second, after the gap/.test(m.text || '')));
  const after = ui.history(C).messages.filter(m => m.kind === 'assistant-text');
  ok(after.length === before + 1 && after.filter(m => /Hello from the PC/.test(m.text)).length === 1, 'resent output from an earlier byte: nothing doubled', JSON.stringify(after.map(m => m.text)));
}
ok(ui.list().find(x => x.id === C).pc.online === true, 'and the header shows the PC online again');

// ── The chat's tools: mcp.list, mcp.call ──
{
  big.send({ type: 'mcp.list', chat: C, req: 'r1' });
  const t = await until(() => big.of('mcp.tools').find(m => m.req === 'r1'));
  const names = (t && t.tools || []).map(x => x.name);
  ok(names.includes('send_message_to_user') && names.includes('show_image') && names.includes('secret_get') && names.includes('factory_start'), 'mcp.list: an owner chat\'s tools', names.length);
  ok(!names.includes('syntax_check') && !names.includes('request_owner_command') && !names.includes('factory_handoff'), 'mcp.list: not syntax_check (files are on the PC), no collaborator or run-only tools');
  big.send({ type: 'mcp.call', chat: C, req: 'r2', tool: 'kanban_list', args: {} });
  big.send({ type: 'mcp.call', chat: C, req: 'r3', tool: 'send_message_to_user', args: { message: 'hi', sessionId: 'someone-else' } });
  big.send({ type: 'mcp.call', chat: C, req: 'r4', tool: 'homepool_status', args: { project: 'other', chatToken: 'forged' } });
  const r2 = await until(() => big.of('mcp.result').find(m => m.req === 'r2'));
  const r3 = await until(() => big.of('mcp.result').find(m => m.req === 'r3'));
  const r4 = await until(() => big.of('mcp.result').find(m => m.req === 'r4'));
  ok(r2 && r2.result.ok && r2.result.tool === 'kanban_list' && r2.result.echo.alias === 'proj', 'mcp.call reaches /api/mcp/call: the project alias filled in, the JSON answer returned', JSON.stringify(r2));
  ok(r3 && r3.result.echo.sessionId === C, 'the session id is the chat\'s own, never the model\'s', JSON.stringify(r3 && r3.result));
  ok(r4 && r4.result.echo.chatToken === chatToken(C, 'proj') && r4.result.echo.project === 'proj' && r4.result.echo.sessionId === C, 'homepool: the chat\'s own project and token', JSON.stringify(r4 && r4.result));
  ok(mcpCalls.length >= 3 && mcpCalls.every(c => c.key === INTERNAL), 'the server calls itself with the internal key (it never goes to the PC)');
  const callsBefore = mcpCalls.length;
  big.send({ type: 'mcp.call', chat: C, req: 'r2', tool: 'kanban_list', args: {} });
  ok(!!(await until(() => big.of('mcp.result').filter(m => m.req === 'r2').length === 2)) && mcpCalls.length === callsBefore, 'a resent mcp.call (same req) is answered again without running the tool twice');
  big.send({ type: 'mcp.call', chat: 'nope', req: 'r5', tool: 'kanban_list', args: {} });
  const r5 = await until(() => big.of('mcp.result').find(m => m.req === 'r5'));
  ok(r5 && r5.result.ok === false && /not running on this PC/.test(r5.result.error), 'a call for a chat that is not this PC\'s is refused');
}

// ── The PC asks for code while preparing: chat.need -> chat.bundle, no req ──
{
  big.send({ type: 'chat.need', chat: C, kind: 'bundle', have: '' });
  const last = await until(() => big.of('chat.bundle', C).find(m => m.last), 8000);
  const bf = join(dir, 'need.bundle');
  if (last) writeFileSync(bf, Buffer.concat(big.of('chat.bundle', C).sort((a, b) => a.seq - b.seq).map(m => Buffer.from(m.b64, 'base64'))));
  let heads = '';
  try { heads = execFileSync('git', ['bundle', 'list-heads', bf], { cwd: proj, encoding: 'utf8' }); } catch { /* none */ }
  ok(last && !last.req && heads.includes(headSha), 'chat.need is answered with a git bundle as chat.bundle, no req', heads);
}

// ── The PC side of a pull: a real clone, commits on pc/<chat>, uncommitted work ──
const pcDir = join(dir, 'pc-clone');
execFileSync('git', ['clone', '-q', proj, pcDir]);
gitIn(pcDir, 'config', 'core.autocrlf', 'false');
gitIn(pcDir, 'checkout', '-q', '-b', `pc/${C}`, headSha);
writeFileSync(join(pcDir, 'a.txt'), 'one\nchanged on the PC\n');
writeFileSync(join(pcDir, 'new.txt'), 'made on the PC\n');
gitIn(pcDir, 'add', '.'); gitIn(pcDir, 'commit', '-qm', 'work on the PC');
const pcHead = gitIn(pcDir, 'rev-parse', 'HEAD');
writeFileSync(join(pcDir, 'b.txt'), 'two\nnot committed yet\n');
function answerPull(w, chat, { patchText } = {}) {
  return (async () => {
    const p = await until(() => w.of('chat.pull', chat).find(m => !m.answered), 8000);
    if (!p) return null;
    p.answered = true;
    const bf = join(dir, `b-${p.req}.bundle`);
    execFileSync('git', ['bundle', 'create', '-q', bf, `refs/heads/pc/${chat}`, `^${p.base}`], { cwd: pcDir, stdio: 'pipe' });
    const bytes = readFileSync(bf);
    const half = Math.floor(bytes.length / 2);
    w.send({ type: 'chat.bundle', chat, req: p.req, seq: 0, b64: bytes.subarray(0, half).toString('base64'), last: false });
    w.send({ type: 'chat.bundle', chat, req: p.req, seq: 1, b64: bytes.subarray(half).toString('base64'), last: true, head: pcHead });
    const patch = patchText != null ? Buffer.from(patchText) : execFileSync('git', ['diff', '--binary', 'HEAD'], { cwd: pcDir });
    w.send({ type: 'chat.patch', chat, req: p.req, b64: patch.toString('base64') });
    return p;
  })();
}

// ── A turn ends: the work comes back ──
{
  const answering = answerPull(big, C);
  off = out(big, C, off, { type: 'result', subtype: 'success', total_cost_usd: 0.01, duration_ms: 5, num_turns: 1 });
  const p = await answering;
  ok(p && p.base === headSha, 'a turn end sends chat.pull with the commit the chat started from', JSON.stringify(p));
  const wt = join(dir, 'data', 'workers', 'worktrees', 'proj', C);
  const br = pullBranch('Big PC', C);
  ok(br === `pc/big-pc/${C.slice(0, 8)}`, 'the branch is pc/<pc>/<chat>', br);
  const landed = await until(() => svc.chats.view(C)?.lastPull?.ok, 15000);
  ok(!!landed, 'the pull lands', JSON.stringify(svc.chats.view(C)));
  ok(existsSync(wt) && gitIn(wt, 'rev-parse', '--abbrev-ref', 'HEAD') === br && gitIn(wt, 'rev-parse', 'HEAD') === pcHead, 'the worktree is on that branch at the PC\'s head', existsSync(wt) ? gitIn(wt, 'log', '--oneline', '-3') : 'no worktree');
  ok(existsSync(wt) && readFileSync(join(wt, 'new.txt'), 'utf8') === 'made on the PC\n' && readFileSync(join(wt, 'b.txt'), 'utf8') === 'two\nnot committed yet\n' && gitIn(wt, 'status', '--porcelain') === 'M b.txt', 'committed work is committed, uncommitted work stays uncommitted', existsSync(wt) ? gitIn(wt, 'status', '--porcelain') : '');
  ok(g('rev-parse', 'HEAD') === headSha && g('status', '--porcelain') === '' && g('rev-parse', '--abbrev-ref', 'HEAD') === 'main', 'nothing is written into the server\'s main checkout', g('status', '--porcelain'));
  const l = ui.list().find(x => x.id === C);
  ok(l.pc.branch === br && l.pc.worktree === wt, 'the chat says where the branch is', JSON.stringify(l.pc));
}

// ── "Bring changes back" again: the uncommitted work changed meanwhile ──
{
  writeFileSync(join(pcDir, 'b.txt'), 'two\nchanged again\n');
  const answering = answerPull(big, C);
  const r = {};
  await svc.handleApi({ method: 'POST' }, null, '/api/workers', {
    principal: {}, isOwner: () => true,
    readBody: async () => JSON.stringify({ action: 'chat-pull', chat: C }),
    json: (_res, body) => { r.body = body; },
  });
  await answering;
  ok(r.body && r.body.ok && r.body.head === pcHead, 'Bring changes back (POST /api/workers chat-pull) answers with the head', JSON.stringify(r.body));
  const wt = join(dir, 'data', 'workers', 'worktrees', 'proj', C);
  ok(readFileSync(join(wt, 'b.txt'), 'utf8') === 'two\nchanged again\n' && gitIn(wt, 'status', '--porcelain') === 'M b.txt', 'the worktree follows the PC (the earlier patch replaced, not doubled)', gitIn(wt, 'status', '--porcelain'));
}

// ── pullChat (phase 5): into a run's worktree, onto its branch ──
{
  const runWt = join(dir, 'run-wt');
  g('worktree', 'add', '-q', '-b', 'factory/run-1', runWt, headSha);
  const answering = answerPull(big, C);
  const r = await svc.workersApi.chats.pullChat(C, { base: headSha, intoWorktree: runWt, branch: 'factory/run-1' });
  const p = await answering;
  ok(p && p.base === headSha, 'pullChat asks from the base it is given');
  ok(r.ok && r.head === pcHead, 'pullChat: ok with the head', JSON.stringify(r));
  ok(gitIn(runWt, 'rev-parse', 'HEAD') === pcHead && gitIn(runWt, 'rev-parse', '--abbrev-ref', 'HEAD') === 'factory/run-1' && gitIn(runWt, 'status', '--porcelain') === 'M b.txt', 'the PC\'s commits are on the run\'s branch, its uncommitted work applied uncommitted', gitIn(runWt, 'status', '--porcelain'));
  const bad = await svc.workersApi.chats.pullChat('0000000000000000', {});
  ok(!bad.ok && bad.error, 'pullChat for a chat not on a PC: an error, not a throw');
  ok(svc.workersApi.resolve('big pc')?.id === k1.id && svc.workersApi.resolve(k1.id)?.name === 'Big PC' && svc.workersApi.resolve('nobody') === null && svc.workersApi.isOnline(k1.id) === true, 'workersApi.resolve and isOnline');
  ok(Object.keys(svc.workersApi).sort().join() === 'chats,isOnline,onChange,resolve', 'workersApi has exactly chats, resolve, isOnline, onChange');
}

// ── A pull that fails on the PC ──
{
  const pulling = svc.chats.pull(C);
  const p = await until(() => big.of('chat.pull', C).find(m => !m.answered));
  p.answered = true;
  big.send({ type: 'chat.patch', chat: C, req: p.req, b64: '', error: 'git diff failed' });
  const r = await pulling;
  ok(!r.ok && /git diff failed/.test(r.error), 'an error on chat.patch fails the pull with it', JSON.stringify(r));
}

// ── Interrupt and close reach the PC ──
{
  ui.interrupt(C);
  ok(!!(await until(() => /"subtype":"interrupt"/.test(stdinOf(big, C)))), 'interrupt goes to the PC\'s claude (a control request)');
  ui.close(C);
  const k = await until(() => big.of('chat.kill', C)[0]);
  ok(k && k.signal === 'SIGTERM', 'closing the chat kills it on the PC', JSON.stringify(k));
  ok(big.of('chat.in', C).some(m => m.eof === true), 'and its stdin is closed there (chat.in eof)');
  big.send({ type: 'chat.exit', chat: C, code: 0 });
}

// ── A factory chat is not pulled at its turn end; it gets the run's tools ──
{
  const f = await ui.create('proj', { where: `worker:${k1.id}`, factory: { runId: 'run1', role: 'employee', employeeId: 'e1' }, extraEnv: { CRUNDI_FACTORY_RUN: 'run1' }, sessionMode: 'new' });
  ok(f.ok, 'a factory chat on the PC', JSON.stringify(f));
  big.send({ type: 'chat.started', chat: f.id, pid: 1 });
  const sp = await until(() => big.of('chat.spawn', f.id)[0]);
  ok(sp && sp.env.CRUNDI_FACTORY_RUN === 'run1', 'its env carries the run');
  big.send({ type: 'mcp.list', chat: f.id, req: 'f1' });
  const t = await until(() => big.of('mcp.tools').find(m => m.req === 'f1'));
  const names = t.tools.map(x => x.name);
  ok(names.includes('factory_handoff') && !names.includes('factory_start'), 'a run\'s chat is offered the run tools, not the building ones');
  out(big, f.id, 0, { type: 'result', subtype: 'success', num_turns: 1 });
  await sleep(300);
  ok(big.of('chat.pull', f.id).length === 0, 'its turn end does not pull (the run does, with pullChat)');
  ui.close(f.id);
}

// ── The PC refuses ──
{
  const r = await ui.create('proj', { where: `worker:${k1.id}`, sessionMode: 'new' });
  const exitP = new Promise((res) => ui.on(r.id, (e) => { if (e.type === 'exit') res(e); }));
  await until(() => big.of('chat.spawn', r.id)[0]);
  big.send({ type: 'chat.refused', chat: r.id, error: 'Chats are off on this PC.' });
  const e = await Promise.race([exitP, sleep(3000).then(() => null)]);
  ok(!!e && ui.list().find(x => x.id === r.id).status === 'exited', 'a refusal from the PC ends the chat');
  ok(ui.history(r.id).messages.some(m => m.kind === 'error' && /Chats are off on this PC/.test(m.text)), 'and says why in the chat');
  ui.close(r.id);
}

// ── The PC no longer knows a chat ──
{
  const r = await ui.create('proj', { where: `worker:${k1.id}`, sessionMode: 'new' });
  await until(() => big.of('chat.spawn', r.id)[0]);
  big.send({ type: 'chat.started', chat: r.id, pid: 3 });
  big.send({ type: 'chat.exit', chat: r.id, code: -1, unknown: true });
  await until(() => ui.list().find(x => x.id === r.id)?.status === 'exited');
  ok(ui.history(r.id).messages.some(m => m.kind === 'error' && /no longer had this chat/.test(m.text)), 'chat.exit unknown: the chat ends saying the PC no longer had it');
  ui.close(r.id);
}

// ── A copy-source chat (a project without git) ──
{
  const r = await ui.create('plain', { where: `worker:${k1.id}`, sessionMode: 'new' });
  ok(r.ok, 'a chat on a project without git', JSON.stringify(r));
  const sp = await until(() => big.of('chat.spawn', r.id)[0]);
  ok(sp && sp.source === 'copy' && !sp.git, 'chat.spawn: the copy source');
  big.send({ type: 'chat.manifest', chat: r.id, files: {} });
  const last = await until(() => big.of('chat.files', r.id).find(m => m.last), 8000);
  ok(last && !last.req && Array.isArray(last.removed), 'chat.manifest is answered with chat.files, no req (the copy as for a job)');
  const tarFile = join(dir, 'src.tar');
  writeFileSync(tarFile, Buffer.concat(big.of('chat.files', r.id).sort((a, b) => a.seq - b.seq).map(m => Buffer.from(m.b64, 'base64'))));
  ok(last && execFileSync('tar', ['-tf', tarFile], { encoding: 'utf8' }).split(/\r?\n/).filter(Boolean).join() === 'notes.txt', 'the PC asks for the files and gets them as a tar');
  big.send({ type: 'chat.started', chat: r.id, pid: 2 });
  // Bring it back: the changed files as a tar.
  const pcCopy = join(dir, 'pc-copy'); mkdirSync(pcCopy);
  writeFileSync(join(pcCopy, 'notes.txt'), 'plain, edited on the PC\n');
  const tf = join(dir, 'back.tar');
  execFileSync('tar', ['-cf', tf, '-C', pcCopy, 'notes.txt']);
  const pulling = svc.chats.pull(r.id);
  const p = await until(() => big.of('chat.pull', r.id)[0]);
  big.send({ type: 'chat.files', chat: r.id, req: p.req, seq: 0, b64: readFileSync(tf).toString('base64'), last: true, removed: ['old.txt'] });
  const res = await pulling;
  const back = join(plain, '.crundi', 'pc-changes', r.id);
  ok(res.ok && readFileSync(join(back, 'notes.txt'), 'utf8') === 'plain, edited on the PC\n' && readFileSync(join(back, 'removed.txt'), 'utf8') === 'old.txt\n', 'copy source without git: the files land in .crundi/pc-changes/<chat>/ with removed.txt', JSON.stringify(res));
  ok(readFileSync(join(plain, 'notes.txt'), 'utf8') === 'plain\n', 'the project\'s own file is untouched');
  ui.close(r.id);
}

// ── A notice from the PC in a chat: a line, never an error ──
{
  const r = await ui.create('proj', { where: `worker:${k1.id}`, sessionMode: 'resume', resumeId: '99999999-0000-0000-0000-000000000000' });
  ok(r.ok, 'a chat on the PC resuming a conversation the PC may not have', JSON.stringify(r));
  const sp = await until(() => big.of('chat.spawn', r.id)[0]);
  ok(sp && sp.resumeSessionId === '99999999-0000-0000-0000-000000000000', 'chat.spawn names the conversation to resume');
  big.send({ type: 'chat.started', chat: r.id, pid: 7 });
  big.send({ type: 'chat.notice', chat: r.id, text: 'This PC did not have that conversation; starting a new one.' });
  const n = await until(() => (ui.history(r.id)?.messages || []).find(m => m.kind === 'notice' && /did not have that conversation/.test(m.text)));
  ok(!!n && /Big PC/.test(n.text), 'chat.notice: shown in the chat as a notice line, with the PC\'s name', n && n.text);
  const init = await until(() => { const t = stdinOf(big, r.id); return t.includes('"subtype":"initialize"') ? t : null; });
  let o = 0;
  o = out(big, r.id, o, { type: 'control_response', response: { subtype: 'success', request_id: JSON.parse(init.split('\n')[0]).request_id, response: { commands: [], models: [] } } });
  o = out(big, r.id, o, { type: 'system', subtype: 'init', session_id: 'abcdefab-1111-2222-3333-444444444444', model: 'opus', tools: [] });
  await until(() => ui.history(r.id).sessionId === 'abcdefab-1111-2222-3333-444444444444');
  ok(ui.history(r.id).sessionId === 'abcdefab-1111-2222-3333-444444444444' && !ui.history(r.id).messages.some(m => m.kind === 'error'), 'starting fresh there is not an error: the new conversation\'s id is taken');
  ok(svc.chats.view(r.id).notice && ui.list().find(x => x.id === r.id).status === 'running', 'the chat carries on (and its record keeps the notice)');
  ui.close(r.id);
  await until(() => big.of('chat.kill', r.id).length);
}

// ── Terminal mode on a PC: claude-terminals.js through the pty adapter ──
{
  const { createClaudeTerminals } = await import('../src/claude-terminals.js');
  const terms = createClaudeTerminals({ apiUrl: base, apiKey: INTERNAL });
  const refusedFirst = await terms.create('proj', { where: `worker:${k1.id}` });
  ok(!refusedFirst.ok && /cannot run terminals on a PC/.test(refusedFirst.error), 'no launcher: a terminal on a PC is refused', refusedFirst.error);
  terms.setRemote({ launch: (w, o) => svc.chats.launchPty(w, o), status: (w, id) => svc.chats.status(w, id) });
  const states = []; terms.onRemoteState((id, st) => states.push(st));
  const metas = []; terms.onMeta((id) => metas.push(id));
  const bad = await terms.create('proj', { where: 'elsewhere' });
  ok(!bad.ok && /Unknown place/.test(bad.error), 'an unknown place is refused');
  const t = await terms.create('proj', { where: `worker:${k1.id}`, cols: 100, rows: 40, sessionMode: 'resume', resumeId: 'aaaa1111-bbbb-cccc-dddd-eeeeeeeeeeee', skipPermissions: true });
  ok(t.ok, 'Claude in a terminal on the PC is created', JSON.stringify(t));
  const T = t.id;
  const sp = await until(() => big.of('chat.spawn', T)[0]);
  ok(sp && sp.pty && sp.pty.cols === 100 && sp.pty.rows === 40 && !sp.command, 'chat.spawn with pty {cols, rows}: claude in a pseudo-terminal there', JSON.stringify(sp && sp.pty));
  ok(sp && sp.args[0] === '--resume' && sp.args.includes('aaaa1111-bbbb-cccc-dddd-eeeeeeeeeeee') && sp.resumeSessionId === 'aaaa1111-bbbb-cccc-dddd-eeeeeeeeeeee' && sp.args.includes('--dangerously-skip-permissions') && !sp.args.includes('--output-format') && !sp.args.includes('claude'), 'its args are claude\'s own (interactive, resume named), no shell, no stream-json', JSON.stringify(sp && sp.args).slice(0, 300));
  ok(sp && sp.source === 'git' && sp.git.commit === headSha, 'the same code preparation as a chat (git source, the server\'s commit)');
  ok(sp && sp.env.CRUNDI_CHAT_ID === T && !sp.env.CRUNDI_API_KEY && !sp.env.CRUNDI_TERMINAL_ID && !sp.env.CRUNDI_API_URL, 'env: the terminal\'s id as its chat id, no API key, no hook id', JSON.stringify(sp && sp.env));
  ok(!existsSync(join(proj, '.mcp.json')) && !existsSync(join(proj, '.claude', 'settings.local.json')), 'no .mcp.json or hooks config written into the server\'s checkout');
  let l = terms.list().find(x => x.id === T);
  ok(l && l.where === `worker:${k1.id}` && l.pc && l.pc.name === 'Big PC' && l.pc.online && l.status === 'running', 'the terminal list says where it runs', JSON.stringify(l));
  ok(terms.meta(T).where === `worker:${k1.id}`, 'its meta keeps where (a parked pane comes back there)');
  terms.write(T, 'early');
  await sleep(80);
  ok(big.of('chat.in', T).length === 0, 'keystrokes wait for chat.started');
  big.send({ type: 'chat.started', chat: T, pid: 99 });
  const typed = await until(() => { const x = stdinOf(big, T); return x.includes('early') ? x : null; });
  ok(!!typed, 'then they reach the PC (chat.in)');
  ok(await until(() => big.of('chat.resize', T).length === 0 ? true : null), 'no resize sent while the size is what it started with');
  terms.resize(T, 132, 50);
  const rz = await until(() => big.of('chat.resize', T)[0]);
  ok(rz && rz.cols === 132 && rz.rows === 50, 'resize: chat.resize {cols, rows}', JSON.stringify(rz));
  const seen = [];
  terms.on(T, (d) => seen.push(d));
  await sleep(700);   // past the echo window after the typing
  // A multi-byte character split across two chat.out messages.
  const bytes = Buffer.from('héllo \x1b[1mworld\x1b[0m\r\n', 'utf8');
  let toff = 0;
  for (const piece of [bytes.subarray(0, 2), bytes.subarray(2)]) { big.send({ type: 'chat.out', chat: T, stream: 'stdout', off: toff, b64: piece.toString('base64') }); toff += piece.length; }
  const got = await until(() => { const x = seen.join(''); return x.includes('héllo') ? x : null; });
  ok(!!got && !got.includes('�'), 'output: the terminal\'s bytes, a character split across messages kept whole', JSON.stringify(seen));
  ok(terms.getScrollback(T).includes('héllo'), 'and kept in the scrollback');
  ok(states.includes('working'), 'output flowing: working (its state comes from its output)', JSON.stringify(states));
  big.send({ type: 'chat.notice', chat: T, text: 'This PC did not have that conversation; starting a new one.' });
  await until(() => terms.list().find(x => x.id === T)?.notice);
  l = terms.list().find(x => x.id === T);
  ok(/did not have that conversation/.test(l.notice) && metas.includes(T) && /\[Crundi\] Big PC: This PC did not have/.test(seen.join('')), 'chat.notice: in the pane\'s bar (list notice) and as a line on its screen', JSON.stringify(l.notice));
  // The PC goes away and comes back: output picks up from the byte it stopped at.
  big.ws.close();
  await until(() => !svc.isOnline(k1.id));
  ok(terms.remoteChanged() >= 1 && terms.list().find(x => x.id === T).pc.online === false, 'the PC goes away: the pane shows it offline');
  terms.write(T, 'while-away');
  big = fakeWorker(k1.key, { chats: [{ chat: T, running: true, outBytes: toff + 5 }] });
  await big.ready;
  const at = await until(() => big.of('chat.attach', T)[0]);
  ok(at && at.from === toff, 'back: chat.attach from the byte the server has', JSON.stringify(at));
  const rz2 = await until(() => big.of('chat.resize', T)[0]);
  ok(rz2 && rz2.cols === 132 && rz2.rows === 50, 'back: its size is sent again (a resize may have been missed)');
  ok(!!(await until(() => stdinOf(big, T).includes('while-away'))), 'keystrokes typed while it was away are delivered');
  big.send({ type: 'chat.out', chat: T, stream: 'stdout', off: toff, b64: Buffer.from('again\r\n').toString('base64') });
  await until(() => seen.join('').includes('again'));
  ok(seen.join('').split('héllo').length === 2, 'nothing doubled after the attach');
  ok(terms.list().find(x => x.id === T).pc.online === true, 'and online again');
  await sleep(4300);
  ok(states[states.length - 1] === 'idle', 'quiet for a while: idle', JSON.stringify(states));
  // Bring changes back works for a terminal as for a chat.
  const pulling = svc.chats.pull(T);
  const pr = await until(() => big.of('chat.pull', T)[0]);
  big.send({ type: 'chat.patch', chat: T, req: pr.req, b64: '' });
  const pres = await pulling;
  ok(pres.ok && pres.branch === pullBranch('Big PC', T), 'Bring changes back: a terminal\'s work comes back like a chat\'s', JSON.stringify(pres));
  big.send({ type: 'chat.exit', chat: T, code: 0 });
  await until(() => terms.list().find(x => x.id === T)?.status === 'exited');
  ok(terms.list().find(x => x.id === T).status === 'exited' && /Process exited with code 0/.test(seen.join('')) && states[states.length - 1] === 'idle', 'chat.exit: the terminal ends');
  terms.close(T);
  // Empty Shell on a PC: the PC's login shell, the same way.
  const sh = await terms.create('proj', { where: `worker:${k1.id}`, shell: true, cols: 90, rows: 30 });
  ok(sh.ok, 'an Empty Shell on the PC', JSON.stringify(sh));
  const ssp = await until(() => big.of('chat.spawn', sh.id)[0]);
  ok(ssp && ssp.command === 'shell' && ssp.args.length === 0 && ssp.pty && ssp.pty.cols === 90 && ssp.source === 'git', 'chat.spawn command "shell": a shell there, no claude args, the same folder preparation', JSON.stringify(ssp && { command: ssp.command, args: ssp.args, pty: ssp.pty }));
  big.send({ type: 'chat.started', chat: sh.id, pid: 5 });
  await sleep(50);
  terms.close(sh.id);
  ok(!!(await until(() => big.of('chat.kill', sh.id)[0])), 'closing the pane ends it on the PC (chat.kill)');
  const ex = await terms.create('proj', { where: `worker:${k1.id}`, command: 'ls' });
  ok(!ex.ok, 'a raw command is not a terminal on a PC (worker_run is)');
}

// ── Offline: refused, and a chat whose PC is away is closed here and stopped there later ──
{
  big.ws.close();
  await until(() => !svc.isOnline(k1.id));
  const r = await ui.create('proj', { where: `worker:${k1.id}` });
  ok(!r.ok && /offline/.test(r.error), 'an offline PC: refused', r.error);
}

ui.closeAll();
svc.dispose();
for (const w of [nc, old]) { try { w.ws.close(); } catch { /* gone */ } }
server.close();
await sleep(50);
try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
