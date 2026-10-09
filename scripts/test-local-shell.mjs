#!/usr/bin/env node
// "Reveal in Explorer" and "Open externally": where they are offered, and what they run.
import { hasDesktop, isLocalRequest, shellCommand, fileManagerName } from '../src/local-shell.js';

let failed = 0;
const ok = (cond, name, detail = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`); if (!cond) failed++; };
const req = (addr, headers = {}) => ({ socket: { remoteAddress: addr }, headers });
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);

ok(isLocalRequest(req('127.0.0.1')) && isLocalRequest(req('::1')) && isLocalRequest(req('::ffff:127.0.0.1')), 'a request from this machine is local');
ok(!isLocalRequest(req('203.0.113.9')), 'a request from another machine is not');
ok(!isLocalRequest(req('127.0.0.1', { 'cf-connecting-ip': '203.0.113.9' })), 'a tunnel connecting from loopback on someone else\'s behalf is not local');
ok(!isLocalRequest(req('127.0.0.1', { 'x-forwarded-for': '203.0.113.9' })), 'nor is a reverse proxy on the same box');
ok(!isLocalRequest(req('')) && !isLocalRequest({ headers: {} }), 'an unknown address is not local');

ok(hasDesktop('win32', {}) && hasDesktop('darwin', {}), 'Windows and macOS have a desktop');
ok(!hasDesktop('linux', {}) && hasDesktop('linux', { DISPLAY: ':0' }) && hasDesktop('linux', { WAYLAND_DISPLAY: 'wayland-0' }), 'Linux only with a display: a headless server offers neither action');

const W = 'C:\\Users\\me\\proj\\a & b (1).txt';
ok(eq(shellCommand('reveal', W, { platform: 'win32' }), ['explorer.exe', ['/select,' + W]]), 'Windows reveal: explorer /select, with the path as ONE argument, spaces and & and all');
ok(eq(shellCommand('open', W, { platform: 'win32' }), ['explorer.exe', [W]]), 'Windows open: explorer <path> hands the file to its default program');
ok(eq(shellCommand('reveal', 'C:\\proj\\src', { platform: 'win32', isDir: true }), ['explorer.exe', ['C:\\proj\\src']]), 'Windows reveal of a folder opens the folder');
ok(eq(shellCommand('reveal', '/Users/me/a b.txt', { platform: 'darwin' }), ['open', ['-R', '/Users/me/a b.txt']]), 'macOS reveal: open -R');
ok(eq(shellCommand('open', '/Users/me/a b.txt', { platform: 'darwin' }), ['open', ['/Users/me/a b.txt']]), 'macOS open');
ok(eq(shellCommand('reveal', '/home/me/proj/a.txt', { platform: 'linux' }), ['xdg-open', ['/home/me/proj']]), 'Linux reveal: opens the containing folder');
ok(eq(shellCommand('open', '/home/me/proj/a.txt', { platform: 'linux' }), ['xdg-open', ['/home/me/proj/a.txt']]), 'Linux open');
ok(fileManagerName('win32') === 'Explorer' && fileManagerName('darwin') === 'Finder', 'the menu names the right file manager');

console.log(failed ? `\n${failed} FAILED` : '\nAll local-shell checks passed.');
process.exit(failed ? 1 : 0);
