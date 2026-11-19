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
import { randomUUID } from 'node:crypto';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
if (args.includes('--version')) { console.log('0.0.0 (Crundi test stub)'); process.exit(0); }
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
  const turn = async (text) => {
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
    if (m.type === 'control_request') {
      const sub = m.request?.subtype;
      // generate_session_title: Claude Code's own title for the chat (the server's first naming).
      const response = sub === 'initialize' ? { commands: [], models: [], account: {} } : sub === 'generate_session_title' ? { title: titleFor(m.request.description) }
        // side_question: the retitle's "a different job?" (a placeholder title is).
        : sub === 'side_question' ? { response: /titled "chat( \(new\))?"/i.test(m.request.question || '') ? 'DIFFERENT' : 'SAME' } : {};
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
