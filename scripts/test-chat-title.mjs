#!/usr/bin/env node
// The title Claude may give its own chat: short, plain, and never over one the
// person typed.
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const tmp = mkdtempSync(join(tmpdir(), 'crundi-title-'));
writeFileSync(join(tmp, 'env'), '');
process.env.DATA_DIR = join(tmp, 'data');
process.env.DOTENV_PATH = join(tmp, 'env');
mkdirSync(process.env.DATA_DIR, { recursive: true });

let failed = 0;
const ok = (cond, name, detail = '') => {
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`);
  if (!cond) failed++;
};

const { cleanAutoTitle, cleanLongTitle, MAX_AUTO_TITLE, MAX_LONG_TITLE } = await import('../src/chat-title.js');

ok(MAX_AUTO_TITLE === 20, 'the limit is 20 characters');
ok(cleanAutoTitle('Fix login redirect').title === 'Fix login redirect', 'a plain title passes');
ok(cleanAutoTitle('  "Orders   dashboard".  ').title === 'Orders dashboard', 'quotes, extra spaces and a trailing full stop are dropped');
ok(cleanAutoTitle('12345678901234567890').ok, 'exactly 20 characters passes');
const long = cleanAutoTitle('123456789012345678901');
ok(!long.ok && /21 characters/.test(long.error), '21 characters is refused, with the count, not cut', long.error);
ok(cleanAutoTitle('ދިވެހި ޓައިޓަލް').ok, 'length is counted in characters, not bytes');
ok(!cleanAutoTitle('').ok && !cleanAutoTitle('   ').ok && !cleanAutoTitle('""').ok && !cleanAutoTitle(null).ok, 'an empty title is refused');
ok(cleanAutoTitle('Line one\nline two').title === 'Line one line two', 'a line break becomes a space');
ok(MAX_LONG_TITLE === 50, 'the long form allows 50 characters');
ok(cleanLongTitle('Fix the redirect loop after password login').ok, 'long: a 42-character title passes');
ok(cleanLongTitle('x'.repeat(50)).ok && !cleanLongTitle('x'.repeat(51)).ok, 'long: 50 passes, 51 is refused');
ok(cleanLongTitle('').ok && cleanLongTitle('').title === '' && cleanLongTitle(undefined).title === '', 'long: leaving it out is fine');

// The person's own title wins, in both kinds of session.
const src = (await import('node:fs')).readFileSync(new URL('../src/claude-ui.js', import.meta.url), 'utf8')
  + (await import('node:fs')).readFileSync(new URL('../src/claude-terminals.js', import.meta.url), 'utf8');
ok((src.match(/if \(auto && \w+\.titleByUser\) return \{ ok: false/g) || []).length === 2, 'chat and terminal both refuse an automatic rename over the person\'s title');
ok((src.match(/if \(!auto\) \w+\.titleByUser = true;/g) || []).length === 2, 'chat and terminal both remember a rename the person made');

const bridge = (await import('node:fs')).readFileSync(new URL('../src/mcp-stdio.js', import.meta.url), 'utf8');
ok(/name === 'rename_chat'\) \{\s*delete args\.sessionId; delete args\.terminalId;/.test(bridge), 'the session to rename comes from the process, never from the caller');
const { COLLABORATOR_MCP_TOOLS } = await import('../src/access-policy.js');
ok(!COLLABORATOR_MCP_TOOLS.has('rename_chat'), 'not offered to collaborators (their session is not checked for ownership yet)');
const { BASE, MAX_LAYER } = await import('../src/system-prompt.js');
ok(/Always name the chat first/.test(BASE) && /at most 20 characters/.test(BASE) && /at most 50 characters/.test(BASE) && /until the scope or topic of the work changes/.test(BASE), 'the base prompt: rename first on a task with both forms, not again until the scope changes');
ok(BASE.length < MAX_LAYER, 'the base prompt still fits in one layer', String(BASE.length));

// ─── The server-side namer ───
{
  const { isDefaultTitle, titleFromReply, titlesFromReply, createAutoTitler } = await import('../src/chat-autotitle.js');
  ok(isDefaultTitle('Chat') && isDefaultTitle('Chat (skip perms)') && !isDefaultTitle('Fix login') && !isDefaultTitle('Nightly report'), 'only a launch title counts as unnamed');
  ok(titleFromReply('CV heading review\n') === 'CV heading review', 'reply: a plain title');
  ok(titleFromReply('NONE') === null && titleFromReply('') === null && titleFromReply(null) === null, 'reply: NONE or nothing means no title');
  ok(titleFromReply('"Refactor the payment retry logic"') === 'Refactor the payment', 'reply: too long is cut at a word, not mid-word', String(titleFromReply('"Refactor the payment retry logic"')));
  ok(titleFromReply('Supercalifragilisticexpialidocious') === null, 'reply: one over-long word gives no title rather than half a word');
  const two = titlesFromReply('SHORT: Fix login redirect\nLONG: Fix the redirect loop after password login');
  ok(two && two.title === 'Fix login redirect' && two.long === 'Fix the redirect loop after password login', 'reply: both forms are read', JSON.stringify(two));
  const over = titlesFromReply('short: Port lookup\nlong: ' + 'word '.repeat(20));
  ok(over && [...over.long].length <= 50 && !/\s$/.test(over.long) && over.long.startsWith('word word'), 'reply: an over-long LONG is cut at a word', JSON.stringify(over));
  ok(titlesFromReply('SHORT: Port lookup\nLONG: port lookup').long === '', 'reply: a long form that repeats the short one is dropped');
  ok(titlesFromReply('SHORT: Port lookup').long === '', 'reply: a missing long form is fine');
  ok(titlesFromReply('LONG: only a long one') === null, 'reply: no short title means no title');

  const { EventEmitter } = await import('node:events');
  const titles = { a: 'Chat', b: 'Chat', c: 'My name', d: 'Chat', e: 'Chat' };
  const calls = [];
  const longs = {};
  const fake = (reply, onSpawn) => (bin, args, opts) => {
    const child = new EventEmitter();
    child.stdout = new EventEmitter();
    child.stdin = { end: (text) => { calls.push({ args, text, cwd: opts.cwd }); if (onSpawn) onSpawn(); setTimeout(() => { child.stdout.emit('data', reply); child.emit('close', 0); }, 5); } };
    child.kill = () => {};
    return child;
  };
  const make = (reply, onSpawn) => createAutoTitler({
    claudeBin: () => '/bin/claude', currentTitle: (id) => (id in titles ? titles[id] : null),
    apply: (id, t, long) => { titles[id] = t; longs[id] = long; return { ok: true }; }, spawnImpl: fake(reply, onSpawn),
  });
  const wait = () => new Promise((r) => setTimeout(r, 40));

  let t = make('SHORT: Port lookup\nLONG: Find which port the server listens on');
  ok(t.maybe('a', 'what port does the server use?') === true, 'namer: starts on the first message');
  await wait();
  ok(titles.a === 'Port lookup' && longs.a === 'Find which port the server listens on', 'namer: applies both forms');
  ok(calls[0].args.includes('--strict-mcp-config') && calls[0].args[calls[0].args.indexOf('--tools') + 1] === '' && calls[0].args.includes('haiku'), 'namer: one-shot call with no tools, no MCP servers, a small model');
  ok(calls[0].text.includes('what port does the server use?') && !calls[0].args.join(' ').includes('what port'), 'namer: the message goes in as data on stdin, not as an argument');
  ok(t.maybe('a', 'and another thing') === false, 'namer: a named chat is not asked about again');
  ok(make('X').maybe('c', 'do a thing') === false && titles.c === 'My name', 'namer: a chat that already has a name is left alone');
  ok(make('X').maybe('b', '/compact') === false && make('X').maybe('b', '   ') === false, 'namer: a slash command or blank line is not a job');
  t = make('NONE'); t.maybe('b', 'hi'); await wait();
  ok(titles.b === 'Chat', 'namer: a greeting leaves the title as it was');
  ok(t.maybe('b', 'now fix the login bug') === true, 'namer: and the next message gets another go');
  await wait();
  ok(t.maybe('b', 'x') === true && (await wait(), t.maybe('b', 'y')) === false, 'namer: three attempts at most');
  // Someone names it while the call is in flight.
  t = make('Late title', () => { titles.d = 'Named meanwhile'; }); t.maybe('d', 'do the thing'); await wait();
  ok(titles.d === 'Named meanwhile', 'namer: does not overwrite a name set while it was thinking');
  ok(createAutoTitler({ claudeBin: () => null, currentTitle: () => 'Chat', apply: () => ({ ok: true }) }).maybe('e', 'job') === true, 'namer: no claude binary is survivable');
  await wait();
  ok(titles.e === 'Chat', 'namer: and leaves the title alone');
}

try { rmSync(tmp, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll chat-title checks passed.');
process.exit(failed ? 1 : 0);
