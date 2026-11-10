/**
 * voice/admin.js — Cru's full admin mode (card 467d2bea1d66776d, 9 Nov 2026).
 *
 * With admin mode on, Cru may do anything the user Crundi runs as may do:
 * run any shell command and write any file. The owner's rules for it:
 *  - switched on by hand only (Settings), never by Cru; Cru can switch it
 *    off ("admin mode off");
 *  - no extra confirmations or timers once on, only a clear indicator;
 *  - Sonnet or above: Haiku never holds it (settings.adminOn);
 *  - only the owner's voice is obeyed: with his voice enrolled
 *    (speaker.js), a spoken turn needs it recognised (too short to tell is
 *    refused too); a typed turn is his. With no voice enrolled it is
 *    allowed, the owner's choice (ownerVoice(), from turnPolicy);
 *  - on its own too (card, 10 Nov 2026): when nobody spoke (the heartbeat,
 *    a ticker or trigger, a chat it follows finishing) Cru has full access,
 *    so it can look after the server unattended. There is no voice to check
 *    and no phone to unlock then; each command and write is also listed in
 *    the day's log (onOwnAct). Restarting or stopping Crundi itself, or the
 *    machine, is refused on its own: only the owner may ask for that.
 *
 * The tools are always offered (the tool list stays the same, for the
 * cache), and refuse with a reason while admin mode is off.
 */

import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync, appendFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { homedir } from 'node:os';
import * as voiceSettings from './settings.js';

const OUT_MAX = 16000;
const TIMEOUT_DEFAULT_S = 120;
const TIMEOUT_MAX_S = 600;

function clip(s) {
  const t = String(s || '');
  return t.length > OUT_MAX ? t.slice(0, OUT_MAX / 2) + `\n…[${t.length - OUT_MAX} characters cut]…\n` + t.slice(-OUT_MAX / 2) : t;
}

