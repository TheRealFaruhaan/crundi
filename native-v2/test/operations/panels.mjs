// operations.mjs's "panels" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
//
// Panels (widgets) against the test server: made with the same MCP calls
// Claude makes (widget_open, widget_set_data), then the tab, its bar and
// panes, the frame and its bridge, a chip's popover, the workbench's
// chooser and pane, an approval, an inline card, close and delete.
import { readFileSync, existsSync } from 'node:fs';

export async function run(T, t) {
  const r = await T.act('ping');
  t.check('panels: the area answers', r.ok, r);

  const port = new URL(T.SERVER).port;
  const keyFile = `/tmp/crundi-devtest-${port}/data/.api-key`;
  const KEY = existsSync(keyFile) ? readFileSync(keyFile, 'utf8').trim() : '';
  t.check('panels: the server\'s MCP key', !!KEY, keyFile);
  const mcp = (tool, args) => fetch(T.SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args }) }).then((x) => x.json()).catch((e) => ({ ok: false, error: String(e) }));
  const mine = (p) => p.areas.ops.panels;
  const view = (p, key) => mine(p).views?.[key];
  const ID = 'build-board';
  const TAB = `wg|tab|demo|${ID}`;
  // Without the browser host (the Desktop area's, web.rs) a copy cannot be
  // drawn: it says so ("The browser did not start…"); the rest is checked
  // the same, and what needs a page (paint, the bridge, sizes) is skipped.
  const web = !!(await T.mine()).panels.browser;
  if (!web) console.log('     (no browser host: copies are checked for the "browser did not start" state; the bridge, paint and sizes skipped)');
  const drawn = (v) => web ? (v && v.state === 'ready' && v.live && v.painted && v.msgs > 0) : (v && v.state === 'error' && /browser did not start/.test(v.error));
  const shown = (v) => web ? v?.painted : drawn(v);

  // ── Empty ──
  await T.ui('tab', 'panels');
  await T.until('panels: the empty tab', (p) => p.currentTab === 'panels' && mine(p).loaded && mine(p).widgets.length === 0);
  await T.snap('panels-empty');

  // ── A panel, as Claude opens one ──
  // On ready it says what it shows (a toast: the bridge out), and its height.
  const html = (v) => `<div class="card" style="padding:12px"><h3 style="margin:0 0 8px">Build board ${v}</h3>`
    + '<p>Status: <b data-bind="status" data-empty="none"></b></p>'
    + '<div style="height:120px;border-radius:8px;background:linear-gradient(90deg,#6366f1,#22c55e)"></div></div>'
    + '<script>crundi.ready(function(){ crundi.toast("status: " + crundi.data.status); });</script>';
  const open1 = await mcp('widget_open', { alias: 'demo', id: ID, title: 'Build board', slot: 'tab', html: html('v1'), data: { status: 'starting' } });
  t.check('panels: widget_open made it', open1.ok !== false, open1);
  await T.until('panels: the list has it, selected', (p) => mine(p).widgets.some((w) => w.id === ID) && mine(p).selected === ID);
  const ready = await T.until('panels: its frame loads, paints and talks (ready, height)', (p) => {
    return drawn(view(p, TAB));
  }, 30000);
  if (web) await T.until('panels: the bridge out: the panel\'s toast', (p) => String(p.toast?.text || p.toast || '').includes('status: starting'), 15000);
  await T.snap('panels-tab');

  // ── Live data: the page loads again with the new value ──
  const loads0 = ready ? view(ready, TAB).loads : 0;
  const set = await mcp('widget_set_data', { alias: 'demo', id: ID, source: 'status', value: 'passing' });
  t.check('panels: widget_set_data', set.ok !== false, set);
  await T.until('panels: new data reaches the panel', (p) => view(p, TAB)?.loads > loads0 && (!web || String(p.toast?.text || p.toast || '').includes('status: passing')), 20000);

  // ── A new version (Claude edits it), then History and Restore ──
  await mcp('widget_open', { alias: 'demo', id: ID, html: html('v2') });
  await T.act('panels-pane', 'history');
  await T.until('panels: History lists the versions', (p) => mine(p).pane === 'history' && mine(p).versionsState === 'list' && mine(p).versions.length >= 2);
  const v1 = (await T.mine()).panels.versions.slice(-1)[0];
  await T.act('panels-restore', ID, v1);
  // (F115: the restored page reloads and its bridge's "status: …" toast soon
  // replaces "Restored v1": watch for the toast on its own, from the act on,
  // then for the pane, not both in one reading.)
  {
    const toastOf = (p) => String(p.toast?.text || p.toast || '');
    let said = false;
    for (const end = Date.now() + 10000; !said && Date.now() < end;) { said = toastOf(await T.props()).includes('Restored v' + v1); if (!said) await t.sleep(40); }
    t.check('panels: Restore says so', said);
    await T.until('panels: …and goes back to the panel', (p) => mine(p).pane === '');
  }

  // ── Checks: none run yet ──
  await T.act('panels-pane', 'checks');
  await T.until('panels: Checks says no check has run', (p) => mine(p).checksState === 'none');
  await T.act('panels-pane', '');

  // ── Pin, global and back ──
  await T.act('panels-pin', ID);
  await T.until('panels: Pin keeps it', (p) => mine(p).widgets.find((w) => w.id === ID)?.lifecycle === 'pinned');
  await T.act('panels-scope', ID, 'true');
  await T.until('panels: Make global', (p) => mine(p).widgets.find((w) => w.id === ID)?.global === true && String(p.toast?.text || p.toast || '').includes('Now global'));
  await T.act('panels-scope', ID, 'false');
  await T.until('panels: back in this project only', (p) => mine(p).widgets.find((w) => w.id === ID)?.global === false);

  // ── The top bar chip and its popover ──
  await T.act('panels-slot', ID, 'chip');
  await T.until('panels: moved to the top bar', (p) => mine(p).widgets.find((w) => w.id === ID)?.place === 'chip');
  await T.act('panels-chip', ID);
  await T.until('panels: the chip\'s popover shows the panel', (p) => mine(p).pop === ID && shown(view(p, `wg|pop|demo|${ID}`)), 30000);
  await T.act('panels-pop-close');
  await T.until('panels: the popover closes', (p) => mine(p).pop === '');

  // ── The workbench: the chooser, then the panel in its pane ──
  await T.act('panels-slot', ID, 'tab');
  await T.until('panels: back to the tab only', (p) => mine(p).widgets.find((w) => w.id === ID)?.slot === 'tab');
  await T.ui('tab', 'workbench');
  await T.act('panels-add-pane');
  await t.sleep(1500);
  await T.snap('panels-chooser');
  // The pane the "+" menu's Panel made (the workbench keys them panel:widget:<n>): pick into it, as a click in its chooser does.
  const pane = (await T.props()).panes.filter((k) => k.startsWith('panel:widget')).pop();
  t.check('panels: an empty Panel pane with its chooser', !!pane, (await T.props()).panes);
  await T.act('panels-pick', pane || 'panel:widget', ID);
  await T.until('panels: picked: the panel is a workbench pane', (p) => mine(p).widgets.find((w) => w.id === ID)?.place === 'cell' && mine(p).picked[pane] === ID && shown(view(p, `wg|cell|demo|${ID}`)), 30000);
  await T.snap('panels-cell');
  const cells = await T.act('panels-cells');
  t.check('panels: cell_ids() lists it for the workbench', cells.cells?.includes(ID), cells);

  // ── Approval: a command source asks first ──
  const GID = 'disk-gauge';
  const g = await mcp('widget_open', {
    alias: 'demo', id: GID, title: 'Disk gauge', slot: 'tab',
    html: '<div style="padding:12px">Disk: <b data-bind="disk" data-empty="waiting"></b></div>',
    manifest: { sources: { disk: { kind: 'command', run: 'echo 42%', every: 30 } } },
  });
  t.check('panels: a panel that needs approval', g.ok !== false, g);
  await T.ui('tab', 'panels');
  await T.act('panels-select', GID);
  const GK = `wg|tab|demo|${GID}`;
  if (web) {
    await T.until('panels: its approval banner and "approve" badge', (p) => mine(p).widgets.find((w) => w.id === GID)?.needsGrant && view(p, GK)?.grant && view(p, GK)?.grantItems > 0, 30000);
    await T.snap('panels-grant');
    await T.act('panels-approve', GK);
    await T.until('panels: approved', (p) => String(p.toast?.text || p.toast || '').includes('Approved') && !mine(p).widgets.find((w) => w.id === GID)?.needsGrant, 20000);
    await T.until('panels: the banner goes once it loads again', (p) => view(p, GK)?.grant === false && view(p, GK)?.state === 'ready');

    // ── The inline card (as the chat log draws it under widget_open, slot inline) ──
    await T.act('panels-card', 'demo', ID, 'chat-x');
    const IK = `wg|inline:chat-x|demo|${ID}`;
    await T.until('panels: the inline card sizes to its content', (p) => view(p, IK)?.painted && view(p, IK)?.measured && view(p, IK)?.height > 0, 30000);
    await T.act('panels-card', '', '', '');
  } else {
    await T.until('panels: its "approve" badge', (p) => mine(p).widgets.find((w) => w.id === GID)?.needsGrant, 30000);
  }

  // ── Close, reopen, delete ──
  await T.act('panels-select', ID);
  await T.act('panels-close', ID);
  await T.until('panels: closed', (p) => mine(p).widgets.find((w) => w.id === ID)?.state === 'closed');
  await T.act('panels-reopen', ID);
  await T.until('panels: reopened', (p) => mine(p).widgets.find((w) => w.id === ID)?.state === 'open');
  await T.act('panels-delete', GID);
  await T.until('panels: Delete asks first', (p) => p.dialog && String(p.dialog.message || '').includes('Disk gauge'));
  await T.dialog(true);
  await T.until('panels: deleted', (p) => !mine(p).widgets.some((w) => w.id === GID));
}
