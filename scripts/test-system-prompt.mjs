#!/usr/bin/env node
/**
 * test-system-prompt.mjs — the combined system prompt, and the quoting that
 * carries it to the CLI.
 *
 * Two things here fail silently rather than loudly, which is why they are
 * tested rather than eyeballed:
 *
 *  1. LAYER ORDER. The user's words must come after Crundi's, or their
 *     instruction quietly loses every disagreement with ours. Nothing throws
 *     when the order is wrong; the model just behaves slightly differently
 *     than asked, forever.
 *
 *  2. SHELL QUOTING. Terminal mode builds ONE `bash -c` string, so a prompt
 *     with a space in it used to be split into separate argv entries. The CLI
 *     would then reject the leftovers with a message about an unknown argument
 *     — never about a system prompt. So the quoting is checked by running it
 *     through a real bash and comparing what came out the other side, not by
 *     inspecting the escaped string.
 *
 * Run: node scripts/test-system-prompt.mjs
 */

import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'crundi-sp-'));
process.env.CRUNDI_DATA_DIR = dir;
const { config } = await import('../src/config.js');
config.dataDir = dir;

const { BASE, MAX_LAYER, MAX_TOTAL, clampLayer, composeSystemPrompt, systemPromptArgs }
  = await import('../src/system-prompt.js');
