#!/usr/bin/env node
/**
 * test-voice-attach.mjs — files for Cru: stored, opened (picture, text, PDF),
 * listed, passed on to a chat, and the turn's lines.
 * Run: node scripts/test-voice-attach.mjs
 */
import { mkdtempSync, readFileSync, existsSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const A = await import('../src/voice/attachments.js');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const dir = mkdtempSync(join(tmpdir(), 'cru-att-'));
const store = A.createAttachmentStore({ dir: join(dir, 'att') });
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64');
const pic = store.add({ name: '../../etc/photo one.png', mime: 'image/png', data: png, preview: png });
ok(pic.id.startsWith('att_') && pic.name === 'photo one.png' && pic.kind === 'image', 'a picture is kept, its name made safe');
const txt = store.add({ name: 'notes.md', mime: 'text/markdown', data: Buffer.from('# Hello\nworld') });
ok(txt.kind === 'text', 'text is recognised');
ok(A.kindOf('application/pdf', 'x.pdf') === 'pdf' && A.kindOf('video/mp4', 'a.mp4') === 'video' && A.kindOf('', 'a.kt') === 'text', 'kinds by type and name');
let threw = false; try { store.add({ name: 'e', data: Buffer.alloc(0) }); } catch { threw = true; }
ok(threw, 'an empty file is refused');
const [open, list] = A.attachmentToolDefs(store);
let r = await open.run({ id: pic.id });
ok(Array.isArray(r.content) && r.content[0].type === 'image' && r.content[0].mimeType === 'image/jpeg', 'open_attachment shows a picture (its preview)');
r = await open.run({ id: txt.id });
ok(r.ok && r.text.includes('world'), 'open_attachment reads text');
r = await open.run({ id: 'att_000000000000' });
ok(r.ok === false, 'an unknown id is refused');
r = await open.run({ id: '../../etc' });
ok(r.ok === false, 'a path is not an id');
ok(list.run().attachments.length === 2, 'list_attachments lists them');
ok(/The owner attached: "notes.md" \(text/.test(A.attachmentLine(txt)) && A.attachmentLine(txt).includes(txt.id), 'the turn line names it and its id');
const proj = join(dir, 'proj'); mkdirSync(proj);
const lines = A.passOn(store, [txt.id], proj, (p) => { const d = join(p, 'crundi_attachments'); mkdirSync(d, { recursive: true }); return d; });
const path = lines[0].match(/^\[File Path: (.*)\]$/)[1];
ok(existsSync(path) && readFileSync(path, 'utf-8').includes('world'), 'passed on: copied into the project, as a [File Path: …] line');
threw = false; try { A.passOn(store, ['att_ffffffffffff'], proj, () => proj); } catch { threw = true; }
ok(threw, 'passing on an unknown id fails');
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nAll attachment checks passed');
