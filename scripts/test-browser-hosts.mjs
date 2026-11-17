#!/usr/bin/env node
// src/browser.js with more than one desktop app attached as a native host.
// Fake hosts answer every request and record what reached them.
//
// Covers: two hosts listed; new pages going to the host used last (the latest
// to connect, or the one that said it was used); on: "desktop:<device>"; a
// page staying with the host that opened it; one host leaving drops only its
// own pages and requests, the other keeps working; drawer tabs per host; a
// single older host (no name, no version) behaving as before.
//
// Run: node scripts/test-browser-hosts.mjs

const b = await import('../src/browser.js');

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : '  ' + String(extra).slice(0, 400)}`); if (!ok) failed++; };

/** A desktop app: answers each request, unless told to hold them. */
function fakeHost(label, { capabilities = ['notify', 'browser-drawer', 'browser-mouse', 'browser-touch'], hold = false } = {}) {
  const got = [];
  const host = { label, got, hold, id: null, tabs: [] };
  host.send = (m) => {
    got.push(m);
    if (!m.requestId || host.hold) return;
    const reply = { type: 'browserResult', requestId: m.requestId, ok: true };
    if (m.type === 'browserDrawerOpen') { const tabId = `${label}-t${host.tabs.length + 1}`; host.tabs.push(tabId); Object.assign(reply, { tabId, network: m.network, url: m.url }); }
    if (m.type === 'browserDrawerList') reply.tabs = host.tabs.map(t => ({ tabId: t, project: 'p', title: t, url: 'about:blank', network: 'client' }));
    if (m.type === 'browserNavigate') reply.url = m.url;
    setImmediate(() => b.handleHostMessage(reply, host.id));
  };
  host.attach = (extra = {}) => {
    host.detach = b.setRemoteHost({ send: host.send, capabilities, ...extra });
    host.id = host.detach.hostId;
    return host;
  };
  host.asked = (type) => got.filter(m => m.type === type);
  return host;
}

// ── One older host: as before ──
{
  const old = fakeHost('old', { capabilities: [] }).attach();
  check('one host: it is the desktop', b.browserPlaces().desktop && b.hasNativeHost());
  const r = await b.openBrowser({ alias: 'p', name: 'a', url: 'https://a.test', on: 'desktop' });
  check('one older host: a page opens on it', r.ok && r.on === 'desktop' && old.asked('browserOpen').length === 1, JSON.stringify(r));
  const n = await b.navigateBrowser(r.key, 'https://b.test');
  check('one older host: later calls go to it', n.ok && old.asked('browserNavigate').length === 1, JSON.stringify(n));
  const d = await b.openBrowser({ alias: 'p', url: 'x', on: 'drawer' });
  check('one older host without the drawer: says the app needs updating', !d.ok && /not supported by the Crundi desktop app/.test(d.error), JSON.stringify(d));
  old.detach();
  check('one host leaves: its page goes, no desktop left', !b.listBrowsers('p').length && !b.browserPlaces().desktop);
}

// ── Two hosts ──
const laptop = fakeHost('laptop').attach({ deviceId: 'dev-laptop', name: 'Laptop', kind: 'electron', version: '1.21.8' });
await new Promise(r => setTimeout(r, 5));
const desk = fakeHost('desk').attach({ deviceId: 'dev-desk', name: 'Desk PC', kind: 'native', version: '2.0.0' });

{
  const hosts = b.listHosts();
  check('two hosts: both listed with device, name, kind, version and since when', hosts.length === 2
    && hosts.some(h => h.id === 'dev-laptop' && h.name === 'Laptop' && h.kind === 'electron' && h.version === '1.21.8' && h.connectedSince)
    && hosts.some(h => h.id === 'dev-desk' && h.name === 'Desk PC' && h.kind === 'native'), JSON.stringify(hosts));
  check('two hosts: the latest to connect is preferred', hosts.find(h => h.preferred)?.id === 'dev-desk');
  const list = await b.listAllBrowsers('p');
  check('browser_list: shows each host', Array.isArray(list.hosts) && list.hosts.length === 2 && /desktop:/.test(list.hostHint || ''), JSON.stringify(list));
}

// Routing to the last used.
const p1 = await b.openBrowser({ alias: 'p', name: 'one', url: 'https://1.test' });
check('a new page goes to the host used last (the desk)', p1.ok && p1.host === 'dev-desk' && desk.asked('browserOpen').length === 1 && !laptop.asked('browserOpen').length, JSON.stringify(p1));
b.markHostUsed('dev-laptop');
check('the person uses the laptop: it is preferred now', b.listHosts().find(h => h.preferred)?.id === 'dev-laptop');
const p2 = await b.openBrowser({ alias: 'p', name: 'two', url: 'https://2.test', on: 'desktop' });
check('the next new page goes to the laptop', p2.ok && p2.host === 'dev-laptop' && laptop.asked('browserOpen').length === 1, JSON.stringify(p2));
const p3 = await b.openBrowser({ alias: 'p', name: 'three', url: 'https://3.test', on: 'desktop:Desk PC' });
check('on: "desktop:<name>" picks that host', p3.ok && p3.host === 'dev-desk' && desk.asked('browserOpen').length === 2, JSON.stringify(p3));
const p4 = await b.openBrowser({ alias: 'p', name: 'four', url: 'x', on: 'desktop:nope' });
check('on: "desktop:<unknown>" says which are connected', !p4.ok && /dev-laptop/.test(p4.error) && /dev-desk/.test(p4.error), JSON.stringify(p4));

// A page stays with its host.
{
  const before = desk.asked('browserNavigate').length;
  const n = await b.navigateBrowser(p1.key, 'https://1b.test');
  check('a page stays with the host that opened it, though another is preferred now', n.ok && desk.asked('browserNavigate').length === before + 1 && !laptop.asked('browserNavigate').length, JSON.stringify(n));
  const listed = b.listBrowsers('p');
  check('browser_list: each page says its host', listed.find(x => x.key === p1.key)?.host === 'dev-desk' && listed.find(x => x.key === p2.key)?.host === 'dev-laptop', JSON.stringify(listed));
}

// Drawer tabs on both.
{
  const t1 = await b.openBrowser({ alias: 'p', url: 'about:blank', on: 'drawer' });
  check('a drawer tab goes to the host used last (the laptop)', t1.ok && t1.host === 'dev-laptop' && laptop.asked('browserDrawerOpen').length === 1, JSON.stringify(t1));
  const t2 = await b.openDrawerTab({ alias: 'p', url: 'about:blank', device: 'dev-desk' });
  check('browser_drawer_open device: picks the desk', t2.ok && t2.host === 'dev-desk', JSON.stringify(t2));
  const tabs = await b.listDrawerTabs('p');
  check('drawer tabs: listed from both, each with its host', tabs.length === 2 && tabs.some(t => t.host === 'dev-laptop') && tabs.some(t => t.host === 'dev-desk'), JSON.stringify(tabs));
  const before = desk.asked('browserNavigate').length;
  await b.navigateBrowser(t2.key, 'https://d.test');
  check('a drawer tab stays with its host', desk.asked('browserNavigate').length === before + 1);
  check('the homepool device of a tab is its own host\'s', b.drawerDeviceId(t2.key) === 'dev-desk' && b.drawerDeviceId(t1.key) === 'dev-laptop' && b.drawerDeviceId() === 'dev-laptop');
}

// One host leaves; the other keeps working.
{
  desk.hold = true;   // a request in flight on the desk when it goes
  const inFlight = b.navigateBrowser(p3.key, 'https://never.test');
  const laptopNav = laptop.asked('browserNavigate').length;
  laptop.hold = true;
  const otherInFlight = b.navigateBrowser(p2.key, 'https://later.test');
  await new Promise(r => setTimeout(r, 5));
  desk.detach();
  const r = await inFlight;
  check('the desk leaves: its request in flight fails at once', !r.ok && /disconnected/.test(r.error), JSON.stringify(r));
  const keys = b.listBrowsers('p').map(x => x.key);
  check('the desk leaves: only its pages go', !keys.includes(p1.key) && !keys.includes(p3.key) && keys.includes(p2.key), JSON.stringify(keys));
  // The laptop's own request in flight is not failed by the desk leaving.
  const pend = laptop.got.filter(m => m.type === 'browserNavigate').slice(-1)[0];
  b.handleHostMessage({ type: 'browserResult', requestId: pend.requestId, ok: true, url: 'https://later.test' }, laptop.id);
  const o = await otherInFlight;
  check('the laptop\'s request in flight still completes', o.ok, JSON.stringify(o));
  laptop.hold = false;
  const n = await b.navigateBrowser(p2.key, 'https://2b.test');
  check('the laptop keeps working', n.ok && laptop.asked('browserNavigate').length === laptopNav + 2, JSON.stringify(n));
  const p5 = await b.openBrowser({ alias: 'p', name: 'five', url: 'https://5.test', on: 'desktop' });
  check('new pages go to the one left', p5.ok && p5.host === 'dev-laptop', JSON.stringify(p5));
  const gone = await b.navigateBrowser(p1.key, 'x');
  check('a page of the host that left is gone', !gone.ok, JSON.stringify(gone));
  check('one host left: listed alone, preferred', b.listHosts().length === 1 && b.listHosts()[0].preferred);
}

// The same PC reconnecting while its old socket lingers: both stay apart.
{
  const again = fakeHost('again').attach({ deviceId: 'dev-laptop' });
  check('the same device again: a separate id, preferred as the newest', again.id !== 'dev-laptop' && b.listHosts().find(h => h.preferred)?.id === again.id, again.id);
  again.detach();
  check('its old self keeps its pages', b.listBrowsers('p').some(x => x.host === 'dev-laptop'));
}

// A tab the person closed on one PC does not drop another PC's page.
{
  const other = fakeHost('other').attach({ deviceId: 'dev-other' });
  const keep = b.listBrowsers('p').find(x => x.host === 'dev-laptop').key;
  b.handleHostMessage({ type: 'browserClosed', key: keep }, other.id);
  check('a close from another host is ignored', b.listBrowsers('p').some(x => x.key === keep));
  b.handleHostMessage({ type: 'browserClosed', key: keep }, laptop.id);
  check('a close from its own host drops it', !b.listBrowsers('p').some(x => x.key === keep));
  other.detach();
}

laptop.detach();
check('all gone: no desktop', !b.browserPlaces().desktop && !b.listHosts().length);
for (const x of b.listBrowsers()) { try { await b.closeBrowser(x.key); } catch { /* none */ } }

console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
