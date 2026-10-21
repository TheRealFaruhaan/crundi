// Writes what the server's own encoder (src/crundi-code.js) makes of a set of
// pairing links, for the app's Kotlin reader to be held to:
//
//   node android/tools/gen-code-fixtures.mjs     (from the repository root)
//
// Output: android/app/src/test/resources/crundi-code.json, used by
// CrundiCodeTest. Run again if crundi-code.js changes its format.
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as cc from '../../src/crundi-code.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const CODE = '0123456789abcdef01234567';
const links = [
  `https://dev.faruhaan.com/pair#${CODE}`,
  `https://crundi.example-team.dev:8443/pair#${CODE}`,
  `https://colgate-instances-moderate-spoken.trycloudflare.com/pair#${CODE}`,
  `https://a-very-long-subdomain-name.another-long-part.example-company.co.uk/pair#${CODE}`,
  `http://192.168.1.20:8888/pair#${'f'.repeat(24)}`,
  `https://myserver.tailnet-name.ts.net/pair#${'0'.repeat(24)}`,
  `https://x.ngrok-free.app/pair#abcdefabcdefabcdefabcdef`,
  `https://www.www.com/pair#${CODE}`,
];
const unencodable = [`https://${'x'.repeat(64)}/pair#${CODE}`, `https://[::1]/pair#${CODE}`, `https://Dev.Faruhaan.com/pair#${CODE}`, 'nonsense'];
const out = {
  codes: links.map((link) => { const e = cc.encode(link); return { link, size: e.cls.name, rings: e.rings.map((r) => Array.from(r).join('')) }; }),
  unencodable,
};
const file = join(root, 'android', 'app', 'src', 'test', 'resources', 'crundi-code.json');
writeFileSync(file, JSON.stringify(out, null, 1) + '\n');
console.log(`wrote ${out.codes.length} codes to ${file}`);
