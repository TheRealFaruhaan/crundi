#!/usr/bin/env node
/**
 * test-workers-real.mjs — the real crundi-worker binary (worker/, Rust)
 * against the real server side (src/workers.js), end to end.
 *
 * Skipped (passes) when no worker binary has been built: the Rust build is
 * not part of npm run check. Looks in CRUNDI_WORKER_BIN, worker/target, and
 * the shared target folder ~/.cache/crundi-native-target.
 *
 * Covers: join with a code; headless: online with real info and labels; a
 * terminal running a command; the owner's pause reaching it; revoke ending it
 * with exit 3; embedded mode (the PC client's): JSON status lines, online,
 * and exit when stdin closes; a whole chat on the PC (phase 4): a fake
 * `claude` on PATH, driven by claude-ui through the worker, calling a Crundi
 * tool through `crundi-worker mcp`, its work coming back as a branch.
 *
 * Run: node scripts/test-workers-real.mjs
 */
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, statSync, readFileSync, mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createWorkersService } from '../src/workers.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const exe = process.platform === 'win32' ? 'crundi-worker.exe' : 'crundi-worker';
const bin = [process.env.CRUNDI_WORKER_BIN, join(root, 'worker/target/release', exe), join(root, 'worker/target/debug', exe),
  join(homedir(), '.cache/crundi-native-target/release', exe), join(homedir(), '.cache/crundi-native-target/debug', exe)]
  .filter(Boolean).find(p => { try { return statSync(p).isFile(); } catch { return false; } });
if (!bin) { console.log('SKIP  no crundi-worker binary built (worker/); nothing to run'); process.exit(0); }

let failed = 0;
const ok = (c, l, extra = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c ? '' : '  ' + String(extra).slice(0, 400))); if (!c) failed++; };
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
async function until(fn, ms = 20000) { const end = Date.now() + ms; while (Date.now() < end) { const v = await fn(); if (v) return v; await sleep(50); } return null; }

const dir = mkdtempSync(join(tmpdir(), 'workers-real-'));

// A fake `claude` on PATH (before any worker starts: it probes for claude once).
// It answers --version, the initialize request, and for each message: calls a
// Crundi tool through the MCP server in its --mcp-config, commits a file,
// leaves one change uncommitted, and says what it saw.
const fakeBin = join(dir, 'fake-bin');
mkdirSync(fakeBin, { recursive: true });
if (process.platform !== 'win32') {
  writeFileSync(join(fakeBin, 'claude'), `#!/usr/bin/env node
const fs = require('fs'), cp = require('child_process'), rl = require('readline');
const args = process.argv.slice(2);
if (args.includes('--version')) { console.log('2.1.999 (Claude Code)'); process.exit(0); }
const out = (o) => process.stdout.write(JSON.stringify(o) + '\\n');
const cfg = JSON.parse(fs.readFileSync(args[args.indexOf('--mcp-config') + 1], 'utf8')).mcpServers.crundi;
function mcp() {
  return new Promise((resolve) => {
    const p = cp.spawn(cfg.command, cfg.args, { stdio: ['pipe', 'pipe', 'inherit'] });
    const got = {}; let buf = '';
    p.stdout.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); try { const m = JSON.parse(l); if (m.id) got[m.id] = m; } catch {} }
      if (got[2] && got[3]) { p.kill(); resolve(got); } });
    const w = (o) => p.stdin.write(JSON.stringify(o) + '\\n');
    w({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'fake', version: '1' } } });
    w({ jsonrpc: '2.0', method: 'notifications/initialized' });
    w({ jsonrpc: '2.0', id: 2, method: 'tools/list' });
    w({ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'kanban_list', arguments: {} } });
    setTimeout(() => resolve(got), 20000);
  });
}
rl.createInterface({ input: process.stdin }).on('line', async (line) => {
  let m; try { m = JSON.parse(line); } catch { return; }
  if (m.type === 'control_request' && m.request && m.request.subtype === 'initialize') { out({ type: 'control_response', response: { subtype: 'success', request_id: m.request_id, response: { commands: [] } } }); return; }
  if (m.type !== 'user') return;
  out({ type: 'system', subtype: 'init', session_id: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', tools: [] });
  const r = await mcp();
  const tools = ((r[2] || {}).result || {}).tools || [];
  const text = (((r[3] || {}).result || {}).content || [])[0];
  fs.writeFileSync('from-claude.txt', 'made by the PC claude\\n');
  const g = (...a) => cp.execFileSync('git', ['-c', 'user.name=pc', '-c', 'user.email=pc@pc', ...a]);
  let gitErr = '';
  try { g('add', 'from-claude.txt'); g('commit', '-qm', 'from the PC'); } catch (err) { gitErr = ' gitErr=' + String(err.message).split('\\n')[0] + ' ls=' + fs.readdirSync('.').join(','); }
  fs.appendFileSync('hello.txt', 'uncommitted on the PC\\n');
  out({ type: 'assistant', message: { id: 'a1', role: 'assistant', content: [{ type: 'text', text: 'tools=' + tools.length + ' cwd=' + process.cwd() + ' kanban=' + (text ? text.text : 'none').replace(/\\s+/g, ' ') + gitErr }] }, parent_tool_use_id: null });
  out({ type: 'result', subtype: 'success', num_turns: 1, total_cost_usd: 0 });
});
`, { mode: 0o755 });
  process.env.PATH = fakeBin + ':' + process.env.PATH;
}
// The PC "clones from GitHub": that URL is this test's own repository.
const fakeGh = 'https://github.com/someone/real-proj.git';
const projectsDir = join(dir, 'projects');
const realProj = join(projectsDir, 'proj');
const gitCfg = join(dir, 'gitconfig');
writeFileSync(gitCfg, `[url "file://${realProj}"]\n\tinsteadOf = ${fakeGh}\n[user]\n\tname = t\n\temail = t@t\n`);
process.env.GIT_CONFIG_GLOBAL = gitCfg;
process.env.DATA_DIR = join(dir, 'data');
process.env.PROJECTS_DIR = projectsDir;

