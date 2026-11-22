#!/usr/bin/env node
// Release naming shared by the packagers (package/CONTRACT.md):
//
//   node native-v2/package/feed.mjs guid <appId>
//       electron-builder's APP_GUID (UUID v5 of the appId in its namespace):
//       the Windows uninstall key, so the new installer takes Electron's entry.
//   node native-v2/package/feed.mjs yml <installer> <out.yml> <version>
//       an update feed in electron-updater's format (version, files, sha512),
//       which Electron's updater and the native one both read.

import { createHash } from 'node:crypto';
import { createReadStream, statSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';

// electron-builder's namespace for appId GUIDs (app-builder-lib, ELECTRON_BUILDER_NS_UUID).
const NS = '50e065bc-3134-11e6-9bab-38c9862bdaf3';

export function guid(appId) {
  const ns = Buffer.from(NS.replace(/-/g, ''), 'hex');
  const h = createHash('sha1').update(Buffer.concat([ns, Buffer.from(appId, 'utf8')])).digest();
  h[6] = (h[6] & 0x0f) | 0x50;
  h[8] = (h[8] & 0x3f) | 0x80;
  const x = h.subarray(0, 16).toString('hex');
  return `${x.slice(0, 8)}-${x.slice(8, 12)}-${x.slice(12, 16)}-${x.slice(16, 20)}-${x.slice(20)}`;
}

export async function yml(file, out, version) {
  const sha512 = await new Promise((res, rej) => {
    const h = createHash('sha512');
    createReadStream(file).on('data', (d) => h.update(d)).on('end', () => res(h.digest('base64'))).on('error', rej);
  });
  const url = basename(file);
  const size = statSync(file).size;
  writeFileSync(out, `version: ${version}\nfiles:\n  - url: ${url}\n    sha512: ${sha512}\n    size: ${size}\npath: ${url}\nsha512: ${sha512}\nreleaseDate: '${new Date().toISOString()}'\n`);
}

const [cmd, ...a] = process.argv.slice(2);
if (cmd === 'guid') console.log(guid(a[0]));
else if (cmd === 'yml') await yml(a[0], a[1], a[2]);
else if (cmd) { console.error('usage: feed.mjs guid <appId> | yml <installer> <out.yml> <version>'); process.exit(2); }
