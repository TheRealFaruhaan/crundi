#!/usr/bin/env node
/**
 * test-compose-stats.mjs — the command parser, and a live check when docker is
 * here.
 *
 * The parser is what decides whether a service is measured by its process tree
 * or by its containers, so getting it wrong is not a cosmetic failure: it puts
 * the service back on the tree that reports 73 MB for a 900 MB workload, and it
 * does so silently, because a wrong number looks exactly like a right one.
 *
 * The live half only runs when there are compose containers on the machine. It
 * compares what this module computes against `docker stats` — the thing the
 * numbers are supposed to agree with — because the whole bug being fixed here
 * came from testing against an assumption instead of against the real source.
 *
 * Run: node scripts/test-compose-stats.mjs
 */

import { execFile } from 'node:child_process';
import {
  parseComposeCommand, tokenize, parseDockerSize,
  listComposeContainers, matchContainers, sampleComposeServices,
} from '../src/compose-stats.js';

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};
const eq = (label, got, want) => check(label, JSON.stringify(got) === JSON.stringify(want), JSON.stringify(got));

// ─── Tokenising ───

// Quoting is preserved rather than flattened, so a path with a space in it
// survives; reaching the nested command is the parser's job, not the lexer's.
eq('a quoted run stays one token',
  tokenize('sg docker -c "docker compose up warp db"'),
  ['sg', 'docker', '-c', 'docker compose up warp db']);
eq('a quoted path keeps its space',
  tokenize('docker compose -f "/my dir/c.yml" up api'),
  ['docker', 'compose', '-f', '/my dir/c.yml', 'up', 'api']);

// ─── The parser ───

const services = (cmd) => { const p = parseComposeCommand(cmd); return p ? p.services : null; };

eq('plain compose up', services('docker compose up instagram-scraper'), ['instagram-scraper']);
eq('through sg', services('sg docker -c "docker compose up instagram-scraper"'), ['instagram-scraper']);
eq('several services', services('sg docker -c "docker compose up warp db ocr-server"'), ['warp', 'db', 'ocr-server']);
eq('hyphenated binary', services('docker-compose up web'), ['web']);
eq('nested two deep', services('sg docker -c \'bash -c "docker compose up api"\''), ['api']);
eq('no service names means all of them', services('docker compose up -d'), []);
eq('boolean flags are not services', services('docker compose up --build --force-recreate api'), ['api']);
// The one that would quietly poison the match: a flag's value read as a name.
eq('-f value is not a service', services('docker compose -f docker-compose.yml up api'), ['api']);
eq('--scale value is not a service', services('docker compose up --scale web=3 web'), ['web']);
eq('-t value is not a service', services('docker compose up -t 30 api'), ['api']);
eq('inline flag value', services('docker compose up --scale=web=3 web'), ['web']);

check('project name is picked up',
  parseComposeCommand('docker compose -p myproj up api')?.projectName === 'myproj');
check('non-compose command is not claimed', parseComposeCommand('npm start') === null);
check('npm script merely mentioning docker is not claimed',
  parseComposeCommand('npm run docker-thing') === null);
// `down`/`logs` leave nothing running, so claiming them would blank a service
// that is measured perfectly well by its process tree.
check('compose down is not claimed', parseComposeCommand('docker compose down') === null);
check('compose logs is not claimed', parseComposeCommand('docker compose logs -f') === null);

// ─── Sizes ───

eq('MiB', parseDockerSize('902.4MiB'), Math.round(902.4 * 1024 * 1024));
eq('GiB', parseDockerSize('1.5GiB'), 1610612736);
eq('decimal MB is not binary', parseDockerSize('100MB'), 100000000);
eq('bare bytes', parseDockerSize('512B'), 512);
eq('junk', parseDockerSize('n/a'), null);

// ─── Against the real docker, if there is one ───

const dockerStats = () => new Promise((resolve) => {
  execFile('docker', ['stats', '--no-stream', '--format', 'json'], { timeout: 30000 },
    (err, stdout) => {
      if (err) return resolve(null);
      const out = new Map();
      for (const line of String(stdout).split('\n')) {
        const t = line.trim();
        if (!t.startsWith('{')) continue;
        try {
          const row = JSON.parse(t);
          out.set(row.Name, parseDockerSize(String(row.MemUsage || '').split('/')[0]));
        } catch { /* skip */ }
      }
      resolve(out);
    });
});

const containers = await listComposeContainers({ force: true }).catch(() => []);
if (!containers.length) {
  console.log('\nSKIP  live check: no compose containers on this machine');
} else {
  const c = containers[0];
  const matched = await matchContainers(
    { cwd: c.workingDir, services: [c.service], projectName: '' }, containers);
  check('a container matches its own working dir and service',
    matched.length === 1 && matched[0].id === c.id, matched.map((m) => m.name).join(','));

  const spec = { key: 'test', cwd: c.workingDir, services: [c.service], projectName: '' };
  // CPU is a rate, so it needs two samples before it means anything.
  await sampleComposeServices([spec]);
  await new Promise((r) => setTimeout(r, 1500));
  const got = (await sampleComposeServices([spec])).get('test');

  check('live sample produces memory', got && got.rssBytes > 0, got?.rssBytes);
  check('live sample produces a CPU rate', got && got.cpuPct != null, got?.cpuPct);
  check('reports which containers it measured',
    got && got.containers.length === 1 && got.containers[0].name === c.name);

  const truth = await dockerStats();
  const want = truth?.get(c.name);
  if (want == null) {
    console.log('SKIP  memory agreement: docker stats gave nothing for ' + c.name);
  } else {
    // Sampled seconds apart on a live container, so exact equality is not on
    // offer; being in the same ballpark is what distinguishes reading the
    // container from reading the wrong process tree entirely.
    const ratio = got.rssBytes / want;
    check(`memory agrees with docker stats for ${c.name}`,
      ratio > 0.75 && ratio < 1.33,
      `${(got.rssBytes / 1048576).toFixed(1)} MiB vs ${(want / 1048576).toFixed(1)} MiB`);
  }
}

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nCompose services are measured by their containers.');
