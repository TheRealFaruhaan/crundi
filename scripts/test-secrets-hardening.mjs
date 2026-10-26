#!/usr/bin/env node
/**
 * test-secrets-hardening.mjs — the data folder is owner-only, secrets lock with
 * a passphrase under a slower scrypt, old secrets keep opening and say they
 * need an upgrade, and a secret can be edited with its current lock
 * (src/private-fs.js, src/secrets-store.js, the 'edit' action in
 * src/webapp.js; capabilities 'secret-passphrase', 'secret-edit',
 * 'secret-upgrade').
 *
 * No shell commands: the release build runs this on Windows, where the POSIX
 * mode checks are skipped (the modes mean nothing there).
 *
 * Run: node scripts/test-secrets-hardening.mjs
 */

import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, statSync, chmodSync, symlinkSync, rmSync, existsSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes, scryptSync, createCipheriv } from 'node:crypto';

const base = mkdtempSync(join(tmpdir(), 'secrets-hardening-'));
process.env.DATA_DIR = join(base, 'cfg', 'data');
const pfs = await import('../src/private-fs.js');
const secrets = await import('../src/secrets-store.js');
const CS = await import('../src/chat-secrets.js');
const FS = await import('../src/factory-secrets.js');
const { CAPABILITIES } = await import('../src/client-version.js');

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const POSIX = process.platform !== 'win32';
const mode = (p) => statSync(p).mode & 0o777;

// ─── Owner-only folders and files ───
const dataDir = process.env.DATA_DIR;
pfs.ensurePrivateDir(dataDir);
if (POSIX) {
  ok(mode(dataDir) === 0o700 && mode(dirname(dataDir)) === 0o700, 'a new data folder, and the config folder made above it, are 0700');
  const f = join(dataDir, 'x.json');
  pfs.writePrivateFile(f, '{"a":1}');
  ok(mode(f) === 0o600 && readFileSync(f, 'utf8') === '{"a":1}', 'writePrivateFile writes 0600');
  chmodSync(f, 0o644);
  pfs.writePrivateFile(f, '{"a":2}');
  ok(mode(f) === 0o600 && readFileSync(f, 'utf8') === '{"a":2}', 'an atomic rewrite replaces an open file with a 0600 one');
  ok(!readdirSync(dataDir).some((n) => n.endsWith('.tmp')), 'and leaves no temporary file behind');
  pfs.writePrivateFile(f, 'z', { atomic: false });
  ok(mode(f) === 0o600, 'a plain write tightens too');

  // An existing, open folder, tightened at start.
  const open = join(base, 'open');
  const cfg = join(open, 'crundi');
  const d = join(cfg, 'data');
  mkdirSync(join(d, 'chrome', 'bin'), { recursive: true });
  mkdirSync(join(d, 'proj', 'node_modules', 'pkg'), { recursive: true });
  for (const p of [open, cfg, d, join(d, 'chrome'), join(d, 'chrome', 'bin'), join(d, 'proj'), join(d, 'proj', 'node_modules'), join(d, 'proj', 'node_modules', 'pkg')]) chmodSync(p, 0o755);
  writeFileSync(join(cfg, '.env'), 'A=1'); chmodSync(join(cfg, '.env'), 0o644);
  writeFileSync(join(d, 'secrets.json'), '{}'); chmodSync(join(d, 'secrets.json'), 0o644);
  writeFileSync(join(d, 'secrets.json.bak-1'), '{}'); chmodSync(join(d, 'secrets.json.bak-1'), 0o666);
  writeFileSync(join(d, '.api-key'), 'k'); chmodSync(join(d, '.api-key'), 0o644);
  writeFileSync(join(d, 'chrome', 'bin', 'chrome'), '#!'); chmodSync(join(d, 'chrome', 'bin', 'chrome'), 0o755);
  writeFileSync(join(d, 'proj', 'node_modules', 'pkg', 'i.js'), ''); chmodSync(join(d, 'proj', 'node_modules', 'pkg', 'i.js'), 0o644);
  const outside = join(base, 'outside.txt'); writeFileSync(outside, 'x'); chmodSync(outside, 0o644);
  symlinkSync(outside, join(d, 'link'));
  const r = pfs.securePrivateFolders({ appDir: cfg, dataDir: d });
  ok(r.changed > 0, 'start: an open folder is tightened');
  ok(mode(cfg) === 0o700 && mode(d) === 0o700 && mode(join(cfg, '.env')) === 0o600, 'the config folder 0700, its .env 0600');
  ok(mode(join(d, 'secrets.json')) === 0o600 && mode(join(d, 'secrets.json.bak-1')) === 0o600 && mode(join(d, '.api-key')) === 0o600, 'secrets.json, its backups and .api-key 0600');
  ok(mode(join(d, 'chrome')) === 0o700 && mode(join(d, 'chrome', 'bin', 'chrome')) === 0o700, 'folders inside 0700, and an executable keeps its owner bits (0700)');
  ok(mode(join(d, 'proj', 'node_modules')) === 0o700 && mode(join(d, 'proj', 'node_modules', 'pkg')) === 0o755, 'a node_modules folder is closed but not walked');
  ok(mode(outside) === 0o644, 'a symbolic link is not followed');
  ok(mode(open) === 0o755, 'nothing above the config folder is touched');
  const again = pfs.securePrivateFolders({ appDir: cfg, dataDir: d });
  ok(again.changed === 0, 'a second start finds nothing to change');
  const big = pfs.tightenPrivateTree(d, { maxEntries: 2 });
  ok(big.truncated, 'the walk stops at its entry limit');
} else {
  console.log('SKIP  POSIX modes (Windows)');
}

