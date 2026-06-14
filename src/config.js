import dotenv from 'dotenv';
import { join, dirname } from 'path';
import { existsSync } from 'fs';
import { fileURLToPath } from 'url';
import { homedir } from 'os';

const __dirname = dirname(fileURLToPath(import.meta.url));

// ─── Platform-specific app data directory ───
// Crundi uses its own data dir, separate from the old "Claude Telegram Bot" app.
// Electron overrides these via DOTENV_PATH / DATA_DIR env vars.
function defaultAppDir() {
  const home = homedir();
  if (process.platform === 'win32') return join(process.env.APPDATA || join(home, 'AppData', 'Roaming'), 'Crundi');
  if (process.platform === 'darwin') return join(home, 'Library', 'Application Support', 'Crundi');
  return join(process.env.XDG_CONFIG_HOME || join(home, '.config'), 'crundi');
}

const appDir = defaultAppDir();

// .env: DOTENV_PATH > <appDir>/.env > <projectRoot>/.env (dev fallback)
const envCandidates = [
  process.env.DOTENV_PATH,
  join(appDir, '.env'),
  join(__dirname, '..', '.env'),
].filter(Boolean);

export const envPath = envCandidates.find(p => existsSync(p)) || envCandidates[0];
dotenv.config({ path: envPath });

function required(key) {
  const val = process.env[key];
  if (!val) {
    console.error(`Missing required env var: ${key}`);
    console.error('Copy .env.example to .env and fill in the values.');
    process.exit(1);
  }
  return val;
}

// Data dir: DATA_DIR env > <appDir>/data > <projectRoot>/data (dev fallback)
const dataDir = process.env.DATA_DIR || join(appDir, 'data');
const projectsDir = process.env.PROJECTS_DIR || '';

// Project mode: 'single' if PROJECTS_DIR is set, 'multi' otherwise
const projectMode = projectsDir ? 'single' : 'multi';

export const config = {
  // Telegram (required for login validation + notifications)
  botToken: required('TELEGRAM_BOT_TOKEN'),
  allowedUsername: required('ALLOWED_USERNAME'),

  // Bot username — populated at runtime after bot.init()
  botUsername: '',

  // Project mode: 'single' (auto-discover from PROJECTS_DIR) or 'multi' (add individually)
  projectMode,

  // Project root directory for auto-discovery (single mode only)
  projectsDir,

  // Data storage (separate from old app)
  dataDir,

  // App directory (for migration detection)
  appDir,

  // Web server port (default 8888)
  webPort: parseInt(process.env.WEB_PORT || '8888', 10),

  // Cloudflare named tunnel (optional — for persistent custom domain)
  tunnelToken: process.env.CLOUDFLARE_TUNNEL_TOKEN || '',
  tunnelUrl: process.env.CLOUDFLARE_TUNNEL_URL || '',
};

// ─── Old app detection (for first-launch import) ───

function oldAppDir() {
  const home = homedir();
  if (process.platform === 'win32') return join(process.env.APPDATA || join(home, 'AppData', 'Roaming'), 'Claude Telegram Bot');
  if (process.platform === 'darwin') return join(home, 'Library', 'Application Support', 'Claude Telegram Bot');
  return join(process.env.XDG_CONFIG_HOME || join(home, '.config'), 'claude-telegram-bot');
}

/**
 * Check if the old "Claude Telegram Bot" app data exists.
 * Returns the path if found, null otherwise.
 */
export function getOldAppDataDir() {
  const dir = oldAppDir();
  // Check for .env or data/ in the old location
  if (existsSync(join(dir, '.env')) || existsSync(join(dir, 'data'))) return dir;
  return null;
}

/**
 * Check if this is a fresh install (no data dir yet).
 */
export function isFreshInstall() {
  return !existsSync(dataDir) && !existsSync(join(appDir, '.env'));
}
