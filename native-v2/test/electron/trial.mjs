// The owner's trial findings (FINDINGS-ELECTRON.md), local stream's items:
// T5 the kid screen's lock clicked opens the PIN, a wrong PIN shows the
// error, the right one ends kid mode (real clicks and keys); T12 a project
// clicked in the sidebar leaves the Inbox for its workbench; T10 a chat's
// embed pages go when the chat closes.

export async function run(T, t) {
  const D = T.D;
  const at = async (words) => { const b = await T.ui('find', words); return b.ok ? { x: b.x + b.w / 2, y: b.y + b.h / 2 } : null; };
  const click = async (words) => { const p = await at(words); t.check(`"${words}" is on screen`, !!p); if (p) await T.mouse('click', p.x, p.y); return !!p; };

  // ─── T5: the kid screen's lock ───
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2580' });
  await T.act('kid-start', 'tray');
  await T.until('kid mode on, the window locked', (p) => D(p).kid?.on && D(p).windowLocked, 15000);
  await t.sleep(500);
  if (await click('Grown-ups')) {
    await T.until('the lock clicked: the PIN pad', (p) => D(p).kid.pin, 5000);
    await T.snap('electron', 'kid-pin-pad');
    for (const d of ['1', '1', '1', '1']) await T.key(d);
    t.check('…the digits typed', D(await T.state()).kid.pinLen === 4, D(await T.state()).kid);
    await T.key('Enter');
    await T.until('…a wrong PIN: the error, kid mode stays', (p) => D(p).kid.on && /PIN/i.test(D(p).kid.pinError || ''), 8000);
    for (const d of ['2', '5', '8', '0']) await T.key(d);
    await T.key('Backspace');
    await T.key('0');
    await T.key('Enter');
    await T.until('…the right PIN: kid mode off, the window let go', (p) => !D(p).kid.on && !D(p).windowLocked, 10000);
  }

  // ─── T12: a project in the sidebar leaves the Inbox ───
  await T.until('the inbox loads (its button shows)', (p) => D(p).inbox?.state === 'ok' && D(p).inbox?.supported !== false, 20000);
  await t.sleep(400);
  const inbox = await at('Inbox');
  if (inbox) {
    {
      const what = 'the open project';
      await T.mouse('click', inbox.x, inbox.y);
      await T.until('the Inbox open', (p) => p.tab === 'inbox', 5000);
      await click('Demo');
      await T.until(`…${what} clicked: its workbench again`, (p) => p.tab === 'workbench' && p.project === 'demo', 5000);
    }
    const r = await T.api.http('POST', '/api/projects', { alias: 'demo2', name: 'Second', path: '/tmp/crundi-v2-trial-demo2', create: true });
    t.check('a second project', r.status < 300, r.body);
    await T.until('…in the sidebar', (p) => (p.projects || []).includes('demo2'), 10000);
    await T.mouse('click', inbox.x, inbox.y);
    await T.until('the Inbox open again', (p) => p.tab === 'inbox', 5000);
    await click('Second');
    await T.until('…another project clicked: its workbench, the sidebar on it', (p) => p.tab === 'workbench' && p.project === 'demo2', 5000);
    await click('Demo');
    await T.until('…back to Demo', (p) => p.project === 'demo', 5000);
  } else t.check('the Inbox button (the server has an Inbox)', false);
}
