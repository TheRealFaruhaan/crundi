#!/usr/bin/env node
// Media Claude shows in a chat: what is accepted, what is stored, what comes
// back for the page to lay out, and what is served.
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, utimesSync, existsSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

const tmp = mkdtempSync(join(tmpdir(), 'crundi-chatmedia-'));
writeFileSync(join(tmp, 'env'), '');
process.env.DATA_DIR = join(tmp, 'data');
process.env.DOTENV_PATH = join(tmp, 'env');
mkdirSync(process.env.DATA_DIR, { recursive: true });

let failed = 0;
const ok = (cond, name, detail = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`); if (!cond) failed++; };

const { addMedia, locate, locateFile, sweep, sniffAv, imageSize, MAX_PER_CALL, LIMITS } = await import('../src/chat-media.js');

// ─── fixtures ───
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');
const png = join(tmp, 'a.png'); writeFileSync(png, PNG);
const svg = join(tmp, 'x.svg'); writeFileSync(svg, '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>');
const fake = join(tmp, 'fake.png'); writeFileSync(fake, '<html><script>alert(1)</script></html>');
// A WAV written by hand: 0.1 s of silence, 8 kHz mono 8-bit.
const wavData = Buffer.alloc(800, 128);
const wavHead = Buffer.alloc(44);
wavHead.write('RIFF', 0); wavHead.writeUInt32LE(36 + wavData.length, 4); wavHead.write('WAVE', 8); wavHead.write('fmt ', 12);
wavHead.writeUInt32LE(16, 16); wavHead.writeUInt16LE(1, 20); wavHead.writeUInt16LE(1, 22); wavHead.writeUInt32LE(8000, 24);
wavHead.writeUInt32LE(8000, 28); wavHead.writeUInt16LE(1, 32); wavHead.writeUInt16LE(8, 34); wavHead.write('data', 36); wavHead.writeUInt32LE(wavData.length, 40);
const wav = join(tmp, 'tone.wav'); writeFileSync(wav, Buffer.concat([wavHead, wavData]));
const doc = join(tmp, 'Q3 report (final).xlsx'); writeFileSync(doc, 'not really a spreadsheet');
let haveFf = false; try { execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' }); execFileSync('ffprobe', ['-version'], { stdio: 'ignore' }); haveFf = true; } catch { /* none */ }
const mp4 = join(tmp, 'clip.mp4');
if (haveFf) { try { execFileSync('ffmpeg', ['-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc=size=320x180:rate=10:duration=3', '-pix_fmt', 'yuv420p', mp4], { stdio: 'ignore', timeout: 30000 }); } catch { haveFf = false; } }
const S = 'abcdef0123456789';

// ─── what a file is ───
ok(sniffAv(Buffer.concat([Buffer.from([0, 0, 0, 24]), Buffer.from('ftypisom'), Buffer.alloc(8)])) === 'mp4', 'sniff: an MP4 by its ftyp box');
ok(sniffAv(Buffer.concat([Buffer.from([0, 0, 0, 24]), Buffer.from('ftypM4A '), Buffer.alloc(8)])) === 'm4a', 'sniff: M4A');
ok(sniffAv(Buffer.concat([Buffer.from([0x1a, 0x45, 0xdf, 0xa3]), Buffer.alloc(12)])) === 'webm', 'sniff: WebM');
ok(sniffAv(Buffer.concat([Buffer.from('ID3'), Buffer.alloc(12)])) === 'mp3' && sniffAv(Buffer.concat([Buffer.from([0xff, 0xfb, 0x90, 0]), Buffer.alloc(12)])) === 'mp3', 'sniff: MP3, tagged and bare');
ok(sniffAv(readFileSync(wav)) === 'wav' && sniffAv(Buffer.from('<html><body>hello there</body>')) === '', 'sniff: WAV yes, HTML no');
ok(JSON.stringify(imageSize(PNG, 'png')) === '{"w":1,"h":1}', 'size: PNG');
const gif = Buffer.concat([Buffer.from('GIF89a'), Buffer.from([0x40, 0x01, 0xf0, 0x00]), Buffer.alloc(8)]);
ok(JSON.stringify(imageSize(gif, 'gif')) === '{"w":320,"h":240}', 'size: GIF');
const jpg = Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x2c, 0x02, 0x80]), Buffer.alloc(12)]);
ok(JSON.stringify(imageSize(jpg, 'jpg')) === '{"w":640,"h":300}', 'size: JPEG, found past the first segment');

// ─── images ───
const one = addMedia(S, 'image', [{ path: png, caption: '  The   login page  ' }]);
ok(one.ok && one.items.length === 1 && /^[0-9a-f]{16}\.png$/.test(one.items[0].name) && one.items[0].caption === 'The login page', 'image: stored under a name of its own, with a tidied caption', JSON.stringify(one));
ok(one.items[0].w === 1 && one.items[0].h === 1 && one.items[0].kind === 'image', 'image: comes back with its pixel size, so the page can reserve the space');
const got = locate(S, one.items[0].name);
ok(got && got.mime === 'image/png' && readFileSync(got.file).equals(PNG), 'image: served back as the same bytes, as image/png');
rmSync(png);
ok(!!locate(S, one.items[0].name), 'image: the copy survives the original being deleted');
writeFileSync(png, PNG);
ok(!addMedia(S, 'image', [{ path: svg }]).ok, 'image: an SVG is refused (it can carry script)');
ok(!addMedia(S, 'image', [{ path: fake }]).ok, 'image: a .png that is really HTML is refused: the bytes decide, not the name');
ok(!addMedia(S, 'image', [{ path: wav }]).ok, 'image: an audio file is not an image');

// ─── audio ───
const au = addMedia(S, 'audio', [{ path: wav, caption: 'Voice-over, take 2' }]);
ok(au.ok && /\.wav$/.test(au.items[0].name) && au.items[0].kind === 'audio' && au.items[0].filename === 'tone.wav', 'audio: a WAV is stored and keeps its real name for display', JSON.stringify(au));
ok(locate(S, au.items[0].name).mime === 'audio/wav', 'audio: served as audio/wav');
ok(!addMedia(S, 'audio', [{ path: png }]).ok && !addMedia(S, 'audio', [{ path: fake }]).ok, 'audio: a picture or a page is not audio');
if (haveFf) ok(au.items[0].duration > 0, 'audio: its duration is known', String(au.items[0].duration));

// ─── video ───
if (haveFf) {
  const v = addMedia(S, 'video', [{ path: mp4 }]);
  ok(v.ok && v.items[0].kind === 'video' && /\.mp4$/.test(v.items[0].name), 'video: an MP4 is stored', JSON.stringify(v));
  ok(v.items[0].w === 320 && v.items[0].h === 180 && Math.round(v.items[0].duration) === 3, 'video: size and duration come back for layout', JSON.stringify(v.items[0]));
  ok(/\.poster\.jpg$/.test(v.items[0].poster || '') && locate(S, v.items[0].poster).mime === 'image/jpeg', 'video: a poster frame is made, so there is a picture before it plays');
  ok(locate(S, v.items[0].name).size === statSync(mp4).size, 'video: located with its size (ranges are served from it)');
  ok(!addMedia(S, 'video', [{ path: wav }]).ok, 'video: an audio file is refused, and says to use show_audio');
} else console.log('SKIP  video with ffmpeg: not installed here. Size, duration and poster NOT verified on this machine.');
const bare = join(tmp, 'bare.mp4'); writeFileSync(bare, Buffer.concat([Buffer.from([0, 0, 0, 24]), Buffer.from('ftypisom'), Buffer.alloc(64)]));
ok(addMedia(S, 'video', [{ path: bare }]).ok, 'video: accepted by its header even where nothing can probe it (no size or poster then)');
ok(!addMedia(S, 'video', [{ path: fake }]).ok, 'video: a page is not a video');

// ─── files (offered, not copied) ───
const f = addMedia(S, 'file', [{ path: doc, caption: 'Third quarter' }]);
ok(f.ok && f.items[0].kind === 'file' && f.items[0].filename === 'Q3 report (final).xlsx' && f.items[0].ext === 'xlsx' && f.items[0].size === 24, 'file: offered with its name, size and extension', JSON.stringify(f));
ok(!f.items[0].path && !JSON.stringify(f.items).includes(tmp), 'file: the path on disk is not sent to the page');
const lf = locateFile(S, f.items[0].id);
ok(lf && lf.file === doc && lf.name === 'Q3 report (final).xlsx', 'file: resolves to the original (not a copy)');
writeFileSync(doc, 'changed since it was offered');
ok(locateFile(S, f.items[0].id).size === 28, 'file: the download is the file as it is NOW');
rmSync(doc);
ok(locateFile(S, f.items[0].id).gone === true, 'file: once the original is gone, that is reported, not a broken download');
ok(locateFile(S, '0000000000000000') === null && locateFile('otherchat0000000', f.items[0].id) === null, 'file: an id that was never offered, or another chat\'s, is nothing');

// ─── the rules common to all ───
ok(!addMedia(S, 'image', []).ok, 'no items is an error');
ok(!addMedia(S, 'image', Array.from({ length: MAX_PER_CALL + 1 }, () => ({ path: png }))).ok, `more than ${MAX_PER_CALL} in one call is refused`);
const partial = addMedia(S, 'image', [{ path: png }, { path: join(tmp, 'nope.png') }]);
ok(!partial.ok && /Item 2/.test(partial.error), 'one bad item fails the whole call and says which', partial.error);
ok(!addMedia('../../etc', 'image', [{ path: png }]).ok && !addMedia('', 'image', [{ path: png }]).ok, 'a session id that is not an id is refused');
ok(locate(S, '../../../etc/passwd') === null && locate(S, 'x.png') === null && locate('../' + S, one.items[0].name) === null, 'reading: only our own names, in a real session folder');
ok(locate('otherchat0000000', one.items[0].name) === null, 'reading: one chat cannot fetch another chat\'s media by name');
const keep = LIMITS.image; LIMITS.image = 10;
ok(!addMedia(S, 'image', [{ path: png }]).ok, 'over the size limit for its kind is refused');
LIMITS.image = keep;

// ─── pictures stored by 1.19.19, under the old folder, are still found ───
const legacyDir = join(process.env.DATA_DIR, 'chat-images', 'legacychat000000');
mkdirSync(legacyDir, { recursive: true });
writeFileSync(join(legacyDir, '0123456789abcdef.png'), PNG);
ok(!!locate('legacychat000000', '0123456789abcdef.png'), 'a picture stored by the previous version is still served');

// ─── housekeeping ───
const old = addMedia('oldchat000000000', 'image', [{ path: png }]);
const oldFile = join(process.env.DATA_DIR, 'chat-media', 'oldchat000000000', old.items[0].name);
const longAgo = new Date(Date.now() - 60 * 24 * 3600 * 1000);
utimesSync(oldFile, longAgo, longAgo);
utimesSync(join(legacyDir, '0123456789abcdef.png'), longAgo, longAgo);
ok(sweep() === 2 && !existsSync(oldFile) && !!locate(S, one.items[0].name), 'a chat untouched for weeks loses its media; a recent one keeps it');

// ─── the wiring that keeps it where it belongs ───
const bridge = readFileSync(new URL('../src/mcp-stdio.js', import.meta.url), 'utf8');
for (const t of ['show_image', 'show_video', 'show_audio', 'show_file', 'show_embed']) ok(new RegExp(`name: '${t}'`).test(bridge), `${t} is offered to Claude`);
ok(/CHAT_ONLY_TOOLS = new Set\(\['show_image', 'show_video', 'show_audio', 'show_file', 'show_embed'\]\)/.test(bridge) && /IN_CHAT \|\| !CHAT_ONLY_TOOLS\.has/.test(bridge), 'all five are listed only inside a chat');
ok(/if \(CHAT_ONLY_TOOLS\.has\(name\)\) \{\s*delete args\.sessionId;/.test(bridge), 'which chat it shows in comes from the process, never from the caller');
const { COLLABORATOR_MCP_TOOLS } = await import('../src/access-policy.js');
ok(!['show_image', 'show_video', 'show_audio', 'show_file'].some((t) => COLLABORATOR_MCP_TOOLS.has(t)), 'none is offered to collaborators: the file is read outside their sandbox');
const { BASE, MAX_LAYER } = await import('../src/system-prompt.js');
// ─── show_embed ───
{
  const { addEmbed, embedTarget } = await import('../src/chat-media.js');
  const yt = embedTarget('https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s');
  ok(yt && yt.src === 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?start=42' && yt.ratio[0] === 16, 'a YouTube link becomes its embed, with the start time', JSON.stringify(yt));
  ok(embedTarget('https://youtu.be/dQw4w9WgXcQ').src.endsWith('/embed/dQw4w9WgXcQ'), 'so does the short form');
  ok(embedTarget('https://www.youtube.com/shorts/abcDEF12345').ratio[0] === 9, 'a short is upright');
  ok(/Tweet\.html\?id=20&/.test(embedTarget('https://x.com/jack/status/20').src), 'a post on X');
  ok(embedTarget('https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT').height === 152, 'a Spotify track gets the compact player');
  ok(embedTarget('https://www.youtube.com/watch?v=x"><script>').known === false, 'an id with markup in it is not treated as a known embed');
  ok(embedTarget('http://example.com') === null && embedTarget('javascript:alert(1)') === null && embedTarget('file:///etc/passwd') === null, 'only https is accepted');
  const plain = embedTarget('https://example.com/page');
  ok(plain && plain.known === false && plain.src === 'https://example.com/page', 'an unknown site is framed as it is, and marked unknown');
  ok(!addEmbed('chat-embed1', {}).ok && !addEmbed('chat-embed1', { url: 'https://a.example', html: '<b>x</b>' }).ok, 'needs url or html, and not both');
  ok(!addEmbed('chat-embed1', { url: 'http://example.com' }).ok, 'an http address is refused with a reason');
  const e = addEmbed('chat-embed1', { url: 'https://youtu.be/dQw4w9WgXcQ', title: 'A video' });
  ok(e.ok && e.items[0].kind === 'embed' && e.items[0].src && !e.items[0].name && e.items[0].w === 16, 'a link is stored as an address, nothing on disk', JSON.stringify(e));
  const h = addEmbed('chat-embed1', { html: '<button onclick="alert(1)">x</button>', height: 5000 });
  ok(h.ok && /^[0-9a-f]{16}\.embed$/.test(h.items[0].name) && h.items[0].height === 1200, 'HTML is stored, and the height is kept within bounds', JSON.stringify(h));
  const at = locate('chat-embed1', h.items[0].name);
  ok(at && /^text\/plain/.test(at.mime), 'stored HTML is served as plain text, never as a page', at && at.mime);
  ok(/<!doctype html>/i.test(readFileSync(at.file, 'utf8')) && readFileSync(at.file, 'utf8').includes('<button onclick'), 'a fragment gets a page around it');
  const g = addEmbed('chat-embed1', { url: 'https://gist.github.com/octocat/6cad326836d38bd3a7ae' });
  ok(g.ok && g.items[0].name && !g.items[0].src && g.items[0].url, 'a gist, which has no frame address, becomes stored HTML');
  ok(!addEmbed('chat-embed1', { html: 'x'.repeat(600 * 1024) }).ok, 'HTML over the limit is refused');
}

ok(['show_image', 'show_video', 'show_audio', 'show_file', 'show_embed'].every((t) => BASE.includes(t)), 'the base prompt names all five');
ok(/NOT visible to the person/.test(BASE), 'the base prompt says a screenshot Claude took is not seen by the person');
ok(/send_\*_to_user|send_photo_to_user/.test(BASE) && /prefer|Prefer/.test(BASE), 'the base prompt says to prefer showing in the chat over sending to Telegram');
ok(BASE.length < MAX_LAYER, 'the base prompt still fits in one layer', String(BASE.length));
const skill = readFileSync(new URL('../skills/crundi/SKILL.md', import.meta.url), 'utf8');
ok(['show_image', 'show_video', 'show_audio', 'show_file', 'show_embed'].every((t) => skill.includes(t)), 'the Crundi skill covers all five');
const ref = readFileSync(new URL('../skills/crundi/reference/mcp-tools.md', import.meta.url), 'utf8');
ok(['show_image', 'show_video', 'show_audio', 'show_file', 'show_embed'].every((t) => ref.includes('`' + t + '`')), 'the tool reference lists all five');

try { rmSync(tmp, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll chat-media checks passed.');
process.exit(failed ? 1 : 0);