// The sources use it.
const st = read('src', 'secrets-store.js');
ok(/writePrivateFile\(FILE\(\)/.test(st) && !/writeFileSync/.test(st), 'secrets.json is written through writePrivateFile (atomic, 0600)');
const web = read('src', 'webapp.js');
ok(/writePrivateFile\(keyFile, internalApiKey\)/.test(web), '.api-key is written 0600');
ok(/writePrivateFile\(envPath, content/.test(web), 'the .env is written 0600');
const idx = read('src', 'index.js');
ok(/securePrivateFolders\(\{ appDir: ownAppDir \? config\.appDir : '', dataDir: config\.dataDir \}\)/.test(idx), 'the server tightens its folders at start');
const tmpWrites = [];
for (const f of readdirSync(join(root, 'src')).filter((n) => n.endsWith('.js') && n !== 'webapp-html.js' && n !== 'rpa.js' && n !== 'private-fs.js')) {
  for (const line of read('src', f).split('\n')) if (/writeFileSync\(tmp\b/.test(line) && !/0o600/.test(line)) tmpWrites.push(f + ': ' + line.trim());
}
ok(!tmpWrites.length, 'every tmp+rename write in src/ makes its file 0600' + (tmpWrites.length ? ' (' + tmpWrites.join('; ') + ')' : ''));
const inst = read('scripts', 'install.sh');
ok(/mkdir -p -m 700 "\$CONFIG_DIR"/.test(inst) && /chmod 700 "\$CONFIG_DIR"/.test(inst) && /chmod go-rwx/.test(inst), 'install.sh makes the config folder owner-only');

// ─── New locks: a passphrase of 8+, not digits only ───
ok(secrets.passphraseProblem('12345678') !== '', '"12345678" (digits only) is refused');
ok(secrets.passphraseProblem('abcdefg') !== '', '"abcdefg" (7) is refused');
ok(secrets.passphraseProblem('abcd1234') === '', '"abcd1234" is accepted');
ok(secrets.passphraseProblem('pässwörd') === '' && secrets.passphraseProblem('ab cd ef') === '', 'any characters, spaces included');
ok(/At least 8 characters, and not only digits/.test(secrets.PASSPHRASE_RULE), 'the rule says so in plain words');
let r = await secrets.addSecret({ name: 'A', value: 'v', pin: '123456' });
ok(!r.ok && /not only digits/.test(r.error), 'create: a 6-digit PIN is no longer accepted');
r = await secrets.addSecret({ name: 'A', value: 'v', pin: '12345678' });
ok(!r.ok, 'create: "12345678" refused');
r = await secrets.addSecret({ name: 'A', value: 'v', pin: 'abcdefg' });
ok(!r.ok, 'create: "abcdefg" refused');
const PASS = 'abcd1234';
const VALUE = 'tok-HARDEN-' + randomBytes(6).toString('hex');
r = await secrets.addSecret({ name: 'Deploy', description: 'd', value: VALUE, pin: PASS });
ok(r.ok && r.id, 'create: "abcd1234" accepted');
const A = r.id;
const file = join(dataDir, 'secrets.json');
if (POSIX) ok(mode(file) === 0o600, 'secrets.json is 0600');
let rec = JSON.parse(readFileSync(file, 'utf8')).secrets.find((s) => s.id === A);
ok(rec.lock === 'passphrase' && rec.kdf && rec.kdf.name === 'scrypt' && rec.kdf.N === 131072 && rec.kdf.r === 8 && rec.kdf.p === 1, 'a new secret records lock "passphrase" and scrypt N=2^17, r=8, p=1');
ok(!JSON.stringify(rec).includes(PASS) && !JSON.stringify(rec).includes(VALUE), 'neither the passphrase nor the value is stored in the clear');
let meta = secrets.getSecretMeta(A);
ok(meta.lock === 'passphrase' && meta.needsUpgrade === false && !('kdf' in meta) && !('salt' in meta), 'its public view: lock and needsUpgrade, no crypto material');
const t0 = Date.now();
let d = await secrets.decryptSecret(A, PASS);
console.log(`      (unlock took ${Date.now() - t0} ms)`);
ok(d.ok && d.value === VALUE && d.lock === 'passphrase' && d.needsUpgrade === false, 'it opens with its passphrase');
d = await secrets.decryptSecret(A, 'abcd1235');
ok(!d.ok && d.error === 'Incorrect passphrase', 'a wrong passphrase: "Incorrect passphrase"');

// ─── Old secrets: 6-digit PIN, N=2^14, no kdf/lock fields ───
function legacyRecord({ id, name, value, pin, kdf = null, lock = null }) {
  const salt = randomBytes(16), iv = randomBytes(12);
  const p = kdf || { N: 16384, r: 8, p: 1 };
  const key = scryptSync(pin, salt, 32, { N: p.N, r: p.r, p: p.p, maxmem: 128 * p.N * p.r * 2 + 32 * 1024 * 1024 });
  const c = createCipheriv('aes-256-gcm', key, iv);
  const data = Buffer.concat([c.update(value, 'utf8'), c.final()]);
  const now = new Date().toISOString();
  return { id, name, description: '', ...(lock ? { lock } : {}), ...(kdf ? { kdf: { name: 'scrypt', ...kdf } } : {}), salt: salt.toString('base64'), iv: iv.toString('base64'), tag: c.getAuthTag().toString('base64'), data: data.toString('base64'), createdAt: now, updatedAt: now };
}
const store = JSON.parse(readFileSync(file, 'utf8'));
const OLDPIN = '482913', OLDVAL = 'tok-OLD-' + randomBytes(4).toString('hex');
store.secrets.push(legacyRecord({ id: 'old0000000000001', name: 'Old', value: OLDVAL, pin: OLDPIN }));
store.secrets.push(legacyRecord({ id: 'old0000000000002', name: 'Old passphrase', value: 'v2', pin: 'weak-kdf-pass', kdf: { N: 16384, r: 8, p: 1 }, lock: 'passphrase' }));
const bad = legacyRecord({ id: 'bad0000000000003', name: 'Tampered', value: 'v3', pin: '111111' });
bad.kdf = { name: 'scrypt', N: 2 ** 30, r: 8, p: 1 };
store.secrets.push(bad);
writeFileSync(file, JSON.stringify(store));
const OLD = 'old0000000000001';
meta = secrets.getSecretMeta(OLD);
ok(meta.lock === 'pin' && meta.needsUpgrade === true, 'an old secret is flagged: lock "pin", needsUpgrade');
ok(secrets.getSecretMeta('old0000000000002').lock === 'passphrase' && secrets.getSecretMeta('old0000000000002').needsUpgrade === true, 'a passphrase on the old parameters is flagged too');
ok(secrets.listSecrets().find((s) => s.id === OLD).needsUpgrade === true, 'the list says so');
d = await secrets.decryptSecret(OLD, OLDPIN);
ok(d.ok && d.value === OLDVAL && d.needsUpgrade === true && d.lock === 'pin', 'it still opens with its 6-digit PIN (old parameters), and says it needs an upgrade');
d = await secrets.decryptSecret(OLD, '000000');
ok(!d.ok && d.error === 'Incorrect PIN', 'a wrong PIN: "Incorrect PIN"');
d = await secrets.decryptSecret(OLD, 'abc');
ok(!d.ok && /6 digits/.test(d.error), 'something that is not a PIN is refused before any work');
d = await secrets.decryptSecret('bad0000000000003', '111111');
ok(!d.ok, 'absurd stored parameters are refused, not attempted');

// ─── Upgrade (edit with newPin only) ───
r = await secrets.editSecret(OLD, { pin: '000000', newPin: 'new-pass-1' });
ok(!r.ok && r.error === 'Incorrect PIN', 'upgrade: refused with a wrong PIN');
ok(secrets.getSecretMeta(OLD).needsUpgrade === true, 'and nothing changed');
r = await secrets.editSecret(OLD, { pin: OLDPIN, newPin: '12345678' });
ok(!r.ok && /not only digits/.test(r.error), 'upgrade: "12345678" refused');
r = await secrets.editSecret(OLD, { pin: OLDPIN, newPin: 'abcdefg' });
ok(!r.ok && /At least 8/.test(r.error), 'upgrade: "abcdefg" refused');
r = await secrets.editSecret(OLD, { pin: OLDPIN, newPin: 'abcd1234' });
ok(r.ok && r.relocked && r.secret.lock === 'passphrase' && r.secret.needsUpgrade === false, 'upgrade with the right PIN: re-locked with a passphrase');
ok(!('value' in r), 'the edit never returns the value');
meta = secrets.getSecretMeta(OLD);
ok(meta.lock === 'passphrase' && meta.needsUpgrade === false, 'an upgraded secret is no longer flagged');
rec = JSON.parse(readFileSync(file, 'utf8')).secrets.find((s) => s.id === OLD);
ok(rec.kdf && rec.kdf.N === 131072, 'and is sealed with the new parameters');
d = await secrets.decryptSecret(OLD, 'abcd1234');
ok(d.ok && d.value === OLDVAL, 'it opens with the new passphrase, value unchanged');
d = await secrets.decryptSecret(OLD, OLDPIN);
ok(!d.ok, 'and no longer with the old PIN');
r = await secrets.editSecret('old0000000000002', { pin: 'weak-kdf-pass', newPin: 'weak-kdf-pass' });
ok(r.ok && secrets.getSecretMeta('old0000000000002').needsUpgrade === false, 'a passphrase on old parameters can be re-sealed with the same passphrase');

// ─── Edit ───
r = await secrets.editSecret(A, { pin: 'wrong-pass', value: 'x' });
ok(!r.ok && r.error === 'Incorrect passphrase', 'edit: a wrong current lock is refused');
r = await secrets.editSecret(A, { pin: PASS, value: 'new-value-1', name: 'Deploy 2', description: 'dd' });
ok(r.ok && !r.relocked, 'edit: a new value under the same passphrase, with a new name');
d = await secrets.decryptSecret(A, PASS);
ok(d.ok && d.value === 'new-value-1' && d.name === 'Deploy 2' && secrets.getSecretMeta(A).description === 'dd', 'it opens with the same passphrase and has the new value, name and description');
r = await secrets.editSecret(A, { pin: PASS, value: '' });
ok(!r.ok, 'edit: an empty value is refused');
r = await secrets.editSecret(A, { pin: PASS, newPin: 'short' });
ok(!r.ok && /At least 8/.test(r.error), 'edit: a short new passphrase is refused');
r = await secrets.editSecret(A, { pin: PASS, newPin: 'another-pass', value: 'new-value-2' });
ok(r.ok && r.relocked, 'edit: new passphrase and new value at once');
ok((await secrets.decryptSecret(A, 'another-pass')).value === 'new-value-2' && !(await secrets.decryptSecret(A, PASS)).ok, 'only the new passphrase opens it');
r = await secrets.editSecret('nope', { pin: PASS, value: 'x' });
ok(!r.ok && /not found/.test(r.error), 'edit: an unknown secret');
// A value change on an old PIN keeps the PIN (still flagged).
store.secrets = JSON.parse(readFileSync(file, 'utf8')).secrets;
store.secrets.push(legacyRecord({ id: 'old0000000000004', name: 'Old 4', value: 'v4', pin: '135790' }));
writeFileSync(file, JSON.stringify(store));
r = await secrets.editSecret('old0000000000004', { pin: '135790', value: 'v4b' });
ok(r.ok && !r.relocked && secrets.getSecretMeta('old0000000000004').lock === 'pin' && secrets.getSecretMeta('old0000000000004').needsUpgrade === true, 'a value change on an old PIN keeps it a PIN, still flagged');
ok((await secrets.decryptSecret('old0000000000004', '135790')).value === 'v4b', 'and opens with that PIN');

// ─── Kept locks follow a re-lock ───
const vault = CS.createChatSecretVault({ timers: false });
ok(vault.grant('chatA', A, { name: 'x', pin: 'another-pass', expiresAt: null }), 'a chat grant takes a passphrase');
ok(vault.grant('chatB', A, { name: 'x', pin: '135790', expiresAt: null }), 'and a 6-digit PIN');
const changed = vault.replacePin(A, 'another-pass', 'third-pass-9');
ok(changed.length === 1 && changed[0] === 'chatA' && vault.pin('chatA', A) === 'third-pass-9' && vault.pin('chatB', A) === '135790', 'chat grants: the old lock is replaced by the new, others left');
const fv = FS.createSecretVault();
fv.remember('run1', A, 'another-pass');
ok(fv.replacePin(A, 'another-pass', 'third-pass-9') === 1 && fv.pin('run1', A) === 'third-pass-9', 'factory runs: the same');
const g = await CS.grantChatSecrets({ vault, chatId: 'chatC', secrets: [{ id: 'old0000000000004', pin: '135790' }], duration: { duration: '1h' }, decrypt: async (id, pin) => { const x = await secrets.decryptSecret(id, pin); return x.ok ? { ok: true, name: x.name, needsUpgrade: x.needsUpgrade } : x; } });
ok(g.ok && JSON.stringify(g.upgradable) === JSON.stringify(['old0000000000004']), 'pre-authorising with an old lock says it is upgradable');

// ─── Server wiring ───
ok(['secret-passphrase', 'secret-edit', 'secret-upgrade'].every((c) => CAPABILITIES.includes(c)), "the handshake lists 'secret-passphrase', 'secret-edit' and 'secret-upgrade'");
const editBlock = web.slice(web.indexOf("case 'edit': {"), web.indexOf("case 'updateMeta':"));
ok(/secrets\.editSecret\(id, \{/.test(editBlock) && /chatSecrets\.replacePin/.test(editBlock) && /relockSecret/.test(editBlock), "POST /api/secrets action 'edit' re-locks and moves kept locks along");
ok(!/value: r\.value|console\./.test(editBlock), 'and neither returns nor logs the value');
ok(/\.\.\.lockInfo\(r\.secretId\)/.test(web) && /return m \? \{ lock: m\.lock, needsUpgrade: m\.needsUpgrade \}/.test(web), 'secret requests carry lock and needsUpgrade');
ok(/dec\.needsUpgrade \? \{ needsUpgrade: true \}/.test(web), 'approve says needsUpgrade');
ok(/lock: r\.lock, needsUpgrade: r\.needsUpgrade/.test(web), 'check says lock and needsUpgrade');
ok(/upgradable/.test(read('src', 'factory-runner.js')), 'a factory start says which unlocked secrets are upgradable');
const { mayAccess, ROLE_COLLABORATOR } = await import('../src/access-policy.js');
ok(!mayAccess({ role: ROLE_COLLABORATOR, collabKey: 'pc:sam', projects: ['demo'], roots: { demo: '/tmp/wt' } }, 'POST', '/api/secrets'), 'a collaborator cannot reach the edit');

// ─── Clients (web) ───
const html = read('src', 'webapp-html.js');
const lockJs = read('app', 'vendor', 'crundi-secret-lock.js');
const page = read('src', 'chat-page.js');
const pageJs = read('app', 'vendor', 'crundi-chat-page.js');
const csJs = read('app', 'vendor', 'crundi-chat-secrets.js');
const fxJs = read('app', 'vendor', 'crundi-factories.js');
ok(html.indexOf('src="/vendor/crundi-secret-lock.js') > 0 && html.indexOf('src="/vendor/crundi-secret-lock.js') < html.indexOf('src="/vendor/crundi-chat-secrets.js'), 'the workbench loads crundi-secret-lock.js before the chat secrets script');
ok(page.indexOf('src="/vendor/crundi-secret-lock.js') > 0 && page.indexOf('src="/vendor/crundi-secret-lock.js') < page.indexOf('src="/vendor/crundi-chat-page.js'), 'and so does the compact chat page');
ok(/At least ' \+ MIN \+ ' characters, and not only digits/.test(lockJs) && /\/\[\^0-9\]\/\.test\(t\)/.test(lockJs), 'the shared script has the same rule as the server');
ok(/This secret uses an old 6-digit PIN\. Set a new passphrase \(8\+ characters\)\./.test(lockJs) && /'Not now'/.test(lockJs) && /'Update'/.test(lockJs), 'the upgrade prompt: its words, Update and Not now');
ok(/supports\('secret-upgrade'\)/.test(lockJs) && /action: 'edit', id: o\.id, pin: oldPin, newPin: np/.test(lockJs), 'the upgrade is capability gated and sends the lock just typed');
ok(!/localStorage|sessionStorage/.test(lockJs) && !/\b(prompt|confirm|alert)\(/.test(lockJs), 'nothing kept in the browser, no browser dialogs');
ok(/r\.lock === 'passphrase'/.test(html) && /L\.upgrade\(\{ id: req\.secretId/.test(html), 'workbench request card: a passphrase field, and the upgrade after an old lock');
ok(/data-sact="edit"/.test(html) && /L\.edit\(s,/.test(html) && /supports\('secret-edit'\)/.test(html), 'workbench Secrets list: Edit, capability gated');
ok(/sec-add-pin2/.test(html) && /id="sec-add-save"[^>]*disabled/.test(html), 'workbench add form: passphrase typed twice, Save disabled until it meets the rule');
ok(/lock = r\.lock === 'passphrase' && SL/.test(pageJs) && /SL\.upgrade\(\{ id: r\.secretId/.test(pageJs), 'chat page request card: a passphrase field and the upgrade');
ok(/d\.upgradable/.test(csJs) && /SL\(\)\.upgrade/.test(csJs) && /s\.lock === 'passphrase'/.test(csJs), 'pre-authorise dialog: passphrase fields and the upgrade');
ok(/r\.upgradable/.test(fxJs) && /L\.upgrade/.test(fxJs) && /x\.lock === 'passphrase'/.test(fxJs), 'factory start: passphrase fields and the upgrade');

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
