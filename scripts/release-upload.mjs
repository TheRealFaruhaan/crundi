#!/usr/bin/env node
// Upload release assets and make sure they arrived whole.
//
//   node scripts/release-upload.mjs --tag v1.2.3 [--repo owner/name] [--upload] FILE...
//
// With --upload every file is uploaded first (gh release upload --clobber).
// Then, either way, each file is checked against the release: the asset of
// the same name must exist, be in state "uploaded", and have exactly the
// local file's size. A missing or short one is uploaded again, up to three
// times, and the job fails if any still does not match.
//
// Why: 1.21.6-dev.56's attach step reported success with the Linux .deb on
// the release cut to 217 of its 370 MB and three files never uploaded
// (find | xargs gh release upload, whose failures did not fail the step).
// The updater then offers files that are broken or are not there.
//
// Needs the gh CLI with GH_TOKEN (every GitHub runner has gh). Finds the
// release by tag among all releases, drafts included (electron-builder can
// leave a draft).

import { execFileSync } from 'node:child_process';
import { statSync, existsSync } from 'node:fs';
import { basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const TRIES = 3;

/** The default gh runner: gh with these arguments, its stdout as text. */
export function ghRun(args) {
  return execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 64 * 1024 * 1024 });
}

/** The release's assets by name: { name: { size, state } }, or null when the tag has no release. */
export function assetsOf({ gh, repo, tag }) {
  const list = JSON.parse(gh(['api', `repos/${repo}/releases?per_page=50`]));
  const rel = list.find((r) => r.tag_name === tag);
  if (!rel) return null;
  const out = {};
  for (const a of rel.assets || []) out[a.name] = { size: a.size, state: a.state };
  return out;
}

/** What is wrong with each file on the release ([] when all are whole). */
export function problems(files, assets) {
  const out = [];
  for (const f of files) {
    const name = basename(f);
    const want = statSync(f).size;
    const a = assets && assets[name];
    if (!a) out.push({ file: f, name, why: 'missing' });
    else if (a.state !== 'uploaded') out.push({ file: f, name, why: `state ${a.state}` });
    else if (a.size !== want) out.push({ file: f, name, why: `${a.size} bytes on the release, ${want} here` });
  }
  return out;
}

/**
 * Upload (optionally) and verify. Returns { ok, report: [lines], bad: [...] }.
 * opts: { gh, repo, tag, files, upload, log, wait(ms) }
 */
export async function uploadAndVerify({ gh = ghRun, repo, tag, files, upload = false, log = console.log, wait = (ms) => new Promise((r) => setTimeout(r, ms)) }) {
  const report = [];
  const say = (m) => { report.push(m); log(m); };
  const missingLocal = files.filter((f) => !existsSync(f));
  if (missingLocal.length) { say(`not built: ${missingLocal.join(', ')}`); return { ok: false, report, bad: missingLocal.map((f) => ({ file: f, why: 'not built' })) }; }
  const put = (f) => {
    for (let i = 1; i <= TRIES; i++) {
      try { gh(['release', 'upload', tag, f, '--clobber', '--repo', repo]); return true; }
      catch (err) { say(`upload ${basename(f)} failed (try ${i}): ${String(err.stderr || err.message).trim().split('\n').pop()}`); }
    }
    return false;
  };
  if (upload) for (const f of files) { if (put(f)) say(`uploaded ${basename(f)} (${statSync(f).size} bytes)`); }
  let bad = [];
  for (let round = 1; round <= TRIES; round++) {
    const assets = assetsOf({ gh, repo, tag });
    if (!assets) { say(`no release for ${tag}`); return { ok: false, report, bad: files.map((f) => ({ file: f, why: 'no release' })) }; }
    bad = problems(files, assets);
    if (!bad.length) { say(`verified: ${files.length} file(s) on ${tag}, every size matches`); return { ok: true, report, bad: [] }; }
    for (const b of bad) say(`${b.name}: ${b.why}${round < TRIES ? ', uploading it again' : ''}`);
    if (round === TRIES) break;
    for (const b of bad) put(b.file);
    await wait(3000 * round);
  }
  say(`FAILED: ${bad.map((b) => `${b.name} (${b.why})`).join('; ')}`);
  return { ok: false, report, bad };
}

// ─── command line ───
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const argv = process.argv.slice(2);
  let tag = '', repo = process.env.GITHUB_REPOSITORY || '', upload = false; const files = [];
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--tag') tag = argv[++i];
    else if (argv[i] === '--repo') repo = argv[++i];
    else if (argv[i] === '--upload') upload = true;
    else files.push(argv[i]);
  }
  if (!tag || !repo || !files.length) { console.error('usage: release-upload.mjs --tag TAG [--repo owner/name] [--upload] FILE...'); process.exit(2); }
  const r = await uploadAndVerify({ repo, tag, files, upload });
  process.exit(r.ok ? 0 : 1);
}
