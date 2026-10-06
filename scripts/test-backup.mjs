#!/usr/bin/env node
/**
 * test-backup.mjs — backups to S3 and restoring them (src/s3.js, src/backup.js,
 * src/restore-apply.js), end to end against a small fake S3 server that checks
 * every request's SigV4 signature.
 *
 *  - signing matches AWS's published examples
 *  - a backup holds the .env, the data dir (minus the browser profile and
 *    working copies) and the newest three transcripts per project
 *  - large archives go up in parts; old scheduled backups are pruned to `keep`
 *  - a wrong passphrase, a tampered archive and an unsafe path are refused
 *  - a staged restore is applied at start, with a safety copy of what it replaced
 *  - a due scheduled backup waits while someone is at Crundi
 *
 * Run: node scripts/test-backup.mjs
 */

import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync, utimesSync, readdirSync, statSync, openSync, readSync, writeSync, closeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { randomBytes, createHash } from 'node:crypto';

const root = mkdtempSync(join(tmpdir(), 'crundi-backup-test-'));
const home = join(root, 'home');
const appDir = join(home, '.config', 'crundi');
const dataDir = join(appDir, 'data');
mkdirSync(dataDir, { recursive: true });
process.env.HOME = home;
process.env.USERPROFILE = home;
process.env.XDG_CONFIG_HOME = join(home, '.config');
process.env.APPDATA = join(home, '.config');
process.env.CRUNDI_NO_RESTORE_APPLY = '1';
delete process.env.DATA_DIR; delete process.env.DOTENV_PATH; delete process.env.PROJECTS_DIR;
writeFileSync(join(appDir, '.env'), 'CRUNDI_PASSWORD_HASH=abc\nCRUNDI_TOTP_SECRET=xyz\nDATA_DIR=\n');

let failed = 0;
const ok = (c, l, d) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c || d === undefined ? '' : '  -> ' + d)); if (!c) failed++; };

const s3mod = await import('../src/s3.js');
const { signRequest } = s3mod;

// ─── SigV4 against AWS's documented examples ───
const AK = 'AKIAIOSFODNN7EXAMPLE', SK = 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY', D = new Date('2013-05-24T00:00:00Z');
ok(signRequest({ method: 'GET', host: 'examplebucket.s3.amazonaws.com', path: '/test.txt', headers: { range: 'bytes=0-9' }, region: 'us-east-1', accessKeyId: AK, secretAccessKey: SK, date: D }).authorization.endsWith('Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41'), 'SigV4: GET object matches the AWS example');
ok(signRequest({ method: 'GET', host: 'examplebucket.s3.amazonaws.com', path: '/', query: { 'max-keys': '2', prefix: 'J' }, region: 'us-east-1', accessKeyId: AK, secretAccessKey: SK, date: D }).authorization.endsWith('Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7'), 'SigV4: list with a query matches the AWS example');