const mcpCalls = [];
let url = '';
const svc = createWorkersService({ dataDir: dir, serverVersion: 'test', mcpApi: () => ({ url, key: 'k-test' }) });
const server = createServer(async (req, res) => {
  const path = new URL(req.url, 'http://x').pathname;
  if (path === '/api/mcp/call' && req.method === 'POST') {
    const parts = []; for await (const c of req) parts.push(c);
    const body = JSON.parse(Buffer.concat(parts).toString('utf8'));
    mcpCalls.push({ key: req.headers['x-api-key'], ...body });
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, board: 'stub', alias: body.args && body.args.alias }));
    return;
  }
  if (await svc.handlePublic(req, res, path)) return;
  res.writeHead(404); res.end();
});
server.on('upgrade', (req, socket, head) => { if (!svc.handleUpgrade(req, socket, head)) socket.destroy(); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
url = `http://127.0.0.1:${server.address().port}`;
const cfg = join(dir, 'config.json');
const work = join(dir, 'work');
console.log(`      binary: ${bin}`);

const procs = [];
function run(args, { stdin = 'ignore' } = {}) {
  const p = spawn(bin, args, { stdio: [stdin, 'pipe', 'pipe'], env: { ...process.env, RUST_LOG: 'warn' } });
  p.out = ''; p.err = ''; p.code = undefined;
  p.stdout.on('data', d => { p.out += d; });
  p.stderr.on('data', d => { p.err += d; });
  p.on('exit', (c) => { p.code = c; });
  procs.push(p);
  return p;
}
const live = () => svc.list().workers[0];

try {
  // ── Join ──
  const { code } = svc.act({ action: 'code' });
  const j = run(['--config', cfg, 'join', url, code, '--name', 'Real PC', '--work-dir', work]);
  await until(() => j.code !== undefined, 30000);
  ok(j.code === 0 && svc.list().workers.length === 1 && live().name === 'Real PC', 'join: the code works, the PC is listed', j.err + j.out);
  const saved = JSON.parse(readFileSync(cfg, 'utf8'));
  ok(/^cw_w[0-9a-f]{10}\./.test(saved.key || '') && saved.id === live().id, 'join: the key and id are saved in its config');

  // ── Headless ──
  const h = run(['--config', cfg, '--headless']);
  const w = await until(() => { const x = live(); return x.online && x.info ? x : null; }, 30000);
  ok(!!w, 'headless: online', h.err);
  if (w) {
    ok(w.info.cpu && w.info.cpu.threads > 0 && w.info.ramMb > 0 && w.info.os && w.info.workDir, 'real info: CPU, memory, OS, work folder', JSON.stringify(w.info));
    ok(w.labels.some(l => l.startsWith('os:')) && w.labels.some(l => l.startsWith('ram:')) && w.labels.includes('tool:git'), 'labels from what it has (git is on this machine)', w.labels.join(','));
    ok(w.kind === 'headless', 'kind: headless');

    // ── A terminal ──
    if (process.platform !== 'win32') {
      const toUi = [];
      const t = svc.uiTerminals((m) => toUi.push(m));
      t.open({ ref: 'r1', worker: w.id, cols: 100, rows: 30 });
      await sleep(500);
      t.input({ ref: 'r1', data: 'echo hello-$((6*7))\r' });
      const got = await until(() => toUi.filter(m => m.type === 'worker-term-data').map(m => m.data).join('').includes('hello-42'), 15000);
      ok(!!got, 'terminal: a command runs on the PC and its output comes back', JSON.stringify(toUi).slice(0, 300));
      t.input({ ref: 'r1', data: 'exit 5\r' });
      const ex = await until(() => toUi.find(m => m.type === 'worker-term-exit'), 15000);
      ok(ex && ex.code === 5, 'terminal: exit code back', JSON.stringify(ex));
    }

    // ── Pause from the owner ──
    svc.act({ action: 'set', id: w.id, paused: true });
    const pz = await until(() => live().info && live().info.paused, 15000);
    ok(!!pz, 'pause: the owner\'s pause reaches the PC (its info says paused)');
    svc.act({ action: 'set', id: w.id, paused: false });

    // ── Revoke ends it with exit 3 ──
    svc.act({ action: 'revoke', id: w.id });
    await until(() => h.code !== undefined, 20000);
    ok(h.code === 3, 'revoke: the headless worker stops with exit 3', `code ${h.code} ${h.err.slice(-300)}`);
  }

  // ── Embedded (the PC client's way) ──
  const s = svc.act({ action: 'join-self', name: 'Client PC', deviceId: 'dev-client', kind: 'electron', version: '1.21.8', os: process.platform });
  const e = run(['--embedded', '--server', url, '--key', s.key, '--work-dir', work, '--name', 'Client PC'], { stdin: 'pipe' });
  const online = await until(() => e.out.split('\n').some(l => { try { const j2 = JSON.parse(l); return j2.event === 'status' && j2.online === true; } catch { return false; } }), 30000);
  ok(!!online, 'embedded: JSON status lines on stdout, online', e.out.slice(-300) + e.err.slice(-300));
  ok(!!(await until(() => svc.list().workers.find(x => x.id === s.id && x.online))), 'embedded: listed online on the server');

  // ── Real jobs through the PC client's way (embedded) ──
  if (process.platform !== 'win32') {
    const proj = join(dir, 'proj');
    mkdirSync(join(proj, 'src'), { recursive: true });
    writeFileSync(join(proj, 'hello.txt'), 'hello from the server\n');
    writeFileSync(join(proj, 'src', 'keep.txt'), 'unchanged\n');
    const r = await svc.mcpRun({ alias: 'proj', command: 'cat hello.txt && mkdir -p out && printf abc > out/r.bin && echo "niceness=$(nice)" && echo "job=$CRUNDI_JOB"', artifacts: ['out/*.bin'], waitMinutes: 2 }, { projectPath: proj });
    ok(r.ok && r.state === 'done' && r.exitCode === 0 && r.source === 'copy', 'a real job: a copy of the project, run on the PC, exit 0', JSON.stringify(r).slice(0, 400));
    ok(/hello from the server/.test(r.tail || '') && /niceness=10/.test(r.tail || '') && new RegExp('job=' + r.job).test(r.tail || ''), 'its output: the copied file, low priority (nice 10), CRUNDI_JOB set', r.tail);
    const names = r.artifacts ? execFileSync('tar', ['-tzf', r.artifacts.path], { encoding: 'utf8' }).replace(/\r/g, '') : '';
    ok(r.artifacts && r.artifacts.count === 1 && /out\/r\.bin/.test(names), 'the file it made came back as a .tar.gz', names);
    writeFileSync(join(proj, 'hello.txt'), 'changed\n');
    const r2 = await svc.mcpRun({ alias: 'proj', command: 'cat hello.txt; ls out', waitMinutes: 2 }, { projectPath: proj });
    ok(r2.ok && r2.exitCode === 0 && /changed/.test(r2.tail) && r2.copy && r2.copy.sent === 1 && /r\.bin/.test(r2.tail), 'the second copy sends only the changed file, and what the last job built is still there (the cache)', JSON.stringify(r2.copy) + ' ' + r2.tail);
    const r3 = await svc.mcpRun({ alias: 'proj', command: 'sleep 30', wait: false }, { projectPath: proj });
    await until(() => svc.jobs.get(r3.job)?.state === 'running', 15000);
    await svc.mcpJob({ job: r3.job, action: 'cancel' });
    await until(() => svc.jobs.FINISHED.has(svc.jobs.get(r3.job)?.state), 15000);
    ok(svc.jobs.get(r3.job).state === 'cancelled', 'cancel stops a running job on the PC', svc.jobs.get(r3.job).state);
  }

  // ── A whole chat on the PC (phase 4), through claude-ui ──
  if (process.platform !== 'win32') {
    mkdirSync(realProj, { recursive: true });
    const g = (...a) => execFileSync('git', a, { cwd: realProj, encoding: 'utf8' }).trim();
    g('init', '-q', '-b', 'main');
    writeFileSync(join(realProj, 'hello.txt'), 'hello\n');
    g('add', '.'); g('commit', '-qm', 'first');
    g('remote', 'add', 'origin', fakeGh);
    const head = g('rev-parse', 'HEAD');
    const { createClaudeUiSessions } = await import('../src/claude-ui.js');
    const ui = createClaudeUiSessions({ apiUrl: url, apiKey: 'k-test' });
    ui.setRemote({ launch: (wh, o) => svc.chats.launch(wh, o), status: (wh, id) => svc.chats.status(wh, id) });
    ui.onTurnEnd((id) => svc.chats.onTurnEnd(id));
    const pc = await until(() => { const x = svc.list().workers.find(v => v.id === s.id); return x && x.canRunChats && x.hasClaude ? x : null; }, 20000);
    ok(!!pc, 'chats: the real worker offers chats and finds the fake claude on PATH', JSON.stringify(svc.list().workers.find(v => v.id === s.id)?.info?.tools));
    const c = await ui.create('proj', { where: `worker:${s.id}`, sessionMode: 'new' });
    ok(c.ok, 'chats: a chat starts on the real worker', JSON.stringify(c));
    if (c.ok) {
      await until(() => ui.meta(c.id)?.running && ui.history(c.id).messages.some(m => m.kind === 'notice' && !/Starting/.test(m.text)) || svc.chats.view(c.id)?.state === 'running', 30000);
      ui.sendMessage(c.id, 'do the thing');
      const said = await until(() => (ui.history(c.id)?.messages || []).find(m => m.kind === 'assistant-text' && /tools=/.test(m.text)), 60000);
      ok(!!said, 'chats: the PC\'s claude answers through the link', JSON.stringify((ui.history(c.id)?.messages || []).slice(-4)) + e.err.slice(-500));
      if (said) {
        const n = Number((/tools=(\d+)/.exec(said.text) || [])[1]);
        ok(n > 50, 'chats: its MCP server (crundi-worker mcp) lists the Crundi tools', said.text);
        ok(/board/.test(said.text) && mcpCalls.some(x => x.tool === 'kanban_list' && x.key === 'k-test' && x.args.alias === 'proj'), 'chats: a tool call reaches the server as that chat (project alias, internal key on the server only)', said.text);
        ok(new RegExp(`proj@${c.id}`).test(said.text), 'chats: a git chat runs in its own worktree on the PC (<project>@<chat>)', said.text);
      }
      const wt = join(dir, 'workers', 'worktrees', 'proj', c.id);
      const back = await until(() => svc.chats.view(c.id)?.lastPull, 60000);
      ok(back && back.ok && back.head && existsSync(join(wt, 'from-claude.txt')), 'chats: at the turn end the PC\'s commit comes back into a worktree', JSON.stringify(back));
      if (back && back.ok) {
        const st = execFileSync('git', ['status', '--porcelain'], { cwd: wt, encoding: 'utf8' }).trim();
        ok(st === 'M hello.txt' && readFileSync(join(wt, 'hello.txt'), 'utf8').includes('uncommitted on the PC'), 'chats: and its uncommitted work, uncommitted', st);
        ok(execFileSync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { cwd: wt, encoding: 'utf8' }).trim() === `pc/client-pc/${c.id.slice(0, 8)}`, 'chats: on branch pc/<pc>/<chat>');
        ok(g('rev-parse', 'HEAD') === head && g('status', '--porcelain') === '', 'chats: the server\'s checkout is untouched');
      }
      const exited = new Promise((r) => ui.on(c.id, (ev) => { if (ev.type === 'exit') r(true); }));
      ui.close(c.id);
      ok(await Promise.race([exited, sleep(15000).then(() => false)]) || svc.chats.view(c.id)?.state === 'ended', 'chats: closing it ends claude on the PC', JSON.stringify(svc.chats.view(c.id)));
    }
    ui.closeAll();
  }

  e.stdin.end();
  await until(() => e.code !== undefined, 15000);
  ok(e.code === 0, 'embedded: stdin closing ends it (exit 0)', `code ${e.code}`);
} finally {
  for (const p of procs) if (p.code === undefined) { try { p.kill(); } catch { /* gone */ } }
  svc.dispose();
  server.close();
  await sleep(200);
  try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
}
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
