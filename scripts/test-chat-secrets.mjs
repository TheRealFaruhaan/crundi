#!/usr/bin/env node
/**
 * test-chat-secrets.mjs — secrets pre-authorised for ONE chat
 * (src/chat-secrets.js, its wiring in src/webapp.js, capability
 * 'chat-secret-preauth').
 *
 * Covers: each PIN checked against its secret before anything is kept (a
 * wrong one keeps none of the batch); secret_run from that chat using it
 * without asking the person, the value reaching the command; secret_get
 * still asking every time; expiry; revoke and revoke all; one chat's grant
 * not reaching another chat; "until this chat ends" ending with the chat;
 * nothing written anywhere (the data folder, JSON, inspect, the list); and a
 * collaborator refused (the routes, the event, a collaborator's chat).
 *
 * Run: node scripts/test-chat-secrets.mjs
 */

import { mkdtempSync, mkdirSync, readdirSync, readFileSync, statSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inspect } from 'node:util';

const base = mkdtempSync(join(tmpdir(), 'chat-secrets-'));
process.env.DATA_DIR = join(base, 'data');
mkdirSync(process.env.DATA_DIR, { recursive: true });
const CS = await import('../src/chat-secrets.js');
const secrets = await import('../src/secrets-store.js');
const { runWithSecret } = await import('../src/secret-run.js');
const { authoriseSecretUse } = await import('../src/factory-secrets.js');
const { mayAccess, ROLE_OWNER, ROLE_COLLABORATOR, COLLABORATOR_MCP_TOOLS } = await import('../src/access-policy.js');
const { CAPABILITIES } = await import('../src/client-version.js');

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const VALUE1 = 'tok-CHAT-one-5d2e81', PIN1 = '318274';
const VALUE2 = 'tok-CHAT-two-a7b9c3', PIN2 = '905163';
const S1 = secrets.addSecret({ name: 'Deploy token', value: VALUE1, pin: PIN1 }).id;
const S2 = secrets.addSecret({ name: 'Other token', value: VALUE2, pin: PIN2 }).id;
const check = (id, pin) => { const d = secrets.decryptSecret(id, pin); return d.ok ? { ok: true, name: d.name } : { ok: false, error: d.error }; };
const decrypt = (id, pin) => secrets.decryptSecret(id, pin);

let clock = 1_000_000;
const now = () => clock;
const changes = [];
const vault = CS.createChatSecretVault({ now, timers: false, onChange: (c) => changes.push(c) });

// ─── Capability ───
ok(CAPABILITIES.includes('chat-secret-preauth'), "the handshake lists 'chat-secret-preauth'");

// ─── Durations ───
ok(CS.parseDuration({ duration: '15m' }).ms === 15 * 60000, '15 minutes preset');
ok(CS.parseDuration({ duration: '8h' }).ms === 8 * 3600000, '8 hours preset');
ok(CS.parseDuration({ duration: 'chat' }).untilChatEnds === true, '"until this chat ends" preset');
ok(CS.parseDuration({ untilChatEnds: true }).untilChatEnds === true, 'untilChatEnds flag');
ok(CS.parseDuration({ minutes: 90 }).ms === 90 * 60000, 'minutes');
ok(!CS.parseDuration({ minutes: 0 }).ok && !CS.parseDuration({ minutes: 100000 }).ok && !CS.parseDuration({}).ok && !CS.parseDuration({ duration: 'year' }).ok, 'nonsense durations refused');

// ─── The PIN is verified ───
let r = CS.grantChatSecrets({ vault, chatId: 'chatA', secrets: [{ id: S1, pin: PIN1 }, { id: S2, pin: '000000' }], duration: { duration: '1h' }, decrypt: check, now });
ok(!r.ok && r.secretId === S2 && /Incorrect PIN/.test(r.error), 'a wrong PIN is refused, naming the secret');
ok(vault.list('chatA').length === 0, 'and none of the batch is kept, not even the right one');
r = CS.grantChatSecrets({ vault, chatId: 'chatA', secrets: [{ id: S1, pin: '12' }], duration: { duration: '1h' }, decrypt: check, now });
ok(!r.ok, 'a PIN that is not 6 digits is refused');
r = CS.grantChatSecrets({ vault, chatId: 'chatA', secrets: [], duration: { duration: '1h' }, decrypt: check, now });
ok(!r.ok, 'no secrets chosen is refused');
r = CS.grantChatSecrets({ vault, chatId: 'chatA', secrets: [{ id: S1, pin: PIN1 }], duration: { duration: '1h' }, decrypt: check, now });
ok(r.ok && r.granted.length === 1, 'the right PIN pre-authorises the secret');
const listed = vault.list('chatA');
ok(listed.length === 1 && listed[0].name === 'Deploy token' && listed[0].expiresAt === new Date(clock + 3600000).toISOString() && listed[0].untilChatEnds === false, 'the list has the name and when it expires');
ok(!JSON.stringify(listed).includes(PIN1) && !('pin' in listed[0]), 'the list carries no PIN');

