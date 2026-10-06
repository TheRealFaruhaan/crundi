// Skills: uploading, managing, and what an install or update may touch.
//
// Two promises are tested here because breaking either loses somebody's work
// without an error anywhere:
//
//   1. An upload is untrusted bytes. Nothing in an archive can write outside
//      the skill's folder, create a link, or half-install.
//   2. ~/.claude/skills is shared. An install or update of Crundi brings its
//      own included skills up to date - in place, never as a second copy - and
//      does not read, change or remove any other skill there.
//
// Everything runs in a temp directory; the real home is never touched.
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, readdirSync, statSync, symlinkSync, rmSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { readZip, writeZip, crc32, looksLikeZip } from '../src/zip.js';
import * as skills from '../src/skills-store.js';
import { syncBundled, BUNDLED_MARK } from '../src/skills-sync.js';
import { mayAccess, ROLE_OWNER, ROLE_COLLABORATOR, COLLABORATOR_MCP_TOOLS } from '../src/access-policy.js';
import * as storeMark from '../src/skills-store.js';

let failed = 0;
const ok = (cond, name, extra = '') => {
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond ? '' : (extra ? ' -> ' + extra : '')}`);
  if (!cond) failed++;
};
const j = (x) => JSON.stringify(x);
const posix = process.platform !== 'win32';

// Made with Python's zipfile, not with zip.js: a .skill-shaped archive with a
// wrapping folder, a deflated SKILL.md, an executable script, macOS junk and a
// symlink entry pointing at /etc/passwd.
const EXTERNAL = Buffer.from('UEsDBBQAAAAIAOapRl0c1+JobgAAAPwBAAASAAAAcGRmLXRvb2xzL1NLSUxMLm1k7YwxDgIxDAR7v2Ilat8DrqBC1DSI+iDOYZGzo8QS4vdE8AkKutmVZpiZbNlkRk2Zw710StJvTWuo24w9ARdvDzw17jgdjshapE/jPndB9oZN2qq2YrGEXotGjDURjzTtPsq3e/X0QlGTP/0EvQFQSwMEFAAAAAgAAAAhAChOo94UAAAAEgAAABgAAABwZGYtdG9vbHMvc2NyaXB0cy9ydW4uc2hTVtRPyszTL87gSk3OyFfIz+YCAFBLAwQUAAAACADmqUZdPtLY1ggAAAAGAAAAHAAAAHBkZi10b29scy9yZWZlcmVuY2Uvbm90ZXMubWTLyy9JLeYCAFBLAwQUAAAACADmqUZdOZz7BgYAAAAEAAAAHQAAAF9fTUFDT1NYL3BkZi10b29scy8uX1NLSUxMLm1kyyrNywYAUEsDBBQAAAAIAOapRl05nPsGBgAAAAQAAAATAAAAcGRmLXRvb2xzLy5EU19TdG9yZcsqzcsGAFBLAwQUAAAAAAAAACEACrkfKQsAAAALAAAADgAAAHBkZi10b29scy9saW5rL2V0Yy9wYXNzd2RQSwECFAMUAAAACADmqUZdHNfiaG4AAAD8AQAAEgAAAAAAAAAAAAAAgAEAAAAAcGRmLXRvb2xzL1NLSUxMLm1kUEsBAhQDFAAAAAgAAAAhAChOo94UAAAAEgAAABgAAAAAAAAAAAAAAO2BngAAAHBkZi10b29scy9zY3JpcHRzL3J1bi5zaFBLAQIUAxQAAAAIAOapRl0+0tjWCAAAAAYAAAAcAAAAAAAAAAAAAACAAegAAABwZGYtdG9vbHMvcmVmZXJlbmNlL25vdGVzLm1kUEsBAhQDFAAAAAgA5qlGXTmc+wYGAAAABAAAAB0AAAAAAAAAAAAAAIABKgEAAF9fTUFDT1NYL3BkZi10b29scy8uX1NLSUxMLm1kUEsBAhQDFAAAAAgA5qlGXTmc+wYGAAAABAAAABMAAAAAAAAAAAAAAIABawEAAHBkZi10b29scy8uRFNfU3RvcmVQSwECFAMUAAAAAAAAACEACrkfKQsAAAALAAAADgAAAAAAAAAAAAAA/6GiAQAAcGRmLXRvb2xzL2xpbmtQSwUGAAAAAAYABgCYAQAA2QEAAAAA', 'base64');
// Same source, with one entry named evil/../../escaped.txt.
const TRAVERSAL = Buffer.from('UEsDBBQAAAAAAOapRl0wp81PIgAAACIAAAANAAAAZXZpbC9TS0lMTC5tZC0tLQpuYW1lOiBldmlsCmRlc2NyaXB0aW9uOiB4Ci0tLQpQSwMEFAAAAAAA5qlGXQ6MDIYGAAAABgAAABYAAABldmlsLy4uLy4uL2VzY2FwZWQudHh0Z290Y2hhUEsBAhQDFAAAAAAA5qlGXTCnzU8iAAAAIgAAAA0AAAAAAAAAAAAAAIABAAAAAGV2aWwvU0tJTEwubWRQSwECFAMUAAAAAADmqUZdDowMhgYAAAAGAAAAFgAAAAAAAAAAAAAAgAFNAAAAZXZpbC8uLi8uLi9lc2NhcGVkLnR4dFBLBQYAAAAAAgACAH8AAACHAAAAAAA=', 'base64');

const root = mkdtempSync(join(tmpdir(), 'crundi-skills-'));
const home = join(root, 'home', '.claude', 'skills');
const projA = join(root, 'projects', 'alpha');
const projB = join(root, 'projects', 'beta');
mkdirSync(home, { recursive: true }); mkdirSync(projA, { recursive: true }); mkdirSync(projB, { recursive: true });
const projects = [{ alias: 'alpha', name: 'Alpha', path: projA }, { alias: 'beta', name: 'Beta', path: projB }];
skills.configure({ globalDir: home, bundled: ['crundi'], projects: () => projects, getProject: (a) => projects.find(p => p.alias === String(a).toLowerCase()) || null });

const MD = (name, desc = 'Does a thing. Use when asked.') => `---\nname: ${name}\ndescription: ${desc}\n---\n\n# ${name}\n`;
const zipOf = (files) => writeZip(Object.entries(files).map(([name, data]) => ({ name, data: Buffer.from(data) })));
const names = (scope) => skills.list().skills.filter(s => s.scope === scope).map(s => s.name).sort();

