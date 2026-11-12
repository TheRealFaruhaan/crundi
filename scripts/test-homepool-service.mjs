// Homepool's MCP tools and owner actions (src/homepool-service.js), in process:
// a chat proves who it is with its chat token, asks, and gets nothing until
// the owner approves. Binds no ports (the engine is not started).
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHomepoolService } from '../src/homepool-service.js';
import { chatToken } from '../src/chat-token.js';

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`); if (!ok) failed++; };
const dir = mkdtempSync(join(tmpdir(), 'homepool-svc-'));
const told = [];
const svc = createHomepoolService({ dataDir: dir, basePort: 17600, publicUrl: () => 'https://dev.example.com', notifyOwner: (m) => told.push(m), log: () => {} });
svc.store.setSettings({ poolBridge: { enabled: false } });

const me = { sessionId: 'chat-a', project: 'insta', chatToken: chatToken('chat-a', 'insta') };

let r = await svc.mcp('homepool_status', { sessionId: 'chat-a', project: 'insta' });
check('a call without the chat token is refused', !r.ok && /verified/.test(r.error), JSON.stringify(r));
r = await svc.mcp('homepool_status', { ...me, sessionId: 'chat-b' });
check('naming another chat with my token is refused', !r.ok, JSON.stringify(r));
r = await svc.mcp('homepool_status', { ...me, project: 'other' });
check('naming another project with my token is refused', !r.ok);
r = await svc.mcp('homepool_status', me);
check('status: not granted yet', r.ok && r.granted === false && r.networks.some(n => n.id === 'home'), JSON.stringify(r));
r = await svc.mcp('homepool_proxy', me);
check('proxy without a grant is refused', !r.ok && r.granted === false);
r = await svc.mcp('homepool_fetch', { ...me, url: 'https://example.com/' });
check('fetch without a grant is refused', !r.ok && r.granted === false);

r = await svc.mcp('homepool_request', { ...me, networks: ['home'], reason: 'scrape public posts', wait: false });
check('a request goes to the owner', r.ok && r.status === 'pending' && told.length === 1 && /asks for access/.test(told[0].body), JSON.stringify(told));
r = await svc.mcp('homepool_request', { ...me, networks: ['nowhere'], reason: 'x', wait: false });
check('an unknown network cannot be asked for', !r.ok);
r = await svc.mcp('homepool_proxy', me);
check('still nothing before the owner answers', !r.ok);

// The owner approves, with a change: the server instead of home.
const reqId = svc.store.requests()[0].id;
const ans = svc.mcp('homepool_request', { ...me, networks: ['home'], reason: 'scrape public posts' });   // waits
await new Promise(res => setTimeout(res, 200));
const dec = await svc.handleApi(
  { method: 'POST', url: '/api/homepool' }, { }, '/api/homepool',
  { principal: { role: 'owner' }, readBody: async () => JSON.stringify({ action: 'request-decide', id: reqId, approve: true, changes: { networks: ['server'] } }), json: (res, d) => { res.d = d; }, isOwner: (p) => p.role === 'owner' },
);
check('the owner route handled it', dec === true);
const waited = await ans;
check('the waiting chat hears it was approved, as changed', waited.ok && waited.status === 'approved' && waited.key.networks.join() === 'server', JSON.stringify(waited));
r = await svc.mcp('homepool_status', me);
check('status: granted, server only', r.ok && r.granted === true && r.key.networks.join() === 'server');
r = await svc.mcp('homepool_proxy', me);
check('proxy URL for this chat', r.ok && /^socks5h:\/\/k[0-9a-f]+:[A-Za-z0-9_-]+@127\.0\.0\.1:17600$/.test(r.socks), r.socks);
r = await svc.mcp('homepool_proxy', { ...me, network: 'home' });
check('…but not for a network it was not given', !r.ok);
r = await svc.mcp('homepool_fetch', { ...me, url: 'http://127.0.0.1:8888/' });
check('fetch onto the server\'s own services is refused', !r.ok && /private/.test(r.error), JSON.stringify(r));

// Another chat of the same project has nothing (the grant was for this chat).
const other = { sessionId: 'chat-c', project: 'insta', chatToken: chatToken('chat-c', 'insta') };
r = await svc.mcp('homepool_status', other);
check('another chat of the project is not covered by a chat grant', r.ok && r.granted === false);

// Software key by request: the secret is handed over once.
const swAns = svc.mcp('homepool_request', { ...other, kind: 'software', name: 'insta-scraper', networks: ['home'], reason: 'the scraper' });
await new Promise(res => setTimeout(res, 200));
const sw = svc.store.requests().find(x => x.kind === 'software');
const res = {};
await svc.handleApi({ method: 'POST', url: '/api/homepool' }, res, '/api/homepool', {
  principal: { role: 'owner' }, readBody: async () => JSON.stringify({ action: 'request-decide', id: sw.id, approve: true }), json: (rr, d) => { rr.d = d; }, isOwner: (p) => p.role === 'owner',
});
const swr = await swAns;
check('approved software key comes with its secret once', swr.ok && swr.secret && swr.use.socks.includes(swr.secret), JSON.stringify(swr).slice(0, 200));
check('…and the chat that asked does not get chat access by it', (await svc.mcp('homepool_status', other)).granted === false);

// Only the owner manages it.
const r403 = {};
await svc.handleApi({ method: 'GET', url: '/api/homepool' }, r403, '/api/homepool', { principal: { role: 'collaborator' }, readBody: async () => '', json: (rr, d, st) => { rr.d = d; rr.st = st; }, isOwner: (p) => p.role === 'owner' });
check('a non-owner is refused', r403.st === 403);

// Owner actions: a node for a friend comes with code, QR and docker line.
const add = {};
await svc.handleApi({ method: 'POST', url: '/api/homepool' }, add, '/api/homepool', { principal: { role: 'owner' }, readBody: async () => JSON.stringify({ action: 'node-add', name: 'Ali', network: 'home' }), json: (rr, d) => { rr.d = d; }, isOwner: (p) => p.role === 'owner' });
const n = add.d && add.d.node;
check('node-add gives key, code, QR and docker line', n && /^hpn_/.test(n.key) && n.code.startsWith('crundi-homepool://') && n.qr.startsWith('<svg') && n.docker.includes('node:22-alpine') && n.docker.includes('/api/homepool/node.mjs'), JSON.stringify(n).slice(0, 200));
// Revoking a chat's key: the chat loses it now.
const kid = svc.store.keyForChat('chat-a', 'insta').id;
await svc.handleApi({ method: 'POST', url: '/api/homepool' }, {}, '/api/homepool', { principal: { role: 'owner' }, readBody: async () => JSON.stringify({ action: 'key-remove', id: kid }), json: () => {}, isOwner: (p) => p.role === 'owner' });
check('a revoked chat has no access', (await svc.mcp('homepool_status', me)).granted === false);

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
