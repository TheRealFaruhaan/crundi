// The chat's composer (F74; 2.5-2.11, 2.24): the box and the Enter switch,
// Send / Queue and Stop, queueing while Claude works (handed over) and while
// it needs input (held), the slash menu, the permission-mode dialog, the
// session id, attach (picker, paste, drop, a dragged reference, the upload
// bar), the schedule panel, saved drafts across the pane's rebuild, the
// narrow layout, the mic (hidden without voice), the exited state; and that
// a streaming chat never redraws its composer.
//
// Run as test/lib.mjs says, on a FRESH tools/stub-server.sh <port> (its
// chats run tools/claude-stub.mjs):
//   CRUNDI_TEST_SERVER=http://localhost:4883 CRUNDI_TEST_PORT=4888 NATIVE_BIN=… \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/chat-composer.mjs

import * as t from './lib.mjs';
import { writeFileSync, readFileSync, readdirSync, mkdtempSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// The upload bar is watched under way: uploads go at 96 KB/s.
process.env.CRUNDI_TEST_UPLOAD_KBPS = process.env.CRUNDI_TEST_UPLOAD_KBPS || '96';
const T = await t.begin('chat-composer');
const A = 'chat-composer';
const C = async () => (await T.state()).composer || {};
const until = async (what, f, ms) => (await T.until(what, (s) => f(s.composer || {}, s), ms))?.composer || null;

await T.ui('new-chat');
await until('a chat pane with its composer', (c) => c.state === 'idle');
await t.sleep(600);
let c = await C();
t.check('at rest: the Enter hint in the placeholder', c.placeholder === 'Message Claude…  (Enter to send, Shift+Enter for newline)', c);
t.check('at rest: Send, no Stop, idle', c.send === 'Send' && !c.stop && c.state === 'idle', c);
t.check('at rest: Ask permissions', c.mode === 'Ask permissions' && c.modeKey === 'default', c);
t.check('the mic is hidden without voice on the server', c.mic === false, c);
await T.snap(A, 'rest');

// ─── A streaming chat never redraws its composer ───
// (Measured before the box ever has the focus: a focused box's caret
// blinks, which redraws it about twice a second, streaming or not.)
await T.ui('composer-text', 'Reply with three hundred numbers in words.');
await T.ui('composer-send');
await until('a long reply streams', (c) => c.state === 'working');
await t.sleep(800);
const r0 = (await T.state()).renders || {};
const b0 = (await T.state()).batches;
await t.sleep(2500);
const s2 = await T.state();
const r1 = s2.renders || {};
const drew = Object.fromEntries(Object.keys(r1).map((k) => [k, (r1[k] || 0) - (r0[k] || 0)]));
console.log('views rendered while a chat streams:', JSON.stringify(drew), 'batches', s2.batches - b0);
t.check('the log streams meanwhile', (drew.log || 0) > 5 && s2.batches - b0 > 5, drew);
t.check('a streaming chat never redraws its composer', !drew.composer, drew);
await until('the reply ends', (c) => c.state === 'idle', 30000);

// ─── The Enter switch ───
await T.ui('chat-enter', 'newline');
c = await until('the switch: Ctrl+Enter sends', (c) => c.enter === 'newline');
t.check('its placeholder hint', c.placeholder === 'Message Claude…  (Ctrl+Enter to send, Enter for newline)', c);
await T.ui('composer-focus');
await T.key('one');
await T.key('Return');
await T.key('two');
c = await C();
t.check('Ctrl+Enter mode: Enter makes a newline (nothing sent)', c.text === 'one\ntwo' && c.state === 'idle', c);
await T.snap(A, 'enter-newline');
await T.ui('chat-enter', 'enter');
await until('the switch back: Enter sends', (c) => c.enter === 'enter' && c.placeholder.includes('Enter to send'));
await T.key('Return', ['shift']);
c = await C();
t.check('Enter mode: Shift+Enter makes a newline', c.text === 'one\ntwo\n', c);
await T.ui('composer-text', '');
await T.key('Return');
c = await C();
t.check('a blank box: Enter does nothing', c.text === '' && c.state === 'idle', c);

// The switch's hover (.cc-toggle:hover, F249): the option not in use and
// the word brighten to the primary text. The pointer on the word "sends":
// the track's pixels (the idle option's icons) change, not only the word's.
{
  await T.mouse('move', 5, 5);
  await t.sleep(300);
  const at = (await C()).enterAt;
  t.check('the switch is drawn (its place in the state)', !!(at && at.w > 40), at);
  if (at) {
    const rest = join(T.OUT, 'enter-rest.png'), over = join(T.OUT, 'enter-hover.png');
    const r1 = await T.d.send({ cmd: 'snapshot', path: rest });
    t.check('at rest: the switch is not hovered', (await C()).enterHover === false);
    await T.mouse('move', at.x + at.w - 6, at.y + at.h / 2);
    await t.sleep(250);
    t.check('the pointer on the word: the switch takes its hover look (the idle option and the word brighten)', (await C()).enterHover === true);
    const r2 = await T.d.send({ cmd: 'snapshot', path: over });
    await T.snap(A, 'enter-hover');
    if (r1?.ok && r2?.ok) {
      // The track: the switch's left part, the word left out.
      const track = `${Math.round(at.w * 0.55)}x${Math.round(at.h)}+${Math.round(at.x)}+${Math.round(at.y)}`;
      const word = `${Math.round(at.w * 0.3)}x${Math.round(at.h)}+${Math.round(at.x + at.w * 0.7)}+${Math.round(at.y)}`;
      const pt = T.diff(rest, over, track), pw = T.diff(rest, over, word);
      t.check('hover: the idle option\'s icons brighten (the track\'s pixels change)', pt > 20, { pt, track });
      t.check('hover: the word brightens', pw > 20, { pw, word });
    } else console.log(`(no hover pixel check: ${r1?.error || r2?.error})`);
    await T.mouse('move', 5, 5);
    await t.sleep(250);
    t.check('the pointer gone: no hover look', (await C()).enterHover === false);
    if (r1?.ok && r2?.ok) {
      const back = join(T.OUT, 'enter-back.png');
      await T.d.send({ cmd: 'snapshot', path: back });
      t.check('the pointer gone: the switch as at rest', T.diff(rest, back, `${Math.round(at.w)}x${Math.round(at.h)}+${Math.round(at.x)}+${Math.round(at.y)}`) === 0);
    }
  }
}

// ─── Send, the meta row, the session id ───
await T.key('Reply with a single word: hello');
await T.key('Return');
c = await C();
t.check('Enter sends: the box empties, working at once (Queue, Stop)', c.text === '' && c.state === 'working' && c.send === 'Queue' && c.stop, c);
c = await until('the reply ends: idle, the model and the session id shown', (c) => c.state === 'idle' && c.model === 'claude-stub' && c.sessionFull.length > 8, 30000);
t.check('the id is its first 8 characters', c && c.session === c.sessionFull.slice(0, 8), c);
await T.ui('composer-sid');
const s1 = await T.state();
const lt = await T.ui('log-texts');
t.check('a click on the id copies the whole id', lt.clipboard === c?.sessionFull, lt.clipboard);
t.check('and a toast says so', s1.toast === `Session id copied: ${c?.sessionFull}`, s1.toast);
await T.snap(A, 'sid-copied');

// ─── Queueing while it works ───
await T.ui('composer-text', 'Reply with three hundred numbers in words.');
await T.ui('composer-send');
await until('a long reply streams', (c) => c.state === 'working');
await T.ui('stub', 'send', JSON.stringify({ ok: true, injected: true, uuid: 'u-1' }));
await T.ui('composer-text', 'and then this');
await T.ui('composer-send');
c = await C();
t.check('sent while working: handed over (the slate drawer), the box empty', c.qSent === 'and then this' && c.text === '', c);
await T.ui('composer-event', JSON.stringify({ type: 'queued-state', uuid: 'u-1', state: 'queued' }));
c = await until('the CLI holds it: it can be taken back', (c) => c.qSentKind === 'recallable');
await T.snap(A, 'queued-sent');
await T.ui('stub', 'cancel-queued', JSON.stringify({ ok: true, cancelled: true }));
await T.ui('take-back');
c = await until('taken back: the text is in the box again, no drawer', (c) => c.text === 'and then this' && c.qSent === '');
await T.ui('composer-text', '');

// ─── Stop ───
await T.ui('composer-stop');
c = await C();
t.check('Stop: one click arms it ("Sure?")', c.stopArmed === true, c);
await T.snap(A, 'stop-armed');
await t.sleep(3400);
c = await C();
t.check('it disarms by itself after 3 s', c.stopArmed === false && c.state === 'working', c);
await T.ui('composer-stop');
await T.ui('composer-stop');
await until('a second click interrupts: idle, Send again', (c) => c.state === 'idle' && c.send === 'Send' && !c.stop, 20000);

// ─── Needs input: held, then sent as one ───
await T.ui('composer-event', JSON.stringify({ type: 'state', state: 'needs-input' }));
await until('needs input: Queue, no Stop', (c) => c.state === 'needs-input' && c.send === 'Queue' && !c.stop);
await T.ui('composer-text', 'first line');
await T.ui('composer-send');
await T.ui('composer-text', 'second line');
await T.ui('composer-send');
c = await C();
t.check('held while it needs input (the dashed drawer), one per send', c.qHeldN === 2 && c.qHeld === 'first line\nsecond line', c);
await T.snap(A, 'queued-held');
await T.ui('stub', 'send', JSON.stringify({ ok: true, injected: false }));
await T.ui('composer-event', JSON.stringify({ type: 'state', state: 'working' }));
await until('no longer needing input: the held lines go out as one', (c) => c.qHeldN === 0);
await until('idle again', (c) => c.state === 'idle', 30000);

// ─── The slash menu ───
await T.ui('composer-text', '/');
c = await C();
t.check('"/" lists nothing while the CLI has sent no commands (not even /btw: showSlash)', !(c.slash?.items || []).length, c.slash);
await T.ui('composer-event', JSON.stringify({ type: 'init', slashCommands: [{ name: 'compact', description: 'Compact the conversation' }, 'clear', 'cost'] }));
await T.ui('composer-text', '/c');
c = await C();
t.check('"/c" lists the CLI\'s commands starting with it, the first selected', JSON.stringify(c.slash?.items) === '["/compact","/clear","/cost"]' && c.slash.sel === 0, c.slash);
await T.snap(A, 'slash');
await T.ui('composer-focus');
await T.key('Down');
c = await C();
t.check('↓ moves the selection', c.slash?.sel === 1, c.slash);
await T.key('Return');
c = await C();
t.check('Enter puts "/clear " in the box, closes the menu, sends nothing', c.text === '/clear ' && c.slash === null && c.state === 'idle', c);
await T.ui('composer-text', '/co');
await T.key('Escape');
c = await C();
t.check('Esc closes it and keeps the text', c.slash === null && c.text === '/co', c);
await T.key('s');
c = await C();
t.check('the next edit opens it again', JSON.stringify(c.slash?.items) === '["/cost"]', c.slash);
await T.ui('composer-text', '');

// ─── The permission-mode dialog ───
await T.ui('mode-open');
c = await C();
t.check('the dialog lists the chat\'s modes, the one in effect focused', c.modePicker?.modes?.length === 5 && c.modePicker.current === 'default' && c.modePicker.focus === 0, c.modePicker);
await T.snap(A, 'mode-dialog');
await T.ui('mode-key', 'down');
await T.ui('mode-key', 'shift-tab');
c = await C();
t.check('↓ and Shift+Tab move the keyboard\'s focus', c.modePicker?.focus === 0, c.modePicker);
await T.ui('mode-key', 'escape');
c = await C();
t.check('Esc closes it', c.modePicker === null, c.modePicker);
await T.ui('mode-open');
await T.ui('mode-choose', 'plan');
c = await C();
t.check('a mode chosen: closed, shown at once', c.modePicker === null && c.modeKey === 'plan' && c.mode === 'Plan mode', c);
await t.sleep(1500);
c = await C();
t.check('the server took it (the button stays)', c.modeKey === 'plan', c);
await T.snap(A, 'mode-plan');

// ─── Attach ───
const dir = mkdtempSync(join(tmpdir(), 'crundi-v2-composer-'));
const big = join(dir, 'notes.txt');
writeFileSync(big, 'x'.repeat(300 * 1024));
await T.ui('pick-next', big);
await T.ui('attach-button');
c = await until('the picked file uploads: the bar shows its name and share', (c) => c.upOn && c.upName === 'notes.txt' && /%|saving/.test(c.upLabel), 8000);
t.check('the paperclip is inert while it uploads', c?.upBusy === true, c);
await T.snap(A, 'upload-bar');
c = await until('done: the server\'s path at the caret, the bar gone', (c) => !c.upOn && /crundi_attachments\/.*notes\.txt/.test(c.text), 20000);
t.check('a toast says it is attached', /^Attached: .*notes\.txt/.test((await T.state()).toast), (await T.state()).toast);
await T.ui('composer-text', 'see ');
const png = join(dir, 'pic.png');
writeFileSync(png, Buffer.from('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360000002000154a24f5d0000000049454e44ae426082', 'hex'));
await T.ui('paste-image', png);
c = await until('a pasted picture uploads as image.png, its path after the text', (c) => /^see \S*crundi_attachments\/\S*image\S*\.png $/.test(c.text), 20000);
if (!c) console.log('box:', JSON.stringify((await C()).text), 'toast:', (await T.state()).toast);
await T.ui('composer-text', 'look at');
await T.ui('drop-files', big);
c = await until('a file dropped from this machine: its own path goes in', (c) => c.text === `look at ${big} `);
t.check('and a toast says "File added"', (await T.state()).toast === 'File added', (await T.state()).toast);
await T.ui('composer-text', 'and');
await T.ui('composer-ref-drop', '[File Path: /p/src/a.rs]');
c = await until('a reference dragged from another panel: in at the caret', (c) => c.text === 'and [File Path: /p/src/a.rs] ');
t.check('and a toast says "Added to chat"', (await T.state()).toast === 'Added to chat', (await T.state()).toast);
await T.ui('composer-text', '');

// ─── The schedule panel ───
await T.ui('composer-text', 'Later: run the tests');
await T.ui('sched-open');
c = await until('the clock opens the panel with the box\'s text, At a time', (c) => c.sched?.open && c.sched.loaded && c.sched.text === 'Later: run the tests' && c.sched.trigger === 'time' && c.sched.at);
t.check('nothing scheduled yet', c?.sched?.list === 'Nothing scheduled for this project.', c?.sched);
await t.sleep(400);
await T.snap(A, 'schedule-open');
await T.ui('sched-trigger', 'turn-end');
c = await C();
t.check('Turn ends: its note, no time field', c.sched.trigger === 'turn-end' && c.sched.note.startsWith('Sends as soon as the current turn finishes'), c.sched);
await T.ui('sched-trigger', 'limit-reset');
c = await C();
t.check('Limit resets without the limit warmer: the web\'s note, not chosen', c.sched.trigger === 'turn-end' && c.sched.note.startsWith('Turn on the limit warmer'), c.sched);
await T.ui('sched-trigger', 'turn-end');
await T.ui('sched-save');
c = await until('scheduled: listed as next, the panel\'s message and the box empty', (c) => c.sched?.items?.length === 1 && c.sched.items[0].next && c.sched.items[0].when === 'when the turn ends' && c.sched.text === '' && c.text === '');
await t.sleep(400);
await T.snap(A, 'schedule-listed');
await T.ui('sched-delete', c?.sched?.items?.[0]?.id || '');
await until('✕ cancels it', (c) => c.sched?.items?.length === 0 && c.sched.list === 'Nothing scheduled for this project.');
await T.ui('sched-open');
await until('the clock again closes it', (c) => c.sched === null);

// ─── Secrets for this chat (2.12: the head's lock opens it) ───
let s4 = await T.state();
t.check('the server can pre-authorise: the lock counts 0', s4.secretsCap === true && s4.secretsCount === 0, { cap: s4.secretsCap, n: s4.secretsCount });
await T.ui('sec-open');
s4 = await T.until('the dialog opens for this chat and loads the secrets', (s) => s.secretsDialog && !s.secretsDialog.loading && s.secretsDialog.chat === s.chat);
t.check('nothing pre-authorised yet', s4 && s4.secretsDialog.grants.length === 0, s4?.secretsDialog);
await T.ui('sec-grant');
s4 = await T.state();
t.check('Pre-authorise with nothing ticked says so', s4.secretsDialog?.err === 'Choose at least one secret.', s4.secretsDialog);
await T.snap(A, 'secrets');
await T.key('Escape');
await T.until('Esc closes it', (s) => s.secretsDialog === null);

// ─── Saved drafts: across the pane's rebuild ───
await T.ui('composer-text', 'a draft that survives');
await t.sleep(800);
let s3 = await T.state();
const saved = JSON.stringify(s3.composer?.savedDrafts || {});
t.check('the draft is saved', saved.includes('a draft that survives'), saved);
const cfg = readdirSync(T.configDir).filter((f) => f.endsWith('.json')).map((f) => readFileSync(join(T.configDir, f), 'utf8')).join('');
t.check('in the client\'s settings on disk', cfg.includes('a draft that survives'));
const other = await T.api.http('POST', '/api/projects', { alias: 'other', name: 'Other', path: `${T.projDir}-other`, create: true });
t.check('a second project', other.status < 300, other.body);
await T.ui('select-project', 'other');
await T.until('the other project open (the chat pane gone)', (s) => s.project === 'other' && !s.composer);
await T.ui('select-project', 'demo');
await until('back: the chat\'s pane is made again and takes its draft back', (c) => c.text === 'a draft that survives', 20000);

// ─── Narrow ───
await T.d.send({ cmd: 'resize', w: 900, h: 800 });
await T.ui('composer-text', '');
await T.d.send({ cmd: 'resize', w: 500, h: 800 });
c = await until('a narrow pane (under 520): the composer stacks', (c) => c.narrow);
t.check('narrow: the placeholder without the hint, no session id', c?.placeholder === 'Message Claude…' && c.session === '', c);
await t.sleep(500);
await T.snap(A, 'narrow');
await T.d.send({ cmd: 'resize', w: 1440, h: 900 });
await until('wide again', (c) => !c.narrow && c.placeholder.includes('Enter to send'));

// ─── Exited ───
const chat = (await T.state()).chat;
const pids = execFileSync('pgrep', ['-f', 'claude-stub.mjs']).toString().trim().split('\n').filter(Boolean);
let killed = 0;
for (const p of pids) {
  try { if (readFileSync(`/proc/${p}/environ`, 'utf8').split('\0').includes(`CRUNDI_CHAT_ID=${chat}`)) { process.kill(Number(p)); killed++; } } catch { /* gone */ }
}
t.check('the chat\'s Claude process stopped', killed > 0, { pids, chat });
c = await until('exited: "Session ended.", the box disabled, the label exited', (c) => c.state === 'exited' && c.placeholder === 'Session ended.' && c.disabled && !c.stop && c.send === 'Send', 20000);
await t.sleep(500);
await T.snap(A, 'exited');

execFileSync('rm', ['-rf', dir]);
await T.end();
