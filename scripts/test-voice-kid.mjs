#!/usr/bin/env node
/**
 * test-voice-kid.mjs — Cru's live screen and kid mode: the PIN and its
 * lockout, the kid's tools (nothing else reachable), the parent's
 * instructions (read only in kid mode, parsed), screen state and patches,
 * the game library and its validator, skill levels, the overnight workshop's
 * decision, instant lines, game events, asking the parent, the video gate,
 * the kid voice. Pure node, no server.
 * Run: node scripts/test-voice-kid.mjs
 */
import { mkdtempSync, readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import { EventEmitter } from 'node:events';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dir = mkdtempSync(join(tmpdir(), 'cru-kid-'));
process.env.DATA_DIR = dir;
const settings = await import('../src/voice/settings.js');
const kid = await import('../src/voice/kid.js');
const screenMod = await import('../src/voice/screen.js');
const skillsMod = await import('../src/voice/kid-skills.js');
const games = await import('../src/voice/kid-games.js');
const workshop = await import('../src/voice/kid-workshop.js');
const reflexMod = await import('../src/voice/kid-reflex.js');
const { createVoiceMemory } = await import('../src/voice/memory.js');
const { READ_ONLY_TOOL_NAMES } = await import('../src/voice/tools.js');
const stream = await import('../src/voice/stream.js');
const tts = await import('../src/voice/tts.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const throws = (fn, re) => { try { fn(); return false; } catch (e) { return !re || re.test(e.message); } };

// ─── The PIN ───
{
  const h = kid.hashPin('2468');
  ok(/^scrypt\$[0-9a-f]{32}\$[0-9a-f]{64}$/.test(h) && !h.includes('2468'), 'the PIN is kept as a salted scrypt hash, never itself');
  ok(kid.hashPin('2468') !== h, 'each hash has its own salt');
  ok(kid.verifyPin('2468', h) && !kid.verifyPin('2469', h) && !kid.verifyPin('', h), 'the right PIN verifies, a wrong one does not');
  ok(throws(() => kid.hashPin('12'), /4 to 8/) && throws(() => kid.hashPin('12345abc'), /4 to 8/), 'a PIN is 4 to 8 digits');
  let t = 0;
  const g = kid.createPinGuard({ now: () => t });
  for (let i = 1; i < kid.MAX_TRIES; i++) { const r = g.attempt('0000', h); ok(!r.ok && !r.locked && r.triesLeft === kid.MAX_TRIES - i, `wrong try ${i}: ${r.triesLeft} left`); }
  let r = g.attempt('0000', h);
  ok(!r.ok && r.locked && r.retryInMs === kid.LOCK_MS, 'the fifth wrong try locks it for a minute');
  ok(g.attempt('2468', h).locked, 'locked: even the right PIN waits');
  t += kid.LOCK_MS;
  for (let i = 0; i < kid.MAX_TRIES - 1; i++) g.attempt('1111', h);
  r = g.attempt('1111', h);
  ok(r.locked && r.retryInMs === kid.LOCK_MS * 2, 'the next lockout is longer (two minutes)');
  t += kid.LOCK_MS * 2;
  ok(g.attempt('2468', h).ok && g.state.lockouts === 0, 'the right PIN opens it and starts afresh');
  settings.updateSettings({});
  ok(settings.setKidOn(true).ok === false, 'kid mode cannot start without a PIN');
  settings.setKidPinHash(h);
  ok(settings.setKidOn(true, 'test').ok && settings.kidOn(), 'with a PIN it starts');
  const pub = JSON.stringify(settings.publicSettings());
  ok(!pub.includes(h) && !pub.includes('pinHash') && settings.publicSettings().kidMode.pinSet === true, 'clients see that a PIN is set, never its hash');
  ok(settings.setKidPinHash('').ok === false, 'the PIN cannot change while kid mode is on');
  settings.setKidOn(false);
  ok(settings.publicSettings().kidPauseMs === 3200 && settings.updateSettings({ kidPauseMs: 900 }).ok === false, 'kid mode waits longer before sending (3.2 s), within limits');
  ok(settings.updateSettings({ kidChattiness: 'chatty' }).ok && settings.publicSettings().kidChattiness === 'chatty' && !settings.updateSettings({ kidChattiness: 'loud' }).ok, 'chattiness: quiet, normal or chatty');
  ok(settings.updateSettings({ kidVoice: 'flux-gemma-en' }).ok && !settings.updateSettings({ kidVoice: 'evil' }).ok, 'the kid voice is a Deepgram Flux or Aura voice');
}

// ─── The parent's instructions ───
{
  const p = kid.parseInstructions(kid.KID_DEFAULT);
  ok(p.youtube.length === 8 && p.youtube.every((y) => y.kind === 'handle' && /^UC[\w-]{22}$/.test(y.channelId)), 'the default lists eight toddler channels, each with its channel id');
  ok(p.games.all && !p.name && !p.age && !p.parent, 'default: every built-in game, name, age and what the child calls the parent not set');
  const custom = kid.parseInstructions(`## About the kid\n- Name: Mia\n- Age: 3\n- What the child calls you: Daddy\n\n## Approved games\n- find-letter\n- counting (only to 5)\n\n## Approved YouTube\n- https://www.youtube.com/watch?v=dQw4w9WgXcQ (a song)\n- playlist: PLabcdefghijklmnop (songs)\n- @Blippi (Blippi)\n- UCbCmjCuTUZos6Inko4u57UQ\n- just some words\n`);
  ok(custom.name === 'Mia' && custom.age === '3' && custom.parent === 'Daddy', 'name, age and "Daddy" are read');
  ok(!custom.games.all && custom.games.ids.join() === 'find-letter,counting', 'only the listed games are approved');
  ok(custom.youtube.length === 4 && custom.youtube[0].kind === 'video' && custom.youtube[1].kind === 'playlist' && custom.youtube[2].kind === 'handle' && !custom.youtube[2].channelId && custom.youtube[3].kind === 'channel', 'videos, playlists, @handles and channel ids are told apart; other lines ignored');
  ok(kid.embedFor({ kind: 'channel', id: 'UCbCmjCuTUZos6Inko4u57UQ' }).startsWith('https://www.youtube-nocookie.com/embed/videoseries?list=UUbCmjCuTUZos6Inko4u57UQ&rel=0'), 'a channel plays as its uploads playlist (UC… → UU…), nocookie, no related videos');
  ok(kid.embedFor({ kind: 'video', id: 'dQw4w9WgXcQ' }).includes('/embed/dQw4w9WgXcQ?rel=0'), 'a video plays on its own');
  ok(kid.channelIdFromPage('<link rel="canonical" href="https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv">') === 'UCabcdefghijklmnopqrstuv', 'a channel page gives its canonical channel id');
  let calls = 0;
  const res = kid.createHandleResolver({ file: join(dir, 'h.json'), fetchImpl: async () => { calls++; return { ok: true, text: async () => '<link rel="canonical" href="https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv">' }; } });
  ok(await res.resolve('@Blippi') === 'UCabcdefghijklmnopqrstuv' && await res.resolve('blippi') === 'UCabcdefghijklmnopqrstuv' && calls === 1, 'a @handle is resolved once and kept');

  // Read only while kid mode is on.
  const mem = createVoiceMemory({ dataDir: dir, index: { add() {}, search: async () => ({ results: [] }), turns: () => [] }, guard: (path) => (settings.kidOn() && path === kid.KID_FILE ? 'read only in kid mode' : '') });
  ok(mem.notebook.list().some((f) => f.path === kid.KID_FILE && f.text === kid.KID_DEFAULT), 'a new install starts with the default kid-mode.md');
  ok(!throws(() => mem.notebook.tool.run({ command: 'str_replace', path: kid.KID_FILE, old_str: '- Name: (not set)', new_str: '- Name: Mia' })), 'outside kid mode Cru edits the parent instructions');
  settings.setKidOn(true);
  ok(throws(() => mem.notebook.tool.run({ command: 'str_replace', path: kid.KID_FILE, old_str: '- Name: Mia', new_str: '- Name: X' }), /read only/), 'in kid mode an edit is refused');
  ok(throws(() => mem.notebook.tool.run({ command: 'create', path: kid.KID_FILE, file_text: 'x' }), /read only/) && throws(() => mem.notebook.tool.run({ command: 'delete', path: kid.KID_FILE }), /read only/), 'and so are create and delete');
  ok(throws(() => mem.notebook.tool.run({ command: 'rename', old_path: kid.KID_FILE, new_path: '/memories/x.md' }), /read only/) && throws(() => mem.notebook.save(kid.KID_FILE, ''), /read only/), 'and rename, and the owner\'s Settings edit');
  ok(!throws(() => mem.notebook.tool.run({ command: 'create', path: '/memories/other.md', file_text: 'fine' })), 'other notebook files are not affected');
  settings.setKidOn(false);
}

// ─── The live screen ───
let changes = 0;
const scrFile = join(dir, 'screen.json');
let kidNow = false;
const lib = games.createGameLibrary({ dir: join(dir, 'kid-games'), initFor: (s) => ({ type: 'init', levels: { letters: 2 }, mastery: {}, focus: { letters: ['G', 'H'] }, avoid: {}, asked: s }) });
const scr = screenMod.createScreen({ file: scrFile, onChange: () => changes++, kid: () => kidNow, gameDoc: (id, o) => lib.doc(id, o) });
{
  const s1 = scr.show({ title: 'Builds', blocks: [{ type: 'heading', text: 'Today' }, { id: 'l', type: 'list', items: ['a', 'b'] }, { id: 'p', type: 'progress', label: 'Tests', value: 0.5 }, { type: 'chips', items: [{ text: 'ok', tone: 'ok' }, { text: 'x', tone: 'weird' }] }] });
  ok(s1.blocks.length === 4 && s1.blocks[2].value === 50 && s1.blocks[3].items[1].tone === 'info', 'show_screen: blocks cleaned (progress as %, unknown tones as info)');
  ok(changes === 1 && JSON.parse(readFileSync(scrFile, 'utf8')).screen.title === 'Builds', 'the state is kept on the server and pushed');
  scr.update({ append: [{ id: 'l', items: ['c'] }], replace: [{ id: 'p', type: 'progress', label: 'Tests', value: 100 }], add: [{ id: 't', type: 'table', rows: [['a', '1']] }], title: 'Builds now' });
  const s2 = scr.get();
  ok(s2.title === 'Builds now' && s2.blocks[1].items.join() === 'a,b,c' && s2.blocks[2].value === 100 && s2.blocks.length === 5, 'update_screen: append, replace by id, add, title');
  scr.update({ remove: ['t'] });
  ok(scr.get().blocks.length === 4 && s2.rev < scr.get().rev, 'remove by id; each change has a new rev');
  ok(throws(() => scr.update({ replace: [{ id: 'nope', type: 'text', text: 'x' }] }), /No block/), 'a patch to a missing block is refused');
  ok(throws(() => scr.show({ blocks: [{ type: 'script' }] }), /Unknown block/), 'unknown block types are refused');
  ok(throws(() => scr.show({ html: 'x'.repeat(screenMod.MAX_HTML + 1) }), /too big/), 'html is capped at 200 KB');
  const s3 = scr.show({ title: 'Page', html: '<p>hi</p><script>cru.say("x")</script>' });
  ok(s3.doc.startsWith('<!doctype html>') && s3.doc.includes("connect-src 'none'") && s3.doc.includes('img-src data: blob:') && s3.doc.includes('window.cru='), 'Cru\'s html is wrapped with a no-network CSP and the say/tell bridge');
  ok(throws(() => scr.show({ blocks: [{ type: 'image', url: 'http://x/a.png' }] }), /https/), 'pictures are https or data: only');
  kidNow = true;
  ok(throws(() => scr.show({ blocks: [{ type: 'image', url: 'https://x/a.png' }] }), /data:image/), 'in kid mode, only data: pictures');
  kidNow = false;
  const again = screenMod.createScreen({ file: scrFile });
  ok(again.get() && again.get().title === 'Page', 'a reopened screen shows it again');
  scr.clear();
  ok(scr.get() === null && JSON.parse(readFileSync(scrFile, 'utf8')).screen === null, 'clear_screen / the owner\'s X takes it away');
  const tools = screenMod.makeScreenTools(scr);
  ok(tools.map((t) => t.name).join() === 'show_screen,update_screen,clear_screen', 'three screen tools');
  // Cru's UI over the whole content area, and the owner's way back.
  const f1 = scr.show({ title: 'Board', html: '<p>board</p>', full: true });
  ok(f1.full === true && f1.doc.includes('board'), 'show_screen full: the whole content area');
  ok(scr.update({ add: [{ type: 'text', text: 'more' }] }).full === true, 'a patch without full keeps it');
  ok(scr.normal() === true && !scr.get().full && scr.get().title === 'Board' && scr.normalAt > 0, 'the owner\'s button: back to the normal view, the content kept as the panel');
  ok(scr.normal() === false, 'nothing taken over: the button changes nothing');
  ok(scr.expand() === true && scr.get().full === true && scr.expand() === false, 'the panel\'s expand button: the full view again (once)');
  scr.normal();
  ok(scr.update({ full: true }).full === true && !scr.update({ full: false }).full, 'update_screen full on and off');
  kidNow = true;
  ok(!scr.show({ title: 'Kid', html: '<p>x</p>', full: true }).full, 'never in kid mode (the kid screen is already whole)');
  kidNow = false;
  ok(tools[0].input_schema.properties.full && tools[1].input_schema.properties.full, 'show_screen and update_screen take full');
  scr.clear();
}

// ─── The kid's tools: nothing else is reachable ───
{
  const sent = [];
  const watch = kid.createGameWatch();
  const tools = kid.makeKidTools({ screen: scr, instructions: () => kid.KID_DEFAULT, games: lib, friend: { write() {} }, watch, sendCommand: (c) => sent.push(c), tellParent: () => ({ ok: true }), asks: kid.createParentAsks(), makeGame: () => ({ ok: true }) });
  const names = tools.map((t) => t.name).sort();
  ok(names.join() === [...kid.KID_TOOL_NAMES].sort().join(), `kid tools are exactly: ${names.join(', ')}`);
  const forbidden = [...READ_ONLY_TOOL_NAMES, 'memory', 'memory_search', 'run_command', 'write_file', 'send_to_chat', 'start_chat', 'admin_mode_off', 'start_kid_mode', 'kid_progress', 'set_voice_setting', 'close_voice'];
  ok(!names.some((n) => forbidden.includes(n)), 'none of the owner\'s tools (projects, chats, settings, memory, admin, actions)');
  ok(!names.some((n) => /exit|stop_kid|unlock|pin/.test(n)), 'no tool can end kid mode');
  const t = (n) => tools.find((x) => x.name === n);
  ok(t('list_games').run().games.length >= 10, 'the approved built-in games are listed');
  ok(t('start_game').run({ game: 'nope' }).ok === false, 'an unknown game is refused');
  ok(t('start_game').run({ game: 'find-letter', options: { letters: 'AB' } }).ok && scr.get().game.id === 'find-letter' && scr.get().doc.includes('window.CRU_OPTIONS={"letters":"AB"}'), 'start_game puts the game, its options and the kit on the screen');
  const limited = kid.makeKidTools({ screen: scr, instructions: () => '## Approved games\n- counting\n', games: lib });
  ok(limited.find((x) => x.name === 'start_game').run({ game: 'find-letter' }).ok === false && limited.find((x) => x.name === 'start_game').run({ game: 'counting' }).ok, 'only the games the parent approved');
  const pv = await t('play_video').run({ which: 'cocomelon' });
  ok(pv.ok && scr.get().video.embed.includes('list=UUbCmjCuTUZos6Inko4u57UQ'), 'play_video plays an approved channel by name');
  ok((await t('play_video').run({ which: 'some random channel' })).ok === false, 'anything not approved is refused');
  t('game_command').run({ action: 'easier' });
  ok(sent.length === 1 && sent[0].type === 'command' && sent[0].action === 'easier' && t('game_command').run({ action: 'explode' }).ok === false, 'game_command forwards { type: command, action } and nothing else');
  ok(throws(() => kid.cleanFriendNote('Mia lives at 12 Oak Street')) && kid.cleanFriendNote('Name: Mia\nLikes: elephants') === 'Name: Mia\nLikes: elephants\n', 'friend notes keep a first name and likes, never addresses or numbers');
  const startTool = kid.makeStartKidTool({ canStart: () => 'Only the owner', start: () => {} });
  ok(startTool.run().ok === false, 'start_kid_mode refuses when it may not (not the owner, no PIN)');
  ok(kid.splitMood('[mood: gentle] Never mind!').mood === 'gentle' && kid.splitMood('[mood: gentle] Never mind!').text === 'Never mind!' && kid.splitMood('Hi!').mood === 'excited', 'kid Cru\'s mood marker is taken off (default excited)');
  ok(tts.MOOD_EXPRESSIVITY.excited === 2 && tts.MOOD_EXPRESSIVITY.gentle === -1 && tts.MOOD_EXPRESSIVITY.calm === 0, 'moods map to Flux expressivity');
  ok(kid.kidContextLine({ now: new Date(), since: Date.now() - 5 * 60000, extraMinutes: 15 }).includes('on for 5 minutes') && kid.kidContextLine({ extraMinutes: 15 }).includes('added 15'), 'kid Cru knows how long kid mode has been on and time added');
  ok(kid.KID_SYSTEM_PROMPT.includes('never call anyone "Sir"') && /tell_parent/.test(kid.KID_SYSTEM_PROMPT) && /Never tell the child how kid mode is switched off/.test(kid.KID_SYSTEM_PROMPT), 'the kid prompt: a friend, never Sir, tells the parent when worried, never how to exit');
}

// ─── Game events wake kid Cru only as subscribed, rate-limited ───
{
  let t = 0;
  const w = kid.createGameWatch({ now: () => t });
  const offer = (e, busy = false) => w.offer(e, { busy });
  ok(offer({ event: 'start', game: 'find-letter' }) && /\[Game: start/.test(kid.gameEventLine({ event: 'start' })), 'the start wakes kid Cru (a personal intro)');
  t += 1000;
  ok(offer({ event: 'level-up' }) === null && w.held === null, 'within the gap: dropped (not important)');
  t += 8000;
  let lines = [];
  for (let i = 0; i < 4; i++) { const l = offer({ event: 'correct', item: 'B', streak: i + 1 }); if (l) lines.push(l); }
  ok(lines.length === 1, 'every 4th correct answer, not each');
  t += 8000;
  ok(offer({ event: 'miss', item: 'D' }) === null && offer({ event: 'miss', item: 'D' }).includes('missed twice'), 'two misses in a row on the same item wake Cru');
  t += 1000;
  ok(offer({ event: 'finished' }) === null && w.held && w.held.event === 'finished', 'too soon: the finish is held, not lost');
  t += 8000;
  ok(/finished/.test(w.due() || ''), 'and comes once Cru may talk');
  t += 8000;
  ok(offer({ event: 'stuck' }, true) === null && w.held.event === 'stuck', 'while Cru speaks only the important ones are kept');
  w.set({ events: [], correctEvery: 0, missTwice: false });
  t += 20000;
  ok(offer({ event: 'finished' }) === null && offer({ event: 'correct' }) === null, 'unwatched: silent (still logged elsewhere)');
  w.reset();
  ok(w.watch.events.includes('finished') && w.watch.correctEvery === 4, 'a new game: the default subscription again');
  ok(kid.CHATTINESS.quiet.events.length < kid.CHATTINESS.chatty.events.length, 'the parent\'s chattiness changes the default');
}

// ─── Instant lines (no model) ───
{
  let t = 0;
  const r = reflexMod.createReflex({ now: () => t, rnd: () => 0 });
  const seen = [];
  for (let i = 0; i < 6; i++) seen.push(r.pick('correct', { item: 'B' }).text);
  ok(seen.every((x, i) => seen.slice(Math.max(0, i - 3), i).indexOf(x) < 0), 'never one of the last three lines for an event');
  ok(reflexMod.fill('Great job, {name}!', {}) === 'Great job!' && reflexMod.fill('Great job, {name}!', { name: 'Mia' }) === 'Great job, Mia!', 'no name known: the slot goes quietly');
  ok(reflexMod.fill('Yes, {item} says {sound}!', { item: 'b' }) === 'Yes, B says buh!' && reflexMod.fill('{item} says {sound}', {}) === null, 'letter slots filled, or the line skipped');
  ok(reflexMod.fill('Let me ask {parent}!', {}) === 'Let me ask your grown-up!' && reflexMod.fill('Let me ask {parent}!', { parent: 'Daddy' }) === 'Let me ask Daddy!', 'the parent as the child calls them');
  ok(r.pick('miss').mood === 'gentle' && r.pick('miss').expressivity === -1 && r.pick('correct').expressivity === 2, 'misses are gentle, cheers excited');
  ok(r.gate('correct') && !r.gate('correct') && (t += reflexMod.GAP_MS, r.gate('miss')), 'at most one instant line every 2.5 s');
  t += reflexMod.GAP_MS;
  ok(!r.gate('correct', { speaking: true }) && r.gate('finished', { speaking: true }), 'nothing while kid Cru speaks, except the finish');
  ok(reflexMod.clipKey('flux-gemma-en', 2, 'Yes!') === reflexMod.clipKey('flux-gemma-en', 2, ' Yes! ') && reflexMod.clipKey('flux-gemma-en', 1, 'Yes!') !== reflexMod.clipKey('flux-gemma-en', 2, 'Yes!'), 'clips are kept by voice + expressivity + text');
  const bank = JSON.parse(readFileSync(join(root, 'src/voice/kid-games/reflex.json'), 'utf8'));
  ok(bank.events.correct.lines.length >= 12 && bank.events.miss.lines.length >= 12, 'twelve or more variants for correct and miss');
  ok(reflexMod.validateReflexBank(bank).problems.length === 0 && reflexMod.validateReflexBank({ events: { miss: { lines: ['That is wrong, stupid', 'visit http://x'] } } }).problems.length === 2, 'the bank is checked: kid-safe, no links');
  // Own words heard back are dropped by the server's echo filter, reflex lines too.
  const line = r.pick('correct', { item: 'B' }).text;
  ok(stream.isEcho(line, line) && stream.isEcho(line.toLowerCase().replace(/[!,]/g, ''), line), 'a reflex line heard back is Cru\'s own voice, dropped');
  const src = readFileSync(join(root, 'app/vendor/crundi-voice.js'), 'utf8');
  ok(/speak\(r\.reflex\.text, \{ reflex: true/.test(src) && /function speak\(text, opts\)[\s\S]{0,1600}if \(S\.live\) \{ wsSend\(\{ type: 'saying', text: text \}\); hold\(\); \}/.test(src), 'instant lines go through speak(): told to the server as saying, the mic held by the local gate');
}

// ─── The game library: validator, seed, checks after the workshop, rollback ───
{
  const v = (h) => games.validateGame(h);
  ok(v('<style>.a{cursor:url("data:image/svg+xml,x") 5 3,auto}</style>').ok && v("<style>.a{background:url('data:image/png;base64,AA')}</style>").ok && v('<style>.a{background:url(data:image/png;base64,AA)}</style>').ok, 'quoted and unquoted data: URLs are fine');
  ok(v('<!-- uses url() and <meta> words --><div></div>').ok && v('<script>// never fetch( here\nvar a = 1; /* or <meta> */</script>').ok, 'words in comments are not code');
  ok(v('<svg xmlns="http://www.w3.org/2000/svg"></svg>').ok, 'the SVG namespace is not a network address');
  for (const [h, what] of [['<script>fetch("x")</script>', 'fetch'], ['<div style="background:url(https://x.com/a.png)"></div>', 'remote url()'], ['<script src="//cdn.x/a.js"></script>', '<script src>'], ['<meta http-equiv="refresh" content="0">', 'meta refresh'], ['<script>new WebSocket("x")</script>', 'WebSocket'], ['<script>import("x")</script>', 'import()'], ['<link href="https://x/a.css" rel="stylesheet">', '<link>'], ['<img src="https://x/a.png">', 'remote img'], ['<script>localStorage.x=1</script>', 'storage'], ['<script>eval("1")</script>', 'eval'], ['<script>var a = ;</script>', 'a syntax error'], ['<iframe></iframe>', 'iframe']]) ok(!v(h).ok, `refused: ${what}`);
  for (const f of readdirSync(join(root, 'src/voice/kid-games/builtin'))) ok(v(readFileSync(join(root, 'src/voice/kid-games/builtin', f), 'utf8')).ok, `built-in ${f} passes`);
  ok(v(readFileSync(join(root, 'src/voice/kid-games/waiting.html'), 'utf8')).ok, 'the waiting scene passes');
  const tmpl = join(root, 'skills/baby-games/reference/game-template.html');
  if (existsSync(tmpl)) ok(v(readFileSync(tmpl, 'utf8')).ok, 'the baby-games skill\'s template passes');
  const ids = new Set(skillsMod.SKILL_IDS);
  const cat = JSON.parse(readFileSync(join(root, 'src/voice/kid-games/catalog.json'), 'utf8'));
  ok(cat.every((g) => g.skills.every((s) => ids.has(s))), 'every built-in game\'s skills are in the taxonomy');
  ok(['feelings', 'rhyme', 'blending', 'music', 'world.animals'].every((s) => ids.has(s)), 'the taxonomy has feelings, rhyme, blending, music and the world');
  const readme = readFileSync(join(root, 'src/voice/kid-games/README.md'), 'utf8');
  ok(skillsMod.SKILL_IDS.every((s) => readme.includes('`' + s + '`')), 'the library README lists every skill id of kid-skills.js');
  const bank = lib.bank();
  ok(bank.items.length >= 250 && games.validateBankItems(bank.items).problems.length === 0, `the trivia bank: ${bank.items.length} items, every picture drawable`);

  const L = lib;
  ok(L.list().length === cat.length && L.list().every((g) => g.createdBy === 'builtin' && g.enabled), 'built-ins are seeded into the library');
  const d = L.doc('counting', { max: 5 });
  ok(d.html.includes('window.CRU_OPTIONS={"max":5}') && d.html.includes('window.CRU_INIT=') && d.html.includes('window.kid ='), 'a game gets its options, what the child knows, and the kit');
  ok(L.doc('picture-trivia', {}).html.includes('window.CRU_BANK='), 'the trivia game gets the bank');
  ok(L.doc('_waiting', { theme: 'egg' }).html.includes('CRU_WAITING=true'), 'the waiting scene is always there');
  // A workshop run: one game improved, one broken, one new and good, one new and bad.
  const snap = L.snapshot();
  const gdir = join(L.dir);
  writeFileSync(join(gdir, 'counting', 'index.html'), readFileSync(join(gdir, 'counting', 'index.html'), 'utf8') + '<!-- better -->');
  writeFileSync(join(gdir, 'quiz', 'index.html'), '<script>fetch("https://x")</script>');
  mkdirSync(join(gdir, 'dino-count')); writeFileSync(join(gdir, 'dino-count', 'index.html'), '<div class="k-title">Dinos</div><script>kid.say("hi")</script>'); writeFileSync(join(gdir, 'dino-count', 'meta.json'), JSON.stringify({ title: 'Dino count', skills: ['counting', 'made-up'] }));
  mkdirSync(join(gdir, 'bad-one')); writeFileSync(join(gdir, 'bad-one', 'index.html'), '<img src="https://x/a.png">'); writeFileSync(join(gdir, 'bad-one', 'meta.json'), '{}');
  writeFileSync(join(gdir, 'trivia.extra.json'), JSON.stringify({ items: [{ id: 'x1', area: 'world.animals', kind: 'fact', fact: 'Cows moo.' }, { id: 'x2', area: 'world.animals', kind: 'fact', fact: 'see http://x' }] }));
  const res = await L.afterWorkshop(snap);
  ok(res.updated.join() === 'counting' && existsSync(join(gdir, 'counting', 'index.prev.html')) && L.get('counting').version === 2, 'an improved game goes live, its previous version kept');
  ok(res.failed.some((f) => f.id === 'quiz') && !readFileSync(join(gdir, 'quiz', 'index.html'), 'utf8').includes('fetch') && existsSync(join(gdir, 'quiz', 'index.failed.html')), 'a change that fails the checks: the live version stays');
  ok(res.created.join() === 'dino-count' && L.get('dino-count').enabled && L.get('dino-count').isNew && L.get('dino-count').skills.join() === 'counting', 'a good new game is on (marked new, unknown skills dropped)');
  ok(L.get('bad-one').enabled === false, 'a new game that fails stays off');
  ok(res.bank.kept === 1 && res.bank.dropped.length === 1, 'workshop trivia is checked; bad items dropped');
  ok(L.rollback('counting').ok && !readFileSync(join(gdir, 'counting', 'index.html'), 'utf8').includes('better') && L.get('counting').version === 1, 'roll back puts the previous version live');
  ok(L.setEnabled('bad-one', true).ok === false, 'a game that failed cannot be switched on');
  // The play log.
  L.startPlay('find-letter', { letters: 'AB' });
  L.event({ kind: 'correct' }); L.event({ kind: 'wrong', detail: 'D' });
  const line = L.endPlay(true);
  ok(line.game === 'find-letter' && line.events.correct === 1 && line.wrongs[0] === 'D' && L.plays().length === 1, 'each game played goes to plays.jsonl');
}

// ─── Skill levels: up slowly, down quickly, with enough evidence ───
{
  const s = { level: 1, ema: skillsMod.RULES.restart, evidence: 0, answers: 0, items: {}, last: 0 };
  let moved = '';
  for (let i = 0; i < skillsMod.RULES.upAfter - 1; i++) moved = skillsMod.applyResult(s, 'A', true) || moved;
  ok(moved === '' && s.level === 1, 'nine right answers are not yet enough to go up');
  ok(skillsMod.applyResult(s, 'A', true) === 'up' && s.level === 2, 'the tenth, with a high average, goes up a level');
  let down = '';
  for (let i = 0; i < skillsMod.RULES.downAfter; i++) down = skillsMod.applyResult(s, 'B', false) || down;
  ok(down === 'down' && s.level === 1, 'four misses in a row bring it down at once');
  let t = Date.parse('2026-11-01T10:00:00Z');
  const sk = skillsMod.createSkills({ file: join(dir, 'skills.json'), historyFile: join(dir, 'progress.jsonl'), now: () => t });
  for (let i = 0; i < 6; i++) sk.record('letters', 'A', true);
  sk.record('letters', 'G', false);
  ok(sk.record('nonsense', 'x', true) === '' && !sk.state.skills.nonsense, 'unknown skills are ignored');
  const init = sk.initFor(['letters', 'counting']);
  ok(init.type === 'init' && init.levels.letters === 1 && init.avoid.letters.join() === 'A' && init.focus.letters.join() === 'G' && !init.levels.counting, 'a game is told the level, what is mastered (avoid) and being learnt (focus)');
  ok(/Language and literacy: letter recognition level 1\/5, knows A, learning G/.test(sk.summary()), 'kid Cru gets a short summary');
  sk.addPlayTime(300);
  t += 86400000;
  sk.record('counting', '3', true);
  const hist = readFileSync(join(dir, 'progress.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  ok(hist.length === 1 && hist[0].skill === 'letters' && hist[0].plays === 7 && hist[0].firstTry === 0.86, 'a new day: yesterday\'s snapshot per skill goes to progress.jsonl');
  const prog = sk.progress({ range: 'week' });
  const lang = prog.areas.find((a) => a.id === 'language');
  ok(lang.series.length === 1 && lang.mastered[0].includes('A') && prog.playTime[0].minutes === 5 && prog.areas.some((a) => a.id === 'world'), 'the parent\'s view: areas over time, mastered and learning, play time');
  ok(skillsMod.ranges(['A', 'B', 'C', 'D', 'F']) === 'A-D, F', 'letters as ranges');
}

// ─── The overnight workshop: when it runs ───
{
  const at = (h, m = 0, dayOff = 0) => new Date(2026, 10, 10 + dayOff, h, m).getTime();
  const base = { now: at(2, 0), from: '01:00', to: '05:00', lastActivity: at(0, 0), lastKidUse: at(17, 0, -1) };
  ok(workshop.shouldRun(base).run, 'kid mode used yesterday, inside the window, nobody about: it runs');
  ok(!workshop.shouldRun({ ...base, lastKidUse: 0 }).run && /never/.test(workshop.shouldRun({ ...base, lastKidUse: 0 }).why), 'never before kid mode is first used');
  ok(!workshop.shouldRun({ ...base, lastKidUse: at(17, 0, -3) }).run, 'not used in the day before: no run, no cost');
  ok(!workshop.shouldRun({ ...base, lastRunNight: workshop.shouldRun(base).night }).run, 'already ran tonight: no second run');
  ok(!workshop.shouldRun({ ...base, lastKidUse: at(17, 0, -1), lastRunAt: at(20, 0, -1) }).run, 'not used since the last run: skips');
  ok(!workshop.shouldRun({ ...base, now: at(6, 0) }).run && !workshop.shouldRun({ ...base, kidOn: true }).run && !workshop.shouldRun({ ...base, lastActivity: at(1, 50) }).run && !workshop.shouldRun({ ...base, enabled: false }).run, 'not outside the window, in kid mode, while in use, or when switched off');
  ok(workshop.inWindow(at(23, 30), '22:00', '02:00') && workshop.inWindow(at(1, 0), '22:00', '02:00') && !workshop.inWindow(at(3, 0), '22:00', '02:00'), 'a window may cross midnight');
  // One run a night, with a fake Claude Code.
  let spawns = 0;
  const fakeSpawn = () => { spawns++; const p = new EventEmitter(); p.stdout = new EventEmitter(); p.stderr = new EventEmitter(); p.kill = () => {}; setTimeout(() => { p.stdout.emit('data', JSON.stringify({ type: 'result', result: 'Made it gentler.', total_cost_usd: 0.1 }) + '\n'); p.emit('exit', 0); }, 5); return p; };
  let now = at(2, 0);
  const wsDir = join(dir, 'ws-lib');
  const wl = games.createGameLibrary({ dir: wsDir });
  const w = workshop.createWorkshop({ lib: wl, dir: wsDir, settings: () => ({ enabled: true, from: '01:00', to: '05:00' }), kidOn: () => false, lastActivity: () => at(0, 0), claudeBin: () => '/bin/true', spawn: fakeSpawn, now: () => now, log: () => {} });
  w.markKidUse(at(17, 0, -1));
  const r1 = await w.run();
  ok(r1.ok && spawns === 1 && existsSync(join(wsDir, '_rules', 'README.md')), 'it runs once, with the library rules in _rules/');
  now = at(3, 0);
  ok(w.decide().run === false && /already ran/.test(w.decide().why) && (await w.run()).skipped, 'the same night it does not run again');
  ok(workshop.workshopPrompt({ hasSkill: true }).includes('baby-games/SKILL.md') && workshop.workshopPrompt({}).includes('No network'), 'the run is told to follow the baby-games skill and the rules');
}

// ─── Asking the parent; worrying things ───
{
  let t = 0;
  const asked = [], done = [];
  const a = kid.createParentAsks({ now: () => t, onAsk: (x) => asked.push(x), onDone: (x, c) => done.push([x.kind, c]) });
  const r = a.ask({ kind: 'video', what: 'Peppa Pig', child: 'Mia' });
  ok(r.ok && asked[0].title === 'Mia wants to watch: Peppa Pig' && asked[0].choices.map((c) => c.id).join() === 'once,always,no', 'a video request: Allow once / Always allow / No');
  ok(a.ask({ kind: 'time', what: 'more' }).ok === false, 'one question at a time');
  ok(a.answer(r.id, 'maybe') === null && a.answer(r.id, 'once') && done[0][1] === 'once', 'only an offered answer is taken');
  const r2 = a.ask({ kind: 'time', what: 'more time' });
  t += kid.ASK_TIMEOUT_MS;
  a.expire();
  ok(done[1][0] === 'time' && done[1][1] === null && !a.pending().length, 'no answer in time: kid Cru is told (null)');
  for (let i = 0; i < 6; i++) { const x = a.ask({ kind: 'other', what: 'q' + i }); if (x.ok) a.answer(x.id, 'no'); }
  ok(a.ask({ kind: 'other', what: 'again' }).ok === false, 'no spamming the parent: a few an hour');
  void r2;
  const notes = [];
  const tell = kid.createParentNotes({ send: (n) => notes.push(n), now: () => t });
  ok(tell('Mia said she was scared of the dark.').told && tell('Mia said she was scared of the dark.').already && notes.length === 1, 'a worrying thing reaches the parent once (inbox and phone)');
}

// ─── The kid voice: Flux, by mood; Aura when it fails ───
{
  settings.updateSettings({ deepgramKey: 'dg-test', kidVoice: 'flux-gemma-en' });
  let url = '';
  const wav = new Uint8Array(100); wav.set([0x52, 0x49, 0x46, 0x46]); wav.set([0x64, 0x61, 0x74, 0x61], 36);
  const st = await tts.speakKidStream('Yay!', { expressivity: -1, fetchImpl: async (u) => { url = u; return { ok: true, body: new Response(wav).body }; } });
  ok(st.ok && url.startsWith('https://api.deepgram.com/v2/speak?') && url.includes('model=flux-gemma-en') && url.includes('expressivity=-1'), 'kid speech: Flux on /v2/speak with the mood\'s expressivity');
  const pcm = new Uint8Array(await new Response(st.body).arrayBuffer());
  ok(pcm.length === 100 - 44, 'a WAV header is taken off: raw PCM, as the screen plays it');
  let fell = false;
  await tts.speakKidStream('Yay!', { fetchImpl: async () => ({ ok: false, status: 400 }), fallback: async () => { fell = true; return { ok: true }; } });
  ok(fell, 'if Flux fails, the owner\'s Aura voice speaks instead');
  settings.updateSettings({ deepgramKey: '' });
}

// ─── The video gate on the voice screen (crundi-voice.js) ───
{
  const ctx = { window: {}, document: {}, navigator: {}, localStorage: {}, setTimeout, clearTimeout };
  vm.runInNewContext(readFileSync(join(root, 'app/vendor/crundi-voice.js'), 'utf8'), ctx);
  const T = ctx.window.CrundiVoice._test;
  const g = T.createVideoGate();
  g.loaded();
  ok(g.player(1) === 'mute' && g.playing, 'the video plays: speech no longer reaches Cru');
  ok(!g.wake('let\'s sing a song', 0.9) && !g.wake('hey cru', 0.2) && g.wake('hey cru', 0.8), 'while it plays only a loud "Hey Cru" alone gets through (not the video\'s own sound)');
  ok(g.player(2) === 'resume' && !g.playing, 'paused (by the kid or by Cru): listening comes back');
  g.player(1);
  ok(g.player(0) === 'resume', 'ended: listening comes back');
  const html = T.blocksHtml([{ type: 'text', text: '<img src=x onerror=alert(1)>' }, { type: 'image', url: 'javascript:alert(1)' }]);
  ok(!html.includes('<img src=x') && !html.includes('javascript:'), 'blocks are escaped; only https/data pictures');
}

if (failed) { console.error(`\n${failed} failed`); process.exit(1); }
console.log('\nAll kid-mode and live-screen checks passed.');
