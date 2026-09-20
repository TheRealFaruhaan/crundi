// Every artifact that carries the server carries an ALLOWLIST of top-level
// directories - the release workflow stages `cp -r src scripts assets ...`, the
// Dockerfile has its own COPY lines, electron-builder has `files` and
// `extraResources`, and install.sh copies into the prefix. Add a new top-level
// directory and every one of those lists has to learn about it separately.
//
// skills/ was added and three of them were not updated. Nothing failed: the
// Linux tarball shipped an install.sh whose skill block is wrapped in
// `if [ -d "$SRC/skills" ]`, so with no skills/ beside it the whole step became
// a silent no-op. The release looked clean and installed nothing.
//
// This fails the build instead of letting that ship again.
import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' -> ' + extra}`);
  if (!ok) failed++;
};

const read = (p) => readFileSync(join(root, p), 'utf8');

// ─── The source of truth ───
const pkg = JSON.parse(read('package.json'));

// ─── Linux + Windows server staging (release-extra.yml) ───
const extra = read('.github/workflows/release-extra.yml');
const stagingLines = extra.split('\n').filter((l) => /^\s*cp -r src\b/.test(l));
check('release-extra.yml has both staging copy lines', stagingLines.length === 2,
  `found ${stagingLines.length}`);
for (const line of stagingLines) {
  const target = line.trim().split(/\s+/).pop();
  check(`staging copy includes skills (-> ${target})`, /\bskills\b/.test(line), line.trim());
}

// ─── Container image ───
const dockerfile = read('docker/Dockerfile');
check('Dockerfile copies skills', /^COPY\s+skills\b/m.test(dockerfile),
  'no "COPY skills" line');

// ─── Windows desktop app (electron-builder) ───
const resources = pkg.build?.extraResources || [];
check('electron-builder ships skills as an extra resource',
  resources.some((r) => r.from === 'skills'),
  JSON.stringify(resources));

// ─── The installers that place it in the user's Claude config ───
const install = read('scripts/install.sh');
check('install.sh copies skills into the prefix',
  /for item in .*\bskills\b.*; do/.test(install),
  'the prefix copy loop does not list skills');
check('install.sh installs skills into ~/.claude/skills',
  /\.claude\/skills/.test(install) && /cp -r "\$skill"/.test(install),
  'no skill install block');

const nsh = read('build/installer.nsh');
check('NSIS installs skills into the user profile',
  /\$PROFILE\\\.claude\\skills/.test(nsh) && /CopyFiles/.test(nsh),
  'no skills copy in customInstall');
check('NSIS guards a build that ships no skills',
  /IfFileExists\s+"\$INSTDIR\\resources\\skills/.test(nsh),
  'the copy is unguarded, so the client build would error');

// ─── The skill itself ───
check('the skill has a SKILL.md with frontmatter',
  /^---\nname:\s*crundi\n/m.test(read('skills/crundi/SKILL.md')),
  'missing or malformed frontmatter');

// It ships to every machine that installs Crundi, so it must name no install.
const skillFiles = [
  'skills/crundi/SKILL.md',
  'skills/crundi/reference/mcp-tools.md',
  'skills/crundi/reference/developing-on-crundi.md',
  'skills/crundi/reference/platform-linux.md',
  'skills/crundi/reference/platform-windows.md',
];
// Any IPv4 that is not a documentation-safe one, and any bare email address.
const IPV4 = /\b(?!127\.0\.0\.1\b)(?!0\.0\.0\.0\b)\d{1,3}(?:\.\d{1,3}){3}\b/;
const EMAIL = /\b[\w.+-]+@[\w-]+\.[\w.-]+\b/;
for (const f of skillFiles) {
  const text = read(f);
  const ip = text.match(IPV4);
  const mail = text.match(EMAIL);
  check(`${f} names no specific host`, !ip, ip && ip[0]);
  check(`${f} names no email address`, !mail, mail && mail[0]);
}

console.log(failed ? `\nskills ship: ${failed} FAILED` : '\nskills ship: all passed');
process.exit(failed ? 1 : 0);
