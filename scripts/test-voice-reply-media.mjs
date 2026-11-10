#!/usr/bin/env node
/**
 * test-voice-reply-media.mjs — what Cru shows inside its replies
 * (voice/reply-media.js): the tools, where files may come from, what a turn
 * hands back and keeps in its history, and that the pages draw it with the
 * chat panel's own code.
 *
 * Run: node scripts/test-voice-reply-media.mjs
 */
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');
const tmp = mkdtempSync(join(tmpdir(), 'crundi-cru-media-'));
writeFileSync(join(tmp, 'env'), '');
process.env.DATA_DIR = join(tmp, 'data');
process.env.DOTENV_PATH = join(tmp, 'env');
mkdirSync(process.env.DATA_DIR, { recursive: true });

let failed = 0;
const ok = (c, l, d = '') => { console.log((c ? 'PASS  ' : 'FAIL  ') + l + (c || !d ? '' : '\n      ' + d)); if (!c) failed++; };

const chatMedia = await import('../src/chat-media.js');
const { makeReplyMediaTools, REPLY_MEDIA_TOOL_NAMES, CRU_MEDIA_SESSION, allowedPath, cleanCard } = await import('../src/voice/reply-media.js');
const { runVoiceTool } = await import('../src/voice/agent.js');
const { createMemoryIndex, splitTurn } = await import('../src/voice/memory-index.js');

const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');
const proj = join(tmp, 'proj'); mkdirSync(proj);
const other = join(tmp, 'elsewhere'); mkdirSync(other);
writeFileSync(join(proj, 'shot.png'), PNG);
writeFileSync(join(proj, 'report.csv'), 'a,b\n1,2\n');
writeFileSync(join(other, 'secret.png'), PNG);

// ─── the tools ───
const tools = makeReplyMediaTools({ media: chatMedia, roots: () => [proj] });
const by = Object.fromEntries(tools.map((t) => [t.name, t]));
ok(JSON.stringify(tools.map((t) => t.name)) === JSON.stringify(REPLY_MEDIA_TOOL_NAMES), 'six tools: show_image, show_video, show_audio, show_file, show_embed, show_card');
ok(tools.every((t) => t.input_schema && t.input_schema.type === 'object' && t.description.length > 40), 'each has a schema and says what it is for');
ok(/^[0-9a-zA-Z_-]{6,64}$/.test(CRU_MEDIA_SESSION), 'Cru\'s media session is a valid chat-media session id');

const img = by.show_image.run({ path: join(proj, 'shot.png'), caption: 'The new header' });
ok(img.ok && img.media.length === 1 && img.media[0].kind === 'image' && /^[0-9a-f]{16}\.png$/.test(img.media[0].name) && img.media[0].w === 1, 'show_image stores the picture with its size', JSON.stringify(img));
ok(!!chatMedia.locate(CRU_MEDIA_SESSION, img.media[0].name), 'and it is served from Cru\'s media session');
const file = by.show_file.run({ path: join(proj, 'report.csv') });
ok(file.ok && file.media[0].kind === 'file' && /^[0-9a-f]{16}$/.test(file.media[0].id) && chatMedia.locateFile(CRU_MEDIA_SESSION, file.media[0].id).name === 'report.csv', 'show_file offers a download');
let threw = '';
try { by.show_image.run({ path: join(other, 'secret.png') }); } catch (e) { threw = e.message; }
ok(/project/.test(threw), 'a file outside the allowed folders is refused', threw);
threw = '';
try { by.show_image.run({ path: join(proj, '..', 'elsewhere', 'secret.png') }); } catch (e) { threw = e.message; }
ok(/project/.test(threw), 'climbing out with .. is refused too');
threw = '';
try { allowedPath('shot.png', [proj]); } catch (e) { threw = e.message; }
ok(/absolute/.test(threw), 'a relative path is refused');
ok(by.show_image.run({}).ok === false, 'no path: a plain error');

