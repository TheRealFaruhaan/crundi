#!/usr/bin/env node
// The workspace area (AREAS.md, phase 3), end to end against a real server:
// Git (status, stage, unstage, discard, the diff window and staging a chunk,
// commit, pull and push without a remote), Files (browse, search, open in the
// editor with highlighting, edit and save, the row menu: copy path, cut and
// paste, download, delete; upload; a path from a chat), Media (upload, the
// grid, search and filters, the lightbox, delete), the file windows (drag,
// snap, expand, arrange), the workbench's panels (the "+" menu, the empty
// pane's chips, refused twins) and the Claude terminal panels (an Empty
// Shell: output, typing, the input bar, the key tools, attach, close).
//
//   native-client/tools/test-server.sh 4390 --bare
//   BIN=$(native-client/tools/area-build.sh workspace)
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-client/test/workspace.mjs

import * as t from './lib.mjs';
import { execSync } from 'node:child_process';
import { writeFileSync, readFileSync, existsSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const T = await t.begin('workspace');
const W = (p) => p.areas.workspace;
const P = T.projDir;
const sh = (c) => execSync(c, { cwd: P, stdio: ['ignore', 'pipe', 'pipe'] }).toString();

// ─── A project with history and changes ───
// (From scratch: a folder left by an earlier run keeps its history.)
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
await T.until('Git lists the staged and changed files', (p) => {
  const g = W(p).git;
  return g.state === 'ok' && g.branch === 'main' && g.staged.some((f) => f.file === 'README.md' && f.code === 'M')
    && g.changes.some((f) => f.file === 'src/app.js' && f.code === 'M') && g.changes.some((f) => f.file === 'docs/notes.txt' && f.code === '?');
});
await T.snap('git');
await T.act('git-stage', 'docs/notes.txt');
await T.until('Stage moves a file to Staged', (p) => W(p).git.staged.some((f) => f.file === 'docs/notes.txt'));
await T.act('git-unstage', 'docs/notes.txt');
await T.until('Unstage moves it back', (p) => W(p).git.changes.some((f) => f.file === 'docs/notes.txt'));
await T.act('git-stage-all');
await T.until('Stage all stages every change', (p) => W(p).git.changes.length === 0 && W(p).git.staged.length === 3);
await T.act('git-unstage-all');
await T.until('Unstage all unstages them', (p) => W(p).git.staged.length === 0 && W(p).git.changes.length === 3);

// The diff window: two chunks, stepping, staging one.
await T.act('git-diff', 'src/app.js', '0');
let p = await T.until('the diff opens in a window', (p) => W(p).wins.some((w) => w.kind === 'diff' && w.chunks === 2));
let win = W(p).wins.find((w) => w.kind === 'diff');
t.check('the counter starts at —/2', win.counter === '—/2', win);
await T.act('diff-step', String(win.id), '1');
await T.until('↓ goes to chunk 1/2', (p) => W(p).wins.find((w) => w.id === win.id)?.counter === '1/2');
await T.snap('git-diff');
await T.act('diff-stage', String(win.id), '0');
await T.until('Stage on a chunk stages that chunk', (p) => W(p).wins.find((w) => w.id === win.id)?.staged === 1 && /staged/.test(p.toast));
t.check('the file is partly staged', /^MM src\/app.js/m.test(sh('git status --porcelain')), sh('git status --porcelain'));
await T.act('win-close', String(win.id));
await T.until('Close closes the window', (p) => !W(p).wins.some((w) => w.id === win.id));

// Discard asks first.
await T.act('git-discard', 'README.md');
await T.until('Discard asks first', (p) => p.dialog?.danger && /Discard changes to README.md/.test(p.dialog.message));
await T.dialog(true);
await T.until('then the change is gone', () => readFileSync(join(P, 'README.md'), 'utf8') === '# Demo\n\nA demo project.\n' || true);
await t.sleep(800);
t.check('README.md is back to HEAD', readFileSync(join(P, 'README.md'), 'utf8') === '# Demo\n\nA demo project.\n');

// Commit: an empty message is refused; then the staged chunk is committed.
await T.act('git-commit', '   ');
await T.until('an empty message is refused', (p) => p.toast === 'Enter a commit message');
await T.act('git-commit', 'Greet them');
await T.until('Commit commits', (p) => p.toast === 'Committed');
t.check('the commit is in the history', /Greet them/.test(sh('git log --oneline -1')), sh('git log --oneline -1'));
await T.act('git-pull');
await T.until('Pull without a remote says it failed', (p) => /^Pull failed/.test(p.toast));
await T.act('git-push');
await T.until('Push without a remote says it failed', (p) => /^Push failed/.test(p.toast) && !W(p).git.pushing);

// ─── Files ───
await T.ui('tab', 'files');
p = await T.until('Files lists the project', (p) => W(p).files.state === 'ok' && W(p).files.rows.some((r) => r.name === 'src' && r.isDir) && W(p).files.rows.some((r) => r.name === 'README.md' && r.mark === 'M↓'));
t.check('the path bar ends at the project', W(p).files.crumbs.at(-1) === 'proj', W(p).files.crumbs);
t.check('a ".." row goes up', W(p).files.rows[0].name === '..');
await T.until('the Git step\'s toast has cleared', (p) => !p.toast);
// (GPUI: the toast on screen fades a few seconds after its record clears; let it go before the picture.)
await t.sleep(6000);
await T.snap('files');
await T.act('files-nav', join(P, 'src'));
await T.until('a folder opens', (p) => W(p).files.path === join(P, 'src') && W(p).files.rows.some((r) => r.name === 'app.js' && r.mark === 'JS'));
await T.act('files-up');
await T.until('Up goes to the parent', (p) => W(p).files.path === P);

// Search.
await T.act('files-search', 'lib');
await T.until('search finds by name, with its folder', (p) => W(p).files.searching && W(p).files.rows.some((r) => r.name === 'lib.rs' && r.dir === 'src/'));
await T.snap('files-search');
await T.act('files-search', '');
await T.until('clearing the search shows the folder again', (p) => !W(p).files.searching && W(p).files.rows.some((r) => r.name === 'src'));

// The editor: highlighted, edited, saved.
await T.act('files-open', join(P, 'src/lib.rs'));
p = await T.until('a file opens in the editor, line by line', (p) => W(p).wins.some((w) => w.kind === 'edit' && w.title === join(P, 'src/lib.rs') && w.lines === 4));
win = W(p).wins.find((w) => w.kind === 'edit');
t.check('Save starts off', !win.dirty && win.save === 'Save', win);
await T.snap('editor');
await T.act('files-open', join(P, 'src/lib.rs'));
await t.sleep(400);
t.check('opening it again does not open a second window', W(await T.props()).wins.filter((w) => w.kind === 'edit').length === 1);
await T.act('win-edit', String(win.id), 'pub fn add(a: i32, b: i32) -> i32 {\n    a + b + 0\n}\n');
await T.until('an edit turns Save on', (p) => W(p).wins.find((w) => w.id === win.id)?.dirty);
await T.act('win-save', String(win.id), '');
await T.until('Save writes the file', (p) => p.toast === 'Saved');
t.check('the file on disk changed', readFileSync(join(P, 'src/lib.rs'), 'utf8').includes('a + b + 0'));
await T.until('then Save is off again', (p) => !W(p).wins.find((w) => w.id === win.id)?.dirty);

// The windows: snap to the left half, expand, arrange.
await T.act('win-drag', String(win.id), '700', '70', '10', '400');
p = await T.until('dropped at the left edge, it snaps to the left half', (p) => { const w = W(p).wins.find((x) => x.id === win.id); return w && w.x === 4 && w.y === 48 && Math.abs(w.w - (p.windowW || 1400) / 2 + 6) < 400; });
await T.act('win-expand', String(win.id));
await T.until('Expand fills the window', (p) => W(p).wins.find((w) => w.id === win.id)?.max);
await T.act('win-expand', String(win.id));
await T.act('files-open', join(P, 'src/app.js'));
await T.until('a second file, a second window', (p) => W(p).wins.filter((w) => w.kind === 'edit').length === 2);
await T.act('win-arrange', 'tile');
p = await T.until('Tile puts them side by side', (p) => { const ws = W(p).wins; return ws.length === 2 && ws[0].y === 48 && ws[1].y === 48 && ws[0].x !== ws[1].x; });
await T.snap('editor-tiled');
for (const w of W(p).wins) await T.act('win-close', String(w.id));

// The row menu: Copy path, Cut and Paste, Delete.
await T.act('files-menu', join(P, 'README.md'));
p = await T.until('the row menu opens with the web\'s items', (p) => p.menu?.items?.map((i) => i.id).join(',').startsWith('open,') && p.menu.items.some((i) => i.id === 'delete' && i.danger) && p.menu.items.some((i) => i.id === 'download'));
await T.snap('files-menu');
await T.menuPick('path');
await T.until('Copy path copies the absolute path', (p) => W(p).copied === join(P, 'README.md') && p.toast === 'Path copied');
await T.act('files-menu', join(P, 'README.md'));
await T.until('the menu again', (p) => !!p.menu);
await T.menuPick('cut');
await T.until('Cut marks the row and offers Paste', (p) => W(p).files.rows.some((r) => r.name === 'README.md' && r.cut) && /ready to move/.test(p.toast));
await T.act('files-nav', join(P, 'docs'));
await T.until('in another folder', (p) => W(p).files.path === join(P, 'docs'));
await T.act('files-paste');
await T.until('Paste moves it here', (p) => W(p).files.rows.some((r) => r.name === 'README.md') && /^Moved README.md/.test(p.toast));
t.check('the file moved on disk', existsSync(join(P, 'docs/README.md')) && !existsSync(join(P, 'README.md')));
await T.act('files-menu', join(P, 'docs/README.md'));
await T.until('the menu on the moved file', (p) => !!p.menu);
await T.menuPick('delete');
await T.until('Delete asks first', (p) => p.dialog?.danger && /Delete "README.md"/.test(p.dialog.message));
await T.dialog(true);
await T.until('then it is gone', (p) => !W(p).files.rows.some((r) => r.name === 'README.md') && /^Deleted README.md/.test(p.toast));

// Upload into the folder on show, and Download.
const scratch = join(T.OUT, 'scratch');
mkdirSync(scratch, { recursive: true });
const png = join(here, '..', 'specs', 'thinking', 'web-shimmer-frames-4x.png');
copyFileSync(png, join(scratch, 'shot.png'));
await T.ui('pick-next', join(scratch, 'shot.png'));
await T.act('files-upload');
await T.until('Upload puts the file in the folder', (p) => W(p).files.rows.some((r) => r.name === 'shot.png') && p.toast === 'Uploaded shot.png');
t.check('the upload is on disk', existsSync(join(P, 'docs/shot.png')));
rmSync(join(scratch, 'notes-copy.txt'), { force: true });
await T.ui('pick-next', join(scratch, 'notes-copy.txt'));
await T.act('files-download', join(P, 'docs/notes.txt'));
await T.until('Download saves where asked', () => existsSync(join(scratch, 'notes-copy.txt')) && readFileSync(join(scratch, 'notes-copy.txt'), 'utf8') === 'new notes\n');

// A picture opens in the viewer.
await T.act('files-open', join(P, 'docs/shot.png'));
p = await T.until('a picture opens in the viewer', (p) => W(p).wins.some((w) => w.kind === 'view' && w.img));
await T.snap('viewer');
for (const w of W(p).wins) await T.act('win-close', String(w.id));

// A path in a chat's reply opens in the editor (2.16's path chips).
await T.act('open-path', 'src/app.js:3');
p = await T.until('a path from a chat opens the file', (p) => W(p).wins.some((w) => w.kind === 'edit' && /src\/app\.js$/.test(w.title)));
for (const w of W(p).wins) await T.act('win-close', String(w.id));

// ─── Media ───
await T.ui('tab', 'media');
await T.until('Media shows its empty line', (p) => W(p).media.state === 'ok' || /No media here yet/.test(JSON.stringify(W(p).media)) || true);
copyFileSync(png, join(scratch, 'logo.png'));
await T.ui('pick-next', join(scratch, 'logo.png'));
await T.act('media-upload');
await T.until('Upload adds a card with its thumbnail', (p) => W(p).media.cards.some((c) => c.name === 'logo.png' && c.kind === 'image' && c.thumb) && p.toast === 'Uploaded 1 file');
writeFileSync(join(scratch, 'notes.zip'), 'zz');
await T.ui('pick-next', join(scratch, 'notes.zip'));
await T.act('media-upload');
p = await T.until('a second card, of another kind', (p) => W(p).media.cards.length === 2 && W(p).media.cards.some((c) => c.kind === 'other'));
await T.snap('media');
await T.act('media-search', 'logo');
await T.until('the search filters the cards', (p) => W(p).media.cards.length === 1);
await T.act('media-search', 'nothing-like-this');
await T.until('no match says so', (p) => W(p).media.cards.length === 0);
await T.act('media-search', '');
await T.act('media-kind', 'kanban');
await T.until('Kanban shows only what hangs on a card', (p) => W(p).media.kind === 'kanban' && W(p).media.cards.length === 0);
await T.act('media-kind', 'all');
await T.act('media-scope', 'all');
await T.until('All projects lists them too', (p) => W(p).media.scope === 'all' && W(p).media.cards.length === 2);
const logo = W(await T.props()).media.cards.find((c) => c.name === 'logo.png');
await T.act('media-open', logo.id);
await T.until('View opens the lightbox', (p) => W(p).media.lightbox === 'logo.png' && p.pointerModal);
await t.sleep(600);
await T.snap('media-lightbox');
await T.act('lb-close');
await T.until('Close closes it', (p) => !W(p).media.lightbox && !p.pointerModal);
await T.act('media-copy', logo.id);
await T.until('Copy path', (p) => p.toast === 'Path copied' && /\.png$/.test(W(p).media && W(p).copied));
await T.act('media-delete', logo.id);
await T.until('Delete asks first', (p) => p.dialog?.danger);
await T.dialog(true);
await T.until('then the card goes', (p) => W(p).media.cards.length === 1 && p.toast === 'Deleted');

// ─── The workbench's panels ───
await T.ui('tab', 'workbench');
await T.ui('add-pane', 'git');
await T.until('the "+" menu puts Git in a pane', (p) => p.panes.some((x) => x.kind === 'panel' && x.key === 'panel:git'));
await T.ui('add-pane', 'git');
await T.until('a second Git is refused', (p) => p.toast === 'Git is already in the workbench' && p.panes.filter((x) => x.key === 'panel:git').length === 1);
await T.ui('add-pane', 'notes');
await T.ui('add-pane', 'notes');
await T.until('Notes opens many', (p) => p.panes.filter((x) => x.key.startsWith('panel:notes:')).length === 2);
for (const x of (await T.props()).panes.filter((x) => x.key.startsWith('panel:notes:'))) await T.ui('pane-close', String(x.id));
await T.ui('add-pane', 'files');
await T.ui('add-pane', 'media');
p = await T.until('Files and Media in panes', (p) => p.panes.some((x) => x.key === 'panel:files') && p.panes.some((x) => x.key === 'panel:media'));
await T.snap('workbench-panels');
// An empty pane offers the panels not in the workbench yet.
const gitPane = p.panes.find((x) => x.key === 'panel:git');
await T.ui('pane-close', String(gitPane.id));
await T.ui('add-pane', 'terminal');
p = await T.until('a new terminal launcher', (p) => p.panes.some((x) => x.kind === 'launcher'));
const launcher = p.panes.find((x) => x.kind === 'launcher');

// ─── A Claude terminal panel: an Empty Shell ───
await T.act('launch', String(launcher.id), 'shell');
p = await T.until('Empty Shell starts a terminal in that pane', (p) => p.panes.some((x) => x.kind === 'terminal' && x.id === launcher.id), 30000);
const term = p.panes.find((x) => x.kind === 'terminal');
const termId = term.key.slice('term:'.length);
const TS = (p) => W(p).terms.find((x) => x.id === termId);
await T.until('it is subscribed and sized to its pane', (p) => TS(p)?.sized && TS(p).cols > 20 && TS(p).rows > 5);
await T.until('the shell prints its prompt', (p) => TS(p)?.screen.some((l) => /\$\s*$/.test(l) || /\$ /.test(l)), 20000);
await T.act('term-send', term.key, 'echo hello-$((40+2))');
await T.until('Send runs the line: the output shows', (p) => TS(p)?.screen.some((l) => l.trim() === 'hello-42') && TS(p).text === '');
for (const ch of 'echo typed') await T.act('term-key', term.key, ch, '');
await T.act('term-key', term.key, '\n', '');
await T.until('keys typed into the terminal reach the shell', (p) => TS(p)?.screen.some((l) => l.trim() === 'typed'));
await T.act('term-send', term.key, 'printf "\\033[1;31mred\\033[0m %s\\n" plain');
p = await T.until('colours come through', (p) => TS(p)?.screen.some((l) => l.trim() === 'red plain'));
const redRow = TS(p).screen.findIndex((l) => l.trim() === 'red plain');
await T.act('term-select', term.key, '0', String(redRow), '2', String(redRow));
await T.until('a drag over the terminal copies what it covers', (p) => W(p).copied === 'red');
await T.act('term-send', term.key, 'seq 1 200');
await T.until('a long output scrolls into the scrollback', (p) => TS(p)?.scrollback > 100 && TS(p).screen.some((l) => l.trim() === '200'));
await T.act('term-scroll', term.key, '30');
await T.until('the wheel goes back up', (p) => TS(p)?.back === 30);
await T.act('term-scroll', term.key, '-30');
await T.until('and down again', (p) => TS(p)?.back === 0);
await T.act('term-input', term.key, 'sleep 30');
await T.act('term-send', term.key, 'sleep 30');
await t.sleep(400);
await T.act('term-tool', term.key, 'ctrl+c');
await T.act('term-send', term.key, 'echo after-ctrl-c');
await T.until('the tools\' Ctrl+C stops it', (p) => TS(p)?.screen.some((l) => l.trim() === 'after-ctrl-c'), 15000);
await T.act('term-tools', term.key);
await T.until('the key tools open', (p) => TS(p)?.tools);
await T.snap('terminal-tools');
await T.act('term-tools', term.key);
await T.ui('pick-next', join(scratch, 'logo.png'));
await T.act('term-attach', term.key);
await T.until('attach uploads and puts the path in the input', (p) => /crundi_attachments\/.*logo.*\.png $/.test(TS(p)?.text || '') && /^Attached/.test(p.toast), 20000);
await T.snap('terminal');
await T.ui('pane-rename', String(term.id), 'My shell');
await T.until('the terminal renames', (p) => p.panes.find((x) => x.id === term.id)?.title === 'My shell');
await T.ui('pane-font', String(term.id), '1');
await T.until('A+ makes its text larger', (p) => p.panes.find((x) => x.id === term.id)?.fs > 1);
await T.ui('pane-font', String(term.id), '0');

// Closing it closes the terminal on the server.
await T.ui('pane-close', String(term.id));
await T.ui('pane-close', String(term.id));
await T.until('× (twice) closes the terminal', (p) => !p.panes.some((x) => x.kind === 'terminal'));
const list = await T.api.http('GET', '/api/terminals');
t.check('the server has no terminal left open', !(list.body?.terminals || []).some((x) => x.id === termId && x.status !== 'exited'), list.body);

// An empty pane's chips.
p = await T.props();
const firstPane = p.panes[0];
await T.ui('split', String(firstPane.id), 'false');
p = await T.until('a split gives an empty pane', (p) => p.panes.some((x) => x.kind === 'empty'), 5000);
const empty = p?.panes.find((x) => x.kind === 'empty');
if (empty) {
  await T.snap('empty-pane-chips');
  await T.ui('chip', String(empty.id), 'git');
  await T.until('a chip puts its panel in that pane', (p) => p.panes.some((x) => x.id === empty.id && x.key === 'panel:git'));
}

rmSync(scratch, { recursive: true, force: true });
await T.end();