// ─── secret_run uses it without asking ───
let asked = 0;
const askPerson = async () => { asked++; return { ok: false, error: 'asked' }; };
const ask = (chatId, secretId) => CS.preauthorisedAsk({ vault, chatId, secretId, decrypt, askPerson });
let a = await ask('chatA', S1)({ keepPin: false });
ok(a.ok && a.value === VALUE1 && a.preauthorised && asked === 0, 'secret_run from that chat: the value, and the person is not asked');
ok(!('pin' in a), 'the PIN is not handed back to a caller that does not keep it');
const auth = await authoriseSecretUse({ runner: null, chatId: 'chatA', secret: { id: S1, name: 'Deploy token' }, command: 'x', decrypt, askPerson: ask('chatA', S1) });
ok(auth.ok && auth.value === VALUE1 && asked === 0, "through secret_run's own decision (authoriseSecretUse)");
const run = await runWithSecret({ command: `node -e "process.stdout.write(String((process.env.SECRET||'').length))"`, value: auth.value, envName: 'SECRET' });
ok(run.ok && run.stdout.trim() === String(VALUE1.length), 'the command gets the value in its environment');
a = await ask('chatA', S2)({});
ok(!a.ok && asked === 1, 'a secret not pre-authorised still asks the person');

// ─── Per chat ───
a = await ask('chatB', S1)({});
ok(!a.ok && asked === 2, "another chat (same project or not) does not get chat A's grant");
ok(vault.list('chatB').length === 0, 'and lists nothing');

