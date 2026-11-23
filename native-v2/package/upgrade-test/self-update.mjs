#!/usr/bin/env node
// The native → native update, driven through the installed app's own updater
// (PARITY-ELECTRON.md line 54): a GitHub of the test's own serving the next
// version's Setup .exe and its feed (electron-updater's format), and the
// app's test driver pressing what the person presses.
//
//   node self-update.mjs serve <dir> <version> <feed.yml> <portfile>
//       the releases API (latest and ?per_page=30) with one release, v<version>,
//       and its files from <dir>; writes its port to <portfile>, runs until killed.
//   node self-update.mjs drive <driver port> <version>
//       waits for "Update available", presses Enter (Download & Install),
//       waits for "Install Crundi <version>", picks Restart & Install (its
//       button, as updates.mjs does), then waits for the app to go.
//
// The app runs with CRUNDI_TEST_PORT=<driver port> and CRUNDI_UPDATE_API=<the
// server>: updater.rs reads the API from there under the driver only.

import http from 'node:http';
import net from 'node:net';
import { createReadStream, existsSync, statSync, writeFileSync } from 'node:fs';
import { join, basename } from 'node:path';

const [cmd, ...a] = process.argv.slice(2);

if (cmd === 'serve') {
  const [dir, version, feed, portfile] = a;
  const tag = `v${version}`;
  const rel = { tag_name: tag, prerelease: version.includes('-'), draft: false, assets: [{ name: feed }] };
  const srv = http.createServer((req, res) => {
    console.log(`${req.method} ${req.url}`);
    const json = (b) => { const s = JSON.stringify(b); res.writeHead(200, { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(s) }); res.end(s); };
    if (req.url === '/repos/TheRealFaruhaan/crundi/releases/latest') return json(rel);
    if (req.url.startsWith('/repos/TheRealFaruhaan/crundi/releases?per_page=30')) return json([rel]);
    const m = req.url.match(/^\/TheRealFaruhaan\/crundi\/releases\/download\/([^/]+)\/([^/?]+)$/);
    const f = m && m[1] === tag && join(dir, basename(decodeURIComponent(m[2])));
    if (f && existsSync(f)) {
      res.writeHead(200, { 'Content-Length': statSync(f).size });
      return createReadStream(f).pipe(res);
    }
    res.writeHead(404); res.end('{}');
  });
  srv.listen(0, '127.0.0.1', () => {
    writeFileSync(portfile, String(srv.address().port));
    console.log(`fake GitHub on ${srv.address().port}: ${tag}, ${feed}`);
  });
} else if (cmd === 'drive') {
  const [port, version] = a;
  let sock, buf = '', waiting = [], closed = false;
  const connect = () => new Promise((res, rej) => {
    sock = net.connect(Number(port), '127.0.0.1');
    sock.once('connect', res); sock.once('error', rej);
    sock.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); waiting.shift()?.(JSON.parse(l)); } });
    sock.on('close', () => { closed = true; waiting.splice(0).forEach((w) => w(null)); });
  });
  const send = (c) => new Promise((res) => { if (closed) return res(null); waiting.push(res); sock.write(`${JSON.stringify(c)}\n`); });
  const dialog = async () => (await send({ cmd: 'state' }))?.state?.dialog || null;
  const until = async (what, f, ms) => {
    const end = Date.now() + ms;
    for (;;) {
      const d = await dialog();
      if (f(d)) { console.log(`ok    ${what}: ${JSON.stringify(d?.title)}`); return d; }
      if (Date.now() > end || closed) { console.log(`FAIL  ${what}: ${JSON.stringify(d)}`); process.exit(1); }
      await new Promise((r) => setTimeout(r, 500));
    }
  };
  for (let i = 0; ; i++) {
    try { await connect(); break; } catch (e) { if (i > 90) { console.log(`FAIL  the driver on :${port}: ${e.message}`); process.exit(1); } await new Promise((r) => setTimeout(r, 1000)); }
  }
  console.log(`driver on :${port}`);
  // A check now (the app's own is 8 s after launch; asks once per version).
  await send({ cmd: 'ui', name: 'update-due', args: [] });
  await until(`"Update available" offers ${version}`, (d) => d?.title === 'Update available' && d.message?.includes(`Crundi ${version}`) && /is available\./.test(d.message), 60000);
  await send({ cmd: 'key', text: 'Return', mods: [] });
  await until(`Enter is Download & Install: "Install Crundi ${version}"`, (d) => d?.title === `Install Crundi ${version}`, 180000);
  const r = await send({ cmd: 'ui', name: 'dialog-pick', args: ['Restart & Install'] });
  console.log(`Restart & Install: ${JSON.stringify(r)}`);
  // The app quits (and runs the Setup after): the driver's socket goes with it.
  const end = Date.now() + 60000;
  while (!closed && Date.now() < end) { await send({ cmd: 'state' }); await new Promise((r) => setTimeout(r, 500)); }
  console.log(closed ? 'ok    the app quit' : 'FAIL  the app is still up 60 s after Restart & Install');
  process.exit(closed ? 0 : 1);
} else {
  console.error('usage: self-update.mjs serve <dir> <version> <feed.yml> <portfile> | drive <port> <version>');
  process.exit(2);
}
