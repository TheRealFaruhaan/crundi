// operations.mjs's "services" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
// The Services tab against the test server: empty, register, a running
// service with its usage strip, logs, a path forward (private, then public),
// the forward switched off, the port cleared, other forwards (project,
// remove), a service registered elsewhere arriving live, stop and delete.
// The services run `sleep 600` (no port of their own); the forwards name
// ports that nothing binds (4560, 4561: this section's own range).

const svc = (p) => p.areas.ops.services;
const card = (p, key) => (svc(p).cards || []).find((c) => c.key === key);

export async function run(T, t) {
  const KEY = 'demo:web';
  await T.ui('tab', 'services');
  let p = await T.until('services: the tab loads, empty', (p) => svc(p).loaded && svc(p).cards.length === 0);
  if (p) {
    t.check('services: the toolbar title', svc(p).title === 'Services', svc(p).title);
    t.check('services: the empty line', svc(p).empty === 'No services registered for this project', svc(p).empty);
    t.check('services: the stats poll runs while on show', svc(p).polling === true);
  }
  await T.snap('services-empty');

  // The register form.
  await T.act('services-register-open');
  await T.until('services: the register form opens', (p) => svc(p).regOpen);
  await T.snap('services-register');
  await T.act('services-register-cancel');
  await T.until('services: Cancel closes it', (p) => !svc(p).regOpen);

  // A name is required (the form stays).
  await T.act('services-register', '', 'sleep 600');
  await T.until('services: "Service name is required"', (p) => /Service name is required/.test(p.toast || ''));
  await T.act('services-register-cancel');

  await T.act('services-register', 'web', 'sleep 600', '', '', '4561');
  p = await T.until('services: the registered service gets a card', (p) => !!card(p, KEY));
  if (p) {
    const c = card(p, KEY);
    t.check('services: the form closes after Register', !svc(p).regOpen);
    t.check('services: the command line', c.command === 'sleep 600', c);
    // A port given at Register turns the tunnel on (the server's default), as the web shows it: Tunnel, "idle".
    t.check('services: the port row (Port 4561, Tunnel, idle)', c.expose && c.portLabel === 'Port 4561' && c.mode === 1 && c.stateWord === 'idle', c);
    t.check('services: no Tunnel button with a port set', c.tunnelBtn === false, c);
  }
  // Off before it starts: a tunnel would publish the port on the internet (a cloudflared quick tunnel).
  await T.act('services-expose', KEY, 'off');
  await T.until('services: Off turns the tunnel off', (p) => card(p, KEY)?.mode === 0 && card(p, KEY)?.stateWord === 'off');

  // Start it: running, uptime, the usage strip.
  if (!card(p || {}, KEY)?.running) await T.act('services-act', 'start', KEY);
  p = await T.until('services: it runs, with its usage strip', (p) => card(p, KEY)?.running && card(p, KEY)?.stats, 30000);
  if (p) {
    const c = card(p, KEY);
    t.check('services: RUNNING pill and Stop/Restart', c.status === 'RUNNING' && c.tone === 'running', c);
    t.check('services: the strip names the pid', /^pid \d+/.test(c.stats.badge), c.stats);
  }
  await T.until('services: the uptime shows', (p) => /up /.test(card(p, KEY)?.uptime || ''), 15000);
  await T.until('services: the 2s poll draws the CPU sparkline', (p) => card(p, KEY)?.stats?.cpuLine, 20000);
  await T.snap('services-running');

  // Logs, a toggle.
  await T.act('services-act', 'logs', KEY);
  await T.until('services: Logs opens the log under the card', (p) => card(p, KEY)?.logsOpen && card(p, KEY).logs.length > 0);
  await T.act('services-act', 'logs', KEY);
  await T.until('services: Logs again closes it', (p) => card(p, KEY) && !card(p, KEY).logsOpen);

  // Subdomain: refused without a domain; with one, the form ends in ".<domain>".
  const opts = (await T.api.http('GET', '/api/forwards/options')).body || {};
  const sub = (opts.choices || []).find((c) => c.id === 'subdomain');
  await T.act('services-expose', KEY, 'subdomain');
  if (sub?.available) {
    const dom = sub.suggestedDomain || opts.domain;
    await T.until('services: Subdomain opens the form (name.' + dom + ')', (p) => card(p, KEY)?.editing && card(p, KEY).editPrefix === '' && card(p, KEY).editSuffix === '.' + dom);
    await T.act('services-edit-cancel');
    await T.until('services: Cancel closes the form', (p) => card(p, KEY) && !card(p, KEY).editing);
  } else {
    await T.until('services: Subdomain needs a domain', (p) => /No domain is configured/.test(p.toast || ''));
  }

  // Path: the naming form, seeded with the service's name.
  await T.act('services-expose', KEY, 'path');
  p = await T.until('services: Path opens the naming form', (p) => card(p, KEY)?.editing);
  if (p) {
    const c = card(p, KEY);
    t.check('services: /tunnel/ name / and the default name', c.editPrefix === '/tunnel/' && c.editSuffix === '/' && c.editName === 'web' && !c.editPublic, c);
  }
  await T.act('services-edit-name', 'Bad_Name');
  await T.act('services-edit-save');
  await T.until('services: a bad name is refused', (p) => /Lowercase letters, digits and hyphens/.test(p.toast || ''));
  await T.act('services-edit-name', 'webx');
  await T.act('services-edit-save');
  p = await T.until('services: the forward is made (Path, its address)', (p) => card(p, KEY)?.mode === 3 && /\/tunnel\/webx\/$/.test(card(p, KEY)?.link || '') && !card(p, KEY).editing);
  if (p) t.check('services: private, with its remove button', !card(p, KEY).public && card(p, KEY).fwdHost === 'webx', card(p, KEY));
  await T.snap('services-forward');

  await T.act('services-open-url', '/tunnel/webx/');
  await T.until('services: a path address opens on this server', (p) => svc(p).lastUrl === T.SERVER + '/tunnel/webx/');

  // Public: the form is seeded from the forward; saving asks first.
  await T.act('services-expose', KEY, 'path');
  await T.until('services: the form again, with the forward\'s name', (p) => card(p, KEY)?.editing && card(p, KEY).editName === 'webx');
  await T.act('services-edit-public', 'true');
  await T.until('services: Public ticked', (p) => card(p, KEY)?.editPublic);
  await T.act('services-edit-save');
  p = await T.until('services: publishing asks first', (p) => p.dialog && /Publish webx\?/.test(p.dialog.message));
  await T.dialog(true);
  await T.until('services: the forward is public', (p) => card(p, KEY)?.public && /\/tunnel\/webx\/$/.test(card(p, KEY)?.link || ''));

  // Off: the forward goes.
  await T.act('services-expose', KEY, 'off');
  await T.until('services: Off removes the forward', (p) => card(p, KEY)?.mode === 0 && card(p, KEY)?.link === '' && card(p, KEY)?.stateWord === 'off');
  const fl = await T.api.http('GET', '/api/forwards');
  t.check('services: no forward left on the server', (fl.body?.forwards || []).length === 0, fl.body);

  // The port: a prompt; blank clears it (and the Tunnel button returns).
  await T.act('services-act', 'port', KEY);
  p = await T.until('services: the Tunnel port prompt', (p) => p.dialog && p.dialog.title === 'Tunnel port' && p.dialog.value === '4561');
  await T.dialog(true, '');
  await T.until('services: the port is cleared', (p) => card(p, KEY) && !card(p, KEY).expose && card(p, KEY).tunnelBtn);

  // Other forwards: one on a port no service has.
  const add = await T.api.http('POST', '/api/forwards', { name: 'loose', port: 4560, mode: 'path' });
  t.check('services: a loose forward on the server', add.body?.ok, add.body);
  await T.act('services-load');
  p = await T.until('services: "Other forwards" lists it', (p) => (svc(p).others || []).some((f) => f.host === 'loose'));
  if (p) {
    const f = svc(p).others.find((f) => f.host === 'loose');
    t.check('services: its address, port and project', /\/tunnel\/loose\/$/.test(f.url) && f.port === '4560' && f.project === '', f);
  }
  await T.act('services-fwd-project', 'loose', 'demo');
  await T.until('services: it now serves demo', (p) => (svc(p).others || []).find((f) => f.host === 'loose')?.project === 'demo');
  await T.act('services-fwd-remove', 'loose');
  await T.until('services: removed', (p) => !(svc(p).others || []).some((f) => f.host === 'loose'));

  // Live: a service registered elsewhere arrives without a reload.
  const reg = await T.api.http('POST', '/api/services', { alias: 'demo', name: 'other', command: 'sleep 600' });
  t.check('services: registered through the API', reg.body?.ok, reg.body);
  await T.until('services: it arrives live (the state event)', (p) => !!card(p, 'demo:other'), 15000);

  // Restart, stop.
  await T.act('services-act', 'restart', KEY);
  await T.until('services: restarted, still running', (p) => /restart demo:web/.test(p.toast || '') || card(p, KEY)?.running);
  await T.act('services-act', 'stop', KEY);
  p = await T.until('services: Stop stops it (Start, no strip)', (p) => card(p, KEY) && !card(p, KEY).running && !card(p, KEY).stats, 20000);
  if (p) t.check('services: the pill says STOPPED', card(p, KEY).status === 'STOPPED', card(p, KEY));

  // Delete: asks first; Cancel keeps it.
  await T.act('services-act', 'delete', 'demo:other');
  await T.until('services: Delete asks first', (p) => p.dialog && p.dialog.message === 'Delete service "demo:other"?');
  await T.dialog(false);
  await T.act('services-act', 'delete', 'demo:other');
  await T.until('services: asked again', (p) => !!p.dialog);
  await T.dialog(true);
  await T.until('services: deleted', (p) => !card(p, 'demo:other') && !!card(p, KEY));
  await T.act('services-act', 'delete', KEY);
  await T.until('services: asked', (p) => !!p.dialog);
  await T.dialog(true);
  await T.until('services: empty again', (p) => svc(p).cards.length === 0);

  // Off the tab: the poll stops.
  await T.ui('tab', 'workbench');
  await T.until('services: the poll stops off the tab', (p) => svc(p).polling === false);
}
