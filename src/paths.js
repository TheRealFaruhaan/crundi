/**
 * paths.js — where Crundi keeps its files, without loading anything else.
 *
 * config.js loads the .env and builds the full config; restore-apply.js has to
 * know the same locations *before* that happens (it may be about to replace
 * the .env), so the rules live here and both use them.
 */

import { join, dirname } from 'node:path';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';

const __dirname = dirname(fileURLToPath(import.meta.url));

/** Platform-specific app data directory. */
export function defaultAppDir() {
  const home = homedir();
  if (process.platform === 'win32') return join(process.env.APPDATA || join(home, 'AppData', 'Roaming'), 'Crundi');
  if (process.platform === 'darwin') return join(home, 'Library', 'Application Support', 'Crundi');
  return join(process.env.XDG_CONFIG_HOME || join(home, '.config'), 'crundi');
}

/** .env: DOTENV_PATH > <appDir>/.env > <projectRoot>/.env (dev fallback). */
export function envCandidates(appDir = defaultAppDir()) {
  return [
    process.env.DOTENV_PATH,
    join(appDir, '.env'),
    join(__dirname, '..', '.env'),
  ].filter(Boolean);
}

export function resolveEnvPath(appDir = defaultAppDir()) {
  const c = envCandidates(appDir);
  return c.find(p => existsSync(p)) || c[0];
}