// ─── Who may manage skills ───
{
  const owner = { role: ROLE_OWNER, projects: [], roots: {} };
  const collab = { role: ROLE_COLLABORATOR, collabKey: 'pc:sam', projects: ['alpha'], roots: { alpha: '/tmp/wt' } };
  const routes = [['GET', '/api/skills'], ['GET', '/api/skills/detail'], ['GET', '/api/skills/download'], ['POST', '/api/skills/upload'], ['POST', '/api/skills/file'], ['POST', '/api/skills/delete'], ['POST', '/api/skills/transfer']];
  ok(routes.every(([m, p]) => mayAccess(owner, m, p)), 'the owner reaches every skills route');
  ok(routes.every(([m, p]) => !mayAccess(collab, m, p)), 'a collaborator reaches none of them', routes.filter(([m, p]) => mayAccess(collab, m, p)).join(' '));
  ok(!['skill_list', 'skill_get', 'skill_install', 'skill_delete'].some(t => COLLABORATOR_MCP_TOOLS.has(t)), 'and none of the skill tools from a chat');
  ok(storeMark.BUNDLED_MARK === BUNDLED_MARK, 'the installer and the manager agree on the marker file');
}

// ─── zip.js ───
{
  const big = Buffer.alloc(300000, 'a');
  const z = writeZip([{ name: 'a/one.txt', data: Buffer.from('hello') }, { name: 'a/big.bin', data: big, mode: 0o755 }, { name: 'empty', data: Buffer.alloc(0) }]);
  const e = readZip(z);
  ok(looksLikeZip(z) && e.length === 3 && e[0].read().toString() === 'hello' && e[1].read().equals(big) && e[2].read().length === 0, 'a zip written here reads back whole');
  ok(z.length < 10000 && e[1].mode === 0o755, 'entries are compressed and keep their mode', `${z.length} bytes, mode ${e[1].mode.toString(8)}`);
  ok(crc32(Buffer.from('123456789')) === 0xcbf43926, 'crc32 matches the reference value');

  const ext = readZip(EXTERNAL);
  const md = ext.find(x => x.name === 'pdf-tools/SKILL.md');
  ok(md && /^---\nname: pdf-tools/.test(md.read().toString()), 'a zip made by another tool reads (deflate)');
  ok(ext.find(x => x.name === 'pdf-tools/link').isSymlink, 'a symlink entry is recognised as one');
  ok((ext.find(x => x.name === 'pdf-tools/scripts/run.sh').mode & 0o111) !== 0, 'the executable bit is read');

  const bad = Buffer.from(z); bad[40] ^= 0xff;
  let threw = ''; try { readZip(bad).forEach(x => x.read()); } catch (err) { threw = err.message; }
  ok(/damaged|unpacked/.test(threw), 'a corrupted entry is refused, not returned', threw);
  threw = ''; try { readZip(Buffer.from('this is not a zip at all, just some text')); } catch (err) { threw = err.message; }
  ok(threw === 'Not a zip archive', 'a non-zip is refused', threw);
  threw = ''; try { readZip(z, { maxEntry: 1000 }); } catch (err) { threw = err.message; }
  ok(/too large/.test(threw), 'an entry over the size limit is refused before unpacking', threw);
  threw = ''; try { readZip(z, { maxTotal: 1000, maxEntry: 1e9 }); } catch (err) { threw = err.message; }
  ok(/size limit/.test(threw), 'an archive over the total limit is refused', threw);
  threw = ''; try { readZip(z, { maxEntries: 2 }); } catch (err) { threw = err.message; }
  ok(/Too many files/.test(threw), 'an archive with too many entries is refused', threw);
  // A size that lies: claims 10 bytes, inflates to 300000.
  const liar = Buffer.from(z);
  const cd = liar.indexOf(Buffer.from([0x50, 0x4b, 0x01, 0x02]), liar.indexOf('a/big.bin') + 10);
  const cdBig = liar.indexOf(Buffer.from([0x50, 0x4b, 0x01, 0x02]), cd + 4);
  liar.writeUInt32LE(10, cdBig + 24);
  threw = ''; try { readZip(liar).find(x => x.name === 'a/big.bin').read(); } catch (err) { threw = err.message; }
  ok(/could not be unpacked|damaged/.test(threw), 'an entry that inflates past its declared size is stopped', threw);
  const enc = Buffer.from(z); enc.writeUInt16LE(0x0801, enc.indexOf(Buffer.from([0x50, 0x4b, 0x01, 0x02])) + 8);
  threw = ''; try { readZip(enc); } catch (err) { threw = err.message; }
  ok(/Password/.test(threw), 'a password-protected zip is refused by name', threw);
}

