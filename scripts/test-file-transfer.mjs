#!/usr/bin/env node
// Moving and copying in the Files tab: nothing overwritten, no folder inside
// itself, and an honest account of what happened.
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { transfer, freeName, listableDir } from '../src/file-transfer.js';

let failed = 0;
const ok = (cond, name, detail = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`); if (!cond) failed++; };
const t = mkdtempSync(join(tmpdir(), 'crundi-xfer-'));
const p = (...a) => join(t, ...a);
mkdirSync(p('a/sub'), { recursive: true }); mkdirSync(p('b'));
writeFileSync(p('a/one.txt'), 'one'); writeFileSync(p('a/sub/deep.txt'), 'deep'); writeFileSync(p('b/one.txt'), 'other one'); writeFileSync(p('.env'), 'x');

ok(freeName(p('b'), 'one.txt') === 'one (2).txt' && freeName(p('b'), 'new.txt') === 'new.txt', 'a taken name gets a number, a free one is kept');
ok(freeName(t, '.env') === '.env (2)', 'a dotfile is numbered after its whole name');

let r = transfer({ sources: [p('a/one.txt')], destDir: p('b'), op: 'copy' });
ok(r.ok && r.done[0].name === 'one (2).txt' && r.done[0].renamed && readFileSync(p('b/one.txt'), 'utf8') === 'other one' && readFileSync(p('b/one (2).txt'), 'utf8') === 'one' && existsSync(p('a/one.txt')),
  'copy onto a taken name: the file there is untouched, the copy is numbered, the original stays', JSON.stringify(r));

r = transfer({ sources: [p('a/one.txt')], destDir: p('a'), op: 'copy' });
ok(r.ok && existsSync(p('a/one (2).txt')), 'copy into its own folder makes a numbered copy');

r = transfer({ sources: [p('a/one.txt')], destDir: p('a'), op: 'move' });
ok(!r.ok && /Already/.test(r.error), 'moving something to where it already is does nothing, and says so');

r = transfer({ sources: [p('a/sub')], destDir: p('b'), op: 'move' });
ok(r.ok && !existsSync(p('a/sub')) && readFileSync(p('b/sub/deep.txt'), 'utf8') === 'deep', 'a folder moves with what is in it');

r = transfer({ sources: [p('b')], destDir: p('b/sub'), op: 'move' });
ok(!r.ok && /into itself/.test(r.error) && existsSync(p('b/sub/deep.txt')), 'a folder cannot be moved inside itself');
r = transfer({ sources: [p('b')], destDir: p('b'), op: 'copy' });
ok(!r.ok && /into itself/.test(r.error), 'nor copied into itself');

r = transfer({ sources: [p('a')], destDir: p('b'), op: 'move', protect: [p('a')] });
ok(!r.ok && existsSync(p('a')), 'a protected folder (the project root) is not moved');
r = transfer({ sources: [p('a')], destDir: p('b'), op: 'copy', protect: [p('a')] });
ok(r.ok && existsSync(p('b/a/one.txt')) && existsSync(p('a/one.txt')), 'but it can be copied');

r = transfer({ sources: [p('a/one.txt'), p('a/gone.txt')], destDir: p('b/sub'), op: 'move' });
ok(r.ok && r.done.length === 1 && r.failed.length === 1 && /No longer/.test(r.failed[0].error), 'one missing item does not stop the others, and is reported');

ok(!transfer({ sources: [p('.env')], destDir: p('nowhere'), op: 'copy' }).ok && !transfer({ sources: [p('.env')], destDir: p('.env'), op: 'copy' }).ok, 'the destination must be a folder that exists');
ok(!transfer({ sources: [], destDir: p('b'), op: 'copy' }).ok && !transfer({ sources: [p('.env')], destDir: p('b'), op: 'delete' }).ok, 'nothing to do, or an unknown operation, is refused');

// ─── the folder to show when the one asked for is gone ───
mkdirSync(p('proj/src'), { recursive: true }); writeFileSync(p('proj/file.txt'), 'x');
ok(listableDir(p('proj/src'), p('proj'), false) === p('proj/src'), 'a folder that exists is shown as asked');
ok(listableDir(p('proj/gone/deeper'), p('proj'), false) === p('proj'), 'a deleted folder falls back to the project folder');
ok(listableDir(p('proj/file.txt'), p('proj'), false) === p('proj'), 'so does a path that is a file, not a folder');
ok(listableDir(p('lost/inner/x'), p('lost/inner'), false) === t, 'a missing project folder falls back to the nearest folder above it');
ok(listableDir(p('proj/gone'), p('proj'), true) === p('proj'), 'a collaborator falls back to their own root');
ok(listableDir(p('lost/inner/x'), p('lost/inner'), true) === null, 'a collaborator whose root is missing gets nothing, never a folder above it');
ok(listableDir('', p('proj'), true) === p('proj'), 'no folder named means the root');

try { rmSync(t, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll file-transfer checks passed.');
process.exit(failed ? 1 : 0);
