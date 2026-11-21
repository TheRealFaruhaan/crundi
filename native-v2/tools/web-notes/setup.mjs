const S = 'http://localhost:' + (process.env.PORT || '4820');
const j = async (m, p, b, t) => { const r = await fetch(S + p, { method: m, headers: { 'Content-Type': 'application/json', ...(t ? { Authorization: 'Bearer ' + t } : {}) }, body: b ? JSON.stringify(b) : undefined }); return { s: r.status, b: await r.json().catch(() => null) }; };
const su = await j('POST', '/api/auth/setup', { method: 'password', password: 'pw-' + Math.random() });
const t = su.b.token;
await j('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: '/tmp/crundi-devtest-' + (process.env.PORT || '4820') + '/proj', create: true }, t);
await j('POST', '/api/whatsnew/seen', {}, t);
const c = await j('POST', '/api/notes', { action: 'create', project: 'demo', title: 'Nesting', clientId: 'seed' }, t);
const id = c.b.page?.id || c.b.id;
const blocks = [
  { id: 'a1', type: 'bullet', text: 'Fruit' },
  { id: 'a2', type: 'bullet', text: 'Apple', level: 1 },
  { id: 'a3', type: 'number', text: 'Red', level: 2 },
  { id: 'a4', type: 'number', text: 'Green', level: 2 },
  { id: 'a5', type: 'todo', text: 'Pear', level: 1, checked: true },
  { id: 'a6', type: 'bullet', text: 'Veg' },
  { id: 'a7', type: 'p', text: 'End' },
];
const sv = await j('POST', '/api/notes', { action: 'save', project: 'demo', id, blocks, clientId: 'seed' }, t);
console.log(JSON.stringify({ t, r: su.b.refreshToken, id, save: sv.s, saveb: sv.b && Object.keys(sv.b) }));
