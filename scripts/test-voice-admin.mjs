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
const { makeAdminTools } = await import('../src/voice/admin.js');
const { makeActionTools } = await import('../src/voice/actions.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
let voice = null, locked = false;
const tools = makeAdminTools({ settings, ownerVoice: () => voice, locked: () => locked, logFile: join(dir, 'admin.jsonl') });
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
