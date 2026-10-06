#!/usr/bin/env node
// Install or update the skills Crundi ships into a user's Claude config.
//
//   node scripts/sync-skills.mjs [--home <dir>] [--source <dir>]
//
// install.sh runs this (as root, for the service user's home, then hands the
// files over), and the server runs the same routine at every start. The rules
// live in src/skills-sync.js: included skills are installed or updated in
// place, and no other skill is touched.
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';
import { syncBundled, describeSync } from '../src/skills-sync.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const arg = (flag) => { const i = process.argv.indexOf(flag); return i > 0 ? process.argv[i + 1] : ''; };

const home = arg('--home') || homedir();
const source = arg('--source') || join(root, 'skills');
const target = join(home, '.claude', 'skills');
let version = '';
try { version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version; } catch { /* unversioned copy */ }

try {
  const r = syncBundled({ source, target, version });
  for (const line of describeSync(r, target)) console.log(line);
  if (r.current.length && !r.installed.length && !r.updated.length) console.log(`Crundi's skills are up to date in ${target}`);
  process.exit(r.ok ? 0 : 1);
} catch (err) {
  // One line, not a stack: this lands in the middle of an installer's output.
  console.error(err.message);
  process.exit(1);
}