// ─── a fake S3 that verifies signatures ───
const KEY_ID = 'test-key', SECRET = 'test-secret/with+chars', BUCKET = 'bk';
const objects = new Map();   // key -> Buffer
const uploads = new Map();   // uploadId -> Map(partNumber -> Buffer)
let badSig = 0, multipartParts = 0;
const srv = createServer((req, res) => {
  const chunks = [];
  req.on('data', c => chunks.push(c));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    const u = new URL(req.url, 'http://x');
    const query = {}; for (const [k, v] of u.searchParams) query[k] = v;
    // Recompute the signature from what arrived.
    const auth = req.headers.authorization || '';
    const signed = (auth.match(/SignedHeaders=([^,]+)/) || [])[1] || '';
    const hdrs = {}; for (const h of signed.split(';')) if (h !== 'host' && h !== 'x-amz-content-sha256' && h !== 'x-amz-date') hdrs[h] = req.headers[h];
    const amz = req.headers['x-amz-date'] || '';
    const date = new Date(amz.replace(/^(\d{4})(\d\d)(\d\d)T(\d\d)(\d\d)(\d\d)Z$/, '$1-$2-$3T$4:$5:$6Z'));
    const ph = req.headers['x-amz-content-sha256'];
    const expect = signRequest({ method: req.method, host: req.headers.host, path: decodeURIComponent(u.pathname), query, headers: hdrs, payloadHash: ph, region: 'auto', accessKeyId: KEY_ID, secretAccessKey: SECRET, date }).authorization;
    if (expect !== auth || (body.length && createHash('sha256').update(body).digest('hex') !== ph)) { badSig++; res.writeHead(403); return res.end('<Error><Code>SignatureDoesNotMatch</Code><Message>bad</Message></Error>'); }
    const parts = decodeURIComponent(u.pathname).split('/').filter(Boolean);
    if (parts[0] !== BUCKET) { res.writeHead(404); return res.end('<Error><Code>NoSuchBucket</Code></Error>'); }
    const key = parts.slice(1).join('/');
    if (req.method === 'GET' && !key) {
      const pre = query.prefix || '';
      const items = [...objects.keys()].filter(k => k.startsWith(pre)).sort();
      const max = Number(query['max-keys'] || 1000);
      res.writeHead(200);
      return res.end('<ListBucketResult>' + items.slice(0, max).map(k => `<Contents><Key>${k.replace(/&/g, '&amp;')}</Key><Size>${objects.get(k).length}</Size><LastModified>2026-01-01T00:00:00.000Z</LastModified></Contents>`).join('') + '<IsTruncated>false</IsTruncated></ListBucketResult>');
    }
    if (req.method === 'POST' && 'uploads' in query) { const id = 'up' + uploads.size; uploads.set(id, new Map()); res.writeHead(200); return res.end(`<InitiateMultipartUploadResult><UploadId>${id}</UploadId></InitiateMultipartUploadResult>`); }
    if (req.method === 'PUT' && query.uploadId) { uploads.get(query.uploadId).set(Number(query.partNumber), body); multipartParts++; res.writeHead(200, { ETag: '"e' + query.partNumber + '"' }); return res.end(); }
    if (req.method === 'POST' && query.uploadId) { const p = uploads.get(query.uploadId); objects.set(key, Buffer.concat([...p.keys()].sort((a, b) => a - b).map(n => p.get(n)))); uploads.delete(query.uploadId); res.writeHead(200); return res.end('<CompleteMultipartUploadResult/>'); }
    if (req.method === 'DELETE' && query.uploadId) { uploads.delete(query.uploadId); res.writeHead(204); return res.end(); }
    if (req.method === 'PUT') { objects.set(key, body); res.writeHead(200); return res.end(); }
    if (req.method === 'GET') { const o = objects.get(key); if (!o) { res.writeHead(404); return res.end('<Error><Code>NoSuchKey</Code></Error>'); } res.writeHead(200, { 'Content-Length': o.length }); return res.end(o); }
    if (req.method === 'DELETE') { objects.delete(key); res.writeHead(204); return res.end(); }
    res.writeHead(400); res.end();
  });
});
await new Promise(r => srv.listen(0, '127.0.0.1', r));
const endpoint = 'http://127.0.0.1:' + srv.address().port;

// ─── a Crundi to back up ───
const projDir = join(root, 'work', 'my.app');
mkdirSync(projDir, { recursive: true });
writeFileSync(join(dataDir, 'projects.json'), JSON.stringify({ app: { alias: 'app', path: projDir, name: 'App' } }));
writeFileSync(join(dataDir, 'kanban.json'), JSON.stringify({ app: [{ id: 't1', title: 'Ship it' }] }));
writeFileSync(join(dataDir, 'panes.json'), '{"layout":1}');
mkdirSync(join(dataDir, 'notes', 'app'), { recursive: true });
writeFileSync(join(dataDir, 'notes', 'app', '0123456789abcdef.json'), '{"title":"Deploy"}');
mkdirSync(join(dataDir, 'media'), { recursive: true });
const bigMedia = randomBytes(20 * 1024 * 1024);   // incompressible: forces a multipart upload
writeFileSync(join(dataDir, 'media', 'clip.bin'), bigMedia);
mkdirSync(join(dataDir, 'chrome', 'Default'), { recursive: true });
writeFileSync(join(dataDir, 'chrome', 'Default', 'Cookies'), 'browser');
mkdirSync(join(dataDir, 'worktrees', 'w1'), { recursive: true });
writeFileSync(join(dataDir, 'worktrees', 'w1', 'file'), 'x');
writeFileSync(join(dataDir, 'update.log'), 'log');
writeFileSync(join(dataDir, 'kanban.json.tmp'), 'partial');
const enc = projDir.replace(/[^a-zA-Z0-9]/g, '-');
const tdir = join(home, '.claude', 'projects', enc);
mkdirSync(tdir, { recursive: true });
for (let i = 1; i <= 5; i++) {
  const f = join(tdir, `s${i}.jsonl`);
  writeFileSync(f, `{"n":${i}}\n`);
  const t = new Date(Date.UTC(2026, 0, i)); utimesSync(f, t, t);
}

