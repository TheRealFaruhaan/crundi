// The workspace area (AREAS.md), end to end against a fresh test server:
// Git (status, stage, unstage, discard, the diff window and staging a chunk,
// commit, pull and push without a remote), Files (browse, search, open in
// the editor, edit and save, the row menu: copy path, cut and paste,
// download, delete; upload; a path from a chat), Media (upload, the grid,
// search and filters, the lightbox, delete), the file windows (drag, snap,
// expand, arrange), the panels in the workbench (an empty pane's chips,
// refused twins) and a Claude terminal pane (an Empty Shell: output,
// typing, the input bar, the key tools, attach, close). Ported from
// native-gpui/test/workspace.mjs, with ARCHITECTURE.md's checks: a
// terminal's output is batched (at most one update a frame), its streaming
// redraws the screen and never the input bar or the pane's head, the frames
// while it streams, and idle draws no frames.
//
// How to run it: test/lib.mjs (a fresh bare test server, under Xvfb).

import * as t from './lib.mjs';
import { execSync } from 'node:child_process';
import { writeFileSync, readFileSync, existsSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const T = await t.begin('workspace');
const W = (s) => s.workspace;
const P = `/tmp/crundi-devtest-${new URL(T.SERVER).port}/proj`;
const act = (n, ...args) => T.ui(`workspace:${n}`, ...args);
const sh = (c) => execSync(c, { cwd: P, stdio: ['ignore', 'pipe', 'pipe'] }).toString();
const snap = (f) => T.snap('workspace', f);

// ─── A project with history and changes ───
for (const f of ['.git', 'src', 'docs', 'README.md']) rmSync(join(P, f), { recursive: true, force: true });
mkdirSync(join(P, 'src'), { recursive: true });
mkdirSync(join(P, 'docs'), { recursive: true });
writeFileSync(join(P, 'src/app.js'), 'function hello(name) {\n  // greet\n  const n = 42;\n  return "Hello, " + name;\n}\nmodule.exports = { hello };\n');
writeFileSync(join(P, 'src/lib.rs'), 'pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n');
writeFileSync(join(P, 'README.md'), '# Demo\n\nA demo project.\n');
writeFileSync(join(P, '.gitignore'), '.claude/\n');
sh('git init -q -b main && git config user.email t@example.com && git config user.name Tester && git add -A && git commit -qm init');
writeFileSync(join(P, 'src/app.js'), 'function hello(name) {\n  // greet them\n  const n = 43;\n  return "Hi, " + name;\n}\nmodule.exports = { hello };\n\nfunction bye() { return 1; }\n');
writeFileSync(join(P, 'README.md'), '# Demo\n\nA changed demo project.\n');
writeFileSync(join(P, 'docs/notes.txt'), 'new notes\n');
sh('git add README.md');

// ─── Git ───
await T.ui('tab', 'git');
let s = await T.until('Git lists the staged and changed files', (s) => {
  const g = W(s).git;
  return g.state === 'ok' && g.branch === 'main' && g.staged.some((f) => f.file === 'README.md' && f.code === 'M')
    && g.changes.some((f) => f.file === 'src/app.js' && f.code === 'M') && g.changes.some((f) => f.file === 'docs/notes.txt' && f.code === '?');
});
t.check('the list is STAGED and its file, CHANGES and their two', W(s)?.git.rows === 5, W(s)?.git);
await snap('git');
await act('git-stage', 'docs/notes.txt');
await T.until('Stage moves a file to Staged', (s) => W(s).git.staged.some((f) => f.file === 'docs/notes.txt'));
await act('git-unstage', 'docs/notes.txt');
await T.until('Unstage moves it back', (s) => W(s).git.changes.some((f) => f.file === 'docs/notes.txt'));
await act('git-stage-all');
await T.until('Stage all stages every change', (s) => W(s).git.changes.length === 0 && W(s).git.staged.length === 3);
await act('git-unstage-all');
await T.until('Unstage all unstages them', (s) => W(s).git.staged.length === 0 && W(s).git.changes.length === 3);
// A refresh that finds nothing new redraws nothing (the 8 s refresh on an idle tab).
{
  const r0 = (await T.state()).renders?.git || 0;
  await act('git-refresh');
  await t.sleep(1500);
  const r1 = (await T.state()).renders?.git || 0;
  t.check('a refresh with nothing new does not redraw Git', r1 === r0, { r0, r1 });
}

// The diff window: two chunks, stepping, staging one.
await act('git-diff', 'src/app.js', '0');
s = await T.until('the diff opens in a window', (s) => W(s).wins.some((w) => w.kind === 'diff' && w.chunks === 2));
let win = W(s)?.wins.find((w) => w.kind === 'diff');
if (win) {
  t.check('the counter starts at —/2', win.counter === '—/2', win);
  await act('diff-step', String(win.id), '1');
  await T.until('↓ goes to chunk 1/2', (s) => W(s).wins.find((w) => w.id === win.id)?.counter === '1/2');
  await snap('git-diff');
  await act('diff-stage', String(win.id), '0');
  await T.until('Stage on a chunk stages that chunk', (s) => W(s).wins.find((w) => w.id === win.id)?.staged === 1 && /staged/.test(s.toast));
  t.check('the file is partly staged', /^MM src\/app.js/m.test(sh('git status --porcelain')), sh('git status --porcelain'));
  await act('win-close', String(win.id));
  await T.until('Close closes the window', (s) => !W(s).wins.some((w) => w.id === win.id));
}

// Discard asks first.
await act('git-discard', 'README.md');
await T.until('Discard asks first', (s) => s.dialog?.danger && /Discard changes to README.md/.test(s.dialog.message));
await T.ui('dialog-answer', 'true');
await t.sleep(1000);
t.check('README.md is back to HEAD', readFileSync(join(P, 'README.md'), 'utf8') === '# Demo\n\nA demo project.\n');

// Commit: an empty message is refused; then the staged chunk is committed.
await act('git-commit', '   ');
await T.until('an empty message is refused', (s) => s.toast === 'Enter a commit message');
await act('git-commit', 'Greet them');
await T.until('Commit commits', (s) => s.toast === 'Committed');
t.check('the commit is in the history', /Greet them/.test(sh('git log --oneline -1')), sh('git log --oneline -1'));
await act('git-pull');
await T.until('Pull without a remote says it failed', (s) => /^Pull failed/.test(s.toast));
await act('git-push');
await T.until('Push without a remote says it failed', (s) => /^Push failed/.test(s.toast) && !W(s).git.pushing);
await T.ui('toasts-clear');

// ─── Files ───
await T.ui('tab', 'files');
s = await T.until('Files lists the project', (s) => W(s).files.state === 'ok' && W(s).files.rows.some((r) => r.name === 'src' && r.isDir) && W(s).files.rows.some((r) => r.name === 'README.md' && r.mark === 'M↓'));
t.check('the path bar ends at the project', W(s)?.files.crumbs.at(-1) === 'proj', W(s)?.files.crumbs);
t.check('a ".." row goes up', W(s)?.files.rows[0].name === '..');
await snap('files');
await act('files-nav', join(P, 'src'));
await T.until('a folder opens', (s) => W(s).files.path === join(P, 'src') && W(s).files.rows.some((r) => r.name === 'app.js' && r.mark === 'JS'));
await act('files-up');
await T.until('Up goes to the parent', (s) => W(s).files.path === P);

// Search.
await act('files-search', 'lib');
await T.until('search finds by name, with its folder', (s) => W(s).files.searching && W(s).files.rows.some((r) => r.name === 'lib.rs' && r.dir === 'src/'));
await snap('files-search');
await act('files-search', '');
await T.until('clearing the search shows the folder again', (s) => !W(s).files.searching && W(s).files.rows.some((r) => r.name === 'src'));

// The editor: edited, saved.
await act('files-open', join(P, 'src/lib.rs'));
s = await T.until('a file opens in the editor, line by line', (s) => W(s).wins.some((w) => w.kind === 'edit' && w.title === join(P, 'src/lib.rs') && w.lines === 4));
win = W(s)?.wins.find((w) => w.kind === 'edit');
if (win) {
  t.check('Save starts off', !win.dirty && win.save === 'Save', win);
  await t.sleep(800);
  await snap('editor');
  await act('files-open', join(P, 'src/lib.rs'));
  await t.sleep(400);
  t.check('opening it again does not open a second window', W(await T.state()).wins.filter((w) => w.kind === 'edit').length === 1);
  await act('win-edit', String(win.id), 'pub fn add(a: i32, b: i32) -> i32 {\n    a + b + 0\n}\n');
  await T.until('an edit turns Save on', (s) => W(s).wins.find((w) => w.id === win.id)?.dirty);
  await act('win-save', String(win.id), '');
  await T.until('Save writes the file', (s) => s.toast === 'Saved');
  t.check('the file on disk changed', readFileSync(join(P, 'src/lib.rs'), 'utf8').includes('a + b + 0'));
  await T.until('then Save is off again', (s) => !W(s).wins.find((w) => w.id === win.id)?.dirty);

  // The windows: snap to the left half, expand, arrange.
  await act('win-drag', String(win.id), '700', '70', '10', '400');
  await T.until('dropped at the left edge, it snaps to the left half', (s) => { const w = W(s).wins.find((x) => x.id === win.id); return w && w.x === 4 && w.y === 48 && Math.abs(w.w - W(s).windowW / 2 + 6) < 1; });
  await act('win-expand', String(win.id));
  await T.until('Expand fills the window', (s) => W(s).wins.find((w) => w.id === win.id)?.max);
  await act('win-expand', String(win.id));
  await act('files-open', join(P, 'src/app.js'));
  await T.until('a second file, a second window', (s) => W(s).wins.filter((w) => w.kind === 'edit').length === 2);
  await act('win-arrange', 'tile');
  s = await T.until('Tile puts them side by side', (s) => { const ws = W(s).wins; return ws.length === 2 && ws[0].y === 48 && ws[1].y === 48 && ws[0].x !== ws[1].x; });
  await snap('editor-tiled');
  for (const w of W(await T.state()).wins) await act('win-close', String(w.id));
}

// The row menu: Copy path, Cut and Paste, Delete.
await act('files-menu', join(P, 'README.md'));
await T.until('the row menu opens with the web\'s items', (s) => W(s).menu?.items?.map((i) => i.id).join(',').startsWith('open,') && W(s).menu.items.some((i) => i.id === 'delete' && i.danger) && W(s).menu.items.some((i) => i.id === 'download'));
await snap('files-menu');
await T.ui('menu-pick', 'path');
await T.until('Copy path copies the absolute path', (s) => W(s).copied === join(P, 'README.md') && s.toast === 'Path copied');
await act('files-menu', join(P, 'README.md'));
await T.until('the menu again', (s) => !!W(s).menu);
await T.ui('menu-pick', 'cut');
await T.until('Cut marks the row and offers Paste', (s) => W(s).files.rows.some((r) => r.name === 'README.md' && r.cut) && /ready to move/.test(s.toast));
await act('files-nav', join(P, 'docs'));
await T.until('in another folder', (s) => W(s).files.path === join(P, 'docs'));
await act('files-paste');
await T.until('Paste moves it here', (s) => W(s).files.rows.some((r) => r.name === 'README.md') && /^Moved README.md/.test(s.toast));
t.check('the file moved on disk', existsSync(join(P, 'docs/README.md')) && !existsSync(join(P, 'README.md')));
await act('files-menu', join(P, 'docs/README.md'));
await T.until('the menu on the moved file', (s) => !!W(s).menu);
await T.ui('menu-pick', 'delete');
await T.until('Delete asks first', (s) => s.dialog?.danger && /Delete "README.md"/.test(s.dialog.message));
await T.ui('dialog-answer', 'true');
await T.until('then it is gone', (s) => !W(s).files.rows.some((r) => r.name === 'README.md') && /^Deleted README.md/.test(s.toast));

// Upload into the folder on show, and Download.
const scratch = join(T.OUT, 'scratch');
mkdirSync(scratch, { recursive: true });
const png = join(here, '..', 'crates', 'app', 'assets', 'icon-256.png');
copyFileSync(png, join(scratch, 'shot.png'));
await act('pick-next', join(scratch, 'shot.png'));
await act('files-upload');
await T.until('Upload puts the file in the folder', (s) => W(s).files.rows.some((r) => r.name === 'shot.png') && s.toast === 'Uploaded shot.png');
t.check('the upload is on disk', existsSync(join(P, 'docs/shot.png')));
rmSync(join(scratch, 'notes-copy.txt'), { force: true });
await act('pick-next', join(scratch, 'notes-copy.txt'));
await act('files-download', join(P, 'docs/notes.txt'));
await T.until('Download saves where asked', () => existsSync(join(scratch, 'notes-copy.txt')) && readFileSync(join(scratch, 'notes-copy.txt'), 'utf8') === 'new notes\n');

// A picture opens in the viewer.
await act('files-open', join(P, 'docs/shot.png'));
s = await T.until('a picture opens in the viewer', (s) => W(s).wins.some((w) => w.kind === 'view' && w.img));
await T.ui('toasts-clear'); await t.sleep(300);
await snap('viewer');
for (const w of W(await T.state()).wins) await act('win-close', String(w.id));

// A path in a chat's reply opens in the editor (2.16's path chips).
await act('open-path', 'src/app.js:3');
await T.until('a path from a chat opens the file', (s) => W(s).wins.some((w) => w.kind === 'edit' && /src\/app\.js$/.test(w.title)));
for (const w of W(await T.state()).wins) await act('win-close', String(w.id));

// ─── Media ───
await T.ui('tab', 'media');
await T.until('Media loads', (s) => W(s).media.state === 'ok');
copyFileSync(png, join(scratch, 'logo.png'));
await act('pick-next', join(scratch, 'logo.png'));
await act('media-upload');
await T.until('Upload adds a card with its thumbnail', (s) => W(s).media.cards.some((c) => c.name === 'logo.png' && c.kind === 'image' && c.thumb) && s.toast === 'Uploaded 1 file');
writeFileSync(join(scratch, 'notes.zip'), 'zz');
await act('pick-next', join(scratch, 'notes.zip'));
await act('media-upload');
await T.until('a second card, of another kind', (s) => W(s).media.cards.length === 2 && W(s).media.cards.some((c) => c.kind === 'other'));
await snap('media');
await act('media-search', 'logo');
await T.until('the search filters the cards', (s) => W(s).media.cards.length === 1);
await act('media-search', 'nothing-like-this');
await T.until('no match says so', (s) => W(s).media.cards.length === 0);
await act('media-search', '');
await act('media-kind', 'kanban');
await T.until('Kanban shows only what hangs on a card', (s) => W(s).media.kind === 'kanban' && W(s).media.cards.length === 0);
await act('media-kind', 'all');
await act('media-scope', 'all');
await T.until('All projects lists them too', (s) => W(s).media.scope === 'all' && W(s).media.cards.length === 2);
const logo = W(await T.state()).media.cards.find((c) => c.name === 'logo.png');
if (logo) {
  await act('media-open', logo.id);
  await T.until('View opens the lightbox', (s) => W(s).media.lightbox === 'logo.png' && W(s).pointerModal);
  await t.sleep(600);
  await snap('media-lightbox');
  await act('lb-close');
  await T.until('Close closes it', (s) => !W(s).media.lightbox && !W(s).pointerModal);
  await act('media-copy', logo.id);
  await T.until('Copy path', (s) => s.toast === 'Path copied' && /\.png$/.test(W(s).copied));
  await act('media-delete', logo.id);
  await T.until('Delete asks first (media)', (s) => s.dialog?.danger);
  await T.ui('dialog-answer', 'true');
  await T.until('then the card goes', (s) => W(s).media.cards.length === 1 && s.toast === 'Deleted');
}

// ─── The workbench: a Claude terminal pane (an Empty Shell) ───
await T.ui('tab', 'workbench');
s = await T.until('the workbench has its New terminal launcher', (s) => s.panes?.some((k) => k.startsWith('pend:')));
await act('launch', '', 'shell');
s = await T.until('Empty Shell starts a terminal in the launcher\'s place', (s) => s.panes?.some((k) => k.startsWith('term:')) && !s.panes.some((k) => k.startsWith('pend:')), 30000);
const key = s?.panes.find((k) => k.startsWith('term:'));
const TS = (s) => W(s).terms.find((x) => x.key === key);
await T.until('it is subscribed and sized to its pane', (s) => TS(s)?.sized && TS(s).cols > 20 && TS(s).rows > 5);
await T.until('the shell prints its prompt', (s) => TS(s)?.screen.some((l) => /\$\s*$/.test(l) || /\$ /.test(l)), 20000);
await act('term-send', key, 'echo hello-$((40+2))');
await T.until('Send runs the line: the output shows', (s) => TS(s)?.screen.some((l) => l.trim() === 'hello-42') && TS(s).text === '');
await act('term-focus', key);
for (const ch of 'echo typed') await act('term-key', key, ch, '');
await act('term-key', key, '\n', '');
await T.until('keys typed into the terminal reach the shell', (s) => TS(s)?.screen.some((l) => l.trim() === 'typed'));
await act('term-send', key, 'printf "\\033[1;31mred\\033[0m %s\\n" plain');
s = await T.until('colours come through', (s) => TS(s)?.screen.some((l) => l.trim() === 'red plain'));
const redRow = TS(s)?.screen.findIndex((l) => l.trim() === 'red plain') ?? 0;
await act('term-select', key, '0', String(redRow), '2', String(redRow));
await T.until('a selection over the terminal copies what it covers', (s) => W(s).copied === 'red');
await snap('terminal');

// Streaming: a long burst of output, batched (at most one update a frame),
// redrawing the screen alone: never the input bar nor the pane's head.
{
  // (Send itself empties the box: that redraw is the send's, so the
  // counts start once it is sent and before the output begins.)
  await act('term-send', key, 'sleep 1; seq 1 20000');
  await t.sleep(400);
  const before = await T.state();
  const b0 = TS(before).batches, r0 = before.renders || {};
  const from = T.frames.mark();
  const after = await T.until('a long output streams into the scrollback', (s) => TS(s)?.screen.some((l) => l.trim() === '20000') && TS(s).scrollback > 1000, 60000);
  const frames = T.frames.since(from);
  const r1 = after?.renders || {};
  const batches = TS(after || before).batches - b0;
  const screens = (r1['term-screen'] || 0) - (r0['term-screen'] || 0);
  console.log(`  streaming 20000 lines: ${batches} batches, ${screens} screen renders; frames ${JSON.stringify(frames)}`);
  t.check('the output came in batches (not a feed per message)', batches >= 1 && batches < 2000, { batches });
  t.check('the screen redrew at most once a batch (and a frame)', screens <= batches + 3 && screens <= frames.frames + 3, { screens, batches, frames: frames.frames });
  t.check('streaming never redrew the input bar', (r1['term-input'] || 0) === (r0['term-input'] || 0), { before: r0['term-input'], after: r1['term-input'] });
  t.check('streaming never redrew a pane head', (r1.head || 0) === (r0.head || 0), { before: r0.head, after: r1.head });
}
await act('term-scroll', key, '30');
await T.until('the wheel goes back up', (s) => TS(s)?.back === 30);
await act('term-scroll', key, '-30');
await T.until('and down again', (s) => TS(s)?.back === 0);
await act('term-send', key, 'sleep 30');
await t.sleep(400);
await act('term-tool', key, 'ctrl+c');
await act('term-send', key, 'echo after-ctrl-c');
await T.until('the tools\' Ctrl+C stops it', (s) => TS(s)?.screen.some((l) => l.trim() === 'after-ctrl-c'), 15000);
await act('term-tools', key);
await T.until('the key tools open', (s) => TS(s)?.tools);
await snap('terminal-tools');
await act('term-tools', key);
await act('pick-next', join(scratch, 'shot.png'));
await act('term-attach', key);
await T.until('attach uploads and puts the path in the input', (s) => /crundi_attachments\/.*shot.*\.png $/.test(TS(s)?.text || '') && /^Attached/.test(s.toast), 20000);
await act('term-input', key, '');
await act('term-zoom', key, '1');
await T.until('A+ makes its text larger', (s) => TS(s)?.zoom === 1);
await act('term-zoom', key, '0');
await T.until('↺ resets it', (s) => TS(s)?.zoom === 0);

// Idle (the terminal not focused: no blink), nothing animating: no frames.
{
  await act('term-bottom', key);
  await T.ui('tab', 'files');
  await T.ui('tab', 'workbench');
  await T.ui('toasts-clear');
  await t.sleep(1500);
  const from = T.frames.mark();
  await t.sleep(5000);
  const idle = T.frames.since(from);
  t.check('idle: no frames at all for 5 s', idle.frames === 0, idle);
}

// An empty pane's chips: a panel each, a second refused.
await act('chip', 'git');
await T.until('the Git chip puts Git in the workbench', (s) => s.panes?.includes('panel:git'));
await act('chip', 'git');
await T.until('a second Git is refused', (s) => s.toast === 'Git is already in the workbench' && s.panes.filter((k) => k === 'panel:git').length === 1);
await act('chip', 'files');
await act('chip', 'media');
await T.until('Files and Media in panes', (s) => s.panes?.includes('panel:files') && s.panes.includes('panel:media'));
await t.sleep(800);
await snap('workbench-panels');

// Closing the terminal closes it on the server.
const termId = key.slice('term:'.length);
await act('term-close', key);
await T.until('× closes the terminal and its pane', (s) => !W(s).terms.some((x) => x.key === key) && !s.panes.includes(key), 30000);
const list = await T.api.http('GET', '/api/terminals');
t.check('the server has no terminal left open', !(list.body?.terminals || list.body || []).some?.((x) => x.id === termId && x.status !== 'exited'), list.body);

rmSync(scratch, { recursive: true, force: true });
await T.end();
