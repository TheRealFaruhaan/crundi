#!/usr/bin/env node
// A stand-in for the `claude` CLI on a test server (tools/stub-server.sh),
// so suites and the PC harness never spend the owner's Claude. It speaks
// the stream-json protocol src/claude-ui.js drives (--input-format and
// --output-format stream-json, --include-partial-messages,
// --replay-user-messages): it answers the host's control requests, says
// init, echoes each user message as taken, streams a reply as text deltas
// (about 600 characters a second, like a fast reply), then sends the
// assistant message and the turn's result. It answers the server's title
// requests too: generate_session_title, the retitle's side_question (a
// placeholder title is a different job), and `claude -p …` (the
// auto-titler's fallback) prints a short title.
//
// The reply depends on the prompt: the PC harness's seeds get what they ask
// for (300 numbers as words, long markdown with a table and code); anything
// else gets "Reply N" and a short markdown list.
//
// For the chat log's suite (test/chat-log.mjs), a few prompts make it do
// what a real turn does besides text: "use tools" thinks (streamed), calls
// three tools, sums them up in one line (tool_use_summary) and replies;
// "ask me" asks an AskUserQuestion and "need permission" a Bash permission
// (can_use_tool, waiting for the answer); "plan please" proposes a plan
// (ExitPlanMode); "suggest next" ends with a prompt_suggestion. A side
// question (side_question that is not the retitle's) is answered in markdown.
import { randomUUID } from 'node:crypto';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
// A version no release is newer than (F244): with 0.0.0 the server's Claude
// Code watch (src/webapp.js checkClaudeVersion, 60 s after it starts) posted
// "Claude Code … is available" to the Inbox, whose new unread count redrew
// the sidebar in whichever suite's idle window it fell.
if (args.includes('--version')) { console.log('999.0.0 (Crundi test stub)'); process.exit(0); }
/** A chat's title from what the person asked. */
const titleFor = (text) => {
  const p = String(text || '').toLowerCase();
  return p.includes('three hundred') || p.includes('one to forty') ? 'Number list in words' : p.includes('hash map') ? 'How hash maps work' : p.includes('cities') ? 'Largest cities' : p.includes('printing press') ? 'History of the printing press' : p.includes('single word') ? 'One word reply' : 'Test chat';
};
if (args.includes('-p')) {
  // The auto-titler: a title for the first message it is given.
  let input = '';
  process.stdin.on('data', (d) => { input += d; });
  process.stdin.on('end', () => process.stdout.write(titleFor(input) + '\n'));
} else {
  const session = randomUUID();
  const out = (m) => process.stdout.write(JSON.stringify(m) + '\n');
  out({ type: 'system', subtype: 'init', session_id: session, model: 'claude-stub', tools: [], slash_commands: [], mcp_servers: [], cwd: process.cwd() });
  let n = 0;
  const queue = [];
  let busy = false;
  const words = (k) => {
    const ones = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten', 'eleven', 'twelve', 'thirteen', 'fourteen', 'fifteen', 'sixteen', 'seventeen', 'eighteen', 'nineteen'];
    const tens = ['', '', 'twenty', 'thirty', 'forty', 'fifty', 'sixty', 'seventy', 'eighty', 'ninety'];
    const h = Math.floor(k / 100), r = k % 100;
    let s = h ? ones[h] + ' hundred' + (r ? ' and ' : '') : '';
    if (r >= 20) s += tens[Math.floor(r / 10)] + (r % 10 ? '-' + ones[r % 10] : ''); else if (r || !h) s += ones[r];
    return s;
  };
  const section = (i) => `## Part ${i}\n\nA hash map stores **values** by *keys*: it hashes the key, picks a bucket and keeps the entry there, so a lookup costs \`O(1)\` on average. See [the docs](https://doc.rust-lang.org/std/collections/struct.HashMap.html).\n\n- the hash spreads keys evenly\n- the load factor decides when to grow\n- growing rehashes every entry\n\n1. hash the key\n2. find the bucket\n3. compare the keys\n\n| Operation | Average | Worst |\n|---|---|---|\n| insert | O(1) | O(n) |\n| lookup | O(1) | O(n) |\n\n\`\`\`rust\nlet mut m = HashMap::new();\nm.insert("key", 42);\n\`\`\`\n\n> A good hash function is not premature.\n\n---\n\n`;
  const replyTo = (p) => {
    const t = p.toLowerCase();
    const word = /single word:\s*([a-z]+)/i.exec(p);
    if (word) return word[1];
    const upTo = /numbers one to (forty|twenty|ten)\b/i.exec(p);
    if (upTo) return Array.from({ length: { forty: 40, twenty: 20, ten: 10 }[upTo[1].toLowerCase()] }, (_, i) => `- ${words(i + 1)}`).join('\n');
    if (t.includes('three hundred')) return Array.from({ length: 300 }, (_, i) => `- ${words(i + 1)}`).join('\n');
    if (t.includes('hash map') || t.includes('printing press') || t.includes('article')) return Array.from({ length: 8 }, (_, i) => section(i + 1)).join('');
    if (t.includes('cities')) return Array.from({ length: 120 }, (_, i) => `- **City ${i + 1}**: one of the largest cities in the world, with a long history and a busy centre.`).join('\n');
    return `Reply ${n}\n\n- one\n- two\n- three`;
  };
  // can_use_tool requests waiting for the host's answer, by request id.
  const asked = new Map();
  const ask = (request) => new Promise((done) => { const id = randomUUID(); asked.set(id, done); out({ type: 'control_request', request_id: id, request }); });
  const say = async (full) => {
    const id = 'msg_' + randomUUID().replace(/-/g, '').slice(0, 20);
    out({ type: 'stream_event', session_id: session, event: { type: 'message_start', message: { id, role: 'assistant', model: 'claude-stub', content: [] } } });
    out({ type: 'stream_event', session_id: session, event: { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } } });
    for (let at = 0; at < full.length; at += 24) {
      out({ type: 'stream_event', session_id: session, event: { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: full.slice(at, at + 24) } } });
      await new Promise((r) => setTimeout(r, 40));
    }
    out({ type: 'stream_event', session_id: session, event: { type: 'content_block_stop', index: 0 } });
    out({ type: 'assistant', session_id: session, message: { id, type: 'message', role: 'assistant', model: 'claude-stub', content: [{ type: 'text', text: full }], stop_reason: 'end_turn', usage: { input_tokens: 10, output_tokens: 10 } } });
  };
  const done = (started, text) => out({ type: 'result', subtype: 'success', is_error: false, duration_ms: Date.now() - started, duration_api_ms: 1, num_turns: 1, result: text, session_id: session, total_cost_usd: 0.0123, usage: { input_tokens: 10, output_tokens: 10 } });
  const special = async (text) => {
    const t = text.toLowerCase();
    const started = Date.now();
    if (t.includes('use tools')) {
      // A thought, streamed.
      const mid = 'msg_' + randomUUID().replace(/-/g, '').slice(0, 20);
      out({ type: 'stream_event', session_id: session, event: { type: 'message_start', message: { id: mid, role: 'assistant', model: 'claude-stub', content: [] } } });
      out({ type: 'stream_event', session_id: session, event: { type: 'content_block_start', index: 0, content_block: { type: 'thinking', thinking: '' } } });
      const thought = 'The person wants the files listed. I will list the folder, read the readme and count the lines.';
      for (let at = 0; at < thought.length; at += 6) {
        out({ type: 'stream_event', session_id: session, event: { type: 'content_block_delta', index: 0, delta: { type: 'thinking_delta', thinking: thought.slice(at, at + 6) } } });
        await new Promise((r) => setTimeout(r, 90));
      }
      out({ type: 'stream_event', session_id: session, event: { type: 'content_block_stop', index: 0 } });
      const calls = [
        { type: 'tool_use', id: 'toolu_' + randomUUID().slice(0, 8), name: 'Bash', input: { command: 'ls -la', description: 'List the current directory' } },
        { type: 'tool_use', id: 'toolu_' + randomUUID().slice(0, 8), name: 'Read', input: { file_path: '/tmp/project/docs/README.md' } },
        { type: 'tool_use', id: 'toolu_' + randomUUID().slice(0, 8), name: 'Grep', input: { pattern: 'TODO', glob: '*.rs' } },
      ];
      out({ type: 'assistant', session_id: session, message: { id: mid, type: 'message', role: 'assistant', model: 'claude-stub', content: [{ type: 'thinking', thinking: thought }, ...calls], usage: { input_tokens: 10, output_tokens: 10 } } });
      for (const c of calls) {
        await new Promise((r) => setTimeout(r, 150));
        out({ type: 'user', session_id: session, message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: c.id, content: c.name === 'Bash' ? 'total 8\ndrwxr-xr-x  2 me me 4096 .\n-rw-r--r--  1 me me  120 README.md' : 'line one\nline two' }] } });
      }
      out({ type: 'tool_use_summary', summary: 'Listed current directory, read README.md, searched for TODO', preceding_tool_use_ids: calls.map((c) => c.id) });
      const reply = 'Done: the folder has **README.md**; see `src/main.rs` and https://example.com/docs for more.\n\n```sh\nls -la\n```';
      await say(reply);
      done(started, reply);
      return true;
    }
    if (t.includes('ask me') || t.includes('need permission') || t.includes('plan please')) {
      const tid = 'toolu_' + randomUUID().slice(0, 8);
      const request = t.includes('ask me')
        ? { subtype: 'can_use_tool', tool_name: 'AskUserQuestion', tool_use_id: tid, input: { questions: [{ header: 'Database', question: 'Which database should I use?', multiSelect: false, options: [{ label: 'Postgres', description: 'Relational, the safe choice' }, { label: 'SQLite', description: 'One file, no server' }] }] } }
        : t.includes('plan please')
          ? { subtype: 'can_use_tool', tool_name: 'ExitPlanMode', tool_use_id: tid, input: { plan: '## The plan\n\n1. Read the code\n2. Write the tests\n3. Fix what fails', planFilePath: '/tmp/plan.md' } }
          : { subtype: 'can_use_tool', tool_name: 'Bash', tool_use_id: tid, input: { command: 'rm -rf build', description: 'Clean the build folder' }, description: 'Clean the build folder', permission_suggestions: [{ type: 'addRules', rules: [{ toolName: 'Bash' }], behavior: 'allow', destination: 'session' }] };
      const answer = await ask(request);
      const reply = answer?.behavior === 'allow' ? 'Thanks, carrying on.' : 'Understood, I will not.';
      await say(reply);
      done(started, reply);
      return true;
    }
    if (t.includes('suggest next')) {
      await say('Here is a short answer.');
      done(started, 'Here is a short answer.');
      out({ type: 'prompt_suggestion', suggestion: 'Tell me more about it', uuid: randomUUID() });
      return true;
    }
    return false;
  };
  const turn = async (text) => {
    if (await special(text)) { n++; return; }
    n++;
    const full = replyTo(text);
    const id = 'msg_' + randomUUID().replace(/-/g, '').slice(0, 20);
    const started = Date.now();
    out({ type: 'stream_event', session_id: session, event: { type: 'message_start', message: { id, role: 'assistant', model: 'claude-stub', content: [] } } });
    out({ type: 'stream_event', session_id: session, event: { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } } });
    for (let at = 0; at < full.length; at += 24) {
      out({ type: 'stream_event', session_id: session, event: { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: full.slice(at, at + 24) } } });
      await new Promise((r) => setTimeout(r, 40));
    }
    out({ type: 'stream_event', session_id: session, event: { type: 'content_block_stop', index: 0 } });
    out({ type: 'assistant', session_id: session, message: { id, type: 'message', role: 'assistant', model: 'claude-stub', content: [{ type: 'text', text: full }], stop_reason: 'end_turn', usage: { input_tokens: 10, output_tokens: Math.ceil(full.length / 4) } } });
    out({ type: 'result', subtype: 'success', is_error: false, duration_ms: Date.now() - started, duration_api_ms: Date.now() - started, num_turns: 1, result: full, session_id: session, total_cost_usd: 0, usage: { input_tokens: 10, output_tokens: Math.ceil(full.length / 4) } });
  };
  const next = async () => {
    if (busy) return;
    busy = true;
    while (queue.length) await turn(queue.shift());
    busy = false;
  };
  createInterface({ input: process.stdin }).on('line', (line) => {
    let m; try { m = JSON.parse(line); } catch { return; }
    // The host's answer to a can_use_tool we asked.
    if (m.type === 'control_response' && asked.has(m.response?.request_id)) { const d = asked.get(m.response.request_id); asked.delete(m.response.request_id); d(m.response.response); return; }
    if (m.type === 'control_request') {
      const sub = m.request?.subtype;
      // generate_session_title: Claude Code's own title for the chat (the server's first naming).
      const response = sub === 'initialize' ? { commands: [], models: [], account: {} } : sub === 'generate_session_title' ? { title: titleFor(m.request.description) }
        // side_question: the retitle's "a different job?" (a placeholder title is).
        : sub === 'side_question' ? { response: /titled "chat( \(new\))?"/i.test(m.request.question || '') ? 'DIFFERENT' : /titled "/i.test(m.request.question || '') ? 'SAME' : 'Writing **design.md** now, then the tests.' } : {};
      out({ type: 'control_response', response: { subtype: 'success', request_id: m.request_id, response } });
      return;
    }
    if (m.type === 'user') {
      out({ ...m, session_id: session });
      const c = m.message?.content;
      const text = typeof c === 'string' ? c : (Array.isArray(c) ? c.filter((b) => b.type === 'text').map((b) => b.text).join('\n') : '');
      queue.push(text);
      next();
    }
  }).on('close', () => { const wait = () => (busy ? setTimeout(wait, 100) : process.exit(0)); wait(); });
}