// ─── Front matter ───
{
  const p = skills.parseFrontmatter;
  ok(p('---\nname: a-b\ndescription: Plain text here\n---\nbody').description === 'Plain text here', 'front matter: plain values');
  ok(p('---\r\nname: "q"\r\ndescription: \'it\'\'s quoted\'\r\n---\r\n').description === "it's quoted", 'front matter: quoted values and CRLF');
  ok(p('---\nname: x\ndescription: >\n  folded\n  text\nother: 1\n---\n').description === 'folded text', 'front matter: folded block');
  ok(p('---\nname: x\ndescription: |\n  line one\n  line two\n---\n').description === 'line one\nline two', 'front matter: literal block');
  ok(p('# no front matter').has === false && p('---\nname: x\n').has === false, 'front matter: absent or unterminated is reported as absent');
}

// ─── Installing from an upload ───
{
  let r = skills.install('global', { filename: 'pdf-tools.skill', data: EXTERNAL });
  ok(r.ok && r.installed.length === 1 && r.installed[0].name === 'pdf-tools', 'a .skill archive installs under its own name', j(r));
  const dir = join(home, 'pdf-tools');
  ok(j(readdirSync(dir).sort()) === j(['SKILL.md', 'reference', 'scripts']), 'junk files are left out and nothing else is added', j(readdirSync(dir)));
  ok(!existsSync(join(dir, 'link')) && r.dropped.length === 1, 'a symlink in the archive is dropped and reported', j(r.dropped));
  if (posix) ok((statSync(join(dir, 'scripts', 'run.sh')).mode & 0o111) !== 0, 'a script keeps its executable bit');
  ok(r.installed[0].description === 'Work with PDF files. Use for merging and splitting.' && r.installed[0].files === 3 && !r.installed[0].problem, 'the listing reads its description and file count', j(r.installed[0]));
  ok(!existsSync(join(dir, BUNDLED_MARK)), 'an uploaded skill is not marked as Crundi\'s');
  ok(readdirSync(home).every(n => !n.startsWith('.crundi-')), 'no staging folders are left behind', j(readdirSync(home)));

  r = skills.install('global', { filename: 'evil.zip', data: TRAVERSAL });
  ok(!r.ok && /not allowed/.test(r.error), 'an archive with a ../ path is refused', j(r));
  ok(!existsSync(join(home, 'evil')) && !existsSync(join(root, 'home', '.claude', 'escaped.txt')) && !existsSync(join(root, 'home', 'escaped.txt')), 'and it wrote nothing, inside or outside');
  for (const bad of ['/abs.txt', 'C:/x.txt', 'a/../../b', 'a//b', 'con:stream', 'nul\0byte', 'trailing./x', '..']) {
    ok(skills.safeRel(bad) === null, `path refused: ${JSON.stringify(bad)}`);
  }
  ok(skills.safeRel('scripts\\win\\run.ps1') === 'scripts/win/run.ps1' && skills.safeRel('a/b/') === 'a/b', 'backslash and trailing-slash paths are normalised');

  // Same skill again.
  writeFileSync(join(dir, 'local-note.md'), 'mine');
  r = skills.install('global', { filename: 'pdf-tools.skill', data: EXTERNAL });
  ok(!r.ok && r.conflict && j(r.existing) === j(['pdf-tools']), 'installing over an existing skill asks first', j(r));
  ok(existsSync(join(dir, 'local-note.md')), 'and the existing one is untouched until it is confirmed');
  r = skills.install('global', { filename: 'pdf-tools.skill', data: EXTERNAL, overwrite: true });
  ok(r.ok && !existsSync(join(dir, 'local-note.md')), 'confirmed, it replaces the whole folder (no stale files)', j(r));

  // A lone SKILL.md.
  r = skills.install('project:alpha', { filename: 'SKILL.md', data: Buffer.from(MD('commit-style')) });
  ok(r.ok && existsSync(join(projA, '.claude', 'skills', 'commit-style', 'SKILL.md')), 'a lone SKILL.md installs into a project, named from its front matter', j(r));
  r = skills.install('global', { filename: 'notes.md', data: Buffer.from('# just notes') });
  ok(!r.ok && /front matter/.test(r.error), 'a markdown file with no front matter is not taken for a skill', j(r));
  r = skills.install('global', { filename: 'photo.png', data: Buffer.from([0x89, 0x50, 0x4e, 0x47, 0, 0, 0, 1]) });
  ok(!r.ok && /\.zip or \.skill/.test(r.error), 'a binary that is not an archive is refused', j(r));
  r = skills.install('global', { filename: 'empty.zip', data: zipOf({ 'readme.txt': 'hi' }) });
  ok(!r.ok && /no SKILL\.md/.test(r.error), 'an archive with no SKILL.md is refused', j(r));

  // Files at the top level, no wrapping folder; lowercase skill.md.
  r = skills.install('global', { filename: 'Meeting Notes!.zip', data: zipOf({ 'skill.md': '---\ndescription: Take notes\n---\n', 'tpl/a.md': 'x' }) });
  ok(r.ok && r.installed[0].name === 'meeting-notes' && existsSync(join(home, 'meeting-notes', 'SKILL.md')), 'top-level files install, named from the upload, with skill.md corrected to SKILL.md', j(r));
  // A GitHub-style download: the folder name is not the skill's name.
  r = skills.install('global', { filename: 'x.zip', data: zipOf({ 'repo-main/SKILL.md': MD('real-name'), 'repo-main/a.txt': '1' }) });
  ok(r.ok && r.installed[0].name === 'real-name', 'the front matter name wins over a wrapping folder name', j(r));
  // A pack.
  r = skills.install('project:beta', { filename: 'pack.zip', data: zipOf({ 'pack/skills/one/SKILL.md': MD('one'), 'pack/skills/one/x.md': '1', 'pack/skills/two/SKILL.md': MD('two'), 'pack/README.md': 'r' }) });
  ok(r.ok && j(names('project:beta')) === j(['one', 'two']), 'an archive of several skills installs each one', j(r));
  r = skills.install('project:beta', { filename: 'pack.zip', data: zipOf({ 'a/SKILL.md': MD('three'), 'b/SKILL.md': MD('two') }) });
  ok(!r.ok && r.conflict && !existsSync(join(projB, '.claude', 'skills', 'three')), 'a pack with one clash installs none of it', j(r));
  // A skill nested inside a skill is part of the outer one.
  r = skills.install('global', { filename: 'n.zip', data: zipOf({ 'outer/SKILL.md': MD('outer'), 'outer/examples/inner/SKILL.md': MD('inner') }) });
  ok(r.ok && r.installed.length === 1 && existsSync(join(home, 'outer', 'examples', 'inner', 'SKILL.md')), 'a SKILL.md below another belongs to the outer skill', j(r));
  r = skills.install('global', { filename: 'x.zip', data: zipOf({ 'x/SKILL.md': MD('Bad Name!!') }), name: 'Not Valid' });
  ok(!r.ok && /lowercase/.test(r.error), 'a chosen name has to be a valid skill name', j(r));
  r = skills.install('global', { filename: 'x.zip', data: zipOf({ 'x/SKILL.md': MD('whatever') }), name: 'chosen' });
  ok(r.ok && r.installed[0].name === 'chosen', 'a chosen name is used', j(r));
  r = skills.install('project:nope', { filename: 'x.zip', data: EXTERNAL });
  ok(!r.ok && r.error === 'Project not found', 'an unknown project is refused', j(r));
  for (const none of ['', undefined, null, '  ']) {
    r = skills.install(none, { filename: 'x.zip', data: EXTERNAL, name: 'unscoped' });
    ok(!r.ok && !existsSync(join(home, 'unscoped')), `a missing scope is refused, not taken to mean every project: ${JSON.stringify(none)}`, j(r));
  }
  ok(!skills.get('', 'pdf-tools').ok && !skills.remove(undefined, 'pdf-tools').ok && existsSync(join(home, 'pdf-tools')), 'and so are reads and deletes without one');
  r = skills.install('global', { filename: 'x.zip', data: Buffer.alloc(0) });
  ok(!r.ok, 'an empty upload is refused');
}

