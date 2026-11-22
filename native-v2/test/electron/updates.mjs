// ─── The updater (app/main.js 955-1450) ───
// A GitHub of the test's own (the releases API and the release files,
// electron-updater's feed format), the client told it is 8.0.0. Production:
// the latest release; Dev: the newest of either kind carrying the feed file.
// The check 8 s after launch, "Download & Install / Later" once per version,
// the download (sha512 checked), "Restart & Install / Not now", and the Linux
// install: sudo -n refused, then pkexec (the terminal offer, the prompt
// closed: 126), then sudo -n allowed: installed and started again.

import http from 'node:http';
import { createHash, randomBytes } from 'node:crypto';
import { mkdtempSync, writeFileSync, readFileSync, existsSync, chmodSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const FEEDS = ['client-linux.yml', 'latest-linux.yml'];
const PAYLOAD = { '9.0.0': randomBytes(300 * 1024), '9.1.0-dev.2': randomBytes(400 * 1024) };
const sha = (b) => createHash('sha512').update(b).digest('base64');
const deb = (v) => `Crundi-Client-${v}-linux-x64.deb`;
const hits = [];

export async function setup(ctx) {
  const srv = http.createServer((req, res) => {
    hits.push(req.url);
    const send = (code, body, type = 'application/json') => { res.writeHead(code, { 'Content-Type': type, 'Content-Length': Buffer.byteLength(body) }); res.end(body); };
    const assets = FEEDS.map((name) => ({ name }));
    if (req.url === '/repos/TheRealFaruhaan/crundi/releases/latest') return send(200, JSON.stringify({ tag_name: 'v9.0.0', prerelease: false, draft: false, assets }));
    if (req.url.startsWith('/repos/TheRealFaruhaan/crundi/releases?per_page=30')) return send(200, JSON.stringify([
      { tag_name: 'v9.2.0-dev.1', prerelease: true, draft: true, assets },
      { tag_name: 'v9.1.0-dev.2', prerelease: true, draft: false, assets },
      { tag_name: 'v9.1.0-dev.9', prerelease: true, draft: false, assets: [{ name: 'latest.yml' }] },
      { tag_name: 'v9.0.0', prerelease: false, draft: false, assets },
    ]));
    const m = req.url.match(/^\/TheRealFaruhaan\/crundi\/releases\/download\/v([^/]+)\/(.+)$/);
    if (m && PAYLOAD[m[1]]) {
      const v = m[1], b = PAYLOAD[v];
      if (FEEDS.includes(m[2])) return send(200, `version: ${v}\nfiles:\n  - url: Crundi-Client-${v}-linux-x64.AppImage\n    sha512: nope\n    size: 1\n  - url: ${deb(v)}\n    sha512: ${sha(b)}\n    size: ${b.length}\npath: Crundi-Client-${v}-linux-x64.AppImage\nsha512: nope\nreleaseDate: '2026-11-20T10:00:00.000Z'\n`, 'text/yaml');
      if (m[2] === deb(v)) { res.writeHead(200, { 'Content-Length': b.length }); return res.end(b); }
    }
    send(404, '{}');
  });
  await new Promise((r) => srv.listen(0, '127.0.0.1', r));
  srv.unref();
  ctx.gh = srv;
  process.env.CRUNDI_UPDATE_API = `http://127.0.0.1:${srv.address().port}`;
  process.env.CRUNDI_TEST_VERSION = '8.0.0';
  process.env.CRUNDI_TEST_PKEXEC_OFFER_MS = '1500';
  // sudo and pkexec of the test's own: they record what they were asked, never install.
  const bin = mkdtempSync(join(tmpdir(), 'crundi-v2-electron-bin-'));
  ctx.drop.push(bin);
  ctx.bin = bin;
  writeFileSync(join(bin, 'sudo'), '#!/bin/sh\nD=$(dirname "$0")\nprintf "%s\\n" "$*" >> "$D/sudo.log"\n[ -f "$D/sudo-ok" ] || exit 1\nexit 0\n');
  writeFileSync(join(bin, 'pkexec'), '#!/bin/sh\nD=$(dirname "$0")\nprintf "%s\\n" "$*" >> "$D/pkexec.log"\nsleep 4\nexit 126\n');
  chmodSync(join(bin, 'sudo'), 0o755); chmodSync(join(bin, 'pkexec'), 0o755);
  process.env.CRUNDI_TEST_PKEXEC = join(bin, 'pkexec');
  process.env.PATH = `${bin}:${process.env.PATH}`;
}

export async function run(T, t, ctx) {
  const D = T.D;
  const U = (p) => D(p).updater || {};
  const st = (p) => p.areas.ops.settings.desktop;
  const tray = (p) => (D(p).tray?.menu || []).find((m) => m.id === 'update') || {};
  let p = await T.state();
  t.check('updates: on by default, production, this build 8.0.0, the client\'s Linux feed', U(p).enabled === true && U(p).channel === 'production' && U(p).current === '8.0.0' && U(p).feed === 'client-linux.yml', U(p));
  t.check('updates: the tray says "Check for Updates…"', tray(p).label === 'Check for Updates…', tray(p));

  // The automatic check, 8 s after launch (the suite signed in before it).
  p = await T.until('updates: 8 s after launch, "Crundi 9.0.0 is available." asks first', (p) => p.dialog?.title === 'Update available' && /^Crundi 9\.0\.0 is available\./.test(p.dialog.message), 30000);
  t.check('updates: …its buttons, Download & Install the default, Later the cancel', JSON.stringify(p.dialog?.buttons) === JSON.stringify(['Download & Install', 'Later']) && p.dialog.ok === 'Download & Install' && p.dialog.cancel === 'Later', p.dialog);
  t.check('updates: …the latest release asked, never the prereleases', hits.includes('/repos/TheRealFaruhaan/crundi/releases/latest') && !hits.some((h) => h.includes('per_page')), hits);
  t.check('updates: …the next check a day away', U(p).nextCheckMs > 23 * 3600e3, U(p).nextCheckMs);
  await T.snap('electron', 'update-available-prompt');
  await T.key('Escape');
  p = await T.until('updates: Esc is Later: closed, the update offered in Settings', (p) => !p.dialog && st(p).status === 'Update available — v9.0.0' && st(p).dot === true);
  await T.act('update-check');
  await t.sleep(2500);
  p = await T.state();
  t.check('updates: a second check of the same version asks no more (once per version)', !p.dialog && U(p).prompted === '9.0.0', p.dialog);

  // Dev: the newest release of either kind with the feed file (not the draft, not one without it).
  await T.ops('settings-do', 'upd-set-channel', '1');
  await T.until('updates: Dev asks first', (p) => /the desktop app/.test(p.dialog?.message || ''));
  await T.key('Enter');
  p = await T.until('updates: Dev offers the newest dev build, 9.1.0-dev.2', (p) => p.dialog?.title === 'Update available' && /^Dev build Crundi 9\.1\.0-dev\.2 is available\./.test(p.dialog.message), 15000);
  const cfg = JSON.parse(readFileSync(join(T.configDir, 'update-config.json'), 'utf8'));
  t.check('updates: the channel saved in update-config.json, as Electron\'s', cfg.channel === 'dev', cfg);
  await T.key('Enter');
  p = await T.until('updates: Enter is Download & Install: downloaded, then "ready to install"', (p) => p.dialog?.title === 'Install Crundi 9.1.0-dev.2' && /^Crundi 9\.1\.0-dev\.2 is ready to install\./.test(p.dialog.message), 30000);
  t.check('updates: …its buttons, Not now the default', JSON.stringify(p.dialog?.buttons) === JSON.stringify(['Restart & Install', 'Not now']) && p.dialog.ok === 'Not now', p.dialog);
  const file = U(p).file || '';
  t.check('updates: the .deb downloaded whole, its sha512 as the feed says', file.endsWith(deb('9.1.0-dev.2')) && existsSync(file) && sha(readFileSync(file)) === sha(PAYLOAD['9.1.0-dev.2']), file);
  t.check('updates: Settings says it is ready; the tray "Restart to Install Update"', st(p).status === 'Dev build available — v9.1.0-dev.2 (ready)' && st(p).downloaded === true && tray(p).label === 'Restart to Install Update', [st(p), tray(p)]);
  await T.snap('electron', 'update-ready-prompt');
  await T.key('Enter');
  p = await T.until('updates: Enter is Not now', (p) => !p.dialog);

  ctx.updFile = file;
}

/** The install (the "install" part, last: it quits the app). */
export async function runInstall(T, t, ctx) {
  const D = T.D;
  const U = (p) => D(p).updater || {};
  const file = ctx.updFile || '';
  let p;
  // Install: sudo -n refused, so pkexec; no password prompt in 1.5 s (20 s in the app): the terminal offer.
  await T.act('tray-pick', 'update');
  await T.until('updates: the tray item asks to install', (p) => p.dialog?.title === 'Install Crundi 9.1.0-dev.2');
  await T.act('dialog-pick', 'Restart & Install');
  p = await T.until('updates: pkexec waits: "No password prompt?" offers the terminal or the command', (p) => p.dialog?.title === 'No password prompt?' && (p.dialog.buttons || []).includes('Copy the install command') && p.dialog.cancel === 'Keep waiting', 15000);
  const pk = existsSync(join(ctx.bin, 'pkexec.log')) ? readFileSync(join(ctx.bin, 'pkexec.log'), 'utf8') : '';
  t.check('updates: …pkexec runs dpkg -i on the file, apt-get -f after', pk.includes(`/bin/sh -c dpkg -i "$1" || apt-get install -f -y crundi-update ${file}`), pk);
  t.check('updates: …after sudo -n true was refused', readFileSync(join(ctx.bin, 'sudo.log'), 'utf8').startsWith('-n true'));
  await T.snap('electron', 'update-no-password-prompt');
  await T.key('Escape');
  p = await T.until('updates: Keep waiting; the prompt closed (126): "The update was not installed."', (p) => p.dialog?.title === 'The update was not installed.', 15000);
  await T.key('Enter');
  await T.until('updates: …OK, and the app carries on', (p) => !p.dialog && !T.exited);

  // sudo without a password: installed in the background, then started again.
  writeFileSync(join(ctx.bin, 'sudo-ok'), '');
  const pid = U(await T.state()).pid;
  await T.act('tray-pick', 'update');
  await T.until('updates: asked again', (p) => p.dialog?.title === 'Install Crundi 9.1.0-dev.2');
  await T.act('dialog-pick', 'Restart & Install');
  const end = Date.now() + 20000;
  while (!T.exited && Date.now() < end) await t.sleep(200);
  t.check('updates: installed with sudo -n, the app quit', T.exited && readFileSync(join(ctx.bin, 'sudo.log'), 'utf8').includes(`-n /bin/sh -c dpkg -i "$1" || apt-get install -f -y crundi-update ${file}`), readFileSync(join(ctx.bin, 'sudo.log'), 'utf8'));
  // The new copy: the driver answers again, from another process.
  let d2 = null;
  try { d2 = await t.driver(Number(process.env.CRUNDI_TEST_PORT), () => true); } catch { /* none */ }
  const s2 = d2 ? (await d2.send({ cmd: 'state' })).state : null;
  const pid2 = s2?.areas?.desktop?.updater?.pid;
  t.check('updates: …and started again (a new process)', !!pid2 && pid2 !== pid, { pid, pid2 });
  const log = readFileSync(join(T.configDir, 'crundi.log'), 'utf8');
  t.check('updates: crundi.log has the updater\'s lines', /\[update\] Available: 9\.0\.0/.test(log) && /\[update\] Installed; restarting/.test(log) && /\[update\] Relaunching/.test(log));
  try { d2?.close(); } catch { /* gone */ }
  if (pid2) { try { process.kill(pid2); } catch { /* gone */ } }
}
