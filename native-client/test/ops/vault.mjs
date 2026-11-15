// operations.mjs's "vault" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
//
// The Secrets tab (src/ops/vault.rs, ui/ops/vault.slint): empty, the add
// form's rule, a secret saved, Reveal through the PIN modal (empty, wrong,
// right), Copy, Hide, leaving the tab forgets the value, Edit (name only;
// a new value with a wrong then the right lock; a new passphrase), the
// lock-update offer, Delete (confirmed), and a pending request (the badge,
// Approve through the PIN modal, Deny).
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const PASS = 'open-sesame';

/** Claude asking for a secret: secret_get through the chat's MCP key (not awaited: it waits for the owner). */
export async function mcpAsk(T, t, name) {
  if (!T.secChat) {
    T.secChat = (await T.api.http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Secrets', skipPermissions: true })).body?.id;
  }
  const DATA = process.env.CRUNDI_TEST_DATA || `/tmp/crundi-devtest-${new URL(T.SERVER).port}/data`;
  const file = join(DATA, 'dev-mcp', `${T.secChat}.json`);
  const end = Date.now() + 20000;
  while (!existsSync(file) && Date.now() < end) await t.sleep(250);
  if (!existsSync(file)) return null;
  const KEY = JSON.parse(readFileSync(file, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY;
  return fetch(T.SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool: 'secret_get', args: { sessionId: T.secChat, alias: 'demo', name, reason: 'e2e' } }) })
    .then((r) => r.json()).catch((e) => ({ ok: false, error: String(e) }));
}

