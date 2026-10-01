#!/usr/bin/env node
/**
 * test-update-channels.mjs — production / dev update channels.
 *
 * Version order is the whole mechanism: 1.2.0-dev.1 < 1.2.0-dev.2 < 1.2.0-dev.10
 * < 1.2.0 < 1.2.1-dev.1. Get it right and "a newer production release is
 * always offered over a dev build" and "leaving dev waits for a newer
 * production release" both follow without special cases.
 *
 * Checks the server's isNewer, the desktop app's copy in app/main.js (they must
 * agree), and the server's release choice per channel against a fake GitHub.
 *
 * Run: node scripts/test-update-channels.mjs
 */

import { mkdtempSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dataDir = mkdtempSync(join(tmpdir(), 'chan-test-'));
process.env.DATA_DIR = dataDir;

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const su = await import('../src/server-update.js');

// The desktop app's copy, lifted out of app/main.js (it imports electron).
const mainSrc = readFileSync(join(root, 'app', 'main.js'), 'utf8').split(/\r?\n/).join('\n');
const fn = mainSrc.match(/function versionGt\(a, b\) \{[^]*?\n\}/);
if (!fn) { console.log('FAIL  could not find versionGt in app/main.js'); process.exit(1); }
const versionGt = new Function(fn[0] + '\nreturn versionGt;')();

const ORDER = ['1.19.8', '1.19.9-dev.1', '1.19.9-dev.2', '1.19.9-dev.10', '1.19.9', '1.19.10-dev.1', '1.20.0'];
for (let i = 0; i < ORDER.length; i++) {
  for (let j = 0; j < ORDER.length; j++) {
    const want = i > j;
    if (su.isNewer(ORDER[i], ORDER[j]) !== want) ok(false, `server: isNewer(${ORDER[i]}, ${ORDER[j]}) should be ${want}`);
    if (versionGt(ORDER[i], ORDER[j]) !== want) ok(false, `desktop: versionGt(${ORDER[i]}, ${ORDER[j]}) should be ${want}`);
  }
}
ok(true, 'server and desktop agree on the order ' + ORDER.join(' < '));
ok(su.isNewer('v1.19.9', '1.19.9-dev.3') && versionGt('v1.19.9', '1.19.9-dev.3'), 'a leading v is ignored');

// ─── release choice against a fake GitHub ───
const rel = (tag, prerelease, extra = {}) => ({
  tag_name: tag, prerelease, draft: false, html_url: 'https://example/' + tag, body: '',
  assets: [{ name: `crundi-server-linux-${process.arch === 'arm64' ? 'arm64' : 'x64'}-${tag.slice(1)}.tar.gz`, browser_download_url: 'https://example/dl/' + tag }],
  ...extra,
});
let LIST = [], LATEST = null, calls = [];
globalThis.fetch = async (url) => {
  calls.push(String(url));
  const body = /releases\/latest$/.test(url) ? LATEST : LIST;
  return { ok: true, status: 200, json: async () => body };
};

// Production follows "latest", whatever prereleases exist.
LATEST = rel('v1.19.8', false);
LIST = [rel('v1.19.9-dev.2', true), rel('v1.19.9-dev.1', true), rel('v1.19.8', false)];
ok(su.getChannel() === 'production', 'the default channel is production');
await su.check({ force: true });
ok(su.status().latest === '1.19.8' && su.status().latestKind === 'production' && /releases\/latest$/.test(calls.at(-1)), 'production offers the latest production release and asks GitHub for "latest"');

// Dev picks the highest of either kind.
su.setChannel('dev');
await su.check({ force: true });
ok(su.getChannel() === 'dev' && /releases\?per_page/.test(calls.at(-1)), 'dev lists releases rather than asking for "latest"');
ok(su.status().latest === '1.19.9-dev.2' && su.status().latestKind === 'dev', 'dev offers the newest dev build: ' + su.status().latest);

// A production release newer than every dev build wins on dev.
LIST = [rel('v1.19.9', false), rel('v1.19.9-dev.2', true), rel('v1.19.9-dev.1', true)];
await new Promise(r => setTimeout(r, 5100));   // the forced-check floor
await su.check({ force: true });
ok(su.status().latest === '1.19.9' && su.status().latestKind === 'production', 'on dev, a newer production release is what is offered');

// Drafts are never offered.
LIST = [rel('v1.20.0', false, { draft: true }), rel('v1.19.9', false)];
await new Promise(r => setTimeout(r, 5100));
await su.check({ force: true });
ok(su.status().latest === '1.19.9', 'a draft release is never offered');

// The channel survives a restart (it is on disk).
ok(JSON.parse(readFileSync(join(dataDir, 'update-channel.json'), 'utf-8')).channel === 'dev', 'the channel is stored on disk');
su.setChannel('production');
ok(su.getChannel() === 'production', 'switching back to production is stored');

rmSync(dataDir, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} channel check(s) failed`); process.exit(1); }
console.log('\nAll update-channel checks passed.');
