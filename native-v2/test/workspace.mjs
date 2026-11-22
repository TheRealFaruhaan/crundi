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
  t.check('the diff is a floating file window, not a dialog', !W(s).pointerModal && !s.dialog);
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
await act('files-escape');
await T.until('Esc in the search clears it: the folder again', (s) => !W(s).files.searching && W(s).files.rows.some((r) => r.name === 'src'));

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

  // The windows: dragged by the header (with the pointer, as a person
  // does), the Arrange bar shows while dragging and a moment after; snap
  // to the left half, resize by the corner, expand, arrange.
  t.check('no Tile / Grid / Cascade in the header: the Arrange bar shows only while dragging', !W(await T.state()).wins.find((w) => w.id === win.id)?.arrange);
  {
    const w0 = W(await T.state()).wins.find((w) => w.id === win.id);
    await T.mouse('drag', w0.x + 60, w0.y + 14, { toX: w0.x + 160, toY: w0.y + 64, steps: 6 });
    s = await T.until('a drag by the header moves the window', (s) => { const w = W(s).wins.find((x) => x.id === win.id); return w && Math.abs(w.x - (w0.x + 100)) < 2 && Math.abs(w.y - (w0.y + 50)) < 2; });
    t.check('the Arrange bar shows after a drag', W(s)?.wins.find((w) => w.id === win.id)?.arrange);
    await snap('editor-arrange');
    await T.until('and goes a moment later', (s) => !W(s).wins.find((w) => w.id === win.id)?.arrange, 8000);
    const w1 = W(await T.state()).wins.find((w) => w.id === win.id);
    await T.mouse('drag', w1.x + w1.w - 5, w1.y + w1.h - 5, { toX: w1.x + w1.w + 75, toY: w1.y + w1.h + 35, steps: 6 });
    await T.until('the corner handle resizes it', (s) => { const w = W(s).wins.find((x) => x.id === win.id); return w && Math.abs(w.w - (w1.w + 80)) < 2 && Math.abs(w.h - (w1.h + 40)) < 2; });
    await act('win-resize', String(win.id), 'e', '0', '0', '-2000', '0');
    await T.until('no narrower than 360', (s) => W(s).wins.find((x) => x.id === win.id)?.w === 360);
  }
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
  // Esc closes the front window (the one opened last, or pressed last).
  const front = W(await T.state()).wins.find((w) => w.front);
  await T.key('Escape');
  await T.until('Esc closes the front window', (s) => W(s).wins.length === 1 && !W(s).wins.some((w) => w.id === front?.id));
  for (const w of W(await T.state()).wins) await act('win-close', String(w.id));
}

