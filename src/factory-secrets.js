/**
 * factory-secrets.js — the secrets a factory may use, and the run's memory of
 * which of them the person has unlocked. Pure (no I/O).
 *
 * A factory lists secrets by id (settings.secrets). When a run starts, the
 * person types each one's PIN; a PIN that opens its secret is kept here, in
 * memory, for that run only, so an unattended run does not stop at every
 * secret_run for someone to type it again.
 *
 * What is kept is the PIN, not the value: the value is decrypted at each use,
 * so the plaintext exists only in the frame that runs the command and in that
 * command's environment, exactly as without a factory. Nothing here is ever
 * written to the run store, the journal, a transcript or a log; it dies with
 * the process, and with the run (stop, finish, discard, a crash).
 *
 * Who may use a listed secret (decide()):
 *   - the Manager and the Gatekeeper: straight away when it is unlocked;
 *   - anyone else: asks the Gatekeeper (or the Manager when there is none),
 *     like a permission request; once approved, an unlocked secret runs;
 *   - when it is not unlocked (a run started by a trigger, or after a crash
 *     or restart), the person is asked for the PIN as before, once, and it is
 *     kept again from then on.
 * A secret not on the list always goes to the person, whoever asks.
 */

import { MANAGER_ID } from './factory-model.js';
import { GATEKEEPER_ID } from './factory-permissions.js';

// The list's shape lives with the rest of the factory's (the editor bundles it).
export { normalizeSecretIds, MAX_SECRETS } from './factory-model.js';

/** Is this person one who uses a listed, unlocked secret without asking anyone? */
export const usesDirectly = (employeeId) => employeeId === MANAGER_ID || employeeId === GATEKEEPER_ID;

/**
 * What happens to one secret_run from a factory chat.
 * @returns {'person'|'direct'|'ask-team'}
 *   person    not on the list: the person's PIN, as outside a factory
 *   direct    the Manager or the Gatekeeper, on the list: no one else is asked
 *             (the PIN is used if unlocked, else the person is asked once)
 *   ask-team  anyone else, on the list: the Gatekeeper or the Manager decides first
 */
export function decide({ employeeId, listed }) {
  if (!listed) return 'person';
  return usesDirectly(employeeId) ? 'direct' : 'ask-team';
}

/**
 * The unlocked secrets of every run, in memory. runId -> Map(secretId -> PIN).
 * Never serialised: there is no method that lists the PINs.
 */
export function createSecretVault() {
  const runs = new Map();
  return {
    /** Keep a PIN that has been checked against its secret. */
    remember(runId, secretId, pin) {
      if (!runId || !secretId || !pin) return;
      let m = runs.get(runId);
      if (!m) { m = new Map(); runs.set(runId, m); }
      m.set(secretId, String(pin));
    },
    pin(runId, secretId) { return runs.get(runId)?.get(secretId) || ''; },
    has(runId, secretId) { return !!runs.get(runId)?.has(secretId); },
    /** The ids unlocked for a run (never the PINs). */
    unlocked(runId) { return [...(runs.get(runId)?.keys() || [])]; },
    forget(runId, secretId) { runs.get(runId)?.delete(secretId); },
    forgetRun(runId) { runs.delete(runId); },
    clear() { runs.clear(); },
    // Kept out of JSON.stringify and util.inspect, so a stray log of the
    // runner cannot print a PIN.
    toJSON() { return {}; },
  };
}

/** Who approved a use, for the journal and the timeline. */
export function approverLabel(list) {
  const names = { manager: 'the Manager', gatekeeper: 'the Gatekeeper', person: 'the person' };
  const a = (Array.isArray(list) ? list : []).map(x => names[x] || x).filter(Boolean);
  return a.length ? a.join(', then ') : 'nobody (pre-authorised)';
}

/**
 * Who may run a command with this secret, and with what value. The whole of
 * secret_run's decision, outside a factory and in one:
 *
 *   - not a factory chat, or a secret not on the factory's list: the person
 *     approves with the PIN (askPerson), as always;
 *   - the Manager or the Gatekeeper, a listed secret: the unlocked PIN, else
 *     the person once (the PIN is then kept for the run);
 *   - anyone else, a listed secret: the Gatekeeper or the Manager first
 *     (runner.askSecret); a refusal ends it; then as above.
 *
 * Every use from a factory chat is journaled (runner.secretUsed): the
 * secret's name, who, who approved, pre-authorised or not. Never the value.
 *
 * @param {object} o
 * @param {object|null} o.runner           the factory runner, or null
 * @param {string} o.chatId                the calling chat ('' if unknown)
 * @param {{id:string, name:string}} o.secret
 * @param {string} o.command
 * @param {(id:string, pin:string) => {ok:boolean, value?:string}} o.decrypt
 * @param {(o:{keepPin:boolean}) => Promise<{ok:boolean, value?:string, pin?:string, error?:string}>} o.askPerson
 * @returns {Promise<{ok:true, value:string, preauthorised:boolean, approvedBy:string[]} | {ok:false, error:string}>}
 */
export async function authoriseSecretUse({ runner, chatId = '', secret, command = '', envName = 'SECRET', reason = '', decrypt, askPerson }) {
  const fx = runner && chatId ? runner.secretContext(chatId, secret.id, command) : null;
  if (fx && fx.deny) return { ok: false, error: fx.deny };
  const approvedBy = [];
  const journal = (extra) => {
    if (!fx) return;
    Promise.resolve(runner.secretUsed(fx.runId, { secretId: secret.id, secretName: secret.name, employeeId: fx.employeeId, employee: fx.employee, approvedBy: approvedBy.slice(), ...extra })).catch(() => {});
  };
  if (fx && fx.route === 'ask-team') {
    const verdict = await runner.askSecret(chatId, { secretId: secret.id, secretName: secret.name, command, envName, reason });
    if (verdict.by) approvedBy.push(verdict.by);
    if (!verdict.allow) { journal({ preauthorised: false, ok: false }); return { ok: false, error: verdict.message || 'The request to use this secret was denied.' }; }
  }
  if (fx && fx.listed) {
    // Read again: the person may have unlocked it while the team decided.
    const pin = runner.secretContext(chatId, secret.id)?.pin || '';
    if (pin) {
      const dec = decrypt(secret.id, pin);
      if (dec && dec.ok) { journal({ preauthorised: true }); return { ok: true, value: dec.value, preauthorised: true, approvedBy }; }
      runner.forgetSecret(fx.runId, secret.id);   // the secret was recreated: its old PIN is no use
    }
  }
  const keepPin = !!(fx && fx.listed);
  const approval = await askPerson({ keepPin });
  if (!approval || !approval.ok) { journal({ preauthorised: false, ok: false }); return { ok: false, error: approval?.error || 'The secret was not approved.' }; }
  approvedBy.push('person');
  // A listed secret: asked once, then remembered for the rest of the run.
  if (keepPin && approval.pin) runner.rememberSecret(fx.runId, secret.id, approval.pin);
  journal({ preauthorised: false });
  return { ok: true, value: approval.value, preauthorised: false, approvedBy };
}
