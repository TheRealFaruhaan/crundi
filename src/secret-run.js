/**
 * secret-run.js — run a command with a secret in its environment.
 *
 * The point is that the secret's value never enters the conversation: the agent
 * writes `$SECRET`, the user approves once with the PIN, and the plaintext goes
 * straight from the secrets store into the child process's environment.
 *
 * Be clear about what this does and does not buy, because the difference
 * matters:
 *
 *   It DOES keep the value out of the transcript, out of the model's context,
 *   out of chat history, and out of command output (see redact below). That
 *   covers every accidental path — the overwhelmingly common one.
 *
 *   It does NOT make the value unobtainable by an agent that can run arbitrary
 *   commands. Anything that can use a secret can print it, encode it or post it
 *   somewhere. Redaction catches the naive cases; it cannot beat `base64`. If a
 *   secret must be unobtainable, the command shape has to be fixed in advance so
 *   the agent never controls the process holding it.
 *
 * The value is passed through the ENVIRONMENT and never interpolated into the
 * command string, so it does not appear in `ps`, in a shell history, or in any
 * error message that echoes the command back.
 */

import { spawn } from 'child_process';

const MAX_OUTPUT = 200 * 1024;
const DEFAULT_TIMEOUT_MS = 120000;

/** A valid POSIX/Windows environment variable name. */
export function isValidEnvName(name) {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(String(name || ''));
}

/**
 * Remove every recognisable form of `value` from text.
 *
 * Raw, base64 and percent-encoded are covered because those are the shapes a
 * secret innocently takes on its way through curl, an Authorization header or a
 * URL — not because they would stop someone trying. A short value is left alone
 * rather than redacted: blanking every occurrence of a three-character string
 * would shred unrelated output and teach nobody anything.
 */
export function redact(text, value) {
  let out = String(text ?? '');
  const v = String(value ?? '');
  if (v.length < 6) return out;
  const forms = new Set([v, Buffer.from(v).toString('base64'), encodeURIComponent(v)]);
  // `user:token` base64, as curl -u builds it.
  forms.add(Buffer.from(':' + v).toString('base64'));
  for (const form of forms) {
    if (!form) continue;
    out = out.split(form).join('[redacted]');
  }
  return out;
}

/** Strip Crundi's own credentials so a child cannot read them back. */
function baseEnv() {
  const e = { ...process.env };
  for (const k of [
    'CRUNDI_API_KEY', 'CRUNDI_PASSWORD_HASH', 'CRUNDI_TOTP_SECRET',
    'BOT_TOKEN', 'TELEGRAM_BOT_TOKEN', 'ANTHROPIC_API_KEY',
    'CF_API_TOKEN', 'CLOUDFLARE_API_TOKEN',
  ]) delete e[k];
  return e;
}

/**
 * Run `command` in a shell with `value` bound to `envName`.
 *
 * @returns {Promise<{ok: boolean, code: number|null, stdout: string, stderr: string, error?: string}>}
 *          stdout/stderr always come back redacted.
 */
export function runWithSecret({ command, value, envName = 'SECRET', cwd, timeoutMs = DEFAULT_TIMEOUT_MS }) {
  return new Promise((resolve) => {
    const cmd = String(command || '').trim();
    if (!cmd) return resolve({ ok: false, code: null, stdout: '', stderr: '', error: 'No command given' });
    if (!isValidEnvName(envName)) {
      return resolve({ ok: false, code: null, stdout: '', stderr: '', error: `"${envName}" is not a valid environment variable name` });
    }

    const env = baseEnv();
    env[envName] = String(value ?? '');

    let child;
    try {
      child = spawn(cmd, { shell: true, cwd: cwd || process.cwd(), env, windowsHide: true });
    } catch (err) {
      return resolve({ ok: false, code: null, stdout: '', stderr: '', error: String(err.message || err) });
    }

    let stdout = '';
    let stderr = '';
    let done = false;
    const finish = (result) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      // Redact on the way out, always — including the timeout and error paths,
      // which is exactly where a half-written line is most likely to carry it.
      resolve({
        ...result,
        stdout: redact(stdout, value).slice(-MAX_OUTPUT),
        stderr: redact(stderr, value).slice(-MAX_OUTPUT),
      });
    };

    const timer = setTimeout(() => {
      try { child.kill('SIGKILL'); } catch { /* already gone */ }
      finish({ ok: false, code: null, error: `Timed out after ${Math.round(timeoutMs / 1000)}s` });
    }, timeoutMs);

    child.stdout?.on('data', (d) => {
      stdout += d.toString();
      if (stdout.length > MAX_OUTPUT * 2) stdout = stdout.slice(-MAX_OUTPUT);
    });
    child.stderr?.on('data', (d) => {
      stderr += d.toString();
      if (stderr.length > MAX_OUTPUT * 2) stderr = stderr.slice(-MAX_OUTPUT);
    });
    child.on('error', (err) => finish({ ok: false, code: null, error: String(err.message || err) }));
    child.on('close', (code) => finish({ ok: code === 0, code }));
  });
}