const { shQuote } = await import('../src/claude-terminals.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

const state = (o) => writeFileSync(join(dir, '.crundi-state.json'), JSON.stringify(o));
const projects = (o) => writeFileSync(join(dir, 'projects.json'), JSON.stringify(o));

// ─── Layers ───

// Nothing configured: Crundi's framing, and it actually names the tools. A
// prompt that describes no tools is the same as no prompt at all.
{
  state({});
  projects({});
  const t = composeSystemPrompt({ project: 'demo' });
  check('base only when nothing is configured', t === BASE);
  check('the base names the out-of-band message tool', /send_message_to_user/.test(t));
  check('the base names the service tools', /register_service/.test(t));
  check('the base does not claim the project is Crundi itself', !/developing Crundi/i.test(t));
}

// The order is the whole point.
{
  state({ systemPrompt: 'GLOBAL-LAYER' });
  projects({ demo: { path: '/tmp/demo', name: 'Demo', systemPrompt: 'PROJECT-LAYER' } });
  const t = composeSystemPrompt({ project: 'demo' });
  const iB = t.indexOf(BASE.slice(0, 40)), iG = t.indexOf('GLOBAL-LAYER'), iP = t.indexOf('PROJECT-LAYER');
  check('all three layers present', iB >= 0 && iG >= 0 && iP >= 0, `${iB}/${iG}/${iP}`);
  check('order is base, then global, then project', iB < iG && iG < iP, `${iB} < ${iG} < ${iP}`);
  check('layers are separated by a blank line', /GLOBAL-LAYER\n\nPROJECT-LAYER/.test(t));
}

// The project layer belongs to the project, not to whoever asked.
{
  const t = composeSystemPrompt({ project: 'other' });
  check('another project does not inherit this one’s prompt', !/PROJECT-LAYER/.test(t));
  check('but the global layer still applies', /GLOBAL-LAYER/.test(t));
}

// Unattended runs: an edit to Settings must not change what 07:00 does.
{
  const t = composeSystemPrompt({ project: 'demo', userLayers: false });
  check('scheduled runs get no global layer', !/GLOBAL-LAYER/.test(t));
  check('scheduled runs get no project layer', !/PROJECT-LAYER/.test(t));
  check('scheduled runs still get the base', t.startsWith(BASE.slice(0, 40)));
  const j = composeSystemPrompt({ project: 'demo', userLayers: false, extra: 'JOB-BRIEFING' });
  check('the job briefing goes last', j.endsWith('JOB-BRIEFING'));
}

// ─── Caps ───
//
// This is one argv entry, and Linux caps a single argument at 128 KB. Past that
// exec fails with E2BIG, which surfaces as "the chat won't start" with nothing
// anywhere pointing at the prompt.
{
  check('an over-long layer is cut to the cap', clampLayer('x'.repeat(MAX_LAYER * 3)).length === MAX_LAYER);
  check('a normal layer is untouched', clampLayer('  hello  ') === 'hello');
  check('blank is blank', clampLayer('   \n  ') === '');
  check('CRLF is normalised', clampLayer('a\r\nb') === 'a\nb');
  state({ systemPrompt: 'G'.repeat(MAX_LAYER * 2) });
  projects({ demo: { path: '/tmp/demo', systemPrompt: 'P'.repeat(MAX_LAYER * 2) } });
  const t = composeSystemPrompt({ project: 'demo' });
  check('the assembled prompt stays under the total cap', t.length <= MAX_TOTAL, t.length);
  check('and far under the 128 KB single-argv limit', t.length < 131072, t.length);
}

// ─── Broken state on disk ───
//
// These files are hand-editable. None of them may take chat down with it.
{
  writeFileSync(join(dir, '.crundi-state.json'), '{ not json');
  writeFileSync(join(dir, 'projects.json'), 'also not json');
  const t = composeSystemPrompt({ project: 'demo' });
  check('corrupt state files fall back to the base', t === BASE);

  // The pre-2024 registry format was a bare path string.
  projects({ demo: '/tmp/demo' });
  state({});
  check('a legacy string project entry carries no prompt', composeSystemPrompt({ project: 'demo' }) === BASE);
  check('an unknown project is not an error', composeSystemPrompt({ project: 'nope' }) === BASE);
}

// ─── The argv pair ───
{
  const a = systemPromptArgs({ project: 'demo' });
  check('two argv entries', a.length === 2, a.length);
  check('the documented flag, not the undocumented -file variant', a[0] === '--append-system-prompt');
}

// ─── Quoting, through a real bash ───
//
// `printf '%s\n'` prints each argv entry on its own line, so what comes back is
// exactly how the shell split the command — the thing that used to be wrong.
const viaBash = (tokens) => new Promise((resolve) => {
  execFile('/bin/bash', ['-c', 'printf \'%s\\n\' ' + tokens.map(shQuote).join(' ')],
    { timeout: 15000 }, (err, out) => resolve(err ? null : String(out).replace(/\n$/, '').split('\n')));
});

{
  const cases = [
    ['a bare word survives unquoted', ['--model', 'opus']],
    ['a value with spaces stays ONE argument', ['--append-system-prompt', 'You are inside Crundi. Use the tools.']],
    ['an apostrophe survives', ['--append-system-prompt', "the machine’s tools; it's fine"]],
    ['double quotes survive', ['--append-system-prompt', 'say "hello" now']],
    ['no variable expansion', ['--append-system-prompt', '$HOME and ${PATH}']],
    ['no command substitution', ['--append-system-prompt', '`whoami` and $(id -u)']],
    ['a semicolon does not start a new command', ['--append-system-prompt', 'done; echo PWNED']],
    ['a newline stays inside the argument', ['--append-system-prompt', 'line one\nline two']],
  ];
  for (const [label, toks] of cases) {
    const got = await viaBash(toks);
    // printf puts each argv entry on its own line, so a value containing a
    // newline legitimately spans two — compare on the flattened form.
    const want = toks.join('\n').split('\n');
    check(label, JSON.stringify(got) === JSON.stringify(want), JSON.stringify(got));
  }

  // Non-vacuous: the old unquoted join really does break these.
  const naive = await new Promise((resolve) => {
    execFile('/bin/bash', ['-c', 'printf \'%s\\n\' ' + ['--append-system-prompt', 'You are inside Crundi. Use the tools.'].join(' ')],
      { timeout: 15000 }, (err, out) => resolve(err ? null : String(out).replace(/\n$/, '').split('\n')));
  });
  check('the unquoted join this replaces really did shred the prompt',
    naive && naive.length > 2 && !naive.includes('You are inside Crundi. Use the tools.'),
    naive && naive.length + ' arguments: ' + JSON.stringify(naive));
}

rmSync(dir, { recursive: true, force: true });

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nThe prompt is assembled in the right order and reaches the CLI intact.');