export async function run(T, t) {
  const { check } = t;
  const http = T.api.http;
  const V = (p) => p.areas.ops.vault;
  const until = (what, f, ms = 10000) => T.until(what, (p) => f(V(p), p), ms);
  const act = (...a) => T.act(...a);

  // ── Empty ──
  await T.ui('tab', 'secrets');
  let p = await until('the Secrets tab loads', (v) => v.loaded && v.error === '');
  if (p) check('empty: no secrets, no requests, no badge', V(p).secrets.length === 0 && V(p).requests.length === 0 && V(p).badge === '', V(p));
  await T.snap('vault-empty');

  // ── The add form's rule ──
  await act('vault-add-set', 'name', 'E2E_ONE');
  await act('vault-add-set', 'desc', 'the first one');
  await act('vault-add-set', 'value', 'v-one');
  await act('vault-add-set', 'pin', 'short');
  let v = (await T.mine()).vault;
  check('a short passphrase: the rule warns, Save off', v.add.warn && !v.add.ok && v.add.rule === 'At least 8 characters, and not only digits.', v.add);
  await act('vault-add-set', 'pin', '12345678');
  v = (await T.mine()).vault;
  check('digits only: still warns', v.add.warn && !v.add.ok, v.add);
  await act('vault-add-set', 'pin', PASS);
  await act('vault-add-set', 'pin2', 'open-sesam');
  v = (await T.mine()).vault;
  check('not the same twice: "The two do not match."', v.add.rule === 'The two do not match.' && v.add.warn && !v.add.ok, v.add);
  await act('vault-add-set', 'pin2', PASS);
  v = (await T.mine()).vault;
  check('typed twice: Save on, no warning', v.add.ok && !v.add.warn, v.add);
  await T.snap('vault-add-filled');
  await act('vault-add-save');
  p = await until('saved: listed, the form empty', (v, q) => v.secrets.some((s) => s.name === 'E2E_ONE') && v.add.name === '' && q.toast === 'Secret saved');
  const listed = (await http('GET', '/api/secrets')).body?.secrets || [];
  check('the server has it', listed.some((s) => s.name === 'E2E_ONE' && s.description === 'the first one'), listed);
  await http('POST', '/api/secrets', { action: 'add', name: 'E2E_TWO', description: '', value: 'v-two', pin: PASS });
  await T.ui('tab', 'workbench');
  await T.ui('tab', 'secrets');
  p = await until('back on the tab: both listed', (v) => v.secrets.length === 2);
  if (p) check('no value on screen without the lock', V(p).secrets.every((s) => !s.revealed && s.value === undefined), V(p).secrets);
  await T.snap('vault-list');

  // ── Reveal through the PIN modal ──
  await act('vault-reveal', 'E2E_ONE');
  p = await until('the PIN modal opens', (v) => v.pin !== null, 5000);
  if (p) check('its words', V(p).pin.title === 'Reveal secret' && V(p).pin.sub === 'Enter the passphrase for "E2E_ONE".' && V(p).pin.label === 'Reveal' && V(p).pin.lock === 'passphrase', V(p).pin);
  if (p) check('a covering dialog: the area is modal', V(p).modal === true);
  await T.snap('vault-pin');
  await act('vault-pin-submit');
  p = await until('empty: "Enter the passphrase"', (v) => v.pin?.error === 'Enter the passphrase', 5000);
  await act('vault-pin-type', 'not-the-one');
  await act('vault-pin-submit');
  p = await until('a wrong passphrase: the server says so, the modal stays', (v) => v.pin && v.pin.error !== '' && v.pin.error !== 'Enter the passphrase' && !v.pin.busy, 10000);
  if (p) check('nothing revealed', !V(p).secrets.some((s) => s.revealed));
  await T.snap('vault-pin-wrong');
  await act('vault-pin-type', PASS);
  await act('vault-pin-submit');
  p = await until('the right one: the value shows', (v) => v.pin === null && v.secrets.find((s) => s.name === 'E2E_ONE')?.value === 'v-one', 10000);
  if (p) check('only that one', !V(p).secrets.find((s) => s.name === 'E2E_TWO').revealed);
  await T.snap('vault-revealed');
  await act('vault-copy', 'E2E_ONE');
  p = await T.until('Copy: "Copied"', (q) => q.toast === 'Copied' || q.toast === 'Copy failed', 5000);
  const clip = await act('vault-clipboard');
  if (p?.toast === 'Copied') check('the clipboard holds the value', clip.text === 'v-one', clip);
  await act('vault-hide', 'E2E_ONE');
  p = await until('Hide: gone from the screen', (v) => !v.secrets.find((s) => s.name === 'E2E_ONE').revealed, 5000);
  // Revealed again, then the tab is left: forgotten.
  await act('vault-reveal', 'E2E_ONE');
  await until('the modal again', (v) => v.pin !== null, 5000);
  await act('vault-pin-type', PASS);
  await act('vault-pin-submit');
  await until('revealed again', (v) => v.secrets.find((s) => s.name === 'E2E_ONE')?.revealed, 10000);
  await T.ui('tab', 'workbench');
  await T.ui('tab', 'secrets');
  p = await until('leaving the tab forgets the value', (v) => v.secrets.length === 2 && v.secrets.every((s) => !s.revealed), 5000);

  // ── Edit ──
  await act('vault-edit', 'E2E_ONE');
  p = await until('Edit opens', (v) => v.edit !== null, 5000);
  if (p) check('Edit: the lock is a passphrase, nothing changed, Save off', V(p).edit.lock === 'passphrase' && !V(p).edit.ok && !V(p).edit.relock, V(p).edit);
  await act('vault-edit-set', 'desc', 'changed words');
  p = await until('a new description: Save on without a lock', (v) => v.edit?.ok === true, 5000);
  await T.snap('vault-edit');
  await act('vault-edit-save');
  p = await until('saved', (v, q) => v.edit === null && q.toast === 'Secret saved', 10000);
  p = await until('the new description shows', (v) => v.secrets.find((s) => s.name === 'E2E_ONE')?.desc === 'changed words', 5000);
  await act('vault-edit', 'E2E_ONE');
  await until('Edit again', (v) => v.edit !== null, 5000);
  await act('vault-edit-set', 'value', 'v-one-b');
  v = (await T.mine()).vault;
  check('a new value needs the current lock', !v.edit.ok, v.edit);
  await act('vault-edit-set', 'cur', 'wrong-pass-1');
  await act('vault-edit-save');
  p = await until('a wrong lock: the error, still open', (v) => v.edit && v.edit.err !== '' && !v.edit.busy, 10000);
  await T.snap('vault-edit-wrong');
  await act('vault-edit-relock', 'true');
  await act('vault-edit-set', 'new', 'brand-new-pass');
  await act('vault-edit-set', 'new2', 'brand-new-pas');
  v = (await T.mine()).vault;
  check('a new passphrase twice, not the same: the hint warns, Save off', v.edit.hint === 'The two do not match.' && v.edit.warn && !v.edit.ok, v.edit);
  await act('vault-edit-set', 'new2', 'brand-new-pass');
  await act('vault-edit-set', 'cur', PASS);
  await act('vault-edit-save');
  p = await until('new value and lock saved', (v, q) => v.edit === null && q.toast === 'Secret saved', 10000);
  await act('vault-reveal', 'E2E_ONE');
  await until('the modal', (v) => v.pin !== null, 5000);
  await act('vault-pin-type', 'brand-new-pass');
  await act('vault-pin-submit');
  await until('the new lock opens the new value', (v) => v.secrets.find((s) => s.name === 'E2E_ONE')?.value === 'v-one-b', 10000);
  await act('vault-hide', 'E2E_ONE');

  // ── The lock-update offer (shown after an old lock was accepted) ──
  await act('vault-up-offer', 'E2E_ONE', 'passphrase', 'brand-new-pass');
  p = await until('the offer opens', (v) => v.upgrade !== null, 5000);
  if (p) check('its words and Update off', V(p).upgrade.name === 'E2E_ONE' && !V(p).upgrade.oldPin && !V(p).upgrade.ok, V(p).upgrade);
  await T.snap('vault-upgrade');
  await act('vault-up-set', 'new', 'third-pass-1');
  await act('vault-up-set', 'new2', 'third-pass-1');
  await act('vault-up-go');
  await until('updated', (v, q) => v.upgrade === null && q.toast === 'Lock updated', 10000);
  const rv = (await http('POST', '/api/secrets', { action: 'reveal', id: listed.find((s) => s.name === 'E2E_ONE')?.id, pin: 'third-pass-1' })).body;
  check('the server takes the new lock', rv?.ok === true, rv);

  // ── Delete ──
  await act('vault-delete', 'E2E_TWO');
  p = await T.until('Delete asks first', (q) => q.dialog?.message === 'Delete secret "E2E_TWO"? This cannot be undone.', 5000);
  await T.snap('vault-delete');
  await T.dialog(true);
  await until('deleted', (v, q) => v.secrets.length === 1 && q.toast === 'Deleted', 10000);

  // ── A pending request: the badge, Approve, Deny ──
  let ask = mcpAsk(T, t, 'E2E_ONE');
  p = await until('a request: listed, the badge counts 1', (v) => v.requests.length === 1 && v.badge === '1', 30000);
  if (p) {
    check('its meta line', V(p).requests[0].name === 'E2E_ONE' && V(p).requests[0].meta === 'project: demo · e2e', V(p).requests[0]);
    await T.snap('vault-request');
    await act('vault-approve', 'E2E_ONE');
    p = await until('Approve opens the PIN modal', (v) => v.pin?.title === 'Approve access', 5000);
    if (p) check('its words', V(p).pin.sub === 'Enter the passphrase for "E2E_ONE" to release it to Claude.' && V(p).pin.label === 'Approve', V(p).pin);
    await act('vault-pin-type', 'third-pass-1');
    await act('vault-pin-submit');
    p = await until('approved: the request and the badge go', (v, q) => v.pin === null && v.requests.length === 0 && v.badge === '' && q.toast === 'Approved', 15000);
    const got = await Promise.race([ask, t.sleep(10000).then(() => null)]);
    check('Claude got the value', got && JSON.stringify(got).includes('v-one-b'), got);
    ask = mcpAsk(T, t, 'E2E_ONE');
    await until('another request', (v) => v.requests.length === 1, 30000);
    await act('vault-deny', 'E2E_ONE');
    await until('Deny: gone', (v) => v.requests.length === 0 && v.badge === '', 10000);
    const denied = await Promise.race([ask, t.sleep(10000).then(() => null)]);
    check('Claude was refused', denied && !JSON.stringify(denied).includes('v-one-b'), denied);
  } else check('a request reached the app (skipped the rest)', false);
}
