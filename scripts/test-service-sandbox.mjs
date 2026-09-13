// A service a collaborator registered runs in a sandbox.
//
// Approving `npm run dev` approves whatever the collaborator's package.json and
// dependencies run, so their services are confined like their chat: writes
// only in their worktree, the owner's projects and credentials hidden, no view
// of other processes — while still able to serve a local port.
//
// Real processes, isolated DATA_DIR / PROJECTS_DIR / worktrees. Skipped where
// the sandbox is not available (Windows, CI runners without bubblewrap).
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, readFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { execFileSync } from 'child_process';
import { createServer } from 'net';
import { request } from 'http';
import { fileURLToPath } from 'url';

const here = fileURLToPath(new URL('.', import.meta.url));
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};
const wait = (ms) => new Promise(r => setTimeout(r, ms));

const svcSrc = readFileSync(join(here, '..', 'src', 'services.js'), 'utf8');
check('collaborator services launch through their sandbox, or not at all',
  /const sandboxed = reg\.createdBy \? collaboratorServiceLaunch\(reg\) : null;\s*if \(sandboxed && !sandboxed\.ok\) return/.test(svcSrc));
check('a collaborator service is never stopped with its own stop command', /if \(entry\.stopCommand && !entry\.createdBy\)/.test(svcSrc));

const { sandboxStatus } = await import('../src/collab-sandbox.js');
if (process.platform !== 'linux' || !sandboxStatus().ok) {
  console.log('\n(sandbox not available here: skipping the live sandbox checks)');
} else {
  const T = mkdtempSync(join(tmpdir(), 'crundi-svcsb-'));
  process.env.DATA_DIR = join(T, 'data');
  process.env.PROJECTS_DIR = join(T, 'projects');
  process.env.COLLAB_WORKTREES_DIR = join(T, 'wt');
  process.env.TELEGRAM_BOT_TOKEN = '';
  mkdirSync(process.env.DATA_DIR, { recursive: true });
  const sh = (cwd, ...a) => execFileSync('git', a, { cwd, stdio: 'pipe' });
  const demo = join(process.env.PROJECTS_DIR, 'demo');
  const other = join(process.env.PROJECTS_DIR, 'other');
  for (const d of [demo, other]) {
    mkdirSync(d, { recursive: true });
    sh(d, 'init', '-q', '-b', 'main'); sh(d, 'config', 'user.email', 't@t'); sh(d, 'config', 'user.name', 't');
    writeFileSync(join(d, d === other ? 'secret.txt' : 'a.txt'), d === other ? 'OTHER-SECRET' : 'a');
    sh(d, 'add', '.'); sh(d, 'commit', '-qm', 'init');
  }
  const outside = join(T, 'outside');
  mkdirSync(outside);

  const port = await new Promise((res) => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });

  try {
    const col = await import('../src/collaborators.js');
    const reg = await import('../src/service-registry.js');
    const services = await import('../src/services.js');
    const inv = (await col.create({ name: 'Sam', project: 'demo', hours: 1, withPasscode: true })).collaborator;
    const key = 'pc:sam';
    const cmd = [
      'echo started',
      'echo hi > inside.txt && echo WROTE_INSIDE',
      `(touch ${outside}/escape.txt 2>/dev/null && echo ESCAPED) || echo WRITE_BLOCKED`,
      `(cat ${other}/secret.txt 2>/dev/null) || echo READ_BLOCKED`,
      'echo PROCS=$(ls /proc | grep -c "^[0-9]")',
      `exec python3 -m http.server ${port} --bind 127.0.0.1`,
    ].join('; ');
    const r = reg.registerService({ alias: 'demo', name: 'site', cwd: inv.worktreePath, command: cmd, createdBy: key });
    check('registered as the collaborator\'s', r.ok, JSON.stringify(r));
    const started = services.startService(r.key);
    check('started in the sandbox', started.ok, JSON.stringify(started));
    await wait(2500);
    const logs = services.getServiceLogs(r.key, 50).join('\n');
    check('writes inside the worktree work', /WROTE_INSIDE/.test(logs) && existsSync(join(inv.worktreePath, 'inside.txt')), logs);
    check('writes outside it are blocked', /WRITE_BLOCKED/.test(logs) && !existsSync(join(outside, 'escape.txt')), logs);
    check('other projects cannot be read', /READ_BLOCKED/.test(logs) && !/OTHER-SECRET/.test(logs), logs);
    const procs = Number((logs.match(/PROCS=(\d+)/) || [])[1] || 999);
    check('it cannot see the host\'s processes', procs < 10, `saw ${procs}`);
    const served = await new Promise((res) => {
      const q = request({ host: '127.0.0.1', port, path: '/inside.txt' }, (resp) => {
        let b = ''; resp.on('data', d => { b += d; }); resp.on('end', () => res({ code: resp.statusCode, body: b }));
      });
      q.on('error', (e) => res({ code: 0, body: e.message }));
      q.end();
    });
    check('it still serves a port the host can reach', served.code === 200 && /hi/.test(served.body), JSON.stringify(served));
    services.stopService(r.key);
    await wait(1000);

    // Access ended: it must not start.
    col.update(inv.id, { revoked: true });
    const again = services.startService(r.key);
    check('refused to start once their access has ended', !again.ok && /no longer has access/.test(again.error || ''), JSON.stringify(again));
    services.deleteService(r.key);
    await col.remove(inv.id);
  } finally {
    try { rmSync(T, { recursive: true, force: true }); } catch { /* temp */ }
    try { rmSync(T + '-cache', { recursive: true, force: true }); } catch { /* temp */ }
  }
}

if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nservice sandbox: all passed');
process.exit(0);