// The row menu: Copy path, Cut and Paste, Delete. Opened as a person does:
// a click on the row's "…" (where it is drawn: the files state's "more"),
// so the menu sits there (F40).
const clickMore = async (path) => {
  await T.until(`the row of ${path.split('/').pop()} is drawn with its "…"`, (s) => W(s).files.rows.find((r) => r.path === path)?.more);
  await t.sleep(300); // (the frame that drew it is on screen)
  const b = W(await T.state()).files.rows.find((r) => r.path === path)?.more;
  if (!b) return null;
  await T.mouse('click', b[0] + b[2] / 2, b[1] + b[3] / 2);
  return b;
};
// A row's tooltip (F160): it shows on a rest, gives way over the row's own
// "…" (whose tooltip is its own), and after a click stays away until the
// pointer has left the row and come back, as a browser's title, so it never
// covers the menu or the window the click opened.
{
  const ROW_TIP = 'Double-click to open. Right-click for more. Drag onto a terminal to insert its path.';
  await T.until('the README row is drawn', (s) => W(s).files.rows.find((r) => r.path === join(P, 'README.md'))?.more);
  await t.sleep(300);
  const b = W(await T.state()).files.rows.find((r) => r.path === join(P, 'README.md')).more;
  const [rx, ry] = [b[0] - 120, b[1] + b[3] / 2];
  const rest = async (x, y) => { await T.mouse('move', x, y); await T.mouse('move', x + 1, y); await t.sleep(1300); return W(await T.state()).tip; };
  let tip = await rest(rx, ry);
  t.check('resting on a row shows its tooltip', tip.text === ROW_TIP && tip.shown && tip.ago < 1200, tip);
  tip = await rest(b[0] + b[2] / 2, b[1] + b[3] / 2);
  t.check('over the row\'s "…" the row\'s tooltip gives way (the button has its own)', tip.text === ROW_TIP && !tip.shown, tip);
  tip = await rest(rx, ry);
  t.check('…and shows again off the button', tip.text === ROW_TIP && tip.shown && tip.ago < 1200, tip);
  await T.mouse('click', rx, ry);
  tip = await rest(rx + 3, ry);
  t.check('after a click on the row its tooltip stays away while the pointer rests there', tip.text === ROW_TIP && !tip.shown, tip);
  await T.mouse('move', rx, 5); await t.sleep(200);
  tip = await rest(rx, ry);
  t.check('…and comes back once the pointer left the row and returned', tip.text === ROW_TIP && tip.shown && tip.ago < 1200, tip);
  await T.mouse('move', rx, 5);
}
let at = await clickMore(join(P, 'README.md'));
s = await T.until('the row menu opens with the web\'s items', (s) => W(s).menu?.items?.map((i) => i.id).join(',').startsWith('open,') && W(s).menu.items.some((i) => i.id === 'delete' && i.danger) && W(s).menu.items.some((i) => i.id === 'download'));
// As the web's openFileMenu: under the button, from its right edge less 4 (kept in the window).
t.check('the menu opens under the row\'s "…" (not the window\'s corner)', at && W(s) && Math.abs(W(s).menu.x - (at[0] + at[2] - 4)) < 1 && Math.abs(W(s).menu.y - (at[1] + at[3] + 2)) < 1, { at, menu: W(s || {})?.menu && [W(s).menu.x, W(s).menu.y] });
await t.sleep(300); // (the frame that drew the menu is on screen: F45)
await snap('files-menu');
await T.ui('menu-pick', 'path');
await T.until('Copy path copies the absolute path', (s) => W(s).copied === join(P, 'README.md') && s.toast === 'Path copied');
await clickMore(join(P, 'README.md'));
await T.until('the menu again', (s) => !!W(s).menu);
await T.ui('menu-pick', 'cut');
await T.until('Cut marks the row and offers Paste', (s) => W(s).files.rows.some((r) => r.name === 'README.md' && r.cut) && /ready to move/.test(s.toast));
await act('files-nav', join(P, 'docs'));
await T.until('in another folder', (s) => W(s).files.path === join(P, 'docs'));
await act('files-paste');
await T.until('Paste moves it here', (s) => W(s).files.rows.some((r) => r.name === 'README.md') && /^Moved README.md/.test(s.toast));
t.check('the file moved on disk', existsSync(join(P, 'docs/README.md')) && !existsSync(join(P, 'README.md')));
await clickMore(join(P, 'docs/README.md'));
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

