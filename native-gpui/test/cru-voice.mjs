// Cru's voice, end to end: the client opens Cru, its listen socket reaches the
// server and the server relays the microphone to Deepgram (the stand-in).
// Written for the owner's 18 Nov report: Cru stayed "Offline" because its
// sockets were built on an empty server address ("HTTP format error").
//
//   node native-gpui/tools/deepgram-stub.mjs 4893
//   DEEPGRAM_HOST=127.0.0.1:4893 NODE_TLS_REJECT_UNAUTHORIZED=0 native-gpui/tools/test-server.sh 4894 --bare
//   NATIVE_BIN=… CRUNDI_TEST_SERVER=http://localhost:4894 CRUNDI_TEST_PORT=4896 \
//     CRUNDI_TEST_MIC_WAV=native-gpui/test/fixtures/heycru/draco-pos-heycru.wav \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-gpui/test/cru-voice.mjs
//
// (CRUNDI_TEST_SERVER may be https://: the same over TLS.)
import * as t from './lib.mjs';
import https from 'node:https';
const stats = () => new Promise((r) => https.get({ host: '127.0.0.1', port: 4893, path: '/ctl/stats', rejectUnauthorized: false }, (res) => { let s = ''; res.on('data', (c) => (s += c)); res.on('end', () => { try { r(JSON.parse(s)); } catch { r(null); } }); }).on('error', () => r(null)));
const T = await t.begin('desktop');
const set = await T.api.http('POST', '/api/voice/settings', { deepgramKey: 'stub', brain: 'claude-code' });
console.log('settings', set.status, JSON.stringify(set.body).slice(0, 120));
await T.act('cru');
const C = (p) => p.areas.desktop.cru;
const p = await T.until('Cru opens and listens (not offline)', (p) => C(p).open && C(p).phase === 'listen', 15000);
t.check('…not offline', !!p && C(p).phase !== 'offline', p && C(p));
await t.sleep(3000);
const s = await stats();
const got = (s?.conns || []).reduce((n, c) => n + (c.frames || 0), 0);
t.check('…and the microphone reaches Deepgram through the server', got > 10, s);
await T.end();