/** Run [command] in a shell; resolves with its exit code and output. */
export function runCommand(command, { cwd = homedir(), timeoutS = TIMEOUT_DEFAULT_S, platform = process.platform } = {}) {
  return new Promise((done) => {
    const win = platform === 'win32';
    const child = win
      ? spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', command], { cwd, windowsHide: true })
      : spawn('bash', ['-lc', command], { cwd, detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '', err = '', timedOut = false;
    child.stdout.on('data', (d) => { if (out.length < OUT_MAX * 4) out += d; });
    child.stderr.on('data', (d) => { if (err.length < OUT_MAX * 4) err += d; });
    const t = setTimeout(() => {
      timedOut = true;
      try { if (win) child.kill(); else process.kill(-child.pid, 'SIGKILL'); } catch { /* gone */ }
    }, Math.min(TIMEOUT_MAX_S, Math.max(1, timeoutS)) * 1000);
    child.on('error', (e) => { clearTimeout(t); done({ ok: false, error: e.message }); });
    child.on('close', (code) => {
      clearTimeout(t);
      done({ ok: !timedOut && code === 0, exitCode: code, timedOut, stdout: clip(out), stderr: clip(err) });
    });
  });
}

/**
 * Would [command] restart or stop Crundi (or the whole machine)? Refused on
 * Cru's own turns: it ends every chat and terminal, and nobody asked.
 */
export function stopsCrundi(command) {
  const c = String(command || '');
  if (/\b(reboot|shutdown|poweroff|halt)\b/i.test(c) || /\binit\s+[06]\b/.test(c)) return true;
  // systemctl/service/pm2 … restart|stop|kill … crundi, in one command of the line.
  return c.split(/[;&|\n]+/).some((part) => /\bcrundi\b/i.test(part)
    && (/\b(systemctl|service|pm2|launchctl|sc(\.exe)?)\b/i.test(part) && /\b(restart|stop|kill|reload|try-restart|disable|mask|delete|unload)\b/i.test(part)
      || /\b(pkill|killall)\b/i.test(part)));
}

/**
 * The admin tools, in the voice tools' shape ({name, description,
 * input_schema, run}).
 * @param deps.settings      voice settings (tests pass a fake)
 * @param deps.ownerVoice()  true / false / null (null: no voice check), or
 *                           { owner, reason } (the turn's speakerPolicy: false refuses with reason)
 * @param deps.locked()     the phone is locked (as for every change)
 * @param deps.logFile       every command and write is logged here
 * @param deps.onChanged()   admin mode was switched off (screens update)
 * @param deps.unattended()  nobody spoke: Cru acting on its own (heartbeat, ticker, trigger)
 * @param deps.onOwnAct(tool, what, result)  a command or write Cru made on its own (the day's log)
 */
export function makeAdminTools({ settings = voiceSettings, ownerVoice = () => null, locked = () => false, logFile = '', onChanged = () => {}, run = runCommand, unattended = () => false, onOwnAct = () => {} } = {}) {
  const log = (what) => {
    if (!logFile) return;
    try { mkdirSync(dirname(logFile), { recursive: true }); appendFileSync(logFile, JSON.stringify({ at: new Date().toISOString(), ...what }) + '\n', { mode: 0o600 }); } catch { /* best effort */ }
  };
  const refuse = () => {
    const s = settings.getSettings();
    if (s.adminMode !== true) return 'Admin mode is off. Only the owner can switch it on, in Settings › Voice assistant; you cannot.';
    if (!settings.adminCapable(s.model)) return 'Admin mode needs Sonnet or above; the current model may not use it.';
    // On its own: full access (the owner's choice), no voice and no phone to check.
    if (unattended()) return '';
    if (locked()) return 'The phone is locked: ask the owner to unlock it first. Nothing was done.';
    // ownerVoice(): true / false / null, or { owner, reason } (webapp.js, speaker.js turnPolicy).
    const v = ownerVoice();
    const owner = v && typeof v === 'object' ? v.owner : v;
    if (owner === false) return (v && v.reason) || 'The speaker was not recognised as the owner, and admin mode only obeys the owner. Nothing was done: say you couldn\'t recognise who they are, so you can\'t do that.';
    return '';
  };
  const obj = (properties, required = []) => ({ type: 'object', properties, required });
  return [
    {
      name: 'run_command',
      description: 'Admin mode only: run a shell command on the server as the user Crundi runs as (bash; PowerShell on Windows) and get its exit code and output. Anything that user may do: installs, git, services, system checks. With admin mode on there is no confirmation, so be exact and careful; prefer reading before changing, and say plainly what you did. Long work: give a timeout (up to 600 s) or start it in the background yourself.',
      input_schema: obj({
        command: { type: 'string', description: 'The command line' },
        cwd: { type: 'string', description: 'Folder to run in (default: the home folder)' },
        timeoutSeconds: { type: 'integer', minimum: 1, maximum: TIMEOUT_MAX_S, description: `Stop it after this long (default ${TIMEOUT_DEFAULT_S})` },
      }, ['command']),
      admin: true,
      run: async (i = {}) => {
        const why = refuse();
        if (why) return { ok: false, error: why };
        const command = String(i.command || '').trim();
        if (!command) return { ok: false, error: 'No command' };
        const cwd = i.cwd ? resolve(String(i.cwd)) : homedir();
        const own = !!unattended();
        if (own && stopsCrundi(command)) {
          log({ tool: 'run_command', command, cwd, own, refused: 'stops Crundi' });
          return { ok: false, error: 'On your own you may not restart or stop Crundi, or the machine: it ends every chat and terminal. Nothing was done; tell the owner it is needed and let him decide.' };
        }
        log({ tool: 'run_command', command, cwd, ...(own ? { own } : {}) });
        const r = await run(command, { cwd, timeoutS: Number(i.timeoutSeconds) || TIMEOUT_DEFAULT_S });
        log({ tool: 'run_command', exitCode: r.exitCode, timedOut: !!r.timedOut, ...(own ? { own } : {}) });
        if (own) { try { onOwnAct('run_command', { command, cwd }, r); } catch { /* logging only */ } }
        return r;
      },
    },
    {
      name: 'write_file',
      description: 'Admin mode only: write a file on the server (whole contents; folders are made). Anywhere the user Crundi runs as may write.',
      input_schema: obj({
        path: { type: 'string', description: 'Absolute path (or relative to the home folder)' },
        content: { type: 'string', description: 'The whole new contents' },
        append: { type: 'boolean', description: 'Add to the end instead of replacing' },
      }, ['path', 'content']),
      admin: true,
      run: (i = {}) => {
        const why = refuse();
        if (why) return { ok: false, error: why };
        const p = String(i.path || '');
        if (!p) return { ok: false, error: 'No path' };
        const file = isAbsolute(p) ? p : join(homedir(), p);
        try {
          mkdirSync(dirname(file), { recursive: true });
          if (i.append) appendFileSync(file, String(i.content ?? '')); else writeFileSync(file, String(i.content ?? ''));
          const bytes = Buffer.byteLength(String(i.content ?? ''));
          const own = !!unattended();
          log({ tool: 'write_file', path: file, bytes, append: !!i.append, ...(own ? { own } : {}) });
          if (own) { try { onOwnAct('write_file', { path: file, bytes, append: !!i.append }, { ok: true }); } catch { /* logging only */ } }
          return { ok: true, path: file };
        } catch (e) { return { ok: false, error: e.message }; }
      },
    },
    {
      name: 'admin_mode_off',
      description: 'Switch admin mode off (when the owner says so, or when you judge it should be off). You can never switch it on.',
      input_schema: obj({}),
      run: () => {
        const r = settings.updateSettings({ adminMode: false });
        if (r.ok) { log({ tool: 'admin_mode_off' }); onChanged(); }
        return r.ok ? { ok: true, adminMode: false } : r;
      },
    },
  ];
}