// ─── secret_get always asks ───
const web = read('src', 'webapp.js');
const getBlock = web.slice(web.indexOf("if (body.tool === 'secret_get')"), web.indexOf("if (body.tool === 'secret_run')"));
ok(getBlock.length > 100 && /waitForSecretApproval\(/.test(getBlock) && !/chatSecrets|preauthorisedAsk/.test(getBlock), 'secret_get never reads the chat grants: it asks the person every time');
const runBlock = web.slice(web.indexOf("if (body.tool === 'secret_run')"), web.indexOf('runWithSecret({', web.indexOf("if (body.tool === 'secret_run')")));
ok(/preauthorisedAsk\(\{\s*vault: chatSecrets, chatId: sid/.test(runBlock), 'secret_run asks the chat grants first, for its own chat id');

// ─── Expiry ───
clock += 3600000 + 1;
a = await ask('chatA', S1)({});
ok(!a.ok && asked === 3, 'run out: the person is asked again');
ok(vault.list('chatA').length === 0 && changes.includes('chatA'), 'and the grant is gone, with a change announced');
// The timer, for real.
const timed = CS.createChatSecretVault({ onChange: (c) => changes.push('timer:' + c) });
timed.grant('chatT', S1, { name: 'x', pin: PIN1, expiresAt: Date.now() + 40 });
await new Promise((res) => setTimeout(res, 120));
ok(!timed.pin('chatT', S1) && changes.includes('timer:chatT'), 'a timed grant drops itself when its time comes');

// ─── Revoke ───
CS.grantChatSecrets({ vault, chatId: 'chatA', secrets: [{ id: S1, pin: PIN1 }, { id: S2, pin: PIN2 }], duration: { duration: 'chat' }, decrypt: check, now });
ok(vault.list('chatA').length === 2 && vault.list('chatA').every((g) => g.untilChatEnds && g.expiresAt === null), 'two secrets until the chat ends');
ok(vault.revoke('chatA', S1) && !vault.pin('chatA', S1) && !!vault.pin('chatA', S2), 'revoking one leaves the other');
CS.grantChatSecrets({ vault, chatId: 'chatA', secrets: [{ id: S1, pin: PIN1 }], duration: { duration: '15m' }, decrypt: check, now });
ok(vault.revokeChat('chatA') === 2 && vault.list('chatA').length === 0, 'revoke all');

// ─── The chat ends ───
CS.grantChatSecrets({ vault, chatId: 'chatE', secrets: [{ id: S1, pin: PIN1 }], duration: { duration: 'chat' }, decrypt: check, now });
const ui = read('src', 'claude-ui.js');
ok(/chatEnded\(s\.id, 'exit'\)/.test(ui) && /chatEnded\(id, 'close'\)/.test(ui) && /onChatEnded,/.test(ui), "claude-ui tells when a chat's Claude exits or the chat is closed");
ok(/claudeUi\.onChatEnded\(\(sid\) => \{ if \(chatSecrets\.revokeChat\(sid\)\)/.test(web), 'and the server drops that chat\'s grants then');
vault.revokeChat('chatE');
ok(!vault.pin('chatE', S1), 'after which nothing is left');

// ─── A recreated secret ───
CS.grantChatSecrets({ vault, chatId: 'chatR', secrets: [{ id: S2, pin: PIN2 }], duration: { duration: 'chat' }, decrypt: check, now });
ok(vault.forgetSecret(S2).includes('chatR') && !vault.pin('chatR', S2), 'deleting a secret drops it from every chat');
vault.grant('chatR', S2, { name: 'Other token', pin: '111111', expiresAt: null });
a = await ask('chatR', S2)({});
ok(!a.ok && !vault.pin('chatR', S2), 'a kept PIN that no longer opens the secret is dropped and the person asked');

// ─── Nothing persisted ───
CS.grantChatSecrets({ vault, chatId: 'chatP', secrets: [{ id: S1, pin: PIN1 }, { id: S2, pin: PIN2 }], duration: { duration: '8h' }, decrypt: check, now });
ok(JSON.stringify(vault) === '{}' && !inspect(vault, { depth: 9 }).includes(PIN1), 'JSON.stringify and inspect of the vault show nothing');
const walk = (d) => readdirSync(d).flatMap((f) => { const p = join(d, f); return statSync(p).isDirectory() ? walk(p) : [p]; });
const onDisk = walk(process.env.DATA_DIR).map((f) => readFileSync(f, 'utf8')).join('\n');
ok(!onDisk.includes(PIN1) && !onDisk.includes(PIN2) && !onDisk.includes('chatP'), 'nothing about a grant, PIN or chat, is written to the data folder');
const modSrc = read('src', 'chat-secrets.js');
ok(!/console\.|writeFile|appendFile|from 'fs'|from 'node:fs'/.test(modSrc), 'the module neither logs nor writes');
const routeBlock = web.slice(web.indexOf("if (path === '/api/secrets/chat')"), web.indexOf('// ─── Secrets (global) ───'));
ok(routeBlock.length > 200 && !/console\.|journal|appendFile|writeFile/.test(routeBlock), 'the route neither logs nor writes');
ok(/event: chat-secrets[^\n]*grants: chatSecrets\.list\(sessionId\)/.test(web), 'the event carries the list (names and times) only');

// ─── Collaborators ───
const owner = { role: ROLE_OWNER, projects: [], roots: {} };
const collab = { role: ROLE_COLLABORATOR, collabKey: 'pc:sam', projects: ['demo'], roots: { demo: '/tmp/wt' } };
ok(mayAccess(owner, 'POST', '/api/secrets/chat') && mayAccess(owner, 'GET', '/api/secrets/chat'), 'the owner reaches the routes');
ok(!mayAccess(collab, 'POST', '/api/secrets/chat') && !mayAccess(collab, 'GET', '/api/secrets/chat'), 'a collaborator is refused the routes');
ok(/if \(path === '\/api\/secrets\/chat'\) \{\s*if \(isConfined\(principal\)\) return json\(res, \{ ok: false, error: 'Not allowed' \}, 403\);/.test(web), 'and refused again by name in the route');
ok(/function broadcastChatSecrets[\s\S]{0,400}if \(isConfined\(client\.principal\)\) continue;/.test(web), "a collaborator's stream never hears of a grant");
ok(/if \(t\.collaborator\) return \{ error: "Secrets cannot be pre-authorised for a collaborator's chat\." \}/.test(web), "nothing can be pre-authorised for a collaborator's chat");
ok(![...COLLABORATOR_MCP_TOOLS].some((t) => /secret_(run|get)/.test(String(t))), "a collaborator's Claude has no secret tools to use a grant with");
ok(/allowForChat[\s\S]{0,200}!isConfined\(principal\)/.test(web), '"Allow for this chat" on approve is for the owner only');

// ─── Clients ───
const html = read('src', 'webapp-html.js');
const vend = read('app', 'vendor', 'crundi-chat-secrets.js');
ok(html.includes('/vendor/crundi-chat-secrets.js') && /data-action="chat-secrets"/.test(html) && /case 'chat-secrets':/.test(html), 'the workbench has the lock in the chat header (folded into its menu when narrow), wired to the dialog');
ok(/allowForChat: allow/.test(html) && /CrundiChatSecrets\.allowRow\(sreqAllow\)/.test(html), 'the workbench PIN prompt offers "Allow for this chat"');
ok(/es\.addEventListener\('chat-secrets'/.test(html), 'the workbench hears grants change');
ok(/var CAP = 'chat-secret-preauth'/.test(vend) && /capabilities/.test(vend) && /does not support this yet/.test(vend), 'the shared script asks for the capability, and an older server is told apart in plain words');
ok(!/\b(prompt|confirm|alert)\(/.test(vend), 'no browser dialogs (the phone app shows none)');
ok(!/localStorage|sessionStorage/.test(vend), 'nothing about a PIN is kept in the browser');

rmSync(base, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
