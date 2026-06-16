/**
 * rpa.js — RPA session management (keep Windows GUI alive while screen is locked)
 *
 * Uses the tscon.exe trick: disconnect an RDP session to the local console so
 * the physical monitor shows the lock screen, but the session keeps rendering
 * in the background. This lets the bot take screenshots, list windows, and
 * interact with the GUI even when nobody is physically at the machine.
 *
 * Setup:
 *   A Windows Scheduled Task ("DisconnectRDP") is created with highest privileges.
 *   The bot can then trigger it at runtime without needing UAC elevation.
 *
 * The task is created automatically:
 *   • During NSIS install (runs as admin)
 *   • On first bot run (attempts elevation; logs a warning if it can't)
 */

import { execSync, execFile } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync, writeFileSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';

const __dirname = dirname(fileURLToPath(import.meta.url));
const TASK_NAME = 'DisconnectRDP';
const BAT_FILENAME = 'disconnect-rdp.bat';

/**
 * Resolve the path to disconnect-rdp.bat.
 * Works for both dev (repo root) and installed (Electron asar unpacked) layouts.
 */
function getBatPath() {
  // Dev: <repo>/scripts/disconnect-rdp.bat
  const devPath = join(__dirname, '..', 'scripts', BAT_FILENAME);
  if (existsSync(devPath)) return devPath;

  // Installed: <app>/resources/app.asar.unpacked/scripts/disconnect-rdp.bat
  const installedPath = join(__dirname, '..', '..', 'scripts', BAT_FILENAME);
  if (existsSync(installedPath)) return installedPath;

  // Electron extraResources: <app>/resources/scripts/disconnect-rdp.bat
  const resourcesPath = join(__dirname, '..', '..', '..', 'resources', 'scripts', BAT_FILENAME);
  if (existsSync(resourcesPath)) return resourcesPath;

  return devPath; // fallback — will fail at task creation time with a clear error
}

/**
 * Check if the DisconnectRDP scheduled task already exists.
 */
export function isTaskInstalled() {
  try {
    execSync(`schtasks /query /tn "${TASK_NAME}" 2>nul`, {
      encoding: 'utf8',
      timeout: 10000,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * Create the DisconnectRDP scheduled task with highest privileges.
 *
 * Approach: write a temp .bat file with the schtasks command, then elevate that
 * .bat via PowerShell Start-Process -Verb RunAs. This avoids the quoting
 * nightmare of passing the command through Node → PowerShell → Start-Process → schtasks.
 *
 * Returns { ok: boolean, error?: string }
 */
export function createTask() {
  const batPath = getBatPath();
  if (!existsSync(batPath)) {
    return { ok: false, error: `Batch file not found: ${batPath}` };
  }

  // /sc once /sd 01/01/2099 /st 00:00 = far-future dummy schedule (never auto-runs)
  // /rl highest = run with admin privileges when triggered
  const schtasksCmd = `schtasks /create /tn "${TASK_NAME}" /tr "\\"${batPath}\\"" /sc once /sd 01/01/2099 /st 00:00 /rl highest /f`;

  // Try direct creation first (works if running as admin)
  try {
    execSync(schtasksCmd, { encoding: 'utf8', timeout: 15000, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    return { ok: true };
  } catch {
    // Access denied — fall through to UAC elevation
  }

  // Write the schtasks command to a temp .bat and elevate that.
  // Setting exit code explicitly so we can tell if schtasks failed even after elevation.
  const tmpBat = join(tmpdir(), `create-disconnect-rdp-task-${Date.now()}.bat`);
  const batContent = `@echo off\r\n${schtasksCmd}\r\nexit /b %ERRORLEVEL%\r\n`;

  try {
    writeFileSync(tmpBat, batContent, 'utf8');

    // Start-Process -Verb RunAs triggers UAC. -Wait blocks until the elevated process exits.
    // We don't capture its stdout/exit code through this path; we verify success by checking
    // if the task exists afterward.
    execSync(
      `powershell -NoProfile -Command "Start-Process -FilePath '${tmpBat}' -Verb RunAs -Wait -WindowStyle Hidden"`,
      { encoding: 'utf8', timeout: 120000, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] }
    );

    if (isTaskInstalled()) return { ok: true };
    return { ok: false, error: 'UAC elevation completed but task was not created. Check Event Viewer or run as admin manually.' };
  } catch (err) {
    return { ok: false, error: err.stderr?.trim() || err.message };
  } finally {
    try { unlinkSync(tmpBat); } catch { /* ignore */ }
  }
}

/**
 * Remove the DisconnectRDP scheduled task.
 * Returns { ok: boolean, error?: string }
 */
export function removeTask() {
  try {
    execSync(
      `schtasks /delete /tn "${TASK_NAME}" /f`,
      { encoding: 'utf8', timeout: 10000, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] }
    );
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err.stderr?.trim() || err.message };
  }
}

/**
 * Trigger the DisconnectRDP scheduled task (runs the bat as admin).
 * The RDP window drops immediately; the session stays unlocked.
 * Returns { ok: boolean, error?: string }
 */
export function disconnectRdp() {
  try {
    execSync(
      `schtasks /run /tn "${TASK_NAME}"`,
      { encoding: 'utf8', timeout: 10000, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] }
    );
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err.stderr?.trim() || err.message };
  }
}

/**
 * Check if the current Windows session is an RDP session.
 * The tscon trick only applies when connected via RDP.
 */
export function isRdpSession() {
  try {
    const result = execSync(
      'powershell -NoProfile -NonInteractive -WindowStyle Hidden -Command "echo $env:SESSIONNAME"',
      { encoding: 'utf8', timeout: 5000, windowsHide: true }
    ).trim();
    // RDP sessions are named "RDP-Tcp#N"; console is "Console"
    return result.startsWith('RDP-Tcp');
  } catch {
    return false;
  }
}

/**
 * Auto-setup: ensure the scheduled task exists.
 * Called during bot startup. Tries to create it if missing.
 * Non-blocking — logs warnings but never throws.
 */
export function ensureRpaSetup() {
  if (isTaskInstalled()) {
    console.log(`[rpa] Scheduled task "${TASK_NAME}" is ready.`);
    return;
  }

  console.log(`[rpa] Scheduled task "${TASK_NAME}" not found — attempting to create...`);
  const result = createTask();
  if (result.ok) {
    console.log(`[rpa] Scheduled task "${TASK_NAME}" created successfully.`);
  } else {
    console.warn(`[rpa] Could not create scheduled task (needs admin): ${result.error}`);
    console.warn('[rpa] RDP disconnect feature will not work until the task is created.');
    console.warn('[rpa] Run the bot installer or execute as admin once to set it up.');
  }
}
