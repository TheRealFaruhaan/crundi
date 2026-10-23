// The fingerprint of everything the editor bundle is built from: its own
// sources and the server modules it bundles (the validator, the engine, the
// skill mentions). The build stamps it into the bundle; the check in
// scripts/test-factory-flow-build.mjs recomputes it, so a change to those
// modules without a rebuild fails `npm run check` instead of shipping an
// editor that disagrees with the server.
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..', '..');

function walk(dir, out = []) {
  for (const n of readdirSync(dir).sort()) {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) walk(p, out); else out.push(p);
  }
  return out;
}

export function sourceFiles() {
  return [
    ...walk(join(here, 'src')),
    join(here, 'package.json'),
    join(repo, 'src', 'factory-model.js'),
    join(repo, 'src', 'factory-engine.js'),
    join(repo, 'src', 'factory-skills.js'),
    join(repo, 'src', 'factory-journal.js'),
  ];
}

export function sourceHash() {
  const h = createHash('sha256');
  for (const f of sourceFiles()) {
    h.update(relative(repo, f).replace(/\\/g, '/'));
    h.update('\0');
    h.update(readFileSync(f).toString('utf8').replace(/\r\n/g, '\n'));
    h.update('\0');
  }
  return h.digest('hex').slice(0, 16);
}
