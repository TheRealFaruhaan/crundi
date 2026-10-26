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
// Claude is told 20; up to 25 is accepted so a near miss costs no retry.
ok(cleanAutoTitle('1234567890123456789012345').ok, 'up to 25 characters is accepted quietly');
const long = cleanAutoTitle('12345678901234567890123456');
ok(!long.ok && /26 characters/.test(long.error) && /limit is 20\b/.test(long.error), '26 characters is refused, with the count, not cut; the stated limit stays 20', long.error);
ok(cleanAutoTitle('ދިވެހި ޓައިޓަލް').ok, 'length is counted in characters, not bytes');
ok(!cleanAutoTitle('').ok && !cleanAutoTitle('   ').ok && !cleanAutoTitle('""').ok && !cleanAutoTitle(null).ok, 'an empty title is refused');
ok(cleanAutoTitle('Line one\nline two').title === 'Line one line two', 'a line break becomes a space');
ok(MAX_LONG_TITLE === 50, 'the long form allows 50 characters');
ok(cleanLongTitle('Fix the redirect loop after password login').ok, 'long: a 42-character title passes');
ok(cleanLongTitle('x'.repeat(55)).ok && !cleanLongTitle('x'.repeat(56)).ok, 'long: 55 passes quietly, 56 is refused');
ok(cleanLongTitle('').ok && cleanLongTitle('').title === '' && cleanLongTitle(undefined).title === '', 'long: leaving it out is fine');

// The person's own title wins, in both kinds of session.
const src = (await import('node:fs')).readFileSync(new URL('../src/claude-ui.js', import.meta.url), 'utf8')
  + (await import('node:fs')).readFileSync(new URL('../src/claude-terminals.js', import.meta.url), 'utf8');
ok((src.match(/if \(auto && \w+\.titleByUser\) return \{ ok: false/g) || []).length === 2, 'chat and terminal both refuse an automatic rename over the person\'s title');
ok((src.match(/if \(!auto\) \w+\.titleByUser = true;/g) || []).length === 2, 'chat and terminal both remember a rename the person made');

const bridge = (await import('node:fs')).readFileSync(new URL('../src/mcp-stdio.js', import.meta.url), 'utf8');
ok(!/name: 'rename_chat'/.test(bridge), 'rename_chat is gone: Claude Code names the session itself');
const { COLLABORATOR_MCP_TOOLS } = await import('../src/access-policy.js');
ok(!COLLABORATOR_MCP_TOOLS.has('rename_chat'), 'not offered to collaborators (their session is not checked for ownership yet)');
const { BASE, MAX_LAYER } = await import('../src/system-prompt.js');
ok(!/rename_chat|name the chat/i.test(BASE), 'the base prompt no longer asks Claude to name its chat');
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

  // Claude Code's own title, from the chat itself, comes first.
  {
    const { titlesFromChat } = await import('../src/chat-autotitle.js');
    const tf = titlesFromChat('Directory listing and a.txt');
    ok(tf && tf.title === 'Directory listing' && tf.long === 'Directory listing and a.txt', 'from chat: the long form as given, the short one cut at a word');
    ok(titlesFromChat('Kanban trigger').long === '', 'from chat: a short title has no separate long form');
    ok(titlesFromChat('Product categorization duplicates').title === 'Product categorization', 'from chat: cut within what a title may be (25), not to one word', JSON.stringify(titlesFromChat('Product categorization duplicates')));
    ok(titlesFromChat('Payments service staging Docker deployment').title === 'Payments service staging', 'from chat: a longer one cut at a word', JSON.stringify(titlesFromChat('Payments service staging Docker deployment')));
    ok(titlesFromChat('') === null && titlesFromChat('Supercalifragilisticexpialidociousness') === null, 'from chat: nothing, or no word that fits, is no title');
    let spawned = 0;
    titles.f = 'Chat';
    const viaChat = createAutoTitler({
      claudeBin: () => '/bin/claude', currentTitle: (id) => titles[id] ?? null,
      apply: (id, t, long) => { titles[id] = t; longs[id] = long; return { ok: true }; },
      fromChat: async () => 'Payment retry logic', spawnImpl: fake('SHORT: Other\nLONG: Other', () => { spawned++; }),
    });
    viaChat.maybe('f', 'refactor the payment retry logic'); await wait();
    ok(titles.f === 'Payment retry logic' && spawned === 0, 'from chat: used when the chat gives one, with no extra process');
    titles.g = 'Chat';
    const fallback = createAutoTitler({
      claudeBin: () => '/bin/claude', currentTitle: (id) => titles[id] ?? null,
      apply: (id, t, long) => { titles[id] = t; longs[id] = long; return { ok: true }; },
      fromChat: async () => null, spawnImpl: fake('SHORT: Port lookup\nLONG: Find the port'),
    });
    fallback.maybe('g', 'what port?'); await wait();
    ok(titles.g === 'Port lookup', 'from chat: none given, the one-off call names it');
  }
}

// ─── A Claude terminal pane takes Claude Code's window title ───
{
  const { claudeWindowTitle, windowTitlesIn } = await import('../src/claude-terminals.js');
  ok(claudeWindowTitle('\u2733 Package.json name field') === 'Package.json name field', 'terminal: the spinner glyph goes, the title stays');
  ok(claudeWindowTitle('\u25d1 Package.json name field') === 'Package.json name field', 'terminal: whichever glyph the spinner is on');
  ok(claudeWindowTitle('Claude Code') === '' && claudeWindowTitle('\u2733 Claude Code') === '' && claudeWindowTitle('  ') === '', 'terminal: the default title is not a name');
  const data = 'x\x1b]0;\u2733 First title\x07y\x1b]2;\u2722 Second\x1b\\z';
  ok(JSON.stringify(windowTitlesIn(data)) === JSON.stringify(['\u2733 First title', '\u2722 Second']), 'terminal: OSC 0 and 2, ended by BEL or ST, in order');
  ok(windowTitlesIn('plain output').length === 0, 'terminal: ordinary output has none');
  const tsrc = (await import('node:fs')).readFileSync(new URL('../src/claude-terminals.js', import.meta.url), 'utf8');
  ok(/if \(shellOnly \|\| rawCommand/.test(tsrc) && /entry\.titleByUser\) continue/.test(tsrc), 'terminal: not a shell, not over a title the person typed');
}

try { rmSync(tmp, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll chat-title checks passed.');
process.exit(failed ? 1 : 0);