// Global Claude skills: two the user put there, the one Crundi ships, one synced.
const skillsDir = join(home, '.claude', 'skills');
for (const [p, c] of Object.entries({
  'mine/SKILL.md': '---\nname: mine\ndescription: d\n---\n', 'mine/scripts/run.sh': '#!/bin/sh\n',
  'other/SKILL.md': '---\nname: other\ndescription: backed up\n---\n',
  'crundi/SKILL.md': '---\nname: crundi\ndescription: d\n---\n',
  'synced/bucket/docx/SKILL.md': '---\nname: docx\ndescription: d\n---\n',
})) { mkdirSync(join(skillsDir, p, '..'), { recursive: true }); writeFileSync(join(skillsDir, p), c); }

const backup = await import('../src/backup.js');
const { applyStagedRestore } = await import('../src/restore-apply.js');

// ─── collect ───
const { files, manifest } = backup.collect();
const rels = files.map(f => f.rel);
ok(rels.includes('env/.env'), 'the .env is included');
ok(['data/projects.json', 'data/kanban.json', 'data/panes.json', 'data/notes/app/0123456789abcdef.json', 'data/media/clip.bin'].every(r => rels.includes(r)), 'projects, kanban, layouts, notes and media are included');
ok(!rels.some(r => /chrome|worktrees|update\.log|\.tmp$/.test(r)), 'browser profile, worktrees, logs and temp files are left out', rels.filter(r => /chrome|worktrees|update\.log|\.tmp$/.test(r)).join(','));
const tr = rels.filter(r => r.startsWith('claude/')).sort();
ok(tr.length === 3 && tr.join() === [`claude/${enc}/s3.jsonl`, `claude/${enc}/s4.jsonl`, `claude/${enc}/s5.jsonl`].join(), 'only the three newest transcripts of the project', tr.join());
ok(manifest.projects.length === 1 && manifest.projects[0].alias === 'app' && manifest.transcripts === 3, 'the manifest lists the projects', JSON.stringify(manifest.projects) + ' ' + manifest.transcripts);

const sk = rels.filter(r => r.startsWith('skills/')).sort();
ok(sk.join() === 'skills/mine/SKILL.md,skills/mine/scripts/run.sh,skills/other/SKILL.md' && manifest.skills === 2, 'global skills the user installed are included; the one Crundi ships and synced ones are not', sk.join() + ' ' + manifest.skills);

// ─── settings ───
ok(!backup.updateConfig({ schedule: { enabled: true } }).ok, 'the schedule cannot be turned on before storage and passphrase are set');
ok(!backup.updateConfig({ passphrase: 'short' }).ok, 'a short passphrase is refused');
const set = backup.updateConfig({ storage: { endpoint, region: 'auto', bucket: BUCKET, prefix: 'nightly', accessKeyId: KEY_ID, secretAccessKey: SECRET }, passphrase: 'correct horse battery', keep: 2 });
ok(set.ok && set.config.configured && set.config.storage.prefix === 'nightly/' && set.config.storage.secretAccessKey === '' && set.config.storage.hasSecret, 'settings save; the secret is never handed back');
ok((statSync(join(dataDir, 'backup.json')).mode & 0o077) === 0 || process.platform === 'win32', 'the settings file is private to the user');

