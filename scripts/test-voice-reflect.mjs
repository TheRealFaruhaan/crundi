#!/usr/bin/env node
/**
 * test-voice-reflect.mjs — Cru learning its owner (reflect.js): size limits on
 * persona.md and owner.md, the reflection at compaction, and routines.
 * Run: node scripts/test-voice-reflect.mjs
 */
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
process.env.DATA_DIR = mkdtempSync(join(tmpdir(), 'cru-reflect-'));
const { capCheck, parseReflection, reflectionAsk, selfNote, LIMITS } = await import('../src/voice/reflect.js');
const { createNotebook } = await import('../src/voice/notebook.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const P = '/memories/persona.md', max = LIMITS[P];
ok(capCheck(P, '', 'x'.repeat(max)) === '', 'up to the limit is fine');
ok(!!capCheck(P, '', 'x'.repeat(max + 1)), 'over the limit is refused');
ok(capCheck(P, 'x'.repeat(max + 500), 'x'.repeat(max + 100)) === '', 'a file over the limit may shrink');
ok(capCheck('/memories/notes.md', '', 'x'.repeat(10000)) === '', 'other files have no limit');

const nb = createNotebook(join(process.env.DATA_DIR, 'nb'));
let threw = '';
try { nb.tool.run({ command: 'create', path: P, file_text: 'y'.repeat(max + 10) }); } catch (e) { threw = e.message; }
ok(/limited to/.test(threw), 'Cru\'s memory tool keeps persona.md within its limit');
nb.save(P, 'z'.repeat(max + 10));
ok(nb.list().find((f) => f.path === P).text.length === max + 10, 'the owner\'s own edit is his');

const r = parseReflection('<summary>We fixed the build.</summary>\n<persona>- Brief.</persona>');
ok(r.summary === 'We fixed the build.' && r.persona === '- Brief.' && r.owner === null, 'a reflection splits into its parts');
ok(parseReflection('Just a summary.').summary === 'Just a summary.' && parseReflection('Just a summary.').persona === null, 'a plain answer is all summary');
ok(/<owner>/.test(reflectionAsk({ persona: 'a', owner: '' })) && /\(empty\)/.test(reflectionAsk({ persona: 'a', owner: '' })), 'the ask shows both files');

const note = selfNote({ persona: '- Brief.', owner: '- Starts the day with a morning check.', routines: [{ path: '/memories/routines/morning-check.md', text: '# When he says "morning check"\n1. workbench_overview' }] });
ok(/persona\.md/.test(note) && /owner\.md/.test(note) && /morning-check: When he says "morning check"/.test(note), 'the opening note carries persona, owner and routines');

// The API brain's compaction asks for the reflection and hands back what it learnt.
const settings = await import('../src/voice/settings.js');
settings.updateSettings({ anthropicKey: 'test-key-not-real' });
const { createVoiceAgent } = await import('../src/voice/agent.js');
let learnt = null, asked = '';
const agent = createVoiceAgent({
  tools: [],
  client: () => ({ messages: { create: async (req) => { asked = req.messages[0].content; return { content: [{ type: 'text', text: '<summary>Did things.</summary><owner>- Likes mornings.</owner>' }], usage: {} }; } } }),
  selfFiles: () => ({ persona: '- Brief.', owner: '' }),
  onReflected: (x) => { learnt = x; },
});
agent.history.push({ role: 'user', content: 'hello' }, { role: 'assistant', content: [{ type: 'text', text: 'Hi.' }] });
const c = await agent.compact({ force: true });
ok(c.ok && c.summary === 'Did things.', 'compaction keeps the summary part');
ok(learnt && learnt.owner === '- Likes mornings.' && !learnt.persona, 'and hands the rewritten owner.md on');
ok(/persona\.md now:/.test(asked), 'the request showed Cru its current files');

console.log(failed ? `\n${failed} FAILED` : '\nAll reflection checks passed');
process.exit(failed ? 1 : 0);
