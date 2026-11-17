// The GPUI client's memory (Linux): PSS summed (smaps_rollup) over the app's
// process tree, which holds the browser host's: idle (signed in on a seeded
// test server, project demo, 40 s), then the drawer open on google.com, then
// a tab more on each further site (TABS, default wikipedia.org, github.com),
// each settled 40 s. Per process, and whether the app has libcef mapped
// (never). Tabs behind the active one follow the unload rule
// (CRUNDI_BROWSER_UNLOAD_SECS, passed through: set it high to keep them).
//
//   native-gpui/tools/test-server.sh 4392          (seeded: session.json)
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a -s "-screen 0 1440x900x24" \
//     node native-gpui/tools/pss.mjs <bin> http://localhost:4392 /tmp/crundi-devtest-4392/session.json [driver port, 4394]
//
// Measure a release build (tools/build.sh --release, about 5 GB; delete
// $CARGO_TARGET_DIR/release under the build lock afterwards). Electron's
// PC client, the same states: 417 MB idle, 473 MB with the tab.
import net from 'node:net';
import { spawn } from 'node:child_process';
import { readFileSync, readdirSync, mkdirSync, rmSync, openSync } from 'node:fs';
import { totpNow } from '../../src/auth-password.js';

const [BIN, SERVER, SESSION] = process.argv.slice(2);
const sess = JSON.parse(readFileSync(SESSION, 'utf8'));
const DPORT = Number(process.argv[5] || 4394);
const base = `/tmp/crundi-gpui-pss-${DPORT}`;
rmSync(base, { recursive: true, force: true });
mkdirSync(base, { recursive: true });
const log = openSync(`${base}/app.log`, 'w');
const app = spawn(BIN, [], { env: { ...process.env, CRUNDI_TEST_PORT: String(DPORT), CRUNDI_CONFIG_DIR: `${base}/config`, CRUNDI_CEF_CACHE: `${base}/cef` }, stdio: ['ignore', log, log] });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function driver(port) {
  return new Promise((resolve, reject) => {
    const go = (left) => {
      const sock = net.connect(port, '127.0.0.1');
      sock.once('connect', () => {
        let buf = '';
        const waiting = [];
        sock.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); waiting.shift()?.(JSON.parse(l)); } });
        resolve({ send: (c) => new Promise((r) => { waiting.push(r); sock.write(JSON.stringify(c) + '\n'); }), close: () => sock.end() });
      });
      sock.once('error', () => (left > 0 ? setTimeout(() => go(left - 1), 500) : reject(new Error('no driver'))));
    };
    go(120);
  });
}

const procs = () => {
  const out = new Map();
  for (const n of readdirSync('/proc')) {
    if (!/^\d+$/.test(n)) continue;
    try { const st = readFileSync(`/proc/${n}/stat`, 'utf8'); const r = st.slice(st.lastIndexOf(')') + 2).split(' '); out.set(+n, +r[1]); } catch { /* gone */ }
  }
  return out;
};
const tree = (pid) => { const all = procs(); const f = [pid]; for (let k = 0; k < f.length; k++) for (const [c, pp] of all) if (pp === f[k] && !f.includes(c)) f.push(c); return f; };
const pss = (pid) => { try { return +(/^Pss:\s+(\d+)/m.exec(readFileSync(`/proc/${pid}/smaps_rollup`, 'utf8'))[1]); } catch { return 0; } };
const label = (pid) => {
  try {
    const a = readFileSync(`/proc/${pid}/cmdline`, 'utf8').split('\0');
    const exe = a[0].split('/').pop();
    const type = a.find((x) => x.startsWith('--type='))?.slice(7);
    const sub = a.find((x) => x.startsWith('--utility-sub-type='))?.slice(19);
    return type ? `${exe} ${type}${sub ? ' (' + sub.split('.').pop() + ')' : ''}` : (a.includes('--browser-host') ? `${exe} (host)` : exe);
  } catch { return '?'; }
};
const libcef = (pid) => { try { return readFileSync(`/proc/${pid}/maps`, 'utf8').includes('libcef'); } catch { return null; } };
function measure(name) {
  const rows = tree(app.pid).map((p) => ({ pid: p, what: label(p), mb: pss(p) / 1024 }));
  const total = rows.reduce((s, r) => s + r.mb, 0);
  console.log(`\n== ${name}: total ${total.toFixed(1)} MB PSS over ${rows.length} processes; app maps libcef: ${libcef(app.pid)}`);
  for (const r of rows) console.log(`  ${r.mb.toFixed(1).padStart(7)} MB  ${r.pid}  ${r.what}`);
  return { total, rows };
}

const d = await driver(DPORT);
const ui = (name, ...args) => d.send({ cmd: 'ui', name, args });
const props = () => d.send({ cmd: 'props' });
const until = async (what, f, ms = 60000) => { const end = Date.now() + ms; while (Date.now() < end) { const p = await props(); try { if (f(p)) return p; } catch { /* */ } await sleep(500); } throw new Error('timeout: ' + what); };
await ui('sign-in-password', SERVER, sess.password, totpNow(sess.secret));
await until('signed in', (p) => p.signedIn);
await ui('select-project', 'demo');
await until('project', (p) => p.currentProject === 'demo');
await sleep(40000);
const idle = measure('idle (signed in, project demo, 40 s)');
await ui('desktop:drawer', 'true');
await sleep(40000);
const tab = measure('drawer open on google.com (40 s)');
const steps = [idle.total, tab.total];
const more = (process.env.TABS ?? 'https://www.wikipedia.org,https://github.com').split(',').filter(Boolean);
for (const [n, u] of more.entries()) {
  await ui('desktop:new-tab', u);
  await sleep(40000);
  steps.push(measure(`${n + 2} tabs, the newest on ${u} (40 s)`).total);
}
const ap = (await d.send({ cmd: 'area', area: 'desktop' })).props;
const host = (ap?.areas?.desktop ?? ap)?.browserHost;
console.log('\nbrowserHost:', JSON.stringify(host));
console.log(`\nRESULT ${steps.map((x, i) => `${i === 0 ? 'idle' : i + 'tab'}=${x.toFixed(0)}`).join(' ')} (Electron 417 idle / 473 one tab)`);
d.close();
app.kill('SIGTERM');
await sleep(3000);
try { process.kill(app.pid, 'SIGKILL'); } catch { /* gone */ }
rmSync(base, { recursive: true, force: true });
process.exit(0);
