// Resuming a conversation from the session chooser must show what was already
// said. Crundi stores only the latest conversation per project, so anything
// else is rebuilt from Claude's transcript.
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'fs';
import { join, resolve } from 'path';
import { tmpdir } from 'os';
import { fileURLToPath } from 'url';

const here = fileURLToPath(new URL('.', import.meta.url));
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};

const home = mkdtempSync(join(tmpdir(), 'crundi-replay-'));
// os.homedir() reads HOME on Linux/macOS but USERPROFILE on Windows (the
// release build runs these checks on Windows), so set both.
process.env.HOME = home;
process.env.USERPROFILE = home;
const project = join(home, 'projects', 'demo');
mkdirSync(project, { recursive: true });
const dir = join(home, '.claude', 'projects', resolve(project).replace(/[^a-zA-Z0-9]/g, '-'));
mkdirSync(dir, { recursive: true });
const uuid = '11111111-2222-3333-4444-555555555555';

const recs = [
  { type: 'ai-title', aiTitle: 'x' },
  { type: 'user', message: { role: 'user', content: 'Build the thing' } },
  { type: 'user', isMeta: true, message: { role: 'user', content: 'meta noise' } },
  { type: 'user', message: { role: 'user', content: '<command-name>/model</command-name>' } },
  { type: 'assistant', message: { id: 'm1', content: [{ type: 'thinking', thinking: '', signature: 'sig' }] } },
  { type: 'assistant', message: { id: 'm1', content: [{ type: 'thinking', thinking: 'Plan it out' }] } },
  { type: 'assistant', message: { id: 'm1', content: [{ type: 'text', text: 'Running it' }] } },
  { type: 'assistant', message: { id: 'm1', content: [{ type: 'tool_use', id: 'tu1', name: 'Bash', input: { command: 'ls' } }] } },
  { type: 'assistant', message: { id: 'm1', content: [{ type: 'tool_use', id: 'tu1', name: 'Bash', input: { command: 'ls' } }] } },
  { type: 'assistant', isSidechain: true, message: { id: 's1', content: [{ type: 'text', text: 'subagent chatter' }] } },
  { type: 'user', message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: 'tu1', content: [{ type: 'text', text: 'a.txt' }] }] } },
  { type: 'assistant', message: { id: 'm2', content: [{ type: 'tool_use', id: 'tu2', name: 'Read', input: {} }] } },
  { type: 'system', subtype: 'compact_boundary' },
  { type: 'user', message: { role: 'user', content: [{ type: 'text', text: 'Commit' }] } },
  { type: 'assistant', message: { id: 'm3', content: [{ type: 'text', text: 'Done' }] } },
];
writeFileSync(join(dir, `${uuid}.jsonl`), recs.map(r => JSON.stringify(r)).join('\n') + '\n{broken json\n');

const { readTranscriptHistory } = await import('../src/claude-ui.js');
const d = readTranscriptHistory(project, uuid);
check('reads the transcript', !!d && d.uuid === uuid);
const kinds = d ? d.messages.map(m => m.kind + ':' + (m.text || m.name || '')) : [];
const want = ['user:Build the thing', 'thinking:Plan it out', 'assistant-text:Running it', 'tool:Bash', 'tool:Read',
  'notice:Conversation compacted', 'user:Commit', 'assistant-text:Done'];
check('entries in order, noise and sidechains skipped, tools deduped', JSON.stringify(kinds) === JSON.stringify(want), JSON.stringify(kinds));
const bash = d && d.messages.find(m => m.toolUseId === 'tu1');
check('tool result attached', !!bash && bash.status === 'done' && bash.result === 'a.txt' && bash.isError === false);
const read = d && d.messages.find(m => m.toolUseId === 'tu2');
check('unfinished tool is not left spinning', !!read && read.status === 'done' && read.result === null);
check('missing conversation returns null', readTranscriptHistory(project, '99999999-0000-0000-0000-000000000000') === null);
check('non-uuid id rejected', readTranscriptHistory(project, '../../etc/passwd') === null);
const capped = readTranscriptHistory(project, uuid, { maxMessages: 3 });
check('count cap keeps the newest', !!capped && capped.messages.length === 3 && capped.messages[2].text === 'Done');

const src = readFileSync(join(here, '..', 'src', 'claude-ui.js'), 'utf8');
const branch = src.slice(src.indexOf("if ((sessionMode === 'resume' || compacting) && resumeId)"), src.indexOf("else if (sessionMode === 'new')"));
check('explicit resume replays', /continueUuid = String\(resumeId\);/.test(branch) && !/if \(compacting\) continueUuid/.test(branch));
check('fork does not re-run replay against its new id', /s\.sessionId !== guessed && !s\.forking/.test(src));
check('replay falls back to the transcript', /readTranscriptHistory\(s\.cwd, uuid/.test(src));

rmSync(home, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nresume replay: all passed');
