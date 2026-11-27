#!/usr/bin/env node
/**
 * test-stats-sampler.mjs — the per-process reader stays cheap when the machine
 * is full of processes.
 *
 * A container that never reaped its Chromium children left 5,500 zombies on
 * the server, and the services sampler read every /proc/<pid>/stat at once,
 * every 3 s, with slow reads overlapping: 17,000 files open together and the
 * server's thread pinned. This checks what stops that: reads shared rather
 * than overlapped and spaced by their cost, and the event loop staying
 * responsive while thousands of entries are read.
 *
 * Run: node scripts/test-stats-sampler.mjs
 */

import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  readProcfs, readProcessSnapshot, resetProcessSnapshot,
  sampleProcessTrees, PROC_READ_BATCH,
} from '../src/stats.js';

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// ─── Parsing a /proc layout, at the size that broke it ───

const root = mkdtempSync(join(tmpdir(), 'crundi-procfs-'));
try {
  const COUNT = 6000;
  for (let pid = 1; pid <= COUNT; pid++) {
    mkdirSync(join(root, String(pid)));
    // Most are zombies (state Z, rss 0), as on the server; a few have a comm
    // with spaces and ')' in it, which is why the parser splits on the LAST ')'.
    const comm = pid % 500 === 0 ? 'odd ) name' : 'chromium';
    const state = pid % 10 === 0 ? 'S' : 'Z';
    const rss = state === 'S' ? 256 : 0;
    const ppid = pid === 1 ? 0 : 1;
    const f = [state, ppid, pid, pid, 0, -1, 0, 0, 0, 0, 0, 7, 3, 0, 0, 20, 0, 1, 0, 0, 0, rss];
    writeFileSync(join(root, String(pid), 'stat'), `${pid} (${comm}) ${f.join(' ')} 0 0 0\n`);
  }
  mkdirSync(join(root, 'self'));
  writeFileSync(join(root, 'meminfo'), 'MemTotal: 1 kB\n');

  // How long the event loop is held up while the read runs: the symptom was
  // requests waiting seconds behind it.
  let maxLag = 0;
  let last = performance.now();
  const probe = setInterval(() => {
    const now = performance.now();
    maxLag = Math.max(maxLag, now - last - 5);
    last = now;
  }, 5);
  const t0 = performance.now();
  const procs = await readProcfs(root);
  const took = performance.now() - t0;
  clearInterval(probe);

  check('every pid directory is read, and nothing else', procs.length === COUNT, procs.length);
  const odd = procs.find((p) => p.pid === 500);
  check('a comm with ") " in it still parses', odd && odd.ppid === 1 && odd.ticks === 10, JSON.stringify(odd));
  const live = procs.find((p) => p.pid === 10);
  check('rss is pages times 4096', live && live.rss === 256 * 4096, live && live.rss);
  check(`the event loop is never held for long (${COUNT} entries in ${took.toFixed(0)} ms)`,
    // The old all-at-once read held it for 250-380 ms; a batch costs a few
    // ms. The margin is for a shared, loaded machine descheduling the test.
    maxLag < 150, `${maxLag.toFixed(1)} ms worst gap`);
  check('a missing root is null, not a throw', (await readProcfs(join(root, 'nope'))) === null);
  console.log(`      (${PROC_READ_BATCH} files between yields)`);
} finally {
  rmSync(root, { recursive: true, force: true });
}

// ─── Shared, spaced reads (Linux only: these use the real /proc) ───

if (process.platform === 'linux') {
  resetProcessSnapshot();
  const a = readProcessSnapshot();
  const b = readProcessSnapshot();
  check('callers during a read share it', a === b);
  const [sa, sb] = await Promise.all([a, b]);
  check('the shared read returns one snapshot', sa && sa === sb);
  check('the snapshot includes this process', sa.procs.some((p) => p.pid === process.pid));

  // Straight after a read, a new one is not started; the last is handed out.
  const again = await readProcessSnapshot();
  const spaced = again.at === sa.at || sa.tookMs * 4 === 0;
  check('a read straight after another reuses it (or the read was instant)', spaced,
    `took ${sa.tookMs} ms`);

  // CPU for a tree: null on the first sample, the same answer for a reused
  // snapshot, a real rate once a new snapshot exists.
  resetProcessSnapshot();
  const first = await sampleProcessTrees([process.pid]);
  const r1 = first.get(process.pid);
  check('a tree has memory on its first sample', r1 && r1.rssBytes > 0, r1 && r1.rssBytes);
  check('and no CPU rate yet', r1 && r1.cpuPct === null);
  const end = Date.now() + 300;
  while (Date.now() < end) Math.sqrt(Math.random());   // something to measure
  resetProcessSnapshot();
  const second = await sampleProcessTrees([process.pid]);
  const r2 = second.get(process.pid);
  check('a new snapshot gives a CPU rate', r2 && typeof r2.cpuPct === 'number' && r2.cpuPct > 0, r2 && r2.cpuPct);
  // Hand the same snapshot out again by holding it inside its gap: make it
  // look slow, so the next read is not due yet.
  const held = await readProcessSnapshot();
  held.tookMs = 60_000;
  const third = await sampleProcessTrees([process.pid]);
  const r3 = third.get(process.pid);
  check('the snapshot was reused', (await readProcessSnapshot()) === held);
  check('a reused snapshot repeats the last rate, not 0 or null',
    r3 && r3.cpuPct === r2.cpuPct, `${r3 && r3.cpuPct} vs ${r2.cpuPct}`);
  const fourth = (await sampleProcessTrees([process.pid])).get(process.pid);
  check('and keeps doing so without losing its baseline', fourth && fourth.cpuPct === r2.cpuPct);
  // Rates need at least 50 ms between real reads; services sample every 3 s.
  await new Promise((r) => setTimeout(r, 120));
  resetProcessSnapshot();
  const fifth = (await sampleProcessTrees([process.pid])).get(process.pid);
  check('the next real read measures from the last real one', fifth && typeof fifth.cpuPct === 'number', fifth && fifth.cpuPct);
} else {
  console.log('SKIP  shared reads (not Linux)');
}

console.log(failures ? `\n${failures} failed` : '\nall passed');
process.exit(failures ? 1 : 0);