// ─── Included (bundled) and synced skills are read-only ───
{
  mkdirSync(join(home, 'crundi', 'reference'), { recursive: true });
  writeFileSync(join(home, 'crundi', 'SKILL.md'), MD('crundi'));
  writeFileSync(join(home, 'crundi', 'reference', 'tools.md'), 'tools');
  mkdirSync(join(home, 'synced', 'bucket-1', 'docx'), { recursive: true });
  writeFileSync(join(home, 'synced', 'bucket-1', 'manifest.json'), '{}');
  writeFileSync(join(home, 'synced', 'bucket-1', 'docx', 'SKILL.md'), MD('docx'));

  const l = skills.list().skills;
  const c = l.find(s => s.scope === 'global' && s.name === 'crundi');
  const d = l.find(s => s.scope === 'synced' && s.name === 'docx');
  ok(c && c.origin === 'bundled' && c.readOnly, 'the included skill is listed as included and read-only', j(c));
  ok(d && d.origin === 'synced' && d.readOnly && !l.some(s => s.name === 'synced' || s.name === 'bucket-1'), 'synced skills are listed, and their folders are not mistaken for skills', j(l.map(s => s.scope + '/' + s.name)));

  ok(!skills.writeFile('global', 'crundi', 'SKILL.md', { content: 'x' }).ok, 'an included skill cannot be edited');
  ok(!skills.deleteFile('global', 'crundi', 'reference/tools.md').ok, 'nor have a file deleted');
  ok(!skills.rename('global', 'crundi', 'mine').ok, 'nor be renamed');
  ok(!skills.remove('global', 'crundi').ok && existsSync(join(home, 'crundi', 'SKILL.md')), 'nor be deleted');
  ok(!skills.transfer('global', 'crundi', 'project:alpha', { move: true }).ok, 'nor be moved away');
  ok(!skills.remove('synced', 'docx').ok && !skills.writeFile('synced', 'docx', 'SKILL.md', { content: 'x' }).ok, 'a synced skill cannot be edited or deleted');
  ok(readFileSync(join(home, 'crundi', 'SKILL.md'), 'utf8') === MD('crundi'), 'and none of that changed it');

  let r = skills.install('global', { filename: 'crundi.zip', data: zipOf({ 'crundi/SKILL.md': MD('crundi', 'an impostor') }), overwrite: true });
  ok(!r.ok && /ships with Crundi/.test(r.error), 'uploading a skill under an included skill\'s name is refused, even with overwrite', j(r));
  ok(!skills.create('global', { name: 'crundi' }).ok && !skills.rename('global', 'pdf-tools', 'crundi').ok, 'so is creating or renaming to that name');
  ok(!skills.transfer('project:alpha', 'commit-style', 'global', { as: 'crundi', overwrite: true }).ok, 'and copying another skill over it');
  ok(!skills.create('global', { name: 'synced' }).ok, 'the name of the synced folder is reserved');

  r = skills.transfer('global', 'crundi', 'project:alpha');
  ok(r.ok && r.skill.origin === 'user' && !r.skill.readOnly, 'an included skill can be copied to a project, and the copy is editable', j(r));
  ok(skills.writeFile('project:alpha', 'crundi', 'SKILL.md', { content: MD('crundi', 'my version') }).ok, 'the copy can be edited');
  r = skills.transfer('synced', 'docx', 'global', { as: 'my-docx' });
  ok(r.ok && existsSync(join(home, 'my-docx', 'SKILL.md')) && existsSync(join(home, 'synced', 'bucket-1', 'docx', 'SKILL.md')), 'a synced skill can be copied out under a new name', j(r));
}

