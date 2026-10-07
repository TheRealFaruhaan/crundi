#!/usr/bin/env node
/**
 * Widgets: the store, the grant, the document that goes in the frame, the data
 * sources, and the one that matters most: a hostile widget cannot get out.
 *
 * The escape test loads a real frame in the headless browser. Without a
 * browser on the machine it is skipped, loudly, rather than passed.
 */
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, appendFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';

const tmp = mkdtempSync(join(tmpdir(), 'crundi-widgets-'));
const projects = join(tmp, 'projects');
mkdirSync(join(projects, 'demo'), { recursive: true });
writeFileSync(join(tmp, 'env'), '');
process.env.DATA_DIR = join(tmp, 'data');
process.env.PROJECTS_DIR = projects;
process.env.DOTENV_PATH = join(tmp, 'env');
mkdirSync(process.env.DATA_DIR, { recursive: true });

let failed = 0;
const ok = (cond, name, detail = '') => {
  if (cond) console.log(`PASS  ${name}`);
  else { failed++; console.log(`FAIL  ${name}${detail ? '\n      ' + detail : ''}`); }
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const store = await import('../src/widget-store.js');
const { buildDoc, CSP, SANDBOX } = await import('../src/widget-doc.js');
const { createWidgetSources, parseCsv, parseAs, summariseSession } = await import('../src/widget-sources.js');
const { fillTemplate, ACTION_TOOLS } = await import('../src/widget-api.js');

const A = 'demo';

// ─── Store ───
{
  ok(!store.upsert(A, 'Bad Id').ok, 'store: rejects an id with spaces or capitals');
  const r = store.upsert(A, 'one', { title: 'One', slot: 'dock', chat: 'abc123' });
  ok(r.ok && r.created && r.widget.slot === 'dock', 'store: creates a widget');
  ok(store.sourceDir(A, 'one') === join(projects, 'demo', '.crundi', 'widgets', 'one'), 'store: source lives in the project');
  ok(!store.upsert(A, 'one', { slot: 'floating' }).ok, 'store: rejects an unknown slot');

  store.setUser(A, 'one', { slot: 'cell' });
  ok(store.effectiveSlot(store.get(A, 'one')) === 'cell', 'placement: the person\'s move wins over the request');
  store.upsert(A, 'one', { slot: 'dock' });
  ok(store.effectiveSlot(store.get(A, 'one')) === 'cell', 'placement: re-opening in the same slot keeps the person\'s move');
  store.upsert(A, 'one', { slot: 'tab' });
  ok(store.effectiveSlot(store.get(A, 'one')) === 'tab', 'placement: asking for a different slot is a fresh request');

  store.setUser(A, 'one', { closed: true });
  ok(store.get(A, 'one').state === 'closed' && store.get(A, 'one').user.closed, 'placement: closing by the person is recorded');

  ok(store.push(A, 'one', 's', { value: { a: 1, b: { c: 2 } } }).ok, 'push: value');
  store.push(A, 'one', 's', { merge: { b: { d: 3 }, a: null } });
  ok(JSON.stringify(store.getPushed(A, 'one').s.value) === '{"b":{"c":2,"d":3}}', 'push: merge is deep and null deletes');
  for (let i = 0; i < 5; i++) store.push(A, 'one', 'feed', { append: i, max: 3 });
  ok(JSON.stringify(store.getPushed(A, 'one').feed.value) === '[2,3,4]', 'push: append keeps the last max');
  ok(!store.push(A, 'one', 'big', { value: 'x'.repeat(store.LIMITS.pushValue + 10) }).ok, 'push: refuses a value over the cap');

  store.addEvent(A, 'one', 'picked', { n: 1 });
  ok(store.drainEvents(A, 'one').length === 1 && store.drainEvents(A, 'one').length === 0, 'events: reading clears them');
}

// ─── Versions ───
{
  store.writeSource(A, 'one', { html: '<p>v1</p>', manifest: { title: 'One' } });
  const a = store.snapshot(A, 'one');
  const same = store.snapshot(A, 'one');
  store.writeSource(A, 'one', { html: '<p>v2</p>' });
  const b = store.snapshot(A, 'one');
  ok(a.changed && !same.changed && b.changed && b.version === a.version + 1, 'versions: one per real change');
  ok(store.rollback(A, 'one', a.version).ok && store.readHtml(A, 'one').html === '<p>v1</p>', 'versions: rollback restores the source');
}

// ─── Grants ───
{
  const plain = { sources: { a: { kind: 'push' }, f: { kind: 'file', path: 'data.json' }, s: { kind: 'session' } }, actions: { e: { kind: 'event' } } };
  ok(store.privileged(A, plain).length === 0, 'grant: push, session, in-project files and event actions need none');
  const risky = { sources: { c: { kind: 'command', run: 'uptime' }, o: { kind: 'file', path: '/etc/hostname' }, up: { kind: 'file', path: '../other/secret.txt' } }, actions: { r: { kind: 'command', run: 'make' } }, allow: ['prompt'] };
  const items = store.privileged(A, risky);
  ok(items.length === 5, 'grant: commands, files outside the project (absolute and ../) and prompt all need one', JSON.stringify(items));

  store.writeSource(A, 'one', { manifest: risky });
  const g1 = store.grantState(A, 'one', risky);
  ok(g1.needed && !g1.granted, 'grant: not granted by default');
  ok(!store.grant(A, 'one', 'wrong-hash').ok, 'grant: a stale or wrong hash is refused');
  ok(store.grant(A, 'one', g1.hash).ok && store.grantState(A, 'one', risky).granted, 'grant: the shown hash is accepted');
  const changed = { ...risky, sources: { ...risky.sources, c: { kind: 'command', run: 'rm -rf /tmp/x' } } };
  ok(!store.grantState(A, 'one', changed).granted, 'grant: changing a command asks again');
}

// ─── Frame document ───
{
  const dir = store.sourceDir(A, 'one');
  writeFileSync(join(dir, 'local.js'), 'window.LOCAL = 1; // </script> in a comment');
  writeFileSync(join(tmp, 'outside.js'), 'window.OUTSIDE = 1;');
  store.writeSource(A, 'one', {
    manifest: {},
    html: '<html><head><title>x</title><meta http-equiv="Content-Security-Policy" content="default-src *"><style>.a{}</style></head>'
      + '<body><p id="p">hi</p><script src="local.js"></script><script src="../../../../outside.js"></script>'
      + '<script src="https://cdn.example.com/lib.js"></script><img src="https://example.com/a.png"></body></html>',
  });
  const d = await buildDoc(A, 'one');
  ok(d.html.includes(`content="${CSP}"`), 'doc: carries Crundi\'s CSP');
  ok(!d.html.includes('default-src *'), 'doc: the author cannot restate the CSP');
  ok(/connect-src 'none'/.test(CSP) && /default-src 'none'/.test(CSP) && !/unsafe-eval/.test(CSP), 'doc: CSP has no network and no eval');
  ok(SANDBOX === 'allow-scripts', 'doc: sandbox is allow-scripts and nothing else', SANDBOX);
  ok(d.html.includes('window.LOCAL = 1') && d.html.includes('<\\/script> in a comment'), 'doc: a local script is inlined and cannot close its own tag');
  ok(!d.html.includes('OUTSIDE'), 'doc: a file outside the widget folder is not inlined');
  ok(!d.html.includes('cdn.example.com/lib.js"') && d.problems.some((p) => /cdn\.example\.com/.test(p)), 'doc: a remote script is removed and reported');
  ok(!/src="https:\/\/example\.com\/a\.png"/.test(d.html), 'doc: a remote image is removed');
  ok(d.html.indexOf('window.crundi') < d.html.indexOf('<p id="p">'), 'doc: the runtime loads before the widget\'s own markup');
}

// ─── Parsing and templates ───
{
  const rows = parseCsv('name,qty,note\n"Smith, J",3,"said ""hi"""\r\nLee,10,\n');
  ok(rows.length === 2 && rows[0].name === 'Smith, J' && rows[0].qty === 3 && rows[0].note === 'said "hi"' && rows[1].qty === 10, 'csv: quotes, commas, CRLF and numbers');
  ok(parseAs('a\nb\nc\n', 'lines', { tail: 2 }).join() === 'b,c', 'lines: tail');
  ok(parseAs('{"a":1}\n{"a":2}\n{"a"', 'jsonl').length === 2, 'jsonl: a half-written last line is skipped');
  ok(fillTemplate('echo {{a}} {{missing}}', { a: "x'; rm -rf /" }, true) === "echo 'x'\\''; rm -rf /' ''", 'template: command parameters are shell-quoted');
  ok(!ACTION_TOOLS.has('secret_get') && !ACTION_TOOLS.has('spawn_terminal') && !ACTION_TOOLS.has('widget_open'), 'actions: secrets, terminals and widgets cannot be wired to a button');
  const s = summariseSession({ state: 'working', messages: [
    { kind: 'user', text: 'go' },
    { kind: 'tool', name: 'TodoWrite', status: 'done', input: { todos: [{ content: 'a', status: 'completed' }, { content: 'b', status: 'in_progress', activeForm: 'Doing b' }] } },
    { kind: 'tool', name: 'Edit', status: 'done', input: { file_path: '/p/src/x.js' } },
    { kind: 'tool', name: 'Bash', status: 'running', input: { description: 'Run tests' } },
  ] });
  ok(s.working && s.progress.pct === 50 && s.counts.edits === 1 && s.now === 'Bash: Run tests' && s.files[0].edits === 1, 'session: todos, counts and the running step', JSON.stringify(s).slice(0, 200));
}

// ─── Sources ───
{
  const root = join(projects, 'demo');
  writeFileSync(join(root, 'data.json'), '{"n":1}');
  writeFileSync(join(root, 'app.log'), 'one\ntwo\n');
  let sqliteOk = true;
  try {
    const { DatabaseSync } = await import('node:sqlite');
    const db = new DatabaseSync(join(root, 't.db'));
    db.exec('create table t(a integer); insert into t values (1),(2),(3)');
    db.close();
  } catch { sqliteOk = false; }

  store.upsert(A, 'src', { title: 'Sources' });
  const manifest = { sources: {
    j: { kind: 'file', path: 'data.json' },
    l: { kind: 'file', path: 'app.log', every: 1 },
    q: { kind: 'sqlite', path: 't.db', query: 'select sum(a) as total from t' },
    w: { kind: 'sqlite', path: 't.db', query: 'insert into t values (9)' },
    c: { kind: 'command', run: 'echo hi' },
    out: { kind: 'file', path: '/etc/hostname' },
    missing: { kind: 'file', path: 'nope.json' },
    k: { kind: 'crundi', what: 'kanban' },
  } };
  store.writeSource(A, 'src', { manifest, html: '<p></p>' });
  const changes = [];
  const sources = createWidgetSources({ onChange: (_a, _i, name) => changes.push(name), crundi: { kanban: () => ({ tasks: [] }) } });
  let v = sources.values(A, 'src');
  await sleep(700);
  v = sources.values(A, 'src');
  ok(v.j.value && v.j.value.n === 1, 'source file: json');
  ok(Array.isArray(v.l.value) && v.l.value.join() === 'one,two', 'source file: .log reads as lines');
  if (sqliteOk) {
    ok(v.q.value && v.q.value[0].total === 6, 'source sqlite: query', JSON.stringify(v.q));
    ok(!!v.w.error, 'source sqlite: opened read-only, a write fails', JSON.stringify(v.w));
  } else console.log('SKIP  source sqlite: node:sqlite is not available on this Node');
  ok(v.c.error === 'needs-approval' && v.out.error === 'needs-approval', 'source: command and outside-project file wait for a grant');
  ok(/No such file/.test(v.missing.error || ''), 'source file: a missing file is an error, not a crash');
  ok(v.k.value && Array.isArray(v.k.value.tasks), 'source crundi: kanban reader');

  changes.length = 0;
  appendFileSync(join(root, 'app.log'), 'three\n');
  await sleep(1800);
  ok(changes.includes('l') && sources.values(A, 'src').l.value.length === 3, 'source file: a change on disk is picked up');

  store.grant(A, 'src', store.grantState(A, 'src', manifest).hash);
  sources.refresh(A, 'src');
  await sleep(900);
  ok(String(sources.values(A, 'src').c.value || '').trim() === 'hi', 'source command: runs once granted');
  sources.stopAll();
}

// ─── The frame cannot get out ───
{
  const headless = await import('../src/browser-headless.js');
  if (!headless.isSupported()) {
    console.log('SKIP  escape: no headless browser on this machine. NOT verified here.');
  } else {
    store.upsert(A, 'hostile', { title: 'Hostile' });
    store.writeSource(A, 'hostile', { manifest: {}, html: `<script>
      (async function () {
        var r = {};
        function t(n, f) { try { r[n] = 'REACHED:' + String(f()).slice(0, 40); } catch (e) { r[n] = 'blocked'; } }
        t('parentStorage', function () { return window.parent.localStorage.getItem('crundi_token'); });
        t('parentDom', function () { return window.parent.document.title; });
        t('ownStorage', function () { return window.localStorage.length; });
        t('cookie', function () { return 'c=' + document.cookie; });
        t('eval', function () { return new Function('return 1')(); });
        try { var x = await fetch(location.ancestorOrigins && location.ancestorOrigins[0] ? location.ancestorOrigins[0] + '/secret' : '/secret'); r.fetch = 'REACHED:' + x.status; } catch (e) { r.fetch = 'blocked'; }
        r.img = await new Promise(function (res) { var i = new Image(); i.onload = function () { res('REACHED'); }; i.onerror = function () { res('blocked'); }; i.src = (location.ancestorOrigins && location.ancestorOrigins[0] || '') + '/pixel.png'; setTimeout(function () { res('blocked'); }, 1200); });
        try { r.open = window.open('about:blank') ? 'REACHED' : 'blocked'; } catch (e) { r.open = 'blocked'; }
        try { window.top.location = 'about:blank#moved'; r.topNav = 'REACHED'; } catch (e) { r.topNav = 'blocked'; }
        crundi.emit('probe', r);
      })();
    </script>` });
    const doc = await buildDoc(A, 'hostile');
    const hits = [];
    // 1x1 PNG, so an allowed image load would succeed.
    const PIXEL = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');
    const server = createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(`<!doctype html><title>host</title><iframe id="f" sandbox="${SANDBOX}"></iframe><script>
          localStorage.setItem('crundi_token', 'SECRET');
          window.__probe = null;
          var f = document.getElementById('f');
          addEventListener('message', function (e) {
            if (e.source !== f.contentWindow || !e.data || e.data.crundi !== 1) return;
            if (e.data.t === 'ready') f.contentWindow.postMessage({ crundi: 1, t: 'init', data: {}, store: {}, context: {} }, '*');
            if (e.data.t === 'call' && e.data.op === 'emit') window.__probe = e.data.args.payload;
          });
          f.srcdoc = ${JSON.stringify(doc.html).replace(/</g, '\\u003c')};
        </script>`);
        return;
      }
      hits.push(req.url);
      if (req.url === '/pixel.png') { res.writeHead(200, { 'Content-Type': 'image/png' }); return res.end(PIXEL); }
      res.writeHead(200, { 'Content-Type': 'text/plain' }); res.end('SECRET');
    });
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const url = `http://127.0.0.1:${server.address().port}/`;
    const key = 'escape-test';
    const opened = await headless.handle({ type: 'browserOpen', key, url, width: 600, height: 400 });
    let probe = null, href = '';
    if (opened.ok) {
      for (let i = 0; i < 40 && !probe; i++) {
        await sleep(200);
        const r = await headless.handle({ type: 'browserEval', key, code: 'window.__probe ? JSON.stringify(window.__probe) : ""' });
        if (r.ok && r.result) probe = JSON.parse(r.result);
      }
      const h = await headless.handle({ type: 'browserEval', key, code: 'location.href' });
      href = h.ok ? h.result : '';
      await headless.handle({ type: 'browserClose', key });
    }
    await headless.shutdown();
    server.close();
    ok(opened.ok && !!probe, 'escape: the probe ran and reported back', opened.error || 'no report');
    if (probe) {
      for (const [name, result] of Object.entries(probe)) ok(result === 'blocked', `escape: ${name} is blocked`, String(result));
      ok(Object.keys(probe).length === 9, 'escape: all nine attempts were made');
    }
    ok(href === url, 'escape: the host page was not navigated away', href);
    ok(hits.filter((u) => u !== '/favicon.ico').length === 0, 'escape: the frame made no request to the host', hits.join(' '));
  }
}

try { rmSync(tmp, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll widget tests passed.');
process.exit(failed ? 1 : 0);
