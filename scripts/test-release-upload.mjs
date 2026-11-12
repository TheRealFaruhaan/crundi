#!/usr/bin/env node
// scripts/release-upload.mjs: a release's assets are checked against the
// files that were built (exists, "uploaded", same size) and the broken ones
// uploaded again. A fake gh plays GitHub, including dev.56's failure: a .deb
// cut short and files that never arrived while the upload "succeeded".
//
// Run: node scripts/test-release-upload.mjs

import { mkdtempSync, writeFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, basename } from 'node:path';
import { uploadAndVerify, problems } from './release-upload.mjs';

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : '  ' + String(extra).slice(0, 300)}`); if (!ok) failed++; };

const dir = mkdtempSync(join(tmpdir(), 'crundi-relup-'));
const mk = (name, size) => { const f = join(dir, name); writeFileSync(f, Buffer.alloc(size, 1)); return f; };
const deb = mk('Crundi-1.0.0-linux-x64.deb', 3700);
const cdeb = mk('Crundi-Client-1.0.0-linux-x64.deb', 850);
const yml = mk('latest-linux.yml', 37);
const files = [deb, cdeb, yml];

// A fake GitHub: a release with assets; uploads can be told to cut short or vanish.
function fakeGh({ tag = 'v1.0.0', cut = {}, vanish = {}, failUpload = {} } = {}) {
  const assets = {};
  const calls = { uploads: [] };
  const gh = (args) => {
    if (args[0] === 'api') return JSON.stringify([{ tag_name: 'v0.9.0', assets: [] }, { tag_name: tag, draft: false, assets: Object.entries(assets).map(([name, a]) => ({ name, ...a })) }]);
    if (args[0] === 'release' && args[1] === 'upload') {
      const f = args[3]; const name = basename(f);
      calls.uploads.push(name);
      if ((failUpload[name] || 0) > 0) { failUpload[name]--; const e = new Error('upload failed'); e.stderr = 'HTTP 502'; throw e; }
      if ((vanish[name] || 0) > 0) { vanish[name]--; return ''; }   // "succeeds", nothing arrives
      const full = statSync(f).size;
      if ((cut[name] || 0) > 0) { cut[name]--; assets[name] = { size: Math.floor(full * 0.58), state: 'uploaded' }; return ''; }
      assets[name] = { size: full, state: 'uploaded' };
      return '';
    }
    throw new Error('unexpected gh ' + args.join(' '));
  };
  return { gh, assets, calls };
}
const quiet = { log: () => {}, wait: async () => {} };

try {
  // All good.
  const a = fakeGh();
  const r1 = await uploadAndVerify({ gh: a.gh, repo: 'o/r', tag: 'v1.0.0', files, upload: true, ...quiet });
  check('everything uploaded whole: verified', r1.ok && a.calls.uploads.length === 3, JSON.stringify(r1.report));

  // dev.56: the .deb cut short, two files that never arrived; all "succeed".
  const b = fakeGh({ cut: { 'Crundi-1.0.0-linux-x64.deb': 1 }, vanish: { 'Crundi-Client-1.0.0-linux-x64.deb': 1, 'latest-linux.yml': 1 } });
  const r2 = await uploadAndVerify({ gh: b.gh, repo: 'o/r', tag: 'v1.0.0', files, upload: true, ...quiet });
  check('a truncated or missing asset is found and uploaded again, then verified', r2.ok && b.calls.uploads.length === 6 && r2.report.some((l) => /2146 bytes on the release, 3700 here/.test(l)) && r2.report.some((l) => /latest-linux\.yml: missing/.test(l)), JSON.stringify(r2.report));

  // One that keeps arriving short: the job fails, naming it.
  const c = fakeGh({ cut: { 'Crundi-1.0.0-linux-x64.deb': 99 } });
  const r3 = await uploadAndVerify({ gh: c.gh, repo: 'o/r', tag: 'v1.0.0', files, upload: true, ...quiet });
  check('still short after three tries: it fails and says which', !r3.ok && r3.bad.length === 1 && r3.bad[0].name === 'Crundi-1.0.0-linux-x64.deb' && /FAILED: Crundi-1\.0\.0-linux-x64\.deb/.test(r3.report.at(-1)), JSON.stringify(r3.report));

  // An upload command that errors is retried.
  const d = fakeGh({ failUpload: { 'latest-linux.yml': 2 } });
  const r4 = await uploadAndVerify({ gh: d.gh, repo: 'o/r', tag: 'v1.0.0', files, upload: true, ...quiet });
  check('an upload that errors is tried again', r4.ok && d.calls.uploads.filter((n) => n === 'latest-linux.yml').length === 3, JSON.stringify(r4.report));

  // Verify only (electron-builder uploaded them): a short one is put right.
  const e = fakeGh();
  await uploadAndVerify({ gh: e.gh, repo: 'o/r', tag: 'v1.0.0', files, upload: true, ...quiet });
  e.assets['latest-linux.yml'].size = 5;
  e.calls.uploads.length = 0;
  const r5 = await uploadAndVerify({ gh: e.gh, repo: 'o/r', tag: 'v1.0.0', files, upload: false, ...quiet });
  check('verify only: a wrong size is uploaded again, nothing else is', r5.ok && e.calls.uploads.join() === 'latest-linux.yml', JSON.stringify(e.calls.uploads));

  // No release, a file that was never built, an asset still uploading.
  const r6 = await uploadAndVerify({ gh: fakeGh({ tag: 'v9' }).gh, repo: 'o/r', tag: 'v1.0.0', files, ...quiet });
  check('no release for the tag: fails', !r6.ok && /no release/.test(r6.report.join()));
  const r7 = await uploadAndVerify({ gh: fakeGh().gh, repo: 'o/r', tag: 'v1.0.0', files: [join(dir, 'nope.exe')], upload: true, ...quiet });
  check('a file that was not built: fails before uploading anything', !r7.ok && /not built/.test(r7.report.join()));
  check('an asset not yet "uploaded" counts as wrong', problems([yml], { 'latest-linux.yml': { size: 37, state: 'starter' } })[0].why === 'state starter');
} finally {
  rmSync(dir, { recursive: true, force: true });
}

console.log(failed ? `\n${failed} failure(s).` : '\nAll passed.');
process.exit(failed ? 1 : 0);
