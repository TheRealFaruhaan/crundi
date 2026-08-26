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

// ─── Login methods ───
//
// Two ways in: Telegram, or a password with a TOTP code. Either is enough, and
// both can be on. Which are ACTIVE is decided at runtime in auth-config.js,
// because Settings can turn them on and off without a restart; what lives here
// is only what the environment supplied.
//
// Nothing is required. A brand-new install has no method configured and no way
// to configure one if it refuses to boot, so it starts open and says so — see
// the note in auth-config.js for how narrowly that is drawn.
const telegramConfigured = !!(process.env.TELEGRAM_BOT_TOKEN && process.env.ALLOWED_USERNAME);
const passwordConfigured = !!(process.env.CRUNDI_PASSWORD_HASH && process.env.CRUNDI_TOTP_SECRET);

// Data dir: DATA_DIR env > <appDir>/data > <projectRoot>/data (dev fallback)
const dataDir = process.env.DATA_DIR || join(appDir, 'data');
const projectsDir = process.env.PROJECTS_DIR || '';

// Project mode: 'single' if PROJECTS_DIR is set, 'multi' otherwise
const projectMode = projectsDir ? 'single' : 'multi';

export const config = {
  // Telegram: login + notifications. Optional now that password login exists,
  // so these are empty strings rather than a hard exit when absent.
  botToken: process.env.TELEGRAM_BOT_TOKEN || '',
  allowedUsername: process.env.ALLOWED_USERNAME || '',
  telegramConfigured,

  // ─── TLS ───
  // Only for a server reached directly. Behind the Cloudflare tunnel, or any
  // reverse proxy that terminates TLS, leave this off.
  tlsMode: (process.env.TLS_MODE || 'off').toLowerCase(),
  tlsDomain: process.env.TLS_DOMAIN || '',
  tlsEmail: process.env.TLS_EMAIL || '',
  tlsCertPath: process.env.TLS_CERT_PATH || '',
  tlsKeyPath: process.env.TLS_KEY_PATH || '',
  // Let's Encrypt rate-limits issuance hard. Point at staging while you are
  // working out whether the DNS and ports are right.
  tlsStaging: process.env.TLS_STAGING === '1',
  tlsPort: parseInt(process.env.TLS_PORT || '443', 10),
  // Where the ACME challenge is answered. Must be 80: the CA does not follow
  // redirects and will not use another port.
  tlsHttpPort: parseInt(process.env.TLS_HTTP_PORT || '80', 10),

  // Password + TOTP login.
  passwordConfigured,
  passwordHash: process.env.CRUNDI_PASSWORD_HASH || '',
  totpSecret: process.env.CRUNDI_TOTP_SECRET || '',
  // Shown on the sign-in screen and used as the session's username.
  localUsername: process.env.CRUNDI_USERNAME || process.env.ALLOWED_USERNAME || 'crundi',

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
