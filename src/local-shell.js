/**
 * local-shell.js — "Reveal in Explorer" and "Open with its default program".
 *
 * These act on the machine the SERVER runs on. They only make sense when that
 * is also the machine the person is sitting at: the desktop app with its own
 * server, or a browser on the same computer. On a remote server they would
 * open a window on a screen nobody is looking at, so they are refused there
 * and the page is never told they exist.
 */

import { spawn } from 'node:child_process';
import { dirname } from 'node:path';

/** Does this machine have a desktop to open a window on? */
export function hasDesktop(platform = process.platform, env = process.env) {
  if (platform === 'win32' || platform === 'darwin') return true;
  return !!(env.DISPLAY || env.WAYLAND_DISPLAY);
}

/**
 * Is the request from this same machine?
 *
 * The socket address alone is not enough: a Cloudflare tunnel, or any reverse
 * proxy on the box, connects from loopback on behalf of someone far away.
 * Those add forwarding headers, and a request carrying one is not local.
 */
export function isLocalRequest(req) {
  const a = String((req.socket && req.socket.remoteAddress) || '');
  const loopback = a === '127.0.0.1' || a === '::1' || a === '::ffff:127.0.0.1';
  if (!loopback) return false;
  const h = req.headers || {};
  for (const name of ['x-forwarded-for', 'x-real-ip', 'cf-connecting-ip', 'forwarded', 'x-forwarded-host']) if (h[name]) return false;
  return true;
}

export function isLocalDesktopRequest(req) { return hasDesktop() && isLocalRequest(req); }

/** What the file manager is called here, for the menu. */
export function fileManagerName(platform = process.platform) {
  return platform === 'win32' ? 'Explorer' : platform === 'darwin' ? 'Finder' : 'file manager';
}

/**
 * The command for an action, as [program, args]. No shell is involved: the
 * path is one argument, whatever characters it holds.
 * @param {'reveal'|'open'} act
 */
export function shellCommand(act, path, { platform = process.platform, isDir = false } = {}) {
  if (platform === 'win32') {
    // explorer /select,<path> shows the item selected in its folder;
    // explorer <path> opens a folder, or a file with its default program.
    return act === 'reveal' && !isDir ? ['explorer.exe', ['/select,' + path]] : ['explorer.exe', [path]];
  }
  if (platform === 'darwin') return act === 'reveal' ? ['open', ['-R', path]] : ['open', [path]];
  // Linux has no portable "select this file": open the folder it is in.
  return ['xdg-open', [act === 'reveal' && !isDir ? dirname(path) : path]];
}

/** Run it, detached: the window outlives the request and is not ours to wait for. */
export function runShell(act, path, opts = {}) {
  const [bin, args] = shellCommand(act, path, opts);
  return new Promise((resolve) => {
    let done = false;
    const finish = (r) => { if (!done) { done = true; resolve(r); } };
    try {
      const child = spawn(bin, args, { detached: true, stdio: 'ignore', windowsHide: false });
      child.on('error', (err) => finish({ ok: false, error: err.code === 'ENOENT' ? `${bin} is not available on this machine` : err.message }));
      child.unref();
      // explorer.exe exits 1 even when it worked, so success is "it started".
      setTimeout(() => finish({ ok: true }), 150);
    } catch (err) { finish({ ok: false, error: err.message }); }
  });
}
