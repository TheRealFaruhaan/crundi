// operations.mjs's "skills" section: Settings → Skills, Backup & restore,
// Usage, and the sign-in screen's restore. run(T, t) with T from
// t.begin('ops') (test/lib.mjs).
//
// Everything that writes stays inside the test server: skills go into the
// test projects' own .claude/skills (never "All projects", which is the
// machine's ~/.claude/skills), no backup or restore runs (no storage is
// set up), and the window warm-up switch is the test server's setting.
import { writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const sk = (p) => p.areas.ops.skills;

export async function run(T, t) {
  const scratch = join(tmpdir(), 'ops-skills-test');
  mkdirSync(scratch, { recursive: true });
  // What an earlier, interrupted run left in the test projects' own skills (never ~/.claude).
  for (const dir of [T.projDir, T.projDir + '2']) for (const n of ['native-test', 'native-renamed']) rmSync(join(dir, '.claude', 'skills', n), { recursive: true, force: true });
  // A second project, so a skill can be copied and moved between two
  // project folders (both under the test server's /tmp).
  const p2 = await T.api.http('POST', '/api/projects', { alias: 'demo2', name: 'Demo two', path: T.projDir + '2', create: true });
  t.check('skills: a second test project', p2.status < 300, p2.body);

  // ── Open Settings: the three sections load ──
  await T.ui('tab', 'settings');
  for (const k of ['server', 'cc', 'whatsnew', 'sysprompt', 'voice', 'channels', 'notify', 'signin', 'phone', 'session', 'config', 'terminal', 'desktop', 'backup', 'usage']) await T.act('settings-open', k, 'false');
  await T.act('settings-open', 'skills', 'true');
  await T.until('skills: the list loads', (p) => sk(p).state === 'data' && sk(p).rows.length > 0);
  let m = await T.mine();
  t.check('skills: groups in the web\'s order (All projects first)', m.skills.rows[0].kind === 'group' && m.skills.rows[0].title === 'All projects', m.skills.rows.slice(0, 3));
  const scopes = m.skills.scopes;
  t.check('skills: the scopes are every project and each project', scopes[0] === 'global' && scopes.includes('project:demo') && scopes.includes('project:demo2'), scopes);
  // Never write outside the test projects: an unknown scope would fall back to the first, "All projects".
  if (!scopes.includes('project:demo') || !scopes.includes('project:demo2')) { t.check('skills: both test projects are there (stopping before any write)', false, scopes); return; }
  const idx = (id) => String(scopes.indexOf(id));

  // ── New: an empty skill in the test project, opened in the editor ──
  await T.act('skills-act', 'target', idx('project:demo'));
  await T.act('skills-act', 'new', '');
  m = await T.mine();
  t.check('skills: New opens its form', m.skills.form === 'new', m.skills.form);
  await T.act('skills-field', 'new-name', 'native-test');
  await T.act('skills-field', 'new-desc', 'A skill the native test made');
  await T.act('skills-act', 'new-go', '');
  await T.until('skills: the new skill opens with SKILL.md in the editor', (p) => sk(p).open?.name === 'native-test' && sk(p).editor?.path === 'SKILL.md');
  m = await T.mine();
  t.check('skills: it is in the project\'s group', m.skills.rows.some((r) => r.kind === 'skill' && r.name === 'native-test' && r.scope === 'project:demo' && r.open), m.skills.rows);
  t.check('skills: SKILL.md holds the description', /A skill the native test made/.test(m.skills.editor.text), m.skills.editor);
  await T.snap('skills-open');

  // ── Edit and save ──
  await T.act('skills-field', 'editor', m.skills.editor.text + '\nEdited by the native client.\n');
  m = await T.mine();
  t.check('skills: an edit makes the file dirty', m.skills.editor.dirty === true, m.skills.editor);
  await T.act('skills-act', 'save', '');
  await T.until('skills: Save writes it (no longer dirty)', (p) => sk(p).editor && !sk(p).editor.dirty && !sk(p).busy);
  const f = await T.api.http('GET', '/api/skills/file?scope=project:demo&name=native-test&path=SKILL.md');
  t.check('skills: the server has the saved text', /Edited by the native client/.test(f.body?.text || ''), f.body);

  // ── Leaving a file with changes asks first ──
  await T.act('skills-field', 'editor', 'unsaved');
  await T.act('skills-act', 'file-close', '');
  let pr = await T.props();
  t.check('skills: closing a changed file asks to discard', /Discard the changes/.test(pr.dialog?.message || ''), pr.dialog);
  await T.dialog(true);
  await T.until('skills: discarding closes the editor', (p) => sk(p).editor === null);

  // ── New file, then delete it ──
  await T.act('skills-act', 'newfile', '');
  await T.act('skills-field', 'newfile', 'reference/notes.md');
  await T.act('skills-act', 'newfile-go', '');
  await T.until('skills: a new file is listed and opened', (p) => sk(p).files.includes('reference/notes.md') && sk(p).editor?.path === 'reference/notes.md');
  await T.act('skills-act', 'file-delete', 'reference/notes.md');
  await T.dialog(true);
  await T.until('skills: Delete removes the file', (p) => !sk(p).files.includes('reference/notes.md') && sk(p).editor === null);

  // ── Upload a file into the skill (the picker answered by the driver) ──
  const asset = join(scratch, 'example.txt');
  writeFileSync(asset, 'an example file\n');
  await T.ui('pick-next', asset);
  await T.act('skills-act', 'asset', '');
  await T.until('skills: Upload file adds it to the skill', (p) => sk(p).files.includes('example.txt'));

  // ── Rename ──
  await T.act('skills-act', 'rename', '');
  await T.act('skills-field', 'rename', 'native-renamed');
  await T.act('skills-act', 'rename-go', '');
  if (!await T.until('skills: Rename renames it and keeps it open', (p) => sk(p).open?.name === 'native-renamed' && sk(p).form === '')) {
    const m2 = await T.props(); console.log('     rename:', JSON.stringify({ open: m2.areas.ops.skills.open, form: m2.areas.ops.skills.form, busy: m2.areas.ops.skills.busy, toast: m2.toast, dialog: m2.dialog }).slice(0, 800));
  }

  // ── Copy to the other project, then a second copy hits the conflict ──
  await T.act('skills-act', 'transfer', '');
  await T.act('skills-field', 'to', idx('project:demo2'));
  await T.act('skills-field', 'as', 'native-renamed');
  await T.act('skills-act', 'copy-go', '');
  await T.until('skills: Copy puts it in the other project and opens the copy', (p) => sk(p).open?.scope === 'project:demo2' && sk(p).rows.some((r) => r.scope === 'project:demo' && r.name === 'native-renamed'));
  await T.act('skills-act', 'open', 'project:demo\nnative-renamed');
  await T.until('skills: the original opens again', (p) => sk(p).open?.scope === 'project:demo' && sk(p).detail === 'data');
  await T.act('skills-act', 'transfer', '');
  await T.act('skills-field', 'to', idx('project:demo2'));
  await T.act('skills-field', 'as', 'native-renamed');
  await T.act('skills-act', 'copy-go', '');
  await T.until('skills: a copy onto an existing one asks to replace', (p) => sk(p).pending?.kind === 'replace');
  await T.act('skills-act', 'pending-replace', '');
  await T.until('skills: Replace overwrites it', (p) => sk(p).pending === null && sk(p).open?.scope === 'project:demo2');

  // ── Upload an archive-less SKILL.md with no name: it asks for one ──
  const md = join(scratch, 'SKILL.md');
  writeFileSync(md, '---\ndescription: uploaded by the native test\n---\nHello.\n');
  await T.act('skills-act', 'target', idx('project:demo'));
  await T.ui('pick-next', md);
  await T.act('skills-act', 'upload', '');
  await T.until('skills: an upload with no name asks for one', (p) => sk(p).pending?.kind === 'name' || sk(p).open?.name === 'skill');
  m = await T.mine();
  if (m.skills.pending?.kind === 'name') {
    await T.act('skills-field', 'pending-name', 'native-upload');
    await T.act('skills-act', 'pending-name', '');
    await T.until('skills: named, it installs and opens', (p) => sk(p).open?.name === 'native-upload' && sk(p).pending === null);
  }
  // Uploading the same again: replace.
  await T.ui('pick-next', md);
  await T.act('skills-act', 'upload', '');
  await T.until('skills: uploading it again asks something (a name or replace)', (p) => sk(p).pending !== null);
  await T.act('skills-act', 'pending-cancel', '');
  await T.until('skills: Cancel drops the question', (p) => sk(p).pending === null);

  // ── Install from a link: a bad link says why; the field is there ──
  await T.act('skills-field', 'link', 'not a link');
  await T.act('skills-act', 'link-go', '');
  await T.until('skills: a bad link shows the server\'s words', (p) => !sk(p).linkBusy && sk(p).error !== '');
  m = await T.mine();
  t.check('skills: the server says whether it installs from links', m.skills.linkOk !== null, m.skills.linkOk);

  // ── Synced: Show / Hide toggles the group (when the account has any) ──
  const synced = m.skills.rows.find((r) => r.toggle);
  if (synced) {
    await T.act('skills-act', 'synced-toggle', '');
    await T.until('skills: Show lists the synced skills', (p) => sk(p).rows.some((r) => r.toggle === 'Hide'));
    await T.act('skills-act', 'synced-toggle', '');
  }

  // ── Download (the save dialog answered by the driver) ──
  const zip = join(scratch, 'skill.zip');
  rmSync(zip, { force: true });
  await T.act('skills-act', 'open', 'project:demo\nnative-renamed');
  await T.until('skills: open for download', (p) => sk(p).open?.name === 'native-renamed' && sk(p).detail === 'data');
  await T.ui('pick-next', zip);
  await T.act('skills-act', 'download', '');
  const end = Date.now() + 10000;
  while (!existsSync(zip) && Date.now() < end) await new Promise((r) => setTimeout(r, 200));
  t.check('skills: Download saves the zip', existsSync(zip));

  // ── Delete (asks, then it is gone) ──
  for (const [scope, name] of [['project:demo', 'native-renamed'], ['project:demo2', 'native-renamed'], ['project:demo', 'native-upload']]) {
    // A click on the open skill closes it (the web's toggle): only open it when it isn't.
    const o = (await T.mine()).skills.open;
    if (!(o?.scope === scope && o?.name === name)) await T.act('skills-act', 'open', `${scope}\n${name}`);
    const opened = await T.until(`skills: ${name} in ${scope} opens`, (p) => sk(p).open?.name === name && sk(p).detail === 'data', 8000);
    if (!opened) continue;
    await T.act('skills-act', 'delete', '');
    pr = await T.props();
    t.check(`skills: Delete asks first (${scope})`, /cannot be undone/.test(pr.dialog?.message || ''), pr.dialog);
    await T.dialog(true);
    await T.until(`skills: deleted (${scope} ${name})`, (p) => !sk(p).rows.some((r) => r.scope === scope && r.name === name) && sk(p).open === null);
  }
  await T.snap('skills-list');

  // ── Usage: Keep the 5-hour window warm ──
  await T.act('settings-open', 'skills', 'false');
  await T.act('settings-open', 'usage', 'true');
  await T.until('usage: the warm-up switch loads', (p) => sk(p).usage.state === 'data');
  m = await T.mine();
  const was = m.skills.usage.on;
  await T.snap('skills-usage');
  await T.act('skills-act', 'us-toggle', was ? 'off' : 'on');
  if (was) { pr = await T.props(); if (pr.dialog) await T.dialog(true); }
  await new Promise((r) => setTimeout(r, 800));
  const s1 = await T.api.http('GET', '/api/settings');
  t.check('usage: the server has it', !!s1.body?.limitWarmup?.enabled === !was, s1.body?.limitWarmup);
  await T.act('skills-act', 'us-toggle', was ? 'on' : 'off');
  if (!was) { pr = await T.props(); if (pr.dialog) await T.dialog(true); }
  await new Promise((r) => setTimeout(r, 800));
  const s2 = await T.api.http('GET', '/api/settings');
  t.check('usage: and back', !!s2.body?.limitWarmup?.enabled === !!was, s2.body?.limitWarmup);

  // ── Backup & restore: not set up (nothing runs) ──
  await T.act('settings-open', 'usage', 'false');
  await T.act('settings-open', 'backup', 'true');
  await T.until('backup: the status and form load', (p) => sk(p).backup.state === 'data' && sk(p).backup.lines.length > 0);
  m = await T.mine();
  t.check('backup: not set up says so', m.skills.backup.lines.some((l) => l.key === 'Next backup' && /Set up the storage/.test(l.text)), m.skills.backup.lines);
  t.check('backup: Back up now is there (off until set up)', m.skills.backup.lines.some((l) => l.kind === 'run'), m.skills.backup.lines);
  await T.snap('skills-backup');
  // Two passphrases that differ: said under Save, nothing sent.
  await T.act('skills-field', 'bk-pass', 'one passphrase');
  await T.act('skills-field', 'bk-pass2', 'another');
  await T.act('skills-act', 'bk-save', '');
  m = await T.mine();
  t.check('backup: differing passphrases are refused', m.skills.backup.saveOut === 'The two passphrases differ', m.skills.backup.saveOut);
  // Test the connection with nothing filled: the server's words, in red.
  await T.act('skills-act', 'bk-test', '');
  await T.until('backup: Test the connection answers', (p) => sk(p).backup.testOut && sk(p).backup.testOut !== 'Checking…', 30000);
  m = await T.mine();
  t.check('backup: an empty storage fails the test', m.skills.backup.testTone === 'bad', m.skills.backup);
  // Schedule: weekly / every few hours change the row.
  await T.act('skills-field', 'bk-freq', '2');
  await T.act('skills-field', 'bk-freq', '0');
  // Show the backups: no storage, so the server's error.
  await T.act('skills-act', 'bk-list', '');
  await T.until('backup: Show lists the storage (an error with none set up)', (p) => ['error', 'empty', 'data'].includes(sk(p).backup.items), 30000);
  m = await T.mine();
  t.check('backup: the list button becomes Refresh', m.skills.backup.listBtn === 'Refresh', m.skills.backup.listBtn);
  await T.act('settings-open', 'backup', 'false');

  // ── Restore at sign-in: the form, and an unreachable storage ──
  await T.act('skills-lr-open', T.SERVER);
  m = await T.mine();
  t.check('restore: opens', m.skills.restore.open === true, m.skills.restore);
  await T.act('skills-field', 'lr-endpoint', 'http://127.0.0.1:9');
  await T.act('skills-field', 'lr-bucket', 'none');
  await T.act('skills-act', 'lr-find', '');
  await T.until('restore: Find backups answers (this server is set up, or the storage is unreachable)', (p) => !sk(p).restore.finding && (sk(p).restore.error !== '' || sk(p).restore.list !== ''), 30000);
  m = await T.mine();
  t.check('restore: the answer is shown, not a list', m.skills.restore.error !== '', m.skills.restore);
  await T.act('skills-act', 'lr-back', '');
  m = await T.mine();
  t.check('restore: "Set up a new password instead" closes it', m.skills.restore.open === false, m.skills.restore);

  rmSync(scratch, { recursive: true, force: true });
}
