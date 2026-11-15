#!/usr/bin/env node
// A stand-in for Deepgram, so a test server's voice routes run for real with no
// key and no network: /api/voice/listen relays the client's audio here, and
// /api/voice/speak streams its speech from here. Written for 2.24 (voice
// input, specs/voice-input.md); the spec's screenshots were taken against it.
//
//   node native-client/tools/deepgram-stub.mjs <port> [certDir]
//
// It serves HTTPS and WSS on 127.0.0.1:<port> with a self-signed certificate
// (made with openssl in certDir, default /tmp/deepgram-stub-<port>, if missing).
// Start the test server pointed at it (test-server.sh passes the environment on):
//
//   DEEPGRAM_HOST=127.0.0.1:<port> NODE_TLS_REJECT_UNAUTHORIZED=0 \
//     native-client/tools/test-server.sh 4810
//   POST /api/voice/settings {"deepgramKey":"stub"}            the mic shows
//   POST /api/voice/settings {"ttsVoice":"aura-2-draco-en"}    replies use /v1/speak here
//
// Control (HTTPS, JSON; curl -k):
//   POST /ctl/say   {"interim":"words"}         words so far (the client's 'interim')
//                   {"final":"words"}           a finished phrase (still 'interim' to the client)
//                   {"final":"words","end":true} ... then UtteranceEnd: the client gets 'utterance'
//                   ("scratch that" / "never mind" / "ignore that" in it: the client gets 'discard')
//   POST /ctl/send  {"msgs":[...]}              raw Deepgram messages to every open stream
//   POST /ctl/mode  {"mode":"refuse"|"ok"}      refuse: streams get 401 (the server says
//                                               "Deepgram refused the key. Check it in Settings › Voice assistant.")
//   GET  /ctl/stats  {mode, conns:[{url, frames, bytes, texts, open}]}: what each stream received
//                   (url carries encoding=linear16&sample_rate=16000; frames/bytes are the audio)
import https from 'node:https';
import { readFileSync, existsSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(join(here, '..', '..', 'package.json'));
const { WebSocketServer } = require('ws');
const PORT = Number(process.argv[2] || 4811);
const DIR = process.argv[3] || `/tmp/deepgram-stub-${PORT}`;
mkdirSync(DIR, { recursive: true });
if (!existsSync(join(DIR, 'key.pem'))) {
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', join(DIR, 'key.pem'),
    '-out', join(DIR, 'cert.pem'), '-days', '3', '-subj', '/CN=127.0.0.1'], { stdio: 'ignore' });
}
let mode = 'ok';
let t0 = 0;
const conns = [];
const srv = https.createServer({ key: readFileSync(join(DIR, 'key.pem')), cert: readFileSync(join(DIR, 'cert.pem')) }, async (req, res) => {
  const body = await new Promise((r) => { let b = ''; req.on('data', (c) => (b += c)); req.on('end', () => r(b)); });
  const j = (o, s = 200) => { res.writeHead(s, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(o)); };
  const u = new URL(req.url, 'https://x');
  if (u.pathname === '/ctl/stats') return j({ mode, conns: conns.map((c) => ({ url: c.url, frames: c.frames, bytes: c.bytes, texts: c.texts, open: c.ws.readyState === 1 })) });
  if (u.pathname === '/ctl/mode') { mode = JSON.parse(body).mode; return j({ ok: true, mode }); }
  if (u.pathname === '/ctl/send' || u.pathname === '/ctl/say') {
    const b = JSON.parse(body || '{}');
    let msgs = b.msgs || [];
    t0 += 1;
    const result = (text, fin) => ({ type: 'Results', is_final: fin, speech_final: fin, start: t0, duration: 1, channel: { alternatives: [{ transcript: text }] } });
    if (b.interim != null) msgs = [result(b.interim, false)];
    if (b.final != null) msgs = [result(b.final, true)];
    if (b.end) msgs = msgs.concat([{ type: 'UtteranceEnd', last_word_end: t0 }]);
    let n = 0;
    for (const c of conns) if (c.ws.readyState === 1) { for (const m of msgs) c.ws.send(JSON.stringify(m)); n++; }
    return j({ ok: true, sentTo: n });
  }
  if (u.pathname.startsWith('/v1/speak') || u.pathname.startsWith('/v2/speak')) {
    // A soft 220 Hz tone, longer for longer text (up to 8 s), 16-bit mono, streamed in pieces.
    const rate = Number(u.searchParams.get('sample_rate')) || 24000;
    let text = '';
    try { text = JSON.parse(body).text || ''; } catch { /* none */ }
    const secs = Math.min(8, 0.8 + text.length / 25);
    const n = Math.floor(rate * secs), buf = Buffer.alloc(n * 2);
    for (let i = 0; i < n; i++) buf.writeInt16LE(Math.round(Math.sin((i / rate) * 2 * Math.PI * 220) * 6000 * Math.min(1, i / 2000)), i * 2);
    res.writeHead(200, { 'Content-Type': u.searchParams.get('encoding') === 'linear16' ? 'audio/L16' : 'audio/mpeg' });
    let at = 0;
    const step = () => { if (at >= buf.length) return res.end(); res.write(buf.subarray(at, at + 9600)); at += 9600; setTimeout(step, 40); };
    return step();
  }
  if (u.pathname.startsWith('/v1/models')) return j({ tts: [{ canonical_name: 'aura-2-draco-en', name: 'draco', languages: ['en-GB'], metadata: { accent: 'British', tags: ['masculine'] } }] });
  j({ err: 'not stubbed', path: u.pathname }, 404);
});
const wss = new WebSocketServer({ noServer: true });
srv.on('upgrade', (req, sock, head) => {
  if (mode === 'refuse') { sock.write('HTTP/1.1 401 Unauthorized\r\n\r\n'); sock.destroy(); return; }
  wss.handleUpgrade(req, sock, head, (ws) => {
    const c = { ws, url: req.url, frames: 0, bytes: 0, texts: [] };
    conns.push(c);
    ws.on('message', (d, bin) => { if (bin) { c.frames++; c.bytes += d.length; } else c.texts.push(String(d).slice(0, 120)); });
  });
});
srv.listen(PORT, '127.0.0.1', () => console.log(`deepgram stub on https://127.0.0.1:${PORT} (certs in ${DIR})`));