let pushes = 0;
let present = false, away = false;
const svc = backup.createBackups({ isPresent: () => present, awayFor: () => away, broadcast: () => { pushes++; }, notify: () => {} });
ok((await svc.test()).ok, 'connection test passes against the fake S3');

// ─── a manual backup ───
const r1 = await svc.run('manual');
ok(r1.ok && objects.has(r1.key) && r1.key.startsWith('nightly/crundi-') && r1.key.endsWith('-manual.crbk'), 'a manual backup is uploaded under the prefix', r1.error || r1.key);
ok(multipartParts >= 2, 'a large backup goes up in parts', multipartParts);
ok(badSig === 0, 'every request was signed correctly', badSig);
ok(!readFileSync(join(dataDir, 'backup.json'), 'utf-8').includes('"last": null'), 'the result is recorded');
ok(!objects.get(r1.key).includes(Buffer.from('Ship it')), 'the uploaded archive is not readable without the passphrase');
ok(pushes > 0, 'progress was pushed to open pages');

// ─── retention ───
for (let i = 0; i < 3; i++) { const r = await svc.run('scheduled'); if (!r.ok) ok(false, 'scheduled run', r.error); }
const listed = await svc.list();
ok(listed.filter(i => i.reason === 'scheduled').length === 2 && listed.some(i => i.reason === 'manual'), 'only the newest 2 scheduled backups remain; manual ones are kept', listed.map(i => i.reason).join(','));

// ─── restore: wrong passphrase, then right ───
const bad = await svc.restore({ key: r1.key, passphrase: 'not it at all' });
ok(!bad.ok && bad.code === 'BAD_PASSPHRASE' && !existsSync(backup.stagingDir()), 'a wrong passphrase is refused and nothing is staged', bad.error);
const good = await svc.restore({ key: r1.key, storage: { endpoint, region: 'auto', bucket: BUCKET, prefix: 'nightly/', accessKeyId: KEY_ID, secretAccessKey: SECRET }, passphrase: 'correct horse battery' });
ok(good.ok && existsSync(join(backup.stagingDir(), 'READY')), 'restoring with the storage typed in (first-run) stages the backup', good.error);
ok(good.projects && good.projects[0].exists, 'the restore reports which project folders exist here');

// Things change after the backup...
writeFileSync(join(dataDir, 'kanban.json'), '{"changed":true}');
rmSync(join(dataDir, 'notes'), { recursive: true });
writeFileSync(join(appDir, '.env'), 'CRUNDI_PASSWORD_HASH=new\n');
rmSync(join(tdir, 's5.jsonl'));
writeFileSync(join(tdir, 's4.jsonl'), 'newer local copy');
rmSync(join(skillsDir, 'mine'), { recursive: true });
writeFileSync(join(skillsDir, 'other', 'SKILL.md'), 'edited since the backup');
const applied = applyStagedRestore({ appDir, log: () => {} });
ok(applied && JSON.parse(readFileSync(join(dataDir, 'kanban.json'), 'utf-8')).app[0].title === 'Ship it', 'applying at start brings the kanban back');
ok(existsSync(join(dataDir, 'notes', 'app', '0123456789abcdef.json')), 'and the notes');
ok(readFileSync(join(dataDir, 'media', 'clip.bin')).equals(bigMedia), 'and the media, byte for byte');
ok(/CRUNDI_PASSWORD_HASH=abc/.test(readFileSync(join(appDir, '.env'), 'utf-8')), 'and the .env (sign-in)');
ok(readFileSync(join(dataDir, 'chrome', 'Default', 'Cookies'), 'utf-8') === 'browser', 'the browser profile is left alone');
ok(existsSync(join(tdir, 's5.jsonl')) && readFileSync(join(tdir, 's4.jsonl'), 'utf-8') === 'newer local copy', 'missing transcripts come back; existing ones are not overwritten');
ok(existsSync(join(skillsDir, 'mine', 'scripts', 'run.sh')) && applied.skills === 1, 'a missing skill comes back whole', String(applied.skills));
ok(readFileSync(join(skillsDir, 'other', 'SKILL.md'), 'utf-8') === 'edited since the backup', 'a skill that is already here is not overwritten by the restore');
ok(existsSync(join(applied.safetyCopy, 'data', 'kanban.json')) && readFileSync(join(applied.safetyCopy, 'data', 'kanban.json'), 'utf-8').includes('changed'), 'what was replaced is kept in a pre-restore copy');
ok(!existsSync(backup.stagingDir()) && existsSync(backup.restoreResultFile()), 'staging is cleared and the result recorded');
ok(applyStagedRestore({ appDir, log: () => {} }) === null, 'nothing happens at the next start');

