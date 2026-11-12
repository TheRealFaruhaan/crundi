/**
 * drawer-net.js — the browser drawer's networks, in the desktop app.
 *
 *   client       this PC's own: no proxy
 *   server       the server's: with the server on this machine, also no proxy;
 *                with a remote server, a loopback proxy whose streams ride the
 *                native-host websocket (net-tunnel-client.js)
 *   hp:<net>     a homepool network: a profile of its own, whose proxy tags
 *                each stream "hp|<net>|" so the server opens it through the PC
 *                browser key on that network; over the websocket to a remote
 *                server, over the IPC channel to a bundled one
 *
 * All the proxies share one channel, so each takes stream ids in its own
 * range and frames are routed back by it. Nothing listens on anything but
 * 127.0.0.1, and every proxy wants its own per-launch password.
 *
 * deps:
 *   isRemote()             is the server on another machine?
 *   remoteUp()             is the native-host websocket open?
 *   remoteFeatures()       what that server offers (native-host-ready)
 *   sendRemote(buf)        a binary frame on that websocket
 *   localUp()              is the bundled server running?
 *   sendLocal(buf)         a frame to the bundled server (IPC)
 *   sessionFor(partition)  Electron session.fromPartition
 *   partitionOf(network)   the drawer's profile for a network
 *   log(line)
 */

import { createServerNetProxy, idRange } from './net-tunnel-client.js';

const HP = /^hp:([a-z0-9][a-z0-9_-]{0,39})$/;

export function createDrawerNet({ isRemote, remoteUp, remoteFeatures, sendRemote, localUp, sendLocal, sessionFor, partitionOf, log = () => {} }) {
  const byRange = new Map();   // idBase → proxy
  const hpProxies = new Map(); // "hp:<net>" → proxy
  let serverProxy = null;
  let serverConfigured = '';
  let nextBase = 1;

  function send(buf) {
    if (isRemote()) { if (!remoteUp()) throw new Error('not connected'); sendRemote(buf); return; }
    if (!localUp()) throw new Error('the server is not running');
    sendLocal(buf);
  }
  const up = (feature) => (isRemote() ? remoteUp() && remoteFeatures().includes(feature) : localUp());

  function info() {
    const remote = isRemote();
    const homepool = remote ? (remoteUp() ? (remoteFeatures().includes('browser-homepool') ? 'tunnel' : 'unsupported') : 'offline') : (localUp() ? 'tunnel' : 'offline');
    if (!remote) return { mode: 'local', status: 'direct', homepool };
    if (!remoteUp()) return { mode: 'remote', status: 'offline', homepool };
    if (!remoteFeatures().includes('net-tunnel')) return { mode: 'remote', status: 'unsupported', homepool };
    return { mode: 'remote', status: 'tunnel', homepool };
  }

  async function ensure(network = 'server') {
    if (network === 'server') {
      const ses = sessionFor(partitionOf('server'));
      if (!isRemote()) {
        if (serverConfigured !== 'direct') { await ses.setProxy({ mode: 'direct' }); serverConfigured = 'direct'; }
        return;
      }
      if (!serverProxy) {
        serverProxy = createServerNetProxy({ send, isConnected: () => up('net-tunnel'), log, idBase: 0 });
        byRange.set(0, serverProxy);
      }
      const port = await serverProxy.start();
      if (serverConfigured !== 'proxy:' + port) {
        // <-loopback> drops Chromium's built-in bypass, so localhost goes to the
        // server's localhost too: that is the point of this network.
        await ses.setProxy({ proxyRules: 'http://127.0.0.1:' + port, proxyBypassRules: '<-loopback>' });
        await ses.closeAllConnections?.();
        serverConfigured = 'proxy:' + port;
        log(`[server-net] drawer tabs on the server network use 127.0.0.1:${port}`);
      }
      return;
    }
    const m = HP.exec(String(network || ''));
    if (!m) return;
    let p = hpProxies.get(network);
    if (!p) {
      const idBase = nextBase++;
      p = createServerNetProxy({ send, isConnected: () => up('browser-homepool'), log, idBase, targetPrefix: `hp|${m[1]}|`, heading: `Homepool: ${m[1]}` });
      p.configured = '';
      hpProxies.set(network, p); byRange.set(idBase, p);
    }
    const port = await p.start();
    if (p.configured !== String(port)) {
      // No bypass at all: a homepool tab never leaves directly.
      await sessionFor(partitionOf(network)).setProxy({ proxyRules: 'http://127.0.0.1:' + port, proxyBypassRules: '<-loopback>' });
      p.configured = String(port);
      log(`[homepool] drawer tabs on ${m[1]} use 127.0.0.1:${port}`);
    }
  }

  /** A frame from the server: to the proxy whose stream it is. */
  function handleFrame(buf) { const p = byRange.get(idRange(buf)); if (p) p.handleFrame(buf); }
  /** The channel to the server went away: every stream on it is dead. */
  function dropped() { for (const p of byRange.values()) p.onDisconnect(); }
  /** Electron's 'login' event: answer only our own proxies' challenges. */
  function login(authInfo) {
    for (const p of byRange.values()) if (p.isOurs(authInfo)) return p.credentials;
    return null;
  }

  return { info, ensure, handleFrame, dropped, login };
}
