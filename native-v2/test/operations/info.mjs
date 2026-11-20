// operations.mjs's "info" section: the Info tab. run(T, t) with T from
// t.begin('ops') (test/lib.mjs).
//
// Nothing here changes the machine: the reclaim buttons and the sandbox
// set-up are never pressed (they act on the real box), and the invite form
// is only filled in (with the sandbox not set up, Create access is off).

const info = (p) => p.areas.ops.info;

export async function run(T, t) {
  await T.ui('tab', 'info');
  await T.until('info: Info is on show and polls the machine', (p) => info(p).shown && info(p).polling);

  // Project and Status (the project "demo" is open).
  const p1 = await T.until('info: the project and its status', (p) => info(p).project?.alias === 'demo');
  if (p1) {
    const pr = info(p1).project;
    t.check('info: the project\'s name and path', pr.name === 'Demo' && pr.path.endsWith('/proj'), pr);
    t.check('info: the Claude terminal line (none yet, or one an earlier section started)', ['Not started', 'Active'].includes(pr.terminal), pr);
    t.check('info: services counted', /^\d+ registered, \d+ running$/.test(pr.services), pr);
  }

  // This machine, from /api/stats (every 2 s).
  const p2 = await T.until('info: the machine\'s numbers', (p) => info(p).sys.ready && info(p).sys.rows.length > 4, 15000);
  if (p2) {
    const sys = info(p2).sys;
    t.check('info: CPU as a percentage', /%$/.test(sys.cpu), sys.cpu);
    t.check('info: one bar per core', sys.cores > 0, sys.cores);
    t.check('info: Memory, Disk, Uptime, Crundi, Machine, Processor', ['Memory', 'Disk', 'Uptime', 'Crundi', 'Machine', 'Processor'].every((k) => sys.rows.some((r) => r[0] === k)), sys.rows);
  }
  await T.until('info: the CPU history draws after two samples', (p) => info(p).sys.spark, 15000);

  // Reclaim disk space and Outside collaborators load (read only).
  await T.until('info: reclaim disk space loads', (p) => ['data', 'empty', 'error'].includes(info(p).maint.state), 90000);   // it measures caches on a busy disk: slow under load
  await T.until('info: outside collaborators load', (p) => ['data', 'error'].includes(info(p).collab.state), 15000);
  let m = await T.mine();
  if (m.info.maint.state === 'data') t.check('info: the reclaim total line', /can be reclaimed|Nothing to reclaim right now/.test(m.info.maint.total), m.info.maint);

  // Tunnel (the test server runs --dev: none configured).
  await T.until('info: the tunnel section', (p) => info(p).tunnel !== null);
  m = await T.mine();
  t.check('info: tunnel status in the web\'s words', ['Connected', 'Connecting...', 'Not configured'].includes(m.info.tunnel.status), m.info.tunnel);

  // Server Logs: loaded, then live lines append.
  await T.until('info: the server log loads', (p) => !info(p).logsLoading);
  const before = (await T.mine()).info.logs;
  await T.act('info-log', 'a warning from the native test', 'warn');
  m = await T.mine();
  t.check('info: a live line appends at the end', m.info.logs === before + 1 && /\] a warning from the native test$/.test(m.info.lastLog), m.info);
  // A real line from the server: adding a forward logs ("[forwards] infolog -> localhost:…"); then it goes again.
  await T.api.http('POST', '/api/forwards', { name: 'infolog', port: 4569, mode: 'path' });
  await T.until('info: the server\'s own lines arrive live (/ws server-log)', (p) => info(p).logs > before + 1, 10000);
  await T.api.http('DELETE', '/api/forwards/infolog');
  await T.snap('info-tab');

  // Copy: every line to the clipboard.
  await T.act('info-act', 'copy-logs', '');
  const pc = await T.props();
  t.check('info: Copy puts the log on the clipboard', /a warning from the native test/.test(pc.clipboard || ''), (pc.clipboard || '').slice(-200));
  t.check('info: and says so', /Logs copied/.test(JSON.stringify(pc.toast || '')), pc.toast);

  // The invite form: the expiry follows the amount and the unit.
  await T.act('info-field', 'amount', '4');
  await T.act('info-field', 'unit', '1');
  m = await T.mine();
  t.check('info: hours under two days say the clock', /^Ends \w{3} \d{1,2}:\d{2} [AP]M\. You can extend/.test(m.info.collab.expiry), m.info.collab.expiry);
  await T.act('info-field', 'amount', '0');
  m = await T.mine();
  t.check('info: zero asks for at least one', m.info.collab.expiry === 'Enter how long, at least 1 hour.', m.info.collab.expiry);
  await T.act('info-field', 'amount', '7');
  await T.act('info-field', 'unit', '0');
  m = await T.mine();
  t.check('info: days say the date', /^Ends \w{3}, \w{3} \d{1,2}\./.test(m.info.collab.expiry), m.info.collab.expiry);
  // Ticking a project; Create with no name says so (nothing is sent).
  await T.act('info-act', 'inv-project', 'demo');
  m = await T.mine();
  t.check('info: a project can be ticked', m.info.collab.projects.some((x) => x.key === 'demo' && x.on), m.info.collab.projects);
  await T.act('info-field', 'name', '');
  await T.act('info-act', 'collab-create', '');
  const pt = await T.props();
  t.check('info: Create access with no name says "Give them a name"', /Give them a name/.test(JSON.stringify(pt.toast || '')), pt.toast);
  await T.act('info-act', 'inv-project', 'demo');

  // Leaving Info stops the polling.
  await T.ui('tab', 'workbench');
  await T.until('info: polling stops when Info is not on show', (p) => !info(p).polling && !info(p).shown, 5000);
}
