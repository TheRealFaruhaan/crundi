#!/usr/bin/env node
/**
 * test-notes-store.mjs — src/notes-store.js.
 *
 * The page HTML is drawn back with innerHTML, so sanitising is the part that
 * matters most: nothing but a short list of inline tags, and only http(s) /
 * mailto links, may survive whatever a client sends. Also: create, save,
 * version conflicts, trash / restore / purge, table shapes, previews.
 *
 * Run: node scripts/test-notes-store.mjs
 */

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'notes-test-'));
process.env.DATA_DIR = dir;
const ns = await import('../src/notes-store.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── sanitising ───
const S = ns.sanitizeInline;
ok(S('<b>bold</b> <i>it</i> <u>u</u> <s>s</s> <code>c</code>') === '<strong>bold</strong> <em>it</em> <u>u</u> <s>s</s> <code>c</code>', 'allowed inline tags survive (b/i normalised to strong/em)');
ok(S('a<br>b<br/>c') === 'a<br>b<br>c', 'line breaks survive');
ok(!/script|alert/.test(S('x<script>alert(1)</script>y')), 'a script element and its content are removed');
ok(!/onerror|onclick/i.test(S('<img src=x onerror=alert(1)><b onclick="x()">b</b>')), 'event-handler attributes never survive');
ok(!/img|iframe/i.test(S('<img src=x><iframe src="https://e.com"></iframe>')), 'images and iframes are dropped');
ok(S('<a href="https://e.com/a?b=1">l</a>') === '<a href="https://e.com/a?b=1" target="_blank" rel="noopener noreferrer">l</a>', 'an https link keeps its href and opens safely');
ok(!/javascript/i.test(S('<a href="javascript:alert(1)">x</a>')), 'a javascript: link loses its href');
ok(!/data:/i.test(S('<a href=\'data:text/html,hi\'>x</a>')), 'a data: link loses its href');
ok(/mailto:a@b\.c/.test(S('<a href="mailto:a@b.c">m</a>')), 'mailto links are kept');
ok(!/style|class/i.test(S('<span style="color:red" class="x">t</span><b style="x">b</b>')), 'styles and classes are dropped');
ok(S('5 &lt; 6 &amp; 7') === '5 &lt; 6 &amp; 7', 'escaped text is left as it is');

// ─── pages ───
const c = ns.createPage('proj', { title: 'First page', blocks: [
  { type: 'h1', text: 'Hello <script>x</script>' },
  { type: 'todo', text: 'do it', checked: 1 },
  { type: 'code', text: '<b>raw</b> stays text', lang: 'js;rm' },
  { type: 'table', rows: [['a', 'b', 'c'], ['d']] },
  { type: 'weird', text: 'becomes a paragraph' },
] });
ok(c.ok && /^[0-9a-f]{16}$/.test(c.page.id) && c.page.version === 1, 'create returns a page with an id and version 1');
const b = c.page.blocks;
ok(b[0].type === 'h1' && !/script/.test(b[0].text), 'block text is sanitised on save');
ok(b[1].checked === true, 'a todo keeps its checked state');
ok(b[2].text === '<b>raw</b> stays text' && b[2].lang === 'jsrm', 'code stays raw text; its language is reduced to safe characters');
ok(b[3].rows.length === 2 && b[3].rows.every(r => r.length === 3), 'table rows are padded to the widest row');
ok(b[4].type === 'p', 'an unknown block type becomes a paragraph');

const id = c.page.id;
const s1 = ns.savePage('proj', id, { title: 'Renamed', blocks: [{ type: 'p', text: 'v2' }], baseVersion: 1 });
ok(s1.ok && s1.version === 2, 'a save based on the current version is written (version 2)');
const s2 = ns.savePage('proj', id, { blocks: [{ type: 'p', text: 'stale' }], baseVersion: 1 });
ok(!s2.ok && s2.conflict && s2.page.blocks[0].text === 'v2', 'a save from a stale copy is refused with the current page');
ok(ns.getPage('proj', id).page.blocks[0].text === 'v2', 'and nothing was overwritten');

const empty = ns.savePage('proj', id, { blocks: [], baseVersion: 2 });
ok(empty.ok && ns.getPage('proj', id).page.blocks.length === 1, 'an empty page keeps one blank paragraph');

ns.createPage('proj', { title: 'Second', blocks: [{ type: 'p', text: 'second body' }] });
let list = ns.listPages('proj');
ok(list.length === 2 && list[0].title === 'Second', 'the list holds both pages, newest first');
ok(/second body/.test(list[0].preview), 'the list carries a text preview');

ok(ns.purgePage('proj', id).ok === false, 'a page cannot be deleted for good before it is in the trash');
ns.deletePage('proj', id);
ok(ns.listPages('proj').length === 1 && ns.listPages('proj', { includeDeleted: true }).length === 2, 'a deleted page leaves the list but stays in the trash');
ok(!ns.savePage('proj', id, { blocks: [] }).ok, 'a page in the trash cannot be edited');
ns.restorePage('proj', id);
ok(ns.listPages('proj').length === 2, 'restoring brings it back');
ns.deletePage('proj', id);
ok(ns.purgePage('proj', id).ok && !ns.getPage('proj', id).ok, 'deleting from the trash removes it for good');

ok(!ns.getPage('proj', '../../etc/passwd').ok && !ns.getPage('proj', 'zz').ok, 'only real page ids are read (no path tricks)');
ok(ns.listPages('other').length === 0, 'pages are per project');

rmSync(dir, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} notes check(s) failed`); process.exit(1); }
console.log('\nAll notes-store checks passed.');