const emb = by.show_embed.run({ html: '<canvas id=c></canvas><script>1</script>', title: 'Spend' });
ok(emb.ok && emb.media[0].kind === 'embed' && /\.embed$/.test(emb.media[0].name), 'show_embed keeps Cru\'s HTML as a frame');
const yt = by.show_embed.run({ url: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ' });
ok(yt.ok && yt.media[0].src && /youtube/.test(yt.media[0].src), 'show_embed turns a link into its embed');

const card = by.show_card.run({ title: 'Services', blocks: [{ type: 'table', headers: ['Name', 'State'], rows: [['api', 'up']] }, { type: 'progress', label: 'Disk', value: 0.42 }, { type: 'chips', items: [{ text: 'ok', tone: 'ok' }] }] });
ok(card.ok && card.media[0].kind === 'card' && card.media[0].blocks.length === 3 && card.media[0].blocks[1].value === 42, 'show_card cleans its blocks as the live screen does');
threw = '';
try { cleanCard({ blocks: [{ type: 'nope' }] }); } catch (e) { threw = e.message; }
ok(/Unknown block type/.test(threw), 'an unknown block is refused');
threw = '';
try { cleanCard({ blocks: [{ type: 'image', url: 'http://x/y.png' }] }); } catch (e) { threw = e.message; }
ok(/https/.test(threw), 'a card picture must be https or data:image');

// ─── through the brain: collected for the reply, not handed back to the model ───
const signals = { pending: [], needsUnlock: false, phoneActions: [], close: false, acted: [], media: [] };
const r1 = await runVoiceTool(by.show_card, 'show_card', { title: 'T', blocks: [{ type: 'text', text: 'hello' }] }, signals);
const r2 = await runVoiceTool(by.show_image, 'show_image', { path: join(proj, 'shot.png') }, signals);
ok(signals.media.length === 2 && signals.media[0].kind === 'card' && signals.media[1].kind === 'image', 'what each call showed joins the turn, in order');
ok(!/"media"/.test(r1.content) && !/"media"/.test(r2.content) && /chat view/.test(r1.content), 'the model gets the note, not the items');
ok(signals.acted.length === 0, 'showing is not an action a cancel would report');
const r3 = await runVoiceTool(by.show_image, 'show_image', { path: join(other, 'secret.png') }, signals);
ok(r3.isError && signals.media.length === 2, 'a refused file is an error and shows nothing');

// ─── kept with the turn, and back from the history ───
const idx = createMemoryIndex({ dir: join(tmp, 'mem'), embedder: { embed: async () => null, status: 'off' } });
idx.add('turn', 'Owner: show me\nCru: Here it is', { at: 1000, media: signals.media });
idx.add('turn', 'Owner: hi\nCru: Hello', { at: 2000 });
const turns = idx.turns({ limit: 10 });
ok(turns.length === 2 && !turns[0].media && Array.isArray(turns[1].media) && turns[1].media.length === 2 && turns[1].media[0].kind === 'card', 'the history gives each turn the media it showed (and none when it showed none)');
{ const q = splitTurn({ id: 2, at: 2, text: 'Owner: a picture please\nCru:' }); ok(q.said === 'a picture please' && q.speak === '' && q.show === '', 'a reply with no words (only media) still splits: kept trimmed as "Cru:"'); }
ok(!('media' in splitTurn({ id: 1, at: 1, text: 'Owner: a\nCru: b', media: '{broken' })), 'unreadable media is left out, the turn still reads');
idx.close();
// An index made before the media column gains it.
const { DatabaseSync } = await import('node:sqlite');
const oldDir = join(tmp, 'old'); mkdirSync(oldDir);
const odb = new DatabaseSync(join(oldDir, 'index.db'));
odb.exec('CREATE TABLE entries (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, ref TEXT, at INTEGER NOT NULL, project TEXT, text TEXT NOT NULL, vec BLOB)');
odb.exec("INSERT INTO entries (kind, at, text) VALUES ('turn', 5, 'Owner: old\nCru: turn')");
odb.close();
const idx2 = createMemoryIndex({ dir: oldDir, embedder: { embed: async () => null, status: 'off' } });
ok(idx2.turns({}).length === 1 && idx2.turns({})[0].said === 'old', 'an older index is upgraded and its turns still read');
idx2.add('turn', 'Owner: new\nCru: one', { at: 6, media: [{ kind: 'card', title: 'x', blocks: [] }] });
ok(idx2.turns({})[0].media && idx2.turns({})[0].media[0].kind === 'card', 'and keeps media from then on');
idx2.close();

// ─── wired in ───
const web = read('src', 'webapp.js');
ok(/makeReplyMediaTools\(/.test(web) && /\.\.\.cruReplyMediaTools/.test(web), 'the owner\'s Cru has the tools');
ok(!/cruReplyMediaTools/.test(read('src', 'voice', 'kid.js')), 'kid Cru does not');
ok(/type: 'report'[^\n]*media/.test(web), 'a report carries what it showed');
ok(/media: signals\.media/.test(read('src', 'voice', 'agent.js')) && /media: turnSignals\.media/.test(read('src', 'voice', 'agent-cli.js')), 'both brains return it with the reply and keep it with the turn');
ok(/'voice-reply-media'/.test(read('src', 'client-version.js')), 'capability voice-reply-media');
ok(/show_card/.test(read('src', 'voice', 'agent.js')), 'Cru\'s instructions say when to use them');

// ─── drawn with the chat panel's code ───
const wg = read('app', 'vendor', 'crundi-widgets.js');
ok(/replyMedia: replyMedia/.test(wg) && /function itemsMedia\(/.test(wg) && /return itemsMedia\(sessionId/.test(wg), 'crundi-widgets: toolMedia and Cru\'s replies share one parser (itemsMedia)');
const cv = read('app', 'vendor', 'crundi-voice.js');
ok(/CrundiWidgets\.replyMedia\(/.test(cv) && /'cru-voice'/.test(cv), 'the web chat view draws media with CrundiWidgets.replyMedia');
ok(/cv-card/.test(cv) && /blocksHtml\(/.test(cv), 'and cards with the live screen\'s block drawing');
ok(/r\.media/.test(cv) && /m\.media/.test(cv), 'from the turn\'s answer, reports and the history');
const and = (p) => read('android', 'app', 'src', 'main', 'java', 'mv', 'obitech', 'crundi', ...p);
ok(/media/.test(and(['api', 'VoiceApi.kt'])) && /fun parseItems\(/.test(and(['api', 'model', 'Chats.kt'])), 'Android: the reply, the history and reports carry media, parsed as a chat\'s');
ok(/CruReplyMedia\(m\.media/.test(and(['voice', 'VoiceScreen.kt'])) && /MediaGroup\(/.test(and(['voice', 'CruReplyMedia.kt'])) && /Blocks\(c\.blocks\)/.test(and(['voice', 'CruReplyMedia.kt'])) && /SESSION = "cru-voice"/.test(and(['voice', 'CruReplyMedia.kt'])), 'Android: Cru\'s bubbles draw them with MediaGroup and the live screen\'s Blocks');

rmSync(tmp, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