// An SVG previews as a picture; its menu offers "Edit the source". A PDF
// and a BMP open in the viewer too.
writeFileSync(join(P, 'docs/mark.svg'), '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><circle cx="20" cy="20" r="18" fill="#6366f1"/></svg>\n');
writeFileSync(join(P, 'docs/paper.pdf'), '%PDF-1.1\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF\n');
await act('files-refresh');
await T.until('the new files are listed', (s) => W(s).files.rows.some((r) => r.name === 'mark.svg') && W(s).files.rows.some((r) => r.name === 'paper.pdf'));
await act('files-open', join(P, 'docs/mark.svg'));
await T.until('an SVG opens as a picture', (s) => W(s).wins.some((w) => w.kind === 'view' && /mark\.svg$/.test(w.title) && w.img));
await clickMore(join(P, 'docs/mark.svg'));
s = await T.until('its menu offers Edit the source', (s) => W(s).menu?.items?.some((i) => i.id === 'source'));
await T.ui('menu-pick', 'source');
await T.until('Edit the source opens it in the editor', (s) => W(s).wins.some((w) => w.kind === 'edit' && /mark\.svg$/.test(w.title) && w.lines >= 1));
await act('files-open', join(P, 'docs/paper.pdf'));
await T.until('a PDF opens in the viewer as a page (not as text)', (s) => W(s).wins.some((w) => w.kind === 'view' && /paper\.pdf$/.test(w.title) && w.page));
await T.ui('toasts-clear'); await t.sleep(300);
await snap('viewer-pdf');
for (const w of W(await T.state()).wins) await act('win-close', String(w.id));
// A Files row dropped on a folder asks Move or Copy.
await act('files-drop-on', join(P, 'docs/notes.txt'), P, 'proj');
s = await T.until('a row dropped on a folder asks Move to or Copy to', (s) => W(s).menu?.items?.map((i) => i.id).join(',') === 'move,copy,cancel');
t.check('worded as the web\'s', W(s)?.menu?.items?.[0]?.label === 'Move to \u201cproj\u201d', W(s)?.menu?.items);
await T.ui('menu-pick', 'copy');
await T.until('Copy to copies it there', (s) => /^Copied notes.txt/.test(s.toast) && existsSync(join(P, 'notes.txt')));
rmSync(join(P, 'notes.txt'), { force: true });

// A path in a chat's reply opens in the editor (2.16's path chips).
await act('open-path', 'src/app.js:3');
await T.until('a path from a chat opens the file', (s) => W(s).wins.some((w) => w.kind === 'edit' && /src\/app\.js$/.test(w.title)));
for (const w of W(await T.state()).wins) await act('win-close', String(w.id));