// ─── Managing ───
{
  let r = skills.create('global', { name: 'release-notes', description: 'Write release notes.\nUse at release time.' });
  ok(r.ok && r.skill.description === 'Write release notes. Use at release time.' && !r.skill.problem, 'a new skill is created with a valid SKILL.md', j(r));
  ok(!skills.create('global', { name: 'release-notes' }).ok && !skills.create('global', { name: 'Has Spaces' }).ok && !skills.create('global', { name: '../up' }).ok, 'duplicate and invalid names are refused');

  r = skills.writeFile('global', 'release-notes', 'reference/style.md', { content: 'be brief' });
  ok(r.ok && r.created && skills.readFile('global', 'release-notes', 'reference/style.md').text === 'be brief', 'a file can be added and read back', j(r));
  r = skills.writeFile('global', 'release-notes', 'assets/logo.bin', { data: Buffer.from([0, 1, 2, 3]) });
  ok(r.ok && skills.readFile('global', 'release-notes', 'assets/logo.bin').binary === true, 'a binary file can be added and is not offered as text', j(r));
  ok(skills.readFileRaw('global', 'release-notes', 'assets/logo.bin').data.length === 4, 'and downloads byte for byte');
  for (const bad of ['../outside.md', '/etc/passwd', 'a/../../b.md', '', BUNDLED_MARK]) {
    ok(!skills.writeFile('global', 'release-notes', bad, { content: 'x' }).ok && !skills.readFile('global', 'release-notes', bad).ok, `file path refused: ${JSON.stringify(bad)}`);
  }
  ok(!existsSync(join(home, 'outside.md')) && !existsSync(join(home, 'b.md')), 'and nothing was written outside the skill');
  ok(!skills.writeFile('global', 'release-notes', 'reference', { content: 'x' }).ok, 'a file cannot replace a folder');
  ok(!skills.writeFile('global', 'release-notes', 'big.txt', { content: 'x'.repeat(skills.LIMITS.editable + 1) }).ok, 'an oversized save is refused');
  if (posix) {
    const secret = join(root, 'secret.txt'); writeFileSync(secret, 'keep out');
    symlinkSync(root, join(home, 'release-notes', 'way-out'));
    symlinkSync(secret, join(home, 'release-notes', 'peek.txt'));
    ok(!skills.readFile('global', 'release-notes', 'way-out/secret.txt').ok && !skills.readFile('global', 'release-notes', 'peek.txt').ok, 'a link inside a skill cannot be read through');
    ok(!skills.writeFile('global', 'release-notes', 'way-out/planted.txt', { content: 'x' }).ok && !skills.writeFile('global', 'release-notes', 'peek.txt', { content: 'x' }).ok, 'nor written through');
    ok(!existsSync(join(root, 'planted.txt')) && readFileSync(secret, 'utf8') === 'keep out', 'and what it points at is unchanged');
    ok(!skills.get('global', 'release-notes').files.some(f => /way-out|peek/.test(f.path)), 'links are not listed as files');
    rmSync(join(home, 'release-notes', 'way-out')); rmSync(join(home, 'release-notes', 'peek.txt'));
  }
  ok(!skills.deleteFile('global', 'release-notes', 'SKILL.md').ok, 'SKILL.md cannot be deleted on its own');
  ok(skills.deleteFile('global', 'release-notes', 'assets/logo.bin').ok && !existsSync(join(home, 'release-notes', 'assets')), 'deleting a file removes the folder it leaves empty');
  const g = skills.get('global', 'release-notes');
  ok(g.ok && j(g.files.map(f => f.path)) === j(['SKILL.md', 'reference/style.md']), 'the file list is exact', j(g.files));

  r = skills.rename('global', 'release-notes', 'changelog');
  ok(r.ok && existsSync(join(home, 'changelog')) && !existsSync(join(home, 'release-notes')), 'a skill can be renamed', j(r));
  ok(/^name: changelog$/m.test(readFileSync(join(home, 'changelog', 'SKILL.md'), 'utf8')) && r.skill.title === 'changelog', 'and its front matter name follows, so Claude sees the new name');
  ok(!skills.rename('global', 'changelog', 'pdf-tools').ok && existsSync(join(home, 'changelog')), 'renaming onto another skill is refused');

  r = skills.transfer('global', 'changelog', 'project:beta');
  ok(r.ok && existsSync(join(home, 'changelog')) && existsSync(join(projB, '.claude', 'skills', 'changelog', 'reference', 'style.md')), 'copy to a project keeps the original', j(r));
  r = skills.transfer('global', 'changelog', 'project:beta');
  ok(!r.ok && r.conflict, 'copying onto an existing skill asks first', j(r));
  r = skills.transfer('global', 'changelog', 'project:alpha', { move: true });
  ok(r.ok && !existsSync(join(home, 'changelog')) && existsSync(join(projA, '.claude', 'skills', 'changelog', 'SKILL.md')), 'move takes it out of where it was', j(r));
  r = skills.transfer('project:alpha', 'changelog', 'global', { move: true });
  ok(r.ok && existsSync(join(home, 'changelog')), 'and a project skill can be made available to all projects', j(r));
  ok(!skills.transfer('global', 'changelog', 'global').ok, 'moving a skill to where it already is does nothing');

  const z = skills.exportZip('global', 'pdf-tools');
  ok(z.ok && z.filename === 'pdf-tools.zip', 'a skill downloads as a zip', j({ ok: z.ok, filename: z.filename }));
  r = skills.install('project:alpha', { filename: z.filename, data: z.data });
  ok(r.ok && r.installed[0].files === 3 && readFileSync(join(projA, '.claude', 'skills', 'pdf-tools', 'scripts', 'run.sh'), 'utf8') === readFileSync(join(home, 'pdf-tools', 'scripts', 'run.sh'), 'utf8'), 'and that zip installs again, identical', j(r));
  if (posix) ok((statSync(join(projA, '.claude', 'skills', 'pdf-tools', 'scripts', 'run.sh')).mode & 0o111) !== 0, 'with the executable bit carried through');

  const src = join(root, 'on-disk'); mkdirSync(join(src, 'sub'), { recursive: true });
  writeFileSync(join(src, 'SKILL.md'), MD('from-folder')); writeFileSync(join(src, 'sub', 'f.txt'), 'f');
  r = skills.installFromPath('global', src);
  ok(r.ok && r.installed[0].name === 'from-folder' && existsSync(join(home, 'from-folder', 'sub', 'f.txt')), 'a folder on this machine installs (the MCP tool\'s way in)', j(r));
  ok(!skills.installFromPath('global', join(root, 'missing')).ok && !skills.installFromPath('global', root).ok, 'a missing path or a folder without SKILL.md is refused');

  ok(skills.remove('global', 'from-folder').ok && !existsSync(join(home, 'from-folder')), 'a skill can be deleted');
  ok(!skills.remove('global', 'from-folder').ok && !skills.get('global', '..').ok && !skills.get('global', 'a/b').ok && !skills.get('global', '.hidden').ok, 'unknown and unsafe names are simply not found');

  // A hand-made folder with more files than the manager will list.
  mkdirSync(join(home, 'huge', 'venv'), { recursive: true }); writeFileSync(join(home, 'huge', 'SKILL.md'), MD('huge'));
  for (let i = 0; i < skills.LIMITS.files + 5; i++) writeFileSync(join(home, 'huge', 'venv', 'f' + i), '');
  const huge = skills.list().skills.find(s => s.name === 'huge');
  ok(huge.more === true && huge.files === skills.LIMITS.files, 'an oversized skill is listed as holding more than was counted', j({ more: huge.more, files: huge.files }));
  ok(!skills.exportZip('global', 'huge').ok, 'and is not downloaded as a zip that would be missing files');
  r = skills.transfer('global', 'huge', 'project:beta');
  ok(r.ok && readdirSync(join(projB, '.claude', 'skills', 'huge', 'venv')).length === skills.LIMITS.files + 5, 'but copying it copies every file', j(r.ok));
  ok(skills.backupFiles().filter(f => f.rel.startsWith('huge/')).length === skills.LIMITS.files + 6, 'and a backup carries every file');
  rmSync(join(home, 'huge'), { recursive: true }); rmSync(join(projB, '.claude', 'skills', 'huge'), { recursive: true });

  mkdirSync(join(home, 'broken')); writeFileSync(join(home, 'broken', 'README.md'), 'no skill file');
  ok(/No SKILL\.md/.test(skills.list().skills.find(s => s.name === 'broken').problem), 'a folder with no SKILL.md is listed with what is wrong with it');
  rmSync(join(home, 'broken'), { recursive: true });

  const bf = skills.backupFiles().map(f => f.rel);
  ok(bf.includes('pdf-tools/SKILL.md') && bf.includes('changelog/reference/style.md'), 'a backup carries the global skills that were put there');
  ok(!bf.some(p => p.startsWith('crundi/') || p.startsWith('synced/') || p.includes(BUNDLED_MARK)), 'and not the included or synced ones', j(bf.filter(p => /crundi|synced/.test(p))));
}

