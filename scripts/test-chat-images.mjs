#!/usr/bin/env node
// Images Claude shows in a chat: what is accepted, what is stored, what is served.
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, utimesSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const tmp = mkdtempSync(join(tmpdir(), 'crundi-chatimg-'));
writeFileSync(join(tmp, 'env'), '');
process.env.DATA_DIR = join(tmp, 'data');
process.env.DOTENV_PATH = join(tmp, 'env');
mkdirSync(process.env.DATA_DIR, { recursive: true });

let failed = 0;
const ok = (cond, name, detail = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`); if (!cond) failed++; };

const { addImages, readImage, sweep, MAX_PER_CALL } = await import('../src/chat-images.js');

const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');
const png = join(tmp, 'a.png'); writeFileSync(png, PNG);
const svg = join(tmp, 'x.svg'); writeFileSync(svg, '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>');
const fake = join(tmp, 'fake.png'); writeFileSync(fake, '<html><script>alert(1)</script></html>');
const S = 'abcdef0123456789';

const one = addImages(S, [{ path: png, caption: '  The   login page  ' }]);
ok(one.ok && one.images.length === 1 && /^[0-9a-f]{16}\.png$/.test(one.images[0].name) && one.images[0].caption === 'The login page', 'stores an image under a name of its own, with a tidied caption', JSON.stringify(one));
const got = readImage(S, one.images[0].name);
ok(got && got.mime === 'image/png' && got.buffer.equals(PNG), 'serves back the same bytes as image/png');
rmSync(png);
ok(!!readImage(S, one.images[0].name), 'the copy survives the original being deleted');

ok(!addImages(S, [{ path: svg }]).ok, 'an SVG is refused (it can carry script)');
ok(!addImages(S, [{ path: fake }]).ok, 'a .png that is really HTML is refused: the bytes decide, not the name');
ok(!addImages(S, [{ path: join(tmp, 'nope.png') }]).ok, 'a missing file is an error');
ok(!addImages(S, []).ok, 'no images is an error');
writeFileSync(png, PNG);
ok(!addImages(S, Array.from({ length: MAX_PER_CALL + 1 }, () => ({ path: png }))).ok, `more than ${MAX_PER_CALL} in one call is refused`);
const before = addImages(S, [{ path: png }, { path: join(tmp, 'nope.png') }]);
ok(!before.ok && /Image 2/.test(before.error), 'one bad image fails the whole call and says which', before.error);
ok(!addImages('../../etc', [{ path: png }]).ok && !addImages('', [{ path: png }]).ok, 'a session id that is not an id is refused');

ok(readImage(S, '../../../etc/passwd') === null && readImage(S, 'x.png') === null && readImage('../' + S, one.images[0].name) === null, 'reading: only our own names, in a real session folder');
ok(readImage('otherchat0000000', one.images[0].name) === null, 'reading: one chat cannot fetch another chat\'s image by name');

// Housekeeping
const old = addImages('oldchat000000000', [{ path: png }]);
const oldFile = join(process.env.DATA_DIR, 'chat-images', 'oldchat000000000', old.images[0].name);
const longAgo = new Date(Date.now() - 60 * 24 * 3600 * 1000);
utimesSync(oldFile, longAgo, longAgo);
ok(sweep() === 1 && !existsSync(oldFile) && !!readImage(S, one.images[0].name), 'a chat untouched for weeks loses its pictures; a recent one keeps them');

// The wiring that keeps it where it belongs
const bridge = readFileSync(new URL('../src/mcp-stdio.js', import.meta.url), 'utf8');
ok(/CHAT_ONLY_TOOLS = new Set\(\['show_image'\]\)/.test(bridge) && /IN_CHAT \|\| !CHAT_ONLY_TOOLS\.has/.test(bridge), 'the tool is only listed inside a chat');
ok(/name === 'show_image'\) \{\s*delete args\.sessionId;/.test(bridge), 'which chat it shows in comes from the process, never from the caller');
const { COLLABORATOR_MCP_TOOLS } = await import('../src/access-policy.js');
ok(!COLLABORATOR_MCP_TOOLS.has('show_image'), 'not offered to collaborators: the file is read outside their sandbox');
const { BASE } = await import('../src/system-prompt.js');
ok(/show_image/.test(BASE) && /NOT visible to the person/.test(BASE), 'the base prompt says when to use it, and that a screenshot Claude took is not seen by the person');
const skill = readFileSync(new URL('../skills/crundi/SKILL.md', import.meta.url), 'utf8');
ok(/show_image/.test(skill), 'the Crundi skill covers it');

try { rmSync(tmp, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll chat-image checks passed.');
process.exit(failed ? 1 : 0);