// ─── tampering and unsafe paths ───
const tmpA = join(root, 'a.crbk');
writeFileSync(tmpA, objects.get(r1.key));
const fd = openSync(tmpA, 'r+'); const b = Buffer.alloc(1); readSync(fd, b, 0, 1, 5000); b[0] ^= 0xff; writeSync(fd, b, 0, 1, 5000); closeSync(fd);
let err = '';
try { await backup.extractArchive(tmpA, 'correct horse battery', join(root, 'x1')); } catch (e) { err = e.message; }
ok(/damaged|changed/.test(err), 'a tampered archive is refused', err);
const evil = join(root, 'evil.crbk');
writeFileSync(join(root, 'payload'), 'pwned');
await backup.writeArchive(evil, 'pw-12345678', { files: [{ abs: join(root, 'payload'), rel: 'data/../../escape', size: 5, mode: 0o600, mtime: 0 }], manifest: { createdAt: '', host: 'h', crundiVersion: '' } });
err = '';
try { await backup.extractArchive(evil, 'pw-12345678', join(root, 'x2')); } catch (e) { err = e.message; }
ok(/unsafe path/.test(err) && !existsSync(join(root, 'escape')), 'an archive with a path outside the restore folder is refused', err);

// ─── schedule ───
const at = (s) => new Date(s).getTime();
const daily = { frequency: 'daily', time: '03:00' };
ok(backup.mostRecentSlot(daily, at('2026-03-10T10:00:00')) === at('2026-03-10T03:00:00'), 'daily: the slot earlier today');
ok(backup.mostRecentSlot(daily, at('2026-03-10T02:00:00')) === at('2026-03-09T03:00:00'), 'daily: before the time it is yesterday’s');
ok(backup.nextSlot(daily, at('2026-03-10T10:00:00')) === at('2026-03-11T03:00:00'), 'daily: the next one is tomorrow');
const weekly = { frequency: 'weekly', time: '04:30', weekday: 0 };   // Sunday
ok(backup.mostRecentSlot(weekly, at('2026-03-11T12:00:00')) === at('2026-03-08T04:30:00'), 'weekly: last Sunday');
const hours = { frequency: 'hours', time: '00:00', everyHours: 6 };
ok(backup.mostRecentSlot(hours, at('2026-03-10T13:15:00')) === at('2026-03-10T12:00:00'), 'every 6 hours: the 12:00 slot');

// A due backup waits while someone is here.
const before = objects.size;
backup.updateConfig({ schedule: { enabled: true, frequency: 'daily', time: '03:00' } });
const c = backup.readConfig(); c.state.lastSlot = 1;
writeFileSync(join(dataDir, 'backup.json'), JSON.stringify(c));
present = true; away = false;
await svc.tick();
ok(svc.status().waiting && objects.size === before, 'while you are here it waits, and says so');
present = false; away = false;
await svc.tick();
ok(svc.status().waiting && objects.size === before, 'just after you leave it still waits a little');
away = true;
await svc.tick();
ok(!svc.status().waiting && svc.status().last.reason === 'scheduled' && svc.status().last.ok, 'once you are away it runs');
await svc.tick();
ok(svc.status().last.at && backup.readConfig().state.lastSlot >= backup.mostRecentSlot(backup.readConfig().schedule), 'and not again for the same slot');

srv.close();
rmSync(root, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} backup check(s) failed`); process.exit(1); }
console.log('\nAll backup checks passed.');
