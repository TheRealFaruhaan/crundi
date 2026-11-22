#!/usr/bin/env node
// Wrap a staged install folder (package/stage.mjs) into the Windows installer
// and its update feed (package/CONTRACT.md, "Release asset names"):
//
//   node native-v2/package/windows/build.mjs --stage DIR --edition full|client --out DIR [--makensis PATH]
//
// Writes Crundi-Setup-<v>.exe + latest.yml, or Crundi-Client-Setup-<v>.exe + client.yml.

import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { guid, yml } from '../feed.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..', '..');
const args = {};
for (let i = 2; i < process.argv.length; i += 2) args[process.argv[i].replace(/^--/, '')] = process.argv[i + 1];
const stage = resolve(args.stage);
const out = resolve(args.out);
const client = args.edition === 'client';
const version = args.version || JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8')).version;
const num = (version.match(/^(\d+)\.(\d+)\.(\d+)/) || [0, 0, 0, 0]).slice(1);
const name = client ? `Crundi-Client-Setup-${version}.exe` : `Crundi-Setup-${version}.exe`;
mkdirSync(out, { recursive: true });

const makensis = args.makensis
  || ['C:\\Program Files (x86)\\NSIS\\makensis.exe', 'C:\\Program Files\\NSIS\\makensis.exe'].find((p) => existsSync(p))
  || 'makensis';
const D = {
  STAGE: stage,
  VERSION: version,
  VIVERSION: `${num.join('.')}.0`,
  EDITION: client ? 'client' : 'full',
  APP_GUID: guid(client ? 'com.crundi.client' : 'com.crundi.app'),
  OUTFILE: join(out, name),
  ICON: join(repo, 'assets', 'icon.ico'),
};
execFileSync(makensis, ['-V2', ...Object.entries(D).map(([k, v]) => `-D${k}=${v}`), join(here, 'crundi.nsi')], { stdio: 'inherit' });
await yml(join(out, name), join(out, client ? 'client.yml' : 'latest.yml'), version);
console.log(join(out, name));
