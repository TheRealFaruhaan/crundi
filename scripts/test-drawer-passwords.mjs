#!/usr/bin/env node
// app/drawer-passwords.js: the browser drawer's saved passwords (the store,
// its checks, the scripts it puts in pages, and what is redacted from chats).
// A stand-in cipher plays safeStorage. The real flow, in Electron (save after
// a sign-in, fill, update, the PIN), is in scripts/test-browser-drawer.mjs.
//
// Run: node scripts/test-drawer-passwords.mjs

import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createPasswordStore, safeStorageBackend, originOf, redactSecrets, captureScript, fillScript } from '../app/drawer-passwords.js';

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : '  ' + String(extra).slice(0, 300)}`); if (!ok) failed++; };

const dir = mkdtempSync(join(tmpdir(), 'crundi-pw-'));
const file = join(dir, 'drawer-passwords.json');
let backendOk = true;
const enc = (s) => Buffer.from('E' + Buffer.from(s, 'utf8').toString('hex'));
const dec = (b) => { const t = String(b); if (t[0] !== 'E') throw new Error('not ours'); return Buffer.from(t.slice(1), 'hex').toString('utf8'); };
const mk = () => createPasswordStore({ file, encrypt: enc, decrypt: dec, backend: () => (backendOk ? { ok: true } : { ok: false, reason: 'no keyring' }) });

try {
  // ── Sites ──
  check('a site is scheme, host and port', originOf('https://a.example.com:8443/login?x=1#y') === 'https://a.example.com:8443');
  check('only web pages have one', originOf('file:///etc/passwd') === null && originOf('about:blank') === null && originOf('nonsense') === null);

  // ── Saving ──
  const st = mk();
  check('an empty store lists nothing', st.list().length === 0 && st.count('https://x.com') === 0);
  check('a new login is "new"', st.check('https://x.com', 'ali', 'pw1') === 'new');
  const a = st.save({ origin: 'https://x.com', username: 'ali', password: 'pw1' });
  check('saved: listed with no password in it', st.list().length === 1 && a.username === 'ali' && !('password' in a) && !('pw' in a));
  const raw = readFileSync(file, 'utf8');
  check('the file never holds the password as text', !raw.includes('pw1') && raw.includes(Buffer.from('E' + Buffer.from('pw1').toString('hex')).toString('base64')), raw);
  if (process.platform !== 'win32') check('the file is private (0600)', (statSync(file).mode & 0o777) === 0o600, (statSync(file).mode & 0o777).toString(8));
  check('the same login again is "same" (nothing to ask)', st.check('https://x.com', 'ali', 'pw1') === 'same');
  check('another password for that user is "update"', st.check('https://x.com', 'ali', 'pw2') === 'update');
  check('another user on the site is "new"', st.check('https://x.com', 'sara', 'pw1') === 'new');
  check('another site is "new" (exact site, as Chrome)', st.check('https://www.x.com', 'ali', 'pw1') === 'new');
  st.save({ origin: 'https://x.com', username: 'ali', password: 'pw2' });
  check('saving it updates the one login, not a second', st.list().length === 1 && st.password(a.id).password === 'pw2');
  st.save({ origin: 'https://x.com', username: 'sara', password: 's' });
  check('count per site', st.count('https://x.com') === 2 && st.list('https://x.com').length === 2);
  const again = mk();
  check('kept across a restart', again.list().length === 2 && again.password(a.id).password === 'pw2');
  let threw = null; try { st.save({ origin: 'javascript:alert(1)', username: 'x', password: 'y' }); } catch (e) { threw = e.message; }
  check('a non-site is refused', !!threw);
  threw = null; try { st.save({ origin: 'https://x.com/path', username: 'x', password: 'y' }); } catch (e) { threw = e.message; }
  check('a URL that is not just a site is refused', !!threw);

  // ── Never, update, remove, clear ──
  st.never('https://y.com');
  check('"never" for a site: nothing is offered there', st.check('https://y.com', 'u', 'p') === 'never' && st.neverList().includes('https://y.com'));
  st.unnever('https://y.com');
  check('...until it is taken off the list', st.check('https://y.com', 'u', 'p') === 'new');
  threw = null; try { st.update(a.id, { username: 'sara' }); } catch (e) { threw = e.message; }
  check('renaming onto another login of the same site is refused', /already has/.test(threw || ''), threw);
  st.update(a.id, { username: 'ali2', password: 'pw3' });
  check('update changes username and password', st.password(a.id).username === 'ali2' && st.password(a.id).password === 'pw3');
  check('remove', st.remove(a.id) === true && st.list().length === 1 && st.remove('nope') === false);
  st.clear();
  check('clear empties everything', st.list().length === 0 && mk().list().length === 0);

  // ── No safe storage: nothing is saved or read ──
  backendOk = false;
  const nb = mk();
  check('status says why', nb.status().available === false && nb.status().reason === 'no keyring');
  threw = null; try { nb.save({ origin: 'https://x.com', username: 'a', password: 'b' }); } catch (e) { threw = e.message; }
  check('without safe storage nothing is saved', threw === 'no keyring' && nb.list().length === 0, threw);
  backendOk = true;

  // ── safeStorage, as Electron gives it ──
  check('safeStorage unavailable: refused', safeStorageBackend({ isEncryptionAvailable: () => false }).ok === false);
  check('Linux with no keyring (basic_text): refused', safeStorageBackend({ isEncryptionAvailable: () => true, getSelectedStorageBackend: () => 'basic_text' }, 'linux').ok === false);
  check('Linux with a keyring: fine', safeStorageBackend({ isEncryptionAvailable: () => true, getSelectedStorageBackend: () => 'gnome_libsecret' }, 'linux').ok === true);
  check('Windows (DPAPI): fine', safeStorageBackend({ isEncryptionAvailable: () => true }, 'win32').ok === true);

  // ── The page scripts ──
  let parses = true; try { new Function(captureScript('\u2063crundi-pw:x:')); new Function(fillScript('https://x.com', 'a"b', "c'd\\e</script>")); } catch (e) { parses = e.message; }
  check('the capture and fill scripts are valid JavaScript, whatever the login holds', parses === true, parses);
  const cs = captureScript('PFX-123:');
  check('the capture script reports only through its prefixed console messages', cs.includes('"PFX-123:"') && /console\.debug\.bind/.test(cs) && !/fetch\(|XMLHttpRequest|postMessage|sendBeacon/.test(cs));
  check('fill checks it is still on the saved site', /location\.origin !== o/.test(fillScript('https://x.com', 'u', 'p')));

  // ── Redaction (what a chat reads back) ──
  const r = redactSecrets({ result: 'pw is hunter2!', logs: [{ text: 'sent hunter2' }], data: 'aHVudGVyMg==hunter2', n: 3 }, new Set(['hunter2']));
  check('a filled password is taken out of what a chat reads', r.result === 'pw is ••••••••!' && r.logs[0].text === 'sent ••••••••' && r.n === 3, JSON.stringify(r));
  check('...but not from a screenshot\'s bytes', r.data === 'aHVudGVyMg==hunter2');
  check('very short ones are left (they would mangle ordinary text)', redactSecrets({ result: 'abc' }, new Set(['abc'])).result === 'abc');
  check('nothing to redact: the same object', (() => { const o = { a: 1 }; return redactSecrets(o, new Set()) === o; })());
} finally {
  rmSync(dir, { recursive: true, force: true });
}

console.log(failed ? `\n${failed} failure(s).` : '\nAll passed.');
process.exit(failed ? 1 : 0);
