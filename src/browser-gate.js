/**
 * browser-gate.js — what a chat may do with the browser drawer's homepool tabs.
 *
 * A drawer tab on a homepool network goes out through the owner's PC browser
 * key. A chat uses such tabs only once he approved it (a homepool request of
 * kind "browser"); opening or switching a tab onto one asks him first and
 * waits for his answer. A network the drawer's own PC is a node of is never
 * offered (the tab would leave through that PC). Collaborators' chats never
 * reach the drawer at all.
 *
 * gateBrowserCall() runs before every browser_* MCP call. It may rewrite the
 * arguments (resolve "homepool" to a network, set _hpAllowed) and returns
 * null to go on, or an { ok: false, error } to answer with.
 */

export async function gateBrowserCall({ tool, args: a, collaborator = false, homepool, browser }) {
  // Never the caller's to set.
  delete a._hpAllowed; delete a._noDrawer; delete a._cru;
  const ident = { sessionId: a.sessionId, project: a.alias, chatToken: a.chatToken };
  delete a.chatToken;
  if (collaborator || !homepool) return null;

  const onto = (tool === 'browser_open' || tool === 'browser_drawer_open' || tool === 'browser_tab_network') && /^homepool(:|$)/i.test(String(a.network || ''));
  if (onto && String(a.network).toLowerCase() === 'homepool') {
    const k = homepool.store.browserKey();
    const first = k && k.networks.find(n => n !== 'server');
    if (!first) return { ok: false, error: 'The owner has not set up homepool for the PC browser yet (browser drawer, network menu, Homepool).' };
    a.network = 'homepool:' + first;
  }
  if (onto) {
    const net = String(a.network).split(':')[1] || '';
    if (homepool.isSelfNode(browser.drawerDeviceId(), net)) {
      return { ok: false, error: `The PC the drawer runs on is itself a node of homepool network "${net}", so its tabs are not offered it (they would leave through that PC). Use another network.` };
    }
  }
  let allowed = homepool.browserAllows(ident).ok;
  if (onto && !allowed) {
    // Nobody to browse with: say so now rather than wait on an approval.
    const avail = browser.drawerAvailability();
    if (!avail.ok) return { ok: false, error: avail.error };
    const asked = await homepool.browserAsk({ ...ident, network: String(a.network).split(':')[1] || '', reason: a.reason });
    if (!asked.ok) return { ok: false, status: asked.status, error: asked.error };
    allowed = true;
  }
  a._hpAllowed = allowed;
  return null;
}
