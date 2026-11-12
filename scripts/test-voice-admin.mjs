#!/usr/bin/env node
/**
 * test-voice-admin.mjs — Cru's full admin mode: on by hand only, never on
 * Haiku, only for the owner's voice, and a command really runs.
 * Run: node scripts/test-voice-admin.mjs
 */
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const dir = mkdtempSync(join(tmpdir(), 'cru-admin-'));
process.env.DATA_DIR = dir;
const settings = await import('../src/voice/settings.js');
const { makeAdminTools, stopsCrundi } = await import('../src/voice/admin.js');
const { makeActionTools } = await import('../src/voice/actions.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
let voice = null, locked = false, own = false;
const ownActs = [];
const tools = makeAdminTools({ settings, ownerVoice: () => voice, locked: () => locked, logFile: join(dir, 'admin.jsonl'), unattended: () => own, onOwnAct: (tool, what, res) => ownActs.push({ tool, what, res }) });
const t = (n) => tools.find((x) => x.name === n);

ok(settings.publicSettings().adminMode === false, 'admin mode starts off');
let r = await t('run_command').run({ command: 'echo hi' });
ok(!r.ok && /off/.test(r.error), 'off: run_command refuses');

// Haiku may not hold it.
settings.updateSettings({ model: 'claude-haiku-5-5' });
ok(settings.updateSettings({ adminMode: true }).ok === false, 'cannot switch on with Haiku');
const capable = Object.keys(settings.MODELS).find((m) => !/haiku/.test(m));
settings.updateSettings({ model: capable });
ok(settings.updateSettings({ adminMode: true }).ok && settings.publicSettings().adminMode === true, `switched on with ${capable}`);

r = await t('run_command').run({ command: 'echo hello-admin' });
ok(r.ok && r.stdout.includes('hello-admin'), 'on: a command runs');
r = await t('write_file').run({ path: join(dir, 'x', 'y.txt'), content: 'written' });
ok(r.ok && readFileSync(join(dir, 'x', 'y.txt'), 'utf8') === 'written', 'on: a file is written');
r = await t('run_command').run({ command: 'sleep 5', timeoutSeconds: 1 });
ok(!r.ok && r.timedOut, 'a command past its time is stopped');

voice = false;
r = await t('run_command').run({ command: 'echo no' });
ok(!r.ok && /not recognised/.test(r.error), 'another voice: refused');
// With the owner's voice enrolled, webapp.js passes the turn's rule (speaker.js turnPolicy).
voice = { owner: false, reason: 'That was too short to tell who is speaking: say it a bit longer.' };
r = await t('run_command').run({ command: 'echo no' });
ok(!r.ok && /longer/.test(r.error), 'a voice turn too short to recognise: refused with its reason');
voice = { owner: true, reason: 'typed by the owner' };
r = await t('run_command').run({ command: 'echo typed-ok' });
ok(r.ok && r.stdout.includes('typed-ok'), 'a typed turn (or the owner\'s voice): runs');
voice = true; locked = true;
r = await t('run_command').run({ command: 'echo no' });
ok(!r.ok && /locked/.test(r.error), 'locked phone: refused');
locked = false;

ok(readFileSync(join(dir, 'admin.jsonl'), 'utf8').includes('hello-admin'), 'commands are logged');
ok(ownActs.length === 0, 'spoken and typed turns are not listed as Cru on its own');

// On its own (heartbeat, ticker, trigger): full access, even with the phone
// locked and no voice to check (webapp.js passes { owner: true } then), each
// command in the day's log; never restarting Crundi or the machine.
own = true; locked = true; voice = { owner: true, reason: 'nobody spoke: Cru on its own' };
r = await t('run_command').run({ command: 'echo on-my-own' });
ok(r.ok && r.stdout.includes('on-my-own'), 'on its own, phone locked: a command runs');
ok(ownActs.length === 1 && ownActs[0].tool === 'run_command' && ownActs[0].what.command === 'echo on-my-own' && ownActs[0].res.exitCode === 0, 'and goes to the day\'s log with its result');
r = await t('write_file').run({ path: join(dir, 'own.txt'), content: 'mine' });
ok(r.ok && readFileSync(join(dir, 'own.txt'), 'utf8') === 'mine' && ownActs.length === 2 && ownActs[1].tool === 'write_file', 'on its own: a write works and is logged');
ok(/"own":true/.test(readFileSync(join(dir, 'admin.jsonl'), 'utf8')), 'the admin log marks what Cru did on its own');
for (const c of ['sudo systemctl restart crundi', 'systemctl stop crundi.service', 'df -h && sudo systemctl restart crundi', 'sudo reboot', 'shutdown -r now', 'pkill -f crundi', 'pm2 restart crundi']) {
  r = await t('run_command').run({ command: c });
  ok(!r.ok && /restart or stop Crundi/.test(r.error), `on its own, refused: ${c}`);
}
ok(ownActs.length === 2, 'refused commands never ran');
for (const c of ['systemctl --failed', 'systemctl status crundi', 'journalctl -u crundi -n 50', 'sudo systemctl restart nginx', 'df -h; free -m']) ok(!stopsCrundi(c), `allowed on its own: ${c}`);
// The owner may still ask for a restart himself.
own = false; locked = false; voice = { owner: true, reason: 'typed by the owner' };
let ran = '';
const asked = makeAdminTools({ settings, ownerVoice: () => voice, run: async (c) => { ran = c; return { ok: true, exitCode: 0 }; } }).find((x) => x.name === 'run_command');
r = await asked.run({ command: 'sudo systemctl restart crundi' });
ok(r.ok && ran === 'sudo systemctl restart crundi', 'asked by the owner, a Crundi restart is his to ask for (not refused)');
// Still off when admin mode is off, on its own too.
settings.updateSettings({ adminMode: false });
own = true;
r = await t('run_command').run({ command: 'echo no' });
ok(!r.ok && /off/.test(r.error), 'on its own with admin mode off: refused');
settings.updateSettings({ adminMode: true });
own = false;

// Cru can never switch it on itself.
const acts = makeActionTools({ voiceSettings: settings, listProjects: () => [] }, { locked: () => false, pending: { newTurn() {}, propose() { return 'x'; }, take() { return {}; }, cancel() {} } });
const set = acts.find((x) => x.name === 'set_voice_setting');
ok(!set.input_schema.properties.setting.enum.includes('adminMode'), 'set_voice_setting cannot touch adminMode');

settings.updateSettings({ model: 'claude-haiku-5-5' });
ok(settings.publicSettings().adminMode === false, 'switching to Haiku switches it off');
settings.updateSettings({ model: capable, adminMode: true });
r = await t('admin_mode_off').run({});
ok(r.ok && settings.publicSettings().adminMode === false, 'Cru can switch it off');

console.log(failed ? `\n${failed} FAILED` : '\nAll admin checks passed');
process.exit(failed ? 1 : 0);
