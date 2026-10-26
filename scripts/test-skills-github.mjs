// Skills from a GitHub link: reading what was pasted, finding the skills in a
// repository's zip, installing one through skills-store, remembering where it
// came from, and updating from there.
//
// Offline: fetch is a fake that serves zips made with writeZip, and everything
// is written under a temp directory. Two promises matter most here:
//
//   1. Nothing from the repository is ever run, and the only hosts asked are
//      GitHub's API and its archive server.
//   2. The upload limits and path checks hold for a download as for an upload.
import { mkdtempSync, mkdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { writeZip } from '../src/zip.js';
import * as skills from '../src/skills-store.js';
import * as gh from '../src/skills-github.js';
import { CAPABILITIES, MIN_CLIENT } from '../src/client-version.js';

let failed = 0;
const ok = (cond, name, extra = '') => {
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond ? '' : (extra ? ' -> ' + extra : '')}`);
  if (!cond) failed++;
};
const j = (x) => JSON.stringify(x);

const root = mkdtempSync(join(tmpdir(), 'crundi-skills-gh-'));
const home = join(root, 'home', '.claude', 'skills');
const projA = join(root, 'projects', 'alpha');
mkdirSync(home, { recursive: true }); mkdirSync(projA, { recursive: true });
const projects = [{ alias: 'alpha', name: 'Alpha', path: projA }];
skills.configure({ globalDir: home, bundled: ['crundi'], projects: () => projects, getProject: (a) => projects.find(p => p.alias === String(a).toLowerCase()) || null });

const MD = (name, desc = 'Does a thing. Use when asked.') => `---\nname: ${name}\ndescription: ${desc}\n---\n\n# ${name}\n`;

// ─── A fake GitHub ───
// repos: 'owner/repo' -> { default, refs: { name: sha }, zips: { sha: Buffer } }
const repos = new Map();
const calls = [];
let rateLimited = false;
let noTree = false;          // the file list cannot be had: the zip is used
const sha = (n) => String(n).repeat(40).slice(0, 40);
function addRepo(full, { branch = 'main', commit = sha('a'), files, extraRefs = {} }) {
  const [, name] = full.split('/');
  const zip = writeZip(Object.entries(files).map(([p, data]) => ({ name: `${name}-${commit.slice(0, 7)}/${p}`, data: Buffer.from(data), mode: p.endsWith('.sh') ? 0o755 : 0o644 })));
  const r = repos.get(full) || { default: branch, refs: {}, zips: {} };
  r.default = branch;
  r.refs[branch] = commit;
  for (const [k, v] of Object.entries(extraRefs)) r.refs[k] = v;
  r.zips[commit] = zip;
  r.zips[branch] = zip;
  r.trees = r.trees || {};
  r.trees[commit] = Object.fromEntries(Object.entries(files).map(([p, d]) => [p, Buffer.from(d)]));
  repos.set(full, r);
}
const reply = (status, body, headers = {}) => new Response(typeof body === 'string' || Buffer.isBuffer(body) ? body : JSON.stringify(body), { status, headers });
async function fakeFetch(url) {
  calls.push(url);
  const u = new URL(url);
  if (u.hostname === 'api.github.com') {
    if (rateLimited) return reply(403, { message: 'API rate limit exceeded' });
    const tm = /^\/repos\/([^/]+)\/([^/]+)\/git\/trees\/([0-9a-f]{40})$/.exec(u.pathname);
    if (tm) {
      const files = repos.get(`${tm[1]}/${tm[2]}`)?.trees?.[tm[3]];
      if (!files || noTree) return noTree ? reply(200, { sha: tm[3], tree: [], truncated: true }) : reply(404, { message: 'Not Found' });
      ok(u.searchParams.get('recursive') === '1', 'the file list is asked for recursively');
      return reply(200, { sha: tm[3], truncated: false, tree: Object.entries(files).map(([p, d]) => ({ path: p, type: 'blob', mode: p.endsWith('.sh') ? '100755' : '100644', size: d.length })) });
    }
    const m = /^\/repos\/([^/]+)\/([^/]+)(?:\/commits\/(.+))?$/.exec(u.pathname);
    const r = m && repos.get(`${m[1]}/${m[2]}`);
    if (!r) return reply(404, { message: 'Not Found' });
    if (!m[3]) return reply(200, { default_branch: r.default });
    const ref = decodeURIComponent(m[3]);
    const c = r.refs[ref] || (/^[0-9a-f]{40}$/.test(ref) && r.zips[ref] ? ref : '');
    return c ? reply(200, { sha: c }) : reply(422, { message: 'No commit found' });
  }
  if (u.hostname === 'raw.githubusercontent.com') {
    const m = /^\/([^/]+)\/([^/]+)\/([0-9a-f]{40})\/(.+)$/.exec(u.pathname);
    const files = m && repos.get(`${m[1]}/${m[2]}`)?.trees?.[m[3]];
    const d = files && files[m[4].split('/').map(decodeURIComponent).join('/')];
    return d ? reply(200, d, { 'content-length': String(d.length) }) : reply(404, '404: Not Found');
  }
  if (u.hostname === 'codeload.github.com') {
    const m = /^\/([^/]+)\/([^/]+)\/zip\/(.+)$/.exec(u.pathname);
    const r = m && repos.get(`${m[1]}/${m[2]}`);
    const ref = m ? decodeURIComponent(m[3]) : '';
    const z = r && (r.zips[ref] || (ref === 'HEAD' && r.zips[r.default]));
    return z ? reply(200, z, { 'content-length': String(z.length) }) : reply(404, 'Not Found');
  }
  return reply(500, 'unexpected host');
}
gh.configure({ fetch: fakeFetch });

// ─── Reading what was pasted ───
{
  const p = (t) => gh.parseSource(t);
  const a = p('npx skills add https://github.com/latent-spaces/brag --skill brag-slim');
  ok(a.ok && a.owner === 'latent-spaces' && a.repo === 'brag' && a.skill === 'brag-slim' && a.ref === '' && a.path === '', 'the npx line from the card', j(a));
  ok(p('npx -y skills add https://github.com/o/r --skill=x').skill === 'x', 'with -y and --skill=x');
  ok(p('npx skills@latest add github.com/o/r -s x').skill === 'x', 'with skills@latest, no scheme, and -s');
  ok(p('https://github.com/o/r.git').repo === 'r', 'a .git link');
  ok(p('o/r').ok && p('o/r').owner === 'o', 'the owner/repo shorthand');
  const t = p('https://github.com/o/r/tree/dev/skills/pdf');
  ok(t.ok && t.ref === 'dev' && t.path === 'skills/pdf' && t.refPath === 'dev/skills/pdf', 'a tree link gives ref and path', j(t));
  const b = p('https://github.com/o/r/blob/main/skills/pdf/SKILL.md');
  ok(b.ok && b.ref === 'main' && b.path === 'skills/pdf', 'a link to the SKILL.md itself means its folder', j(b));
  ok(!p('https://gitlab.com/o/r').ok, 'another host is refused');
  ok(!p('https://evil.example/github.com/o/r').ok, 'github.com in the path of another host is refused');
  ok(!p('https://github.com.evil.example/o/r').ok, 'a host that only starts with github.com is refused');
  ok(!p('https://user@github.com/o/r').ok, 'credentials in the link are refused');
  ok(!p('https://github.com/o/r/tree/main/../../etc').ok, 'a path that climbs out is refused');
  ok(!p('https://github.com/o').ok, 'a link without a repository is refused');
  ok(!p('').ok && !p('npx skills add').ok, 'nothing to read is an error, not a guess');
}

// ─── A pack of skills under skills/ ───
addRepo('latent-spaces/brag', {
  files: {
    'README.md': '# brag',
    'package.json': JSON.stringify({ scripts: { postinstall: 'touch /tmp/crundi-pwned' } }),
    'install.sh': '#!/bin/sh\ntouch "$HOME/crundi-pwned"\n',
    'skills/brag/SKILL.md': MD('brag', 'Make a launch video.'),
    'skills/brag/scripts/render.sh': '#!/bin/sh\necho render\n',
    'skills/brag-slim/SKILL.md': MD('brag-slim', 'A smaller launch video.'),
    'skills/brag-slim/reference/notes.md': 'notes',
    'node_modules/dep/SKILL.md': MD('dep'),
  },
});

{
  const pv = await gh.preview('npx skills add https://github.com/latent-spaces/brag');
  ok(pv.ok && j(pv.skills.map(s => s.name)) === j(['brag', 'brag-slim']), 'preview lists the skills under skills/, not node_modules', j(pv));
  ok(pv.source && pv.source.ref === 'main' && pv.source.commit === sha('a') && pv.source.repo === 'latent-spaces/brag', 'and says which branch and commit', j(pv.source));
  ok(!existsSync(join(home, 'brag')), 'a preview installs nothing');

  const choose = await gh.installFromLink('global', 'https://github.com/latent-spaces/brag');
  ok(!choose.ok && choose.choose && choose.skills.length === 2, 'several skills and no choice: asks which', j(choose));

  const r = await gh.installFromLink('global', 'npx skills add https://github.com/latent-spaces/brag --skill brag-slim');
  ok(r.ok && r.installed.length === 1 && r.installed[0].name === 'brag-slim', 'the npx line installs the one skill it names', j(r));
  ok(readFileSync(join(home, 'brag-slim', 'reference', 'notes.md'), 'utf8') === 'notes', 'with its files');
  ok(!existsSync(join(home, 'brag')), 'and not the other one');
  ok(!existsSync('/tmp/crundi-pwned') && !existsSync(join(root, 'home', 'crundi-pwned')), 'nothing from the repository was run');
  ok(calls.every(u => /^https:\/\/(api\.github\.com|raw\.githubusercontent\.com|codeload\.github\.com)\//.test(u)), 'only GitHub was asked', j(calls));
  ok(!calls.some(u => u.includes('codeload')), 'with the file list, the whole repository was not downloaded');
  ok(!calls.some(u => u.includes('/skills/brag/scripts/') || u.endsWith('install.sh') || u.endsWith('README.md') || u.endsWith('package.json')), 'only the chosen skill\'s files (and the SKILL.md of each, for the list) were fetched', j(calls.filter(u => u.includes('raw.'))));

  const listed = skills.list().skills.find(s => s.name === 'brag-slim');
  ok(listed.source && listed.source.repo === 'latent-spaces/brag' && listed.source.path === 'skills/brag-slim' && listed.source.commit === sha('a') && listed.source.ref === 'main', 'the source is remembered on the skill', j(listed.source));
  ok(listed.origin === 'user' && !listed.readOnly, 'and it is the user\'s own skill');
  const d = skills.get('global', 'brag-slim');
  ok(!d.files.some(f => f.path === skills.SOURCE_MARK), 'the source record is not one of its files', j(d.files));
  ok(!skills.writeFile('global', 'brag-slim', skills.SOURCE_MARK, { content: '{}' }).ok, 'and the editor cannot write it');
  ok(!skills.readFile('global', 'brag-slim', skills.SOURCE_MARK).ok, 'or read it as a file');
  const exported = skills.exportZip('global', 'brag-slim');
  ok(exported.ok && !exported.data.includes(Buffer.from(skills.SOURCE_MARK)), 'a download leaves it out');
  ok(skills.backupFiles().some(f => f.rel === 'brag-slim/' + skills.SOURCE_MARK), 'a backup keeps it');

  const again = await gh.installFromLink('global', 'https://github.com/latent-spaces/brag', { skill: 'brag-slim' });
  ok(!again.ok && again.conflict && j(again.existing) === j(['brag-slim']), 'installing it again is a conflict, as for an upload', j(again));
  const over = await gh.installFromLink('global', 'https://github.com/latent-spaces/brag', { skill: 'brag-slim', overwrite: true });
  ok(over.ok, 'and overwrite replaces it');

  const missing = await gh.installFromLink('global', 'https://github.com/latent-spaces/brag', { skill: 'nope' });
  ok(!missing.ok && /brag, brag-slim/.test(missing.error), 'an unknown skill name says what there is', missing.error);

  const all = await gh.installFromLink('project:alpha', 'https://github.com/latent-spaces/brag', { all: true });
  ok(all.ok && all.installed.map(s => s.name).sort().join() === 'brag,brag-slim', 'all:true installs every skill, into a project', j(all));
  const brag = skills.list().skills.find(s => s.scope === 'project:alpha' && s.name === 'brag');
  ok(brag.source && brag.source.path === 'skills/brag', 'each with its own path');
  ok((readFileSync(join(projA, '.claude', 'skills', 'brag', 'scripts', 'render.sh')).length > 0), 'scripts are copied as files');

  const t = await gh.installFromLink('global', 'https://github.com/latent-spaces/brag/tree/main/skills/brag');
  ok(t.ok && t.installed[0].name === 'brag', 'a tree link to one skill installs it without asking', j(t));
  skills.remove('global', 'brag');
}

// ─── One skill at the root, a branch with a slash ───
addRepo('solo/one-skill', { files: { 'SKILL.md': MD('solo-helper'), 'more/x.md': 'x' }, commit: sha('b'), extraRefs: { 'feature/new': sha('b') } });
{
  const r = await gh.installFromLink('global', 'solo/one-skill');
  ok(r.ok && r.installed[0].name === 'solo-helper', 'a SKILL.md at the root is the one skill', j(r));
  const s = skills.list().skills.find(x => x.name === 'solo-helper');
  ok(s.source.path === '' && s.source.repo === 'solo/one-skill', 'with an empty path', j(s.source));
  skills.remove('global', 'solo-helper');

  const f = await gh.preview('https://github.com/solo/one-skill/tree/feature/new');
  ok(f.ok && f.source.ref === 'feature/new' && f.source.path === '', 'a branch name with a slash is found', j(f.source || f));

  const bad = await gh.preview('https://github.com/solo/one-skill/tree/nope');
  ok(!bad.ok && /no branch/.test(bad.error), 'an unknown branch is a plain error', bad.error);
  const gone = await gh.preview('https://github.com/nobody/nothing');
  ok(!gone.ok && /no public GitHub repository/.test(gone.error), 'an unknown repository is a plain error', gone.error);
}

// ─── Rate limited: download by name, no commit ───
{
  rateLimited = true;
  calls.length = 0;
  const pv = await gh.preview('solo/one-skill');
  ok(pv.ok && pv.skills[0].name === 'solo-helper', 'with the API rate limited it still lists', j(pv));
  ok(calls.some(u => u.includes('codeload.github.com/solo/one-skill/zip/HEAD')), 'from the zip of the default branch', j(calls));
  rateLimited = false;
}

// ─── A file list too long for one answer: the zip ───
{
  noTree = true;
  calls.length = 0;
  const r = await gh.installFromLink('global', 'https://github.com/latent-spaces/brag', { skill: 'brag' });
  ok(r.ok && r.installed[0].name === 'brag', 'installs from the zip instead', j(r));
  ok(calls.some(u => u.includes('codeload')) && !calls.some(u => u.includes('raw.githubusercontent')), 'which it downloaded once', j(calls));
  ok(existsSync(join(home, 'brag', 'scripts', 'render.sh')), 'with the same files');
  if (process.platform !== 'win32') ok((statSync(join(home, 'brag', 'scripts', 'render.sh')).mode & 0o111) !== 0, 'and its script still runnable by Claude');
  skills.remove('global', 'brag');
  noTree = false;
}

// ─── Update from the source ───
{
  addRepo('up/date', { files: { 'skills/tool/SKILL.md': MD('tool', 'Version one.') }, commit: sha('c') });
  const r = await gh.installFromLink('global', 'https://github.com/up/date');
  ok(r.ok, 'installed the first version');
  const same = await gh.update('global', 'tool');
  ok(same.ok && same.upToDate, 'update with no new commit changes nothing', j(same));
  addRepo('up/date', { files: { 'skills/tool/SKILL.md': MD('tool', 'Version two.'), 'skills/tool/new.md': 'new' }, commit: sha('d') });
  const u = await gh.update('global', 'tool');
  ok(u.ok && !u.upToDate, 'update installs the new commit', j(u));
  ok(/Version two/.test(readFileSync(join(home, 'tool', 'SKILL.md'), 'utf8')) && existsSync(join(home, 'tool', 'new.md')), 'with the new files');
  ok(skills.list().skills.find(s => s.name === 'tool').source.commit === sha('d'), 'and the new commit remembered');
  ok(!(await gh.update('global', 'nothing-here')).ok, 'updating a skill that does not exist fails');
  skills.create('global', { name: 'handmade' });
  const hm = await gh.update('global', 'handmade');
  ok(!hm.ok && /not installed from a link/.test(hm.error), 'a skill not from a link has nothing to update from');
  addRepo('up/date', { files: { 'README.md': 'empty now' }, commit: sha('e') });
  const g = await gh.update('global', 'tool');
  ok(!g.ok && /no longer has this skill/.test(g.error), 'a skill that left the repository says so', g.error);
  ok(existsSync(join(home, 'tool', 'SKILL.md')), 'and the installed copy is kept');
}

// ─── Untrusted archives ───
{
  // An upload cannot bring a source record of its own.
  const zip = writeZip([{ name: 'fake/SKILL.md', data: Buffer.from(MD('fake')) }, { name: 'fake/' + skills.SOURCE_MARK, data: Buffer.from(JSON.stringify({ repo: 'evil/repo', ref: 'main', path: '' })) }]);
  const r = skills.install('global', { filename: 'fake.zip', data: zip });
  ok(r.ok && !skills.list().skills.find(s => s.name === 'fake').source, 'an uploaded archive cannot set a source', j(r));
  ok(!existsSync(join(home, 'fake', skills.SOURCE_MARK)), 'the file is not written');

  addRepo('evil/climb', { files: { 'skills/evil/SKILL.md': MD('evil'), 'skills/evil/../../../escaped.txt': 'gotcha' }, commit: sha('f') });
  const e = await gh.installFromLink('global', 'evil/climb');
  ok(!e.ok && !existsSync(join(home, 'evil')) && !existsSync(join(root, 'escaped.txt')) && !existsSync(join(home, '..', 'escaped.txt')), 'a path that climbs out of the skill installs nothing', j(e));

  const many = { 'skills/big/SKILL.md': MD('big') };
  for (let i = 0; i < skills.LIMITS.files + 1; i++) many[`skills/big/f${i}.txt`] = 'x';
  addRepo('evil/many', { files: many, commit: sha('1') });
  const m = await gh.installFromLink('global', 'evil/many');
  ok(!m.ok && /too many files/.test(m.error) && !existsSync(join(home, 'big')), 'the upload file-count limit holds', m.error);

  // A repository zip larger than it may be (the file list cannot be had, so
  // the whole zip is downloaded).
  noTree = true;
  const realFetch = fakeFetch;
  gh.configure({ fetch: async (url) => (url.includes('codeload') ? new Response('x', { status: 200, headers: { 'content-length': String(3 * skills.LIMITS.upload + 1) } }) : realFetch(url)) });
  const L = await gh.preview('solo/one-skill');
  ok(!L.ok && /too large/.test(L.error), 'a zip over the limit is refused by its length', L.error);
  // And one that streams past it without saying.
  gh.configure({ fetch: async (url) => {
    if (!url.includes('codeload')) return realFetch(url);
    let sent = 0;
    const body = new ReadableStream({ pull(c) { if (sent > 3 * skills.LIMITS.upload) { c.close(); return; } const chunk = new Uint8Array(1024 * 1024); sent += chunk.length; c.enqueue(chunk); } });
    return new Response(body, { status: 200 });
  } });
  const S = await gh.preview('solo/one-skill');
  ok(!S.ok && /too large/.test(S.error), 'and one that streams past it is cut off', S.error);
  gh.configure({ fetch: fakeFetch });
  noTree = false;

  // One file of a skill larger than an upload may hold, by the file list.
  addRepo('evil/huge', { files: { 'SKILL.md': MD('huge'), 'big.bin': 'x'.repeat(16) }, commit: sha('2') });
  repos.get('evil/huge').trees[sha('2')]['big.bin'] = { length: skills.LIMITS.file + 1 };
  calls.length = 0;
  const H = await gh.installFromLink('global', 'evil/huge');
  ok(!H.ok && /too large/.test(H.error) && !existsSync(join(home, 'huge')), 'a file over the upload limit is refused before it is downloaded', H.error);
  ok(!calls.some(u => u.endsWith('/big.bin')), 'without asking for it');
}

// ─── Wiring ───
{
  const web = readFileSync(new URL('../src/webapp.js', import.meta.url), 'utf8');
  const mcp = readFileSync(new URL('../src/mcp-stdio.js', import.meta.url), 'utf8');
  const html = readFileSync(new URL('../src/webapp-html.js', import.meta.url), 'utf8');
  ok(CAPABILITIES.includes('skills-install-link'), 'the server lists the skills-install-link capability');
  ok(MIN_CLIENT.web === '1.20.0-dev.1' && MIN_CLIENT.android === '1.0.0-dev.10', 'and no minimum client was raised');
  ok(web.includes("'/api/skills/install-from-link'") && web.includes("'/api/skills/update-from-source'"), 'the two new routes exist');
  ok(web.includes("path === '/api/skills/upload'"), 'the upload route is still there');
  const tool = /name: 'skill_install'[^\n]*/.exec(mcp)[0];
  ok(/url: \{/.test(tool) && /skill: \{/.test(tool) && /all: \{/.test(tool), 'skill_install takes url, skill and all');
  ok(html.includes('/api/skills/install-from-link') && html.includes('skills-install-link'), 'the web page uses the route, gated on the capability');
  ok(!/child_process|\bspawn\(|execSync|execFile|\bimport\(/.test(readFileSync(new URL('../src/skills-github.js', import.meta.url), 'utf8')), 'skills-github.js runs no processes');
}

console.log(failed ? `\n${failed} skills-from-a-link check(s) FAILED` : '\nAll skills-from-a-link checks passed');
process.exit(failed ? 1 : 0);
