/**
 * Fetch the keyword spotter's engine and model on first use, into
 * <userData>/voice-wake/ (see manifest.js for what and why).
 *
 * Each archive is downloaded to a temporary file, its SHA-256 checked against
 * the pinned value, unpacked with the system's tar into a staging folder, and
 * only the listed files are moved into place, each checked again. A folder
 * counts as installed only once its .ok marker is written, so a download cut
 * short is simply tried again next time.
 *
 * tar: Windows 10 1803 and later ship bsdtar as %SystemRoot%\System32\tar.exe,
 * which reads both .tgz and .tar.bz2; Linux and macOS have tar with bzip2.
 */

import { createHash } from 'crypto';
import { execFile } from 'child_process';
import { createWriteStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync, copyFileSync } from 'fs';
import { dirname, join } from 'path';
import { ENGINE, MODEL } from './manifest.js';

const PARTS = [ENGINE, MODEL];

function okMarker(baseDir, part) { return join(baseDir, part.id, '.ok'); }

export function isInstalled(baseDir) {
  return PARTS.every((p) => {
    try { return readFileSync(okMarker(baseDir, p), 'utf8').trim() === p.sha256; } catch { return false; }
  });
}

function sha256File(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

function tarBin() {
  if (process.platform === 'win32') {
    const sys = join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'tar.exe');
    if (existsSync(sys)) return sys;
  }
  return 'tar';
}

function untar(archive, dest, bz2) {
  return new Promise((resolve, reject) => {
    execFile(tarBin(), [bz2 ? '-xjf' : '-xzf', archive, '-C', dest], { windowsHide: true, timeout: 120000 }, (err, _o, stderr) => {
      if (err) reject(new Error('Could not unpack ' + archive + ': ' + (String(stderr || '').trim() || err.message)));
      else resolve();
    });
  });
}

async function download(fetchImpl, url, file, onBytes) {
  const res = await fetchImpl(url, { redirect: 'follow' });
  if (!res.ok || !res.body) throw new Error('Download failed (' + res.status + '): ' + url);
  const hash = createHash('sha256');
  const out = createWriteStream(file);
  let got = 0;
  try {
    const reader = res.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      hash.update(value);
      got += value.length;
      if (!out.write(value)) await new Promise((r) => out.once('drain', r));
      if (onBytes) onBytes(got);
    }
  } finally {
    await new Promise((r) => out.end(r));
  }
  return hash.digest('hex');
}

/**
 * Make sure both parts are on disk. Safe to call again; does nothing when
 * they are. Throws with a sentence fit to show the person.
 *
 * @param {string} baseDir
 * @param {{ fetch?: Function, log?: (s: string) => void, onProgress?: (p: {part: string, bytes: number}) => void }} [opts]
 */
export async function ensureInstalled(baseDir, opts = {}) {
  const fetchImpl = opts.fetch || globalThis.fetch;
  const log = opts.log || (() => {});
  mkdirSync(baseDir, { recursive: true });
  for (const part of PARTS) {
    try { if (readFileSync(okMarker(baseDir, part), 'utf8').trim() === part.sha256) continue; } catch { /* not yet */ }
    const bz2 = part.url.endsWith('.bz2');
    const tmp = join(baseDir, part.id + '.download');
    const stage = join(baseDir, part.id + '.staging');
    const final = join(baseDir, part.id);
    rmSync(stage, { recursive: true, force: true });
    rmSync(final, { recursive: true, force: true });
    try {
      log('[voice-wake] downloading ' + part.url);
      const got = await download(fetchImpl, part.url, tmp, (bytes) => opts.onProgress && opts.onProgress({ part: part.id, bytes }));
      if (got !== part.sha256) throw new Error('The download of ' + part.id + ' did not match its checksum (got ' + got + ')');
      mkdirSync(stage, { recursive: true });
      await untar(tmp, stage, bz2);
      for (const [rel, want] of Object.entries(part.files)) {
        const src = join(stage, rel);
        if (!existsSync(src)) throw new Error(part.id + ' is missing ' + rel);
        const have = sha256File(src);
        if (have !== want) throw new Error(rel + ' did not match its checksum');
        // ENGINE keeps package/..., MODEL keeps <model dir>/...; both under final.
        const dst = part === MODEL ? join(baseDir, rel) : join(final, rel);
        mkdirSync(dirname(dst), { recursive: true });
        try { renameSync(src, dst); } catch { copyFileSync(src, dst); }
      }
      writeFileSync(okMarker(baseDir, part), part.sha256);
      log('[voice-wake] installed ' + part.id);
    } finally {
      rmSync(tmp, { force: true });
      rmSync(stage, { recursive: true, force: true });
    }
  }
}