// ─── Install and update: included skills only ───
{
  const target = join(root, 'sync-home', '.claude', 'skills');
  const source = join(root, 'shipped');
  const aside = join(root, 'sync-home', '.claude', 'skills-replaced-by-crundi');
  const ship = (name, files) => { for (const [p, c] of Object.entries(files)) { mkdirSync(join(source, name, p, '..'), { recursive: true }); writeFileSync(join(source, name, p), c); } };
  const snapshot = (dir) => { const out = {}; const walk = (d, rel) => { for (const n of readdirSync(d).sort()) { const p = join(d, n), r = rel ? rel + '/' + n : n; const st = statSync(p); if (st.isDirectory()) walk(p, r); else out[r] = readFileSync(p).toString('base64') + '@' + st.mtimeMs; } }; walk(dir, ''); return j(out); };
  const run = () => syncBundled({ source, target, aside, version: '9.9.9' });

  ship('crundi', { 'SKILL.md': MD('crundi', 'v1'), 'reference/tools.md': 'tools v1', 'reference/old.md': 'dropped in v2' });

  // The user's own skills, there before Crundi ever ran.
  mkdirSync(join(target, 'marketing', 'refs'), { recursive: true });
  writeFileSync(join(target, 'marketing', 'SKILL.md'), MD('marketing'));
  writeFileSync(join(target, 'marketing', 'refs', 'brand.md'), 'brand');
  mkdirSync(join(target, 'synced', 'b', 'docx'), { recursive: true });
  writeFileSync(join(target, 'synced', 'b', 'docx', 'SKILL.md'), MD('docx'));
  const old = new Date(Date.now() - 86400000);
  for (const p of ['marketing/SKILL.md', 'marketing/refs/brand.md', 'synced/b/docx/SKILL.md']) utimesSync(join(target, p), old, old);
  const userBefore = snapshot(join(target, 'marketing')) + snapshot(join(target, 'synced'));
  const userSame = () => snapshot(join(target, 'marketing')) + snapshot(join(target, 'synced')) === userBefore;

  let r = run();
  ok(r.ok && j(r.installed) === j(['crundi']) && existsSync(join(target, 'crundi', 'reference', 'tools.md')), 'first install: the included skill is installed', j(r));
  ok(existsSync(join(target, 'crundi', BUNDLED_MARK)), 'and marked as Crundi\'s');
  ok(userSame(), 'first install: the user\'s own skills are byte-for-byte and timestamp-for-timestamp untouched');

  const first = snapshot(join(target, 'crundi'));
  r = run();
  ok(j(r.current) === j(['crundi']) && !r.installed.length && !r.updated.length && snapshot(join(target, 'crundi')) === first, 'reinstall of the same release: nothing is rewritten', j(r));
  ok(j(readdirSync(target).sort()) === j(['crundi', 'marketing', 'synced']), 'reinstall: no second copy of the included skill appears', j(readdirSync(target)));

  // The next release changes one file, drops one, adds one.
  writeFileSync(join(source, 'crundi', 'SKILL.md'), MD('crundi', 'v2'));
  rmSync(join(source, 'crundi', 'reference', 'old.md'));
  ship('crundi', { 'reference/new.md': 'added in v2' });
  r = run();
  ok(j(r.updated) === j(['crundi']) && /v2/.test(readFileSync(join(target, 'crundi', 'SKILL.md'), 'utf8')), 'update: the included skill gets the new content', j(r));
  ok(existsSync(join(target, 'crundi', 'reference', 'new.md')) && !existsSync(join(target, 'crundi', 'reference', 'old.md')), 'update: added files arrive and dropped files go');
  ok(j(readdirSync(target).sort()) === j(['crundi', 'marketing', 'synced']) && !existsSync(aside), 'update: still one copy, and nothing was set aside', j(readdirSync(target)));
  ok(userSame(), 'update: the user\'s own skills are still untouched');

  // A hand edit to the included copy is corrected by the next install.
  writeFileSync(join(target, 'crundi', 'SKILL.md'), 'edited in place');
  r = run();
  ok(j(r.updated) === j(['crundi']) && /v2/.test(readFileSync(join(target, 'crundi', 'SKILL.md'), 'utf8')), 'a changed included skill is put back to the shipped copy', j(r));

  // An install from before the marker existed (1.19.12 and earlier).
  rmSync(join(target, 'crundi', BUNDLED_MARK));
  writeFileSync(join(target, 'crundi', 'SKILL.md'), MD('crundi', 'v1 from an old installer'));
  r = run();
  ok(j(r.updated) === j(['crundi']) && !existsSync(aside) && existsSync(join(target, 'crundi', BUNDLED_MARK)), 'an unmarked crundi skill from an older installer is updated in place, not duplicated', j(r));
  rmSync(join(target, 'crundi', BUNDLED_MARK));
  r = run();
  ok(j(r.current) === j(['crundi']) && existsSync(join(target, 'crundi', BUNDLED_MARK)), 'and an identical unmarked one just gains the marker', j(r));

  // Crundi starts shipping a skill whose name the user already uses.
  ship('marketing', { 'SKILL.md': MD('marketing', 'the included one') });
  r = run();
  ok(r.setAside.length === 1 && /included one/.test(readFileSync(join(target, 'marketing', 'SKILL.md'), 'utf8')), 'a newly included skill is installed even where the user had one of that name', j(r));
  const kept = r.setAside[0] && r.setAside[0].to;
  ok(kept && readFileSync(join(kept, 'refs', 'brand.md'), 'utf8') === 'brand' && readFileSync(join(kept, 'SKILL.md'), 'utf8') === MD('marketing'), 'and the user\'s one is kept whole, outside the skills folder', kept);
  ok(j(readdirSync(target).sort()) === j(['crundi', 'marketing', 'synced']), 'so Claude sees one skill of that name, not two', j(readdirSync(target)));
  r = run();
  ok(!r.setAside.length && j(r.current) === j(['crundi', 'marketing']), 'and the next install has nothing left to do', j(r));

  // The user's skills that share no name with an included one.
  mkdirSync(join(target, 'mine')); writeFileSync(join(target, 'mine', 'SKILL.md'), MD('mine'));
  skills.configure({ globalDir: target, bundled: ['crundi', 'marketing'] });
  const copy = skills.transfer('global', 'crundi', 'global', { as: 'my-crundi' });
  ok(copy.ok && !existsSync(join(target, 'my-crundi', BUNDLED_MARK)), 'a copy of an included skill does not carry the marker', j(copy));
  const up = skills.install('global', { filename: 'pdf-tools.skill', data: EXTERNAL });
  ok(up.ok, 'an uploaded skill sits beside the included ones');
  const mine = snapshot(join(target, 'mine')) + snapshot(join(target, 'my-crundi')) + snapshot(join(target, 'pdf-tools'));

  // Crundi stops shipping one of its skills.
  rmSync(join(source, 'marketing'), { recursive: true });
  writeFileSync(join(source, 'crundi', 'SKILL.md'), MD('crundi', 'v3'));
  r = run();
  ok(j(r.removed) === j(['marketing']) && !existsSync(join(target, 'marketing')), 'a skill Crundi no longer ships is removed', j(r));
  ok(j(r.updated) === j(['crundi']) && snapshot(join(target, 'mine')) + snapshot(join(target, 'my-crundi')) + snapshot(join(target, 'pdf-tools')) === mine, 'while uploaded, hand-made and copied skills are untouched by the same update');
  ok(j(readdirSync(target).filter(n => !n.startsWith('.')).sort()) === j(['crundi', 'mine', 'my-crundi', 'pdf-tools', 'synced']), 'the folder holds exactly what it should', j(readdirSync(target)));
  ok(readdirSync(target).every(n => !n.startsWith('.crundi-')), 'no staging folders are left behind');

  // A build that ships no skills (the client-only app) must remove nothing.
  r = syncBundled({ source: join(root, 'no-such-dir'), target, aside });
  ok(r.ok && !r.removed.length && existsSync(join(target, 'crundi', 'SKILL.md')), 'a build that ships no skills removes nothing', j(r));
  mkdirSync(join(root, 'empty-ship'));
  r = syncBundled({ source: join(root, 'empty-ship'), target, aside });
  ok(!r.removed.length && existsSync(join(target, 'crundi', 'SKILL.md')), 'nor does an empty skills folder');

  if (posix) {
    const dev = join(root, 'checkout-crundi'); mkdirSync(dev); writeFileSync(join(dev, 'SKILL.md'), 'my working copy');
    rmSync(join(target, 'crundi'), { recursive: true }); symlinkSync(dev, join(target, 'crundi'));
    r = run();
    ok(r.skipped.length === 1 && readFileSync(join(dev, 'SKILL.md'), 'utf8') === 'my working copy', 'a skill name linked to another folder is left alone, and so is the folder', j(r));
  }
}

rmSync(root, { recursive: true, force: true });
console.log(failed ? `\n${failed} skills check(s) failed` : '\nAll skills checks passed');
process.exit(failed ? 1 : 0);
