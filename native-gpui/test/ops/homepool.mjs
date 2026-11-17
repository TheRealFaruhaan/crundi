// operations.mjs's "homepool" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
//
// Everything here is the test server's own homepool (its own data folder,
// HOMEPOOL_DISABLE=1: no listeners); the live server's is never touched.
export async function run(T, t) {
  const hp = (p) => p.areas.ops.homepool;
  await T.ui('tab', 'homepool');
  let p = await T.until('homepool: it loads', (p) => hp(p).state === 'data' && hp(p).nets.length >= 3, 20000);
  if (!p) return;
  let h = hp(p);
  t.check('homepool: Networks, Access, Requests, Activity', h.tabs.map((x) => x[0]).join(',') === 'Networks,Access,Requests,Activity', h.tabs);
  t.check('homepool: the summary', h.sum.map((x) => x[1]).join(',') === 'NODES UP,KEYS IN USE,DATA TODAY,WAITING FOR YOU', h.sum);
  t.check('homepool: the server (always up), the bridge, Home', h.nets[0].kind === 'server' && h.nets[0].chip === 'always up' && h.nets.some((n) => n.kind === 'pool') && h.nets.some((n) => n.name === 'Home' && /No node/.test(n.body)), h.nets);
  await T.snap('homepool-networks');

  // A network, by the shared prompt.
  await T.act('homepool-act', 'net-add', '');
  p = await T.until('homepool: Network asks for a name', (p) => p.dialog?.title === 'Add a network');
  await T.dialog(true, 'Office');
  p = await T.until('homepool: the network is added', (p) => hp(p).nets.some((n) => n.name === 'Office' && n.kind === 'own'));
  const office = hp(p).nets.find((n) => n.name === 'Office').id;

  // A node in Home: its code shown once, with the QR.
  await T.act('homepool-act', 'node-add', '');
  p = await T.until('homepool: the node dialog', (p) => hp(p).dlg === 'node' && hp(p).dlgTitle === 'Add a node' && hp(p).modal);
  await T.act('homepool-dlg-ok');
  await T.until('homepool: a node needs a name', (p) => p.toast && /name/.test(JSON.stringify(p.toast)) && hp(p).dlg === 'node');
  await T.act('homepool-node-fill', 'Test node', '1');
  await T.act('homepool-dlg-ok');
  p = await T.until('homepool: the node is ready: its code and QR', (p) => hp(p).dlg === 'show' && /is ready/.test(hp(p).dlgTitle) && hp(p).qr && hp(p).show.length >= 1, 15000);
  if (p) t.check('homepool: the code and the docker line', hp(p).show[0][1].length > 20 && hp(p).show.some((x) => x[1].startsWith('docker run')), hp(p).show);
  await T.snap('homepool-node-setup');
  await T.act('homepool-dlg-close');
  p = await T.until('homepool: the node sits in Home, offline', (p) => !hp(p).modal && hp(p).nets.find((n) => n.name === 'Home')?.nodes.some((x) => x.name === 'Test node' && x.state === 'offline' && x.net === 1));
  const node = hp(p).nets.find((n) => n.name === 'Home').nodes[0].id;
  t.check('homepool: its line says what it is', /Standalone node · last seen never · 0 slots · 0 B today/.test(hp(p).nets.find((n) => n.name === 'Home').nodes[0].meta), hp(p).nets.find((n) => n.name === 'Home').nodes[0].meta);

  // Move, switch off, rename.
  const officeIdx = hp(p).moveOpts.indexOf('Office');
  await T.act('homepool-move', node, String(officeIdx));
  await T.until('homepool: moved to Office', (p) => hp(p).nets.find((n) => n.id === office)?.nodes.some((x) => x.id === node));
  await T.act('homepool-toggle', 'node', node, 'false');
  await T.until('homepool: switched off', (p) => hp(p).nets.find((n) => n.id === office).nodes[0].state === 'switched off');
  await T.act('homepool-act', 'node-rename', node);
  await T.until('homepool: the rename prompt', (p) => p.dialog?.title === 'Rename node' && p.dialog.value === 'Test node');
  await T.dialog(true, 'Lab node');
  await T.until('homepool: renamed', (p) => hp(p).nets.find((n) => n.id === office).nodes[0].name === 'Lab node');
  await T.act('homepool-act', 'node-reissue', node);
  await T.until('homepool: New key asks first', (p) => p.dialog?.title === 'New key for "Lab node"?');
  await T.dialog(true);
  await T.until('homepool: the new code is shown', (p) => hp(p).dlg === 'show' && hp(p).dlgTitle === 'New key for "Lab node"');
  await T.act('homepool-dlg-close');

  // Access for a piece of software: the secret shown once.
  await T.act('homepool-tab', 'access');
  p = await T.until('homepool: no access yet', (p) => hp(p).tab === 'access' && hp(p).keys.length === 0);
  await T.act('homepool-act', 'key-add', '');
  p = await T.until('homepool: the access dialog', (p) => hp(p).dlg === 'key' && hp(p).dlgTitle === 'Give access' && hp(p).kNets.some(([l, on]) => l === 'Server' && on));
  await T.act('homepool-key-fill', '0', 'scraper-test', '', 'false', '0', '8', '', '0', '');
  await T.act('homepool-dlg-ok');
  await T.until('homepool: at least one network', (p) => /network/.test(JSON.stringify(p.toast || '')) && hp(p).dlg === 'key');
  await T.act('homepool-key-fill', '0', 'scraper-test', `server,${office}`, 'true', '0', '4', '', '0', '');
  await T.act('homepool-dlg-ok');
  p = await T.until('homepool: the secret is shown once', (p) => hp(p).dlg === 'show' && hp(p).dlgTitle === 'Access "scraper-test"' && /^user: .*\npassword: /.test(hp(p).show[0]?.[1] || ''), 15000);
  const cp = await T.act('homepool-copy', hp(p).show[0][1]);
  t.check('homepool: Copy copies', cp.text === hp(p).show[0][1]);
  await T.act('homepool-dlg-close');
  p = await T.until('homepool: the key, its networks, its line', (p) => hp(p).keys.length === 1 && hp(p).keys[0].nets.join(',') === 'Server,Office' && /Software, own login · 0 B today/.test(hp(p).keys[0].meta) && /falls back to the server/.test(hp(p).keys[0].meta));
  const key = hp(p).keys[0].id;
  await T.act('homepool-act', 'key-use', key);
  await T.until('homepool: How to use', (p) => hp(p).keys[0].open && hp(p).keys[0].uses.some((u) => u.startsWith('socks5h://')));
  await T.snap('homepool-access');
  await T.act('homepool-act', 'key-use', key);
  await T.act('homepool-toggle', 'key', key, 'false');
  await T.until('homepool: the key switched off', (p) => hp(p).keys[0].on === false);
  await T.act('homepool-act', 'key-edit', key);
  p = await T.until('homepool: Change locks its kind', (p) => hp(p).dlg === 'key' && hp(p).kKindLocked && hp(p).kName === 'scraper-test' && hp(p).dlgOk === 'Save');
  await T.act('homepool-key-fill', '0', 'scraper-test', 'server', 'false', '100', '4', '2030-01-02 03:04', '0', '');
  await T.act('homepool-dlg-ok');
  await T.until('homepool: saved', (p) => hp(p).dlg === '' && /cap 100 MB\/day/.test(hp(p).keys[0].meta) && /expires/.test(hp(p).keys[0].meta) && hp(p).keys[0].nets.join(',') === 'Server');

  // Requests (none: a chat asks through its MCP tool) and the activity log.
  await T.act('homepool-tab', 'requests');
  await T.until('homepool: no requests', (p) => hp(p).tab === 'requests' && hp(p).reqs.length === 0);
  await T.act('homepool-tab', 'activity');
  p = await T.until('homepool: the activity log', (p) => hp(p).tab === 'activity' && hp(p).log.some((l) => l.startsWith('added network Office')) && hp(p).log.some((l) => l.startsWith('gave access scraper-test')));
  await T.snap('homepool-activity');

  // The standalone bridge off and on again.
  await T.act('homepool-tab', 'networks');
  await T.act('homepool-toggle', 'pool', 'pool', 'false');
  await T.until('homepool: the bridge off', (p) => hp(p).nets.find((n) => n.kind === 'pool').chip === 'bridge off');
  await T.act('homepool-toggle', 'pool', 'pool', 'true');
  await T.until('homepool: the bridge on', (p) => hp(p).nets.find((n) => n.kind === 'pool').on);

  // Revoke, remove the node and the network (each asks first).
  await T.act('homepool-act', 'key-remove', key);
  await T.until('homepool: Revoke asks first', (p) => p.dialog?.title === 'Revoke "scraper-test"?' && p.dialog.danger);
  await T.dialog(true);
  await T.until('homepool: revoked', (p) => hp(p).keys.length === 0);
  await T.act('homepool-act', 'node-remove', node);
  await T.until('homepool: Remove asks first', (p) => p.dialog?.title === 'Remove "Lab node"?');
  await T.dialog(true);
  await T.until('homepool: the node removed', (p) => !hp(p).nets.some((n) => n.nodes.length));
  await T.act('homepool-act', 'net-remove', office);
  await T.until('homepool: Remove network asks first', (p) => p.dialog?.title === 'Remove "Office"?');
  await T.dialog(true);
  await T.until('homepool: the network removed', (p) => !hp(p).nets.some((n) => n.name === 'Office'));
}