// F209: Files reloads each time its tab is shown (the web's switchTab:
// loadFiles), keeping the folder on show; Git too (loadGitInfo), at once
// rather than on its 8 s timer.
await act('files-nav', join(P, 'docs'));
await T.until('Files is in docs', (s) => W(s).files.path === join(P, 'docs') && W(s).files.rows.some((r) => r.name === 'notes.txt'));
writeFileSync(join(P, 'docs/made-by-claude.txt'), 'new\n');
await T.ui('tab', 'git');
await T.until('a file made on disk shows in Git when its tab is shown', (s) => [...W(s).git.changes, ...W(s).git.staged].some((f) => /made-by-claude\.txt$/.test(f.file)), 4000);
await T.ui('tab', 'files');
await T.until('…and in Files when its tab is shown again, in the same folder', (s) => W(s).files.path === join(P, 'docs') && W(s).files.rows.some((r) => r.name === 'made-by-claude.txt'), 4000);
// F219: a reload clears the search, and the box with it (the web's
// filesSearchQuery = '' and a fresh empty input).
await act('files-search', 'notes');
await T.until('a search is typed in the box', (s) => W(s).files.searching && W(s).files.query === 'notes');
await T.ui('tab', 'git');
await T.ui('tab', 'files');
await T.until('a tab switch reloads Files: the folder again and the search box empty', (s) => !W(s).files.searching && W(s).files.query === '' && W(s).files.rows.some((r) => r.name === 'made-by-claude.txt'), 4000);
rmSync(join(P, 'docs/made-by-claude.txt'), { force: true });

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
  await T.until('View opens the lightbox (a dark layer, not a dialog)', (s) => W(s).media.lightbox === 'logo.png' && !W(s).pointerModal);
  await t.sleep(600);
  await snap('media-lightbox');
  // A press on the dark around the picture closes it.
  await T.mouse('click', 30, Math.round(W(await T.state()).windowW > 0 ? 600 : 600));
  await T.until('a press on the dark closes it', (s) => !W(s).media.lightbox);
  await act('media-open', logo.id);
  await T.until('View opens it again', (s) => W(s).media.lightbox === 'logo.png');
  await act('lb-close');
  await T.until('Close closes it', (s) => !W(s).media.lightbox);
  await act('media-copy', logo.id);
  await T.until('Copy path', (s) => s.toast === 'Path copied' && /\.png$/.test(W(s).copied));
  await act('media-delete', logo.id);
  await T.until('Delete asks first (media)', (s) => s.dialog?.danger);
  await T.ui('dialog-answer', 'true');
  await T.until('then the card goes', (s) => W(s).media.cards.length === 1 && s.toast === 'Deleted');
}
// F209: Media reloads each time its tab is shown (loadMedia).
{
  await T.ui('tab', 'files');
  const up = await T.api.http('POST', '/api/media/upload', { name: 'from-elsewhere.png', data: readFileSync(png).toString('base64'), project: 'demo' });
  t.check('a media file added elsewhere', up.body?.ok, up.body);
  await T.ui('tab', 'media');
  await T.until('shows in Media when its tab is shown', (s) => W(s).media.cards.some((c) => c.name === 'from-elsewhere.png'), 4000);
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
// The screen derives and shapes a line once per change (F46/F50): the
// wheel derives only the lines that came into view, a short echo only the
// lines it wrote, never the whole screen again.
let d0 = TS(await T.state())?.derived || 0;
const rowsOn = TS(await T.state())?.rows || 0;
await act('term-scroll', key, '30');
s = await T.until('the wheel goes back up', (s) => TS(s)?.back === 30);
await t.sleep(300);
let d1 = TS(await T.state())?.derived || 0;
t.check('scrolling back derives only the lines that came into view', d1 - d0 >= 1 && d1 - d0 <= Math.min(30, rowsOn) + 2, { derived: d1 - d0, rows: rowsOn });
await act('term-scroll', key, '-30');
await T.until('and down again', (s) => TS(s)?.back === 0);
await t.sleep(300);
// The screen is drawn a line at a time (F205): while the shell prints, the
// lines that only moved up are replayed where they now are and only the
// lines written render (each moved line renders once more when it stops:
// GPUI's settle), not every line on every frame.
{
  await act('term-send', key, "sleep 0.5; for ((i=1;i<=800;i++)); do printf 'tick %d\\n' $i; sleep 0.003; done");
  await t.sleep(300);
  const b = await T.state(), from = T.frames.mark();
  const e = await T.until('a slow print streams', (s) => TS(s)?.screen.some((l) => l.trim() === 'tick 800'), 60000);
  await t.sleep(400);
  const a = await T.state(), frames = T.frames.since(from).frames;
  const R = (s, k) => s?.renders?.[k] || 0;
  const lines = R(a, 'term-line') - R(b, 'term-line'), screens = R(a, 'term-screen') - R(b, 'term-screen');
  console.log(`  printing 800 lines: ${screens} screen renders, ${lines} line renders, ${frames} frames, ${rowsOn} rows`);
  t.check('…and renders only the lines written, not every line a frame (F205)', !!e && screens > 10 && lines < screens * rowsOn * 0.5 + rowsOn * 2, { lines, screens, frames, rows: rowsOn });
}
d0 = TS(await T.state())?.derived || 0;
await act('term-send', key, 'echo one-line');
await T.until('a short echo shows', (s) => TS(s)?.screen.some((l) => l.trim() === 'one-line'));
await t.sleep(500);
d1 = TS(await T.state())?.derived || 0;
t.check('a short echo derives only the lines it wrote (not the screen)', d1 - d0 >= 1 && d1 - d0 <= 8 && d1 - d0 < rowsOn, { derived: d1 - d0, rows: rowsOn });
// The terminal's words can be found by Cru's highlight (F202): a mark over each line.
{
  const DK = (s) => s.areas?.desktop || {};
  await T.ui('desktop:cru-ui-go', JSON.stringify({ id: 'tl1', tab: 'workbench', highlight: 'one-line', note: 'This line' }));
  const p = await T.until('Cru\'s highlight finds words in the terminal (F202)', (p) => DK(p).cru?.lastAck?.id === 'tl1', 8000);
  const ack = DK(p).cru?.lastAck, at = DK(p).cruSpot?.ring?.at, leaf = p?.leaves?.find((l) => l.key === key);
  const inLeaf = at && leaf && at.x >= leaf.x - 1 && at.y >= leaf.y - 1 && at.x + at.w <= leaf.x + leaf.w + 1 && at.y + at.h <= leaf.y + leaf.h + 1;
  t.check('…found, rung on the terminal\'s line', ack?.found === true && inLeaf && at.h < 40, { ack, at, leaf });
  if (at) { await t.sleep(400); await T.mouse('click', at.x + 4, at.y + at.h / 2); await T.until('…a press: the ring goes', (p) => !DK(p).cruSpot?.ring, 4000); }
}
await act('term-send', key, 'sleep 30');
await t.sleep(400);
await act('term-tool', key, 'ctrl+c');
await act('term-send', key, 'echo after-ctrl-c');
await T.until('the tools\' Ctrl+C stops it', (s) => TS(s)?.screen.some((l) => l.trim() === 'after-ctrl-c'), 15000);
await act('term-tools', key);
s = await T.until('the key tools open, and the bar grows to hold them (the box stays under them)', (s) => TS(s)?.tools && TS(s).inputH > 150);
await snap('terminal-tools');
await act('term-tools', key);
await T.until('closed, the bar is its own height again', (s) => !TS(s)?.tools && TS(s).inputH < 100);

// The newline key (Settings › Terminal): Shift+Enter by default, Ctrl+Enter
// when chosen there; either sends ESC CR (cat -v shows it as ^[).
await act('term-send', key, 'cat -v');
await t.sleep(500);
await act('term-focus', key);
await act('term-key', key, '\n', 'shift');
await T.until('Shift+Enter makes a new line (ESC CR)', (s) => TS(s)?.screen.some((l) => l.trim() === '^['));
// (Chosen as a person does, in Operations' Settings: the client's saved settings, F147.)
await T.ui('ops:settings-do', 'set-newline-key', '1');
await act('term-key', key, 'x', '');
await act('term-key', key, '\n', 'ctrl');
await T.until('with Ctrl+Enter chosen, Ctrl+Enter does', (s) => TS(s)?.screen.some((l) => l.trim() === 'x^['));
await act('term-key', key, 'y', '');
await act('term-key', key, '\n', 'shift');
await T.until('and Shift+Enter is a plain Enter then', (s) => TS(s)?.screen.some((l) => l.trim() === 'y'));
await T.ui('ops:settings-do', 'set-newline-key', '0');
await act('term-tool', key, 'ctrl+c');

// A picture pasted on the screen is saved in the project, its path typed in.
await act('term-paste-image', key, png);
await T.until('a pasted picture: "Screenshot saved", its path typed in', (s) => /^Screenshot saved: /.test(s.toast) && TS(s)?.screen.some((l) => /\.png/.test(l)), 15000);
await act('term-tool', key, 'ctrl+c');
await act('term-input', key, '');
await act('term-zoom', key, '1');
await T.until('A+ makes its text larger', (s) => TS(s)?.zoom === 1);
await act('term-zoom', key, '0');
await T.until('↺ resets it', (s) => TS(s)?.zoom === 0);

// Idle (the terminal not focused: no blink), nothing animating: no frames.
{
  await act('term-bottom', key);
  await act('term-blur', key);
  await T.ui('tab', 'files');
  await T.ui('tab', 'workbench');
  await T.ui('toasts-clear');
  // (The box's caret, which had the keys, rests 10 s after the last key.)
  await T.settle(1500, 15000);
  const from = T.frames.mark();
  const r0 = (await T.state()).renders || {};
  await t.sleep(5000);
  const idle = T.frames.since(from);
  const r1 = (await T.state()).renders || {};
  const drew = Object.fromEntries(Object.keys(r1).filter((k) => r1[k] !== (r0[k] || 0)).map((k) => [k, r1[k] - (r0[k] || 0)]));
  t.check('idle: no frames at all for 5 s', idle.frames === 0, { idle, drew });
}

// A box that had the keys stops its caret when it loses them (F109): the
// box, then the screen (whose own cursor blinks, redrawing it alone); the
// bar is not drawn again. Then nothing has the keys: no frames.
{
  await act('term-input-focus', key);
  await t.sleep(1200);
  await act('term-focus', key);
  await t.sleep(600);
  const r0 = (await T.state()).renders || {};
  await t.sleep(3000);
  const r1 = (await T.state()).renders || {};
  t.check('a blurred box stops its caret: the input bar is not drawn again', (r1['term-input'] || 0) === (r0['term-input'] || 0), { before: r0['term-input'], after: r1['term-input'] });
  await act('term-blur', key);
  await T.settle(1500, 6000);
  const from = T.frames.mark();
  const q0 = (await T.state()).renders || {};
  await t.sleep(4000);
  const q1 = (await T.state()).renders || {};
  const drew = Object.fromEntries(Object.keys(q1).filter((k) => q1[k] !== (q0[k] || 0)).map((k) => [k, q1[k] - (q0[k] || 0)]));
  t.check('…and with the keys nowhere: no frames for 4 s', T.frames.since(from).frames === 0, { idle: T.frames.since(from), drew });
}

// The input bar's box.
// In the box: it uploads, its path goes in.
await act('term-input-paste-image', key, png);
await T.until('a picture pasted in the box uploads, its path goes in', (s) => /crundi_attachments\/.*\.png $/.test(TS(s)?.text || '') && /^Attached/.test(s.toast), 20000);
await act('term-input', key, '');
// Files from the system dropped on the terminal: their paths into the box.
await act('term-drop-files', key, join(scratch, 'shot.png'));
await T.until('a file dropped on the terminal: its path in the box, "File added"', (s) => (TS(s)?.text || '').includes(join(scratch, 'shot.png')) && s.toast === 'File added');
await act('term-input', key, '');
await act('pick-next', join(scratch, 'shot.png'));
await act('term-attach', key);
await T.until('attach uploads and puts the path in the input', (s) => /crundi_attachments\/.*shot.*\.png $/.test(TS(s)?.text || '') && /^Attached/.test(s.toast), 20000);
await act('term-input', key, '');

// The terminal renamed: the head shows the new title (from the event
// stream's list, as every client sees it).
await act('term-rename', key, 'My shell');
await T.until('the terminal renames, its head with it', (s) => s.heads?.[key] === 'My shell', 15000);

// An empty pane's chips: the panel goes into the leaf whose chip was
// clicked (F35), a second of a kind is refused. Two empty leaves first
// (split right, then down), and the chip of the second one.
await T.ui('workbench:pane-split', key, 'false');
await T.until('a split gives an empty leaf', (s) => s.leaves?.filter((l) => !l.key).length === 1);
await T.ui('workbench:pane-split', key, 'true');
s = await T.until('a second split, a second empty leaf', (s) => s.leaves?.filter((l) => !l.key).length === 2);
const empties = (s?.leaves || []).filter((l) => !l.key);
if (empties.length === 2) {
  const [first, second] = empties;
  await T.ui('workbench:leaf-chip', String(second.id), 'git');
  await T.until('the Git chip fills the leaf it was clicked in (not the first empty one)', (s) => s.leaves.find((l) => l.id === second.id)?.key === 'panel:git' && !s.leaves.find((l) => l.id === first.id)?.key);
  await T.ui('workbench:leaf-chip', String(first.id), 'git');
  await T.until('a second Git is refused, its leaf stays empty', (s) => s.toast === 'Git is already in the workbench' && s.panes.filter((k) => k === 'panel:git').length === 1 && !s.leaves.find((l) => l.id === first.id)?.key);
  await T.ui('workbench:leaf-chip', String(first.id), 'files');
  await T.until('the Files chip fills its leaf', (s) => s.leaves.find((l) => l.id === first.id)?.key === 'panel:files');
}
await act('chip', 'media');
await T.until('Git, Files and Media in panes', (s) => s.panes?.includes('panel:git') && s.panes.includes('panel:files') && s.panes.includes('panel:media'));
await T.ui('toasts-clear');
await t.sleep(800);
await snap('workbench-panels');

// F220: showing the Workbench tab again reloads each panel in its layout
// (the web's switchTab('workbench') -> embedWbPanels -> wbLoad).
{
  await act('files-nav', P);
  await T.until('the Files pane lists the project', (s) => W(s).files.path === P && W(s).files.rows.some((r) => r.name === 'src'));
  await T.ui('tab', 'kanban');
  writeFileSync(join(P, 'made-on-kanban.txt'), 'new\n');
  const up = await T.api.http('POST', '/api/media/upload', { name: 'while-on-kanban.png', data: readFileSync(png).toString('base64'), project: 'demo' });
  t.check('a media file added while away', up.body?.ok, up.body);
  await t.sleep(300);
  await T.ui('tab', 'workbench');
  await T.until('back on the Workbench, the Files pane shows a file made meanwhile', (s) => W(s).files.rows.some((r) => r.name === 'made-on-kanban.txt'), 2500);
  await T.until('…the Git pane lists it as a change', (s) => [...W(s).git.changes, ...W(s).git.staged].some((f) => /made-on-kanban\.txt$/.test(f.file)), 2500);
  await T.until('…and the Media pane its new card', (s) => W(s).media.cards.some((c) => c.name === 'while-on-kanban.png'), 2500);
  rmSync(join(P, 'made-on-kanban.txt'), { force: true });
}

// A Files row dragged onto the terminal (with the pointer): its path goes
// to the program, "Sent to terminal".
{
  await act('files-nav', join(P, 'docs'));
  const row = await T.until('the Files pane lists docs', (s) => W(s).files.rows.find((r) => r.name === 'notes.txt')?.more);
  const b = W(row || {})?.files?.rows.find((r) => r.name === 'notes.txt')?.more;
  const leaf = row?.leaves?.find((l) => l.key === key);
  const fl = row?.leaves?.find((l) => l.key === 'panel:files');
  if (b && leaf && fl) {
    await act('term-send', key, 'cat > /dev/null');
    await t.sleep(400);
    await T.ui('toasts-clear');
    await T.mouse('drag', fl.x + 60, b[1] + b[3] / 2, { toX: leaf.x + leaf.w / 2, toY: leaf.y + leaf.h / 3, steps: 12 });
    const got = await T.until('dropped on the terminal: its path is typed there, "Sent to terminal"', (s) => s.toast === 'Sent to terminal' && (TS(s)?.screen || []).join('').includes('[File Path: ' + join(P, 'docs/notes.txt') + ']'), 10000);
    if (!got) { const z = await T.state(); console.log('  drag from', fl.x + 60, b[1], 'to', leaf, 'toast', z.toast, 'screen', TS(z)?.screen.slice(-4)); }
    await act('term-tool', key, 'ctrl+c');
  } else t.check('the row and the terminal are on screen to drag between', false, { b, leaf, fl });
}

// The head's × (F36): one click arms "Sure?", the second closes the
// terminal on the server, and its pane goes.
const termId = key.slice('term:'.length);
await T.ui('workbench:pane-close', key);
await T.until('the first × arms "Sure?"', (s) => s.leaves?.find((l) => l.key === key)?.armed);
await T.ui('workbench:pane-close', key);
await T.until('the second closes the terminal and its pane', (s) => !W(s).terms.some((x) => x.key === key) && !s.panes.includes(key), 30000);
const list = await T.api.http('GET', '/api/terminals');
t.check('the server has no terminal left open', !(list.body?.terminals || list.body || []).some?.((x) => x.id === termId && x.status !== 'exited'), list.body);

// A Git row, a Media card and a Files row dragged onto a chat (with the
// pointer, F140): each goes in at the box's caret as the web's labelled
// reference, "Added to chat". Each from its pane alone beside the chat
// (more panes than two would overflow the window at their minimum width).
{
  writeFileSync(join(P, 'docs/dropped.txt'), 'to drag\n');
  await act('git-refresh');
  await T.ui('new-chat');
  s = await T.until('a chat pane', (s) => s.composer?.state === 'idle' && s.leaves?.some((l) => l.key?.startsWith('live:')), 30000);
  const chatLeaf = (s) => s.leaves.find((l) => l.key?.startsWith('live:'));
  const onScreen = (s, l) => l && l.x >= 0 && l.x + l.w <= (s.window?.w || 1400) + 1;
  // Only `panel:<kind>` and the chat in the workbench, both on screen.
  const beside = async (kind) => {
    for (const k of (await T.state()).panes || []) if (k.startsWith('panel:') && k !== `panel:${kind}`) await T.ui('workbench:pane-close', k);
    if (!(await T.state()).panes.includes(`panel:${kind}`)) await T.ui('add-pane', kind);
    await T.ui('add-pane', 'fit');
    return T.until(`${kind} and the chat side by side`, (s) => s.panes?.length === 2 && onScreen(s, chatLeaf(s)) && onScreen(s, s.leaves.find((l) => l.key === `panel:${kind}`)));
  };
  // A box drawn where its pane now is (boxes are kept as last painted).
  const inLeaf = (s, key, b) => { const l = s.leaves?.find((x) => x.key === key); return !!(l && b && b[0] >= l.x && b[0] + 20 < l.x + l.w && b[1] > l.y && b[1] < l.y + l.h); };
  const dropOnChat = async (what, x, y, want) => {
    // (A press in the chat's log focuses it: its box is the one emptied.)
    const c0 = chatLeaf(await T.state());
    await T.mouse('click', c0.x + c0.w / 2, c0.y + 120);
    await T.until('the chat focused', (q) => q.focusedPane === c0.key && q.composer);
    await T.ui('composer-text', '');
    await T.ui('toasts-clear');
    const cl = chatLeaf(await T.state());
    await T.mouse('move', x, y);
    await t.sleep(200);
    await T.mouse('drag', x, y, { toX: cl.x + cl.w / 2, toY: cl.y + cl.h - 40, steps: 12 });
    const got = await T.until(`${what} dropped on the chat: "${want}" in its box, "Added to chat"`, (q) => q.toast === 'Added to chat' && (q.composer?.text || '').includes(want), 10000);
    if (!got) { const z = await T.state(); console.log('  drag from', x, y, 'to', cl, 'toast', z.toast, 'box', JSON.stringify(z.composer?.text)); }
  };
  if (s) {
    // A Git row: its path from the project's root.
    await beside('git');
    const gitBox = (s) => inLeaf(s, 'panel:git', W(s).git.changes?.find((f) => f.file === 'docs/dropped.txt')?.box);
    await T.until('Git lists docs/dropped.txt on screen', gitBox);
    await t.sleep(300);
    s = await T.until('…where its pane now is', gitBox);
    const g = s && W(s).git.changes.find((f) => f.file === 'docs/dropped.txt').box;
    if (g) await dropOnChat('a Git row', g[0] + 60, g[1] + g[3] / 2, '[File Path: ' + join(P, 'docs/dropped.txt') + ']');
    // A Media card: its path on the server.
    await beside('media');
    const card = (s) => W(s).media.cards?.find((c) => inLeaf(s, 'panel:media', c.box));
    await T.until('a Media card on screen', card);
    await t.sleep(300);
    s = await T.until('…where its pane now is', card);
    const mc = s && card(s);
    if (mc) await dropOnChat('a Media card', mc.box[0] + mc.box[2] / 2, mc.box[1] + 40, '[Media Path: ');
    // A Files row.
    await beside('files');
    await act('files-nav', join(P, 'docs'));
    // (Its row where the pane now is: drawn since the panes moved.)
    const inFiles = (s) => inLeaf(s, 'panel:files', W(s).files.rows.find((r) => r.name === 'notes.txt')?.more);
    await T.until('the Files pane lists docs', inFiles);
    await t.sleep(300);
    s = await T.until('…its rows settled', inFiles);
    const fl = s?.leaves.find((l) => l.key === 'panel:files');
    const b = s && W(s).files.rows.find((r) => r.name === 'notes.txt').more;
    if (b && fl) await dropOnChat('a Files row', fl.x + 60, b[1] + b[3] / 2, '[File Path: ' + join(P, 'docs/notes.txt') + ']');
    await T.ui('composer-text', '');
  }
  rmSync(join(P, 'docs/dropped.txt'), { force: true });
}

rmSync(scratch, { recursive: true, force: true });
await T.end();
