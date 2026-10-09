/**
 * chat-media.js — things Claude puts in front of the person, in the chat:
 * pictures, video, audio, and files to download.
 *
 * A chat's transcript is text. A tool result carrying an image reaches the page
 * as the word "[image]", and a path is just a path. So when Claude has
 * something worth looking at, listening to or taking away, it calls show_image,
 * show_video, show_audio or show_file, and the chat draws it under that call.
 *
 * Pictures, video and audio are COPIED here, per chat, when the call is made:
 * the transcript outlives the turn, a render in /tmp does not, and a path that
 * still exists may by then hold something else. A file offered for download is
 * NOT copied (it may be a database or an archive); the chat keeps a reference,
 * and the download is of the file as it is when the person takes it.
 *
 * What a file is, is decided by its own first bytes, never its name. No SVG and
 * no HTML: these are served from Crundi's own origin, and both can carry script.
 *
 * Every item comes back with its dimensions (and a video its poster frame), so
 * the page can lay it out before a single byte of it has loaded. Without that a
 * transcript jumps as each picture arrives.
 */

import { join, basename, extname } from 'node:path';
import {
  mkdirSync, writeFileSync, readFileSync, readdirSync, statSync, rmSync, existsSync,
  copyFileSync, openSync, readSync, closeSync,
} from 'node:fs';
import { randomBytes } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { config } from './config.js';
import { sniffImage } from './image-input.js';

export const MAX_PER_CALL = 8;
export const LIMITS = {
  image: 10 * 1024 * 1024,
  audio: 60 * 1024 * 1024,
  video: 250 * 1024 * 1024,
  file: 2 * 1024 * 1024 * 1024,    // offered, not copied: only a sanity bound
  perChatFiles: 150,               // oldest stored media dropped beyond these
  perChatBytes: 1024 * 1024 * 1024,
  fileRefs: 300,
};
const MAX_AGE_MS = 45 * 24 * 3600 * 1000;   // a chat untouched this long loses its media

const MIME = {
  png: 'image/png', jpg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp',
  mp4: 'video/mp4', webm: 'video/webm', mov: 'video/quicktime',
  mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg', m4a: 'audio/mp4', flac: 'audio/flac', weba: 'audio/webm',
  // HTML given to show_embed. Served as plain text ON PURPOSE: the page reads
  // it and puts it in a sandboxed frame. Served as HTML it would run with
  // Crundi's own origin for anyone who opened its address.
  embed: 'text/plain; charset=utf-8',
};
const EXT_RE = Object.keys(MIME).join('|');
const validSession = (id) => /^[0-9a-zA-Z_-]{6,64}$/.test(String(id || ''));
const validName = (n) => new RegExp(`^[0-9a-f]{16}(\\.poster)?\\.(${EXT_RE})$`).test(String(n || ''));
const validRef = (n) => /^[0-9a-f]{16}$/.test(String(n || ''));

const root = () => join(config.dataDir, 'chat-media');
// Where 1.19.19 kept pictures; still read, so older transcripts keep theirs.
const legacyRoot = () => join(config.dataDir, 'chat-images');
const dirFor = (sessionId) => join(root(), String(sessionId));

// ─── What is this file? ───

function head(path, n = 64) {
  const fd = openSync(path, 'r');
  try { const b = Buffer.alloc(n); const got = readSync(fd, b, 0, n, 0); return b.subarray(0, got); }
  finally { closeSync(fd); }
}

/** 'mp4' | 'mov' | 'm4a' | 'webm' | 'mp3' | 'wav' | 'ogg' | 'flac' | '' — by magic number. */
export function sniffAv(buf) {
  if (!buf || buf.length < 12) return '';
  const s = (a, b) => buf.subarray(a, b).toString('latin1');
  if (s(4, 8) === 'ftyp') {
    const brand = s(8, 12);
    if (/^M4A|^M4B/.test(brand)) return 'm4a';
    if (/^qt/.test(brand)) return 'mov';
    return 'mp4';
  }
  if (buf[0] === 0x1a && buf[1] === 0x45 && buf[2] === 0xdf && buf[3] === 0xa3) return 'webm';
  if (s(0, 4) === 'RIFF' && s(8, 12) === 'WAVE') return 'wav';
  if (s(0, 4) === 'OggS') return 'ogg';
  if (s(0, 4) === 'fLaC') return 'flac';
  if (s(0, 3) === 'ID3') return 'mp3';
  if (buf[0] === 0xff && (buf[1] & 0xe0) === 0xe0 && (buf[1] & 0x06) !== 0) return 'mp3';   // MPEG audio frame sync
  return '';
}

/** Pixel size of an image from its header, or null. */
export function imageSize(buf, ext) {
  try {
    if (ext === 'png') return { w: buf.readUInt32BE(16), h: buf.readUInt32BE(20) };
    if (ext === 'gif') return { w: buf.readUInt16LE(6), h: buf.readUInt16LE(8) };
    if (ext === 'webp') {
      const kind = buf.subarray(12, 16).toString('latin1');
      if (kind === 'VP8X') return { w: 1 + buf.readUIntLE(24, 3), h: 1 + buf.readUIntLE(27, 3) };
      if (kind === 'VP8 ') return { w: buf.readUInt16LE(26) & 0x3fff, h: buf.readUInt16LE(28) & 0x3fff };
      if (kind === 'VP8L') { const b = buf.readUInt32LE(21); return { w: 1 + (b & 0x3fff), h: 1 + ((b >> 14) & 0x3fff) }; }
      return null;
    }
    if (ext === 'jpg') {
      let i = 2;
      while (i + 9 < buf.length) {
        if (buf[i] !== 0xff) { i++; continue; }
        const m = buf[i + 1];
        if (m === 0xff) { i++; continue; }
        // Start-of-frame markers carry the size; the rest are skipped by their length.
        if (m >= 0xc0 && m <= 0xcf && m !== 0xc4 && m !== 0xc8 && m !== 0xcc) return { w: buf.readUInt16BE(i + 7), h: buf.readUInt16BE(i + 5) };
        if (m === 0xd8 || m === 0x01 || (m >= 0xd0 && m <= 0xd7)) { i += 2; continue; }
        i += 2 + buf.readUInt16BE(i + 2);
      }
    }
  } catch { /* a header we cannot read is a size we do not know */ }
  return null;
}

// ─── ffprobe / ffmpeg, when the machine has them ───

let ffState = null;
function ff() {
  if (ffState) return ffState;
  const has = (bin) => { try { execFileSync(bin, ['-version'], { stdio: 'ignore', timeout: 4000 }); return true; } catch { return false; } };
  ffState = { probe: has('ffprobe'), mpeg: has('ffmpeg') };
  return ffState;
}

/** { w, h, duration, hasVideo, hasAudio } as far as ffprobe can tell; {} without it. */
function probe(path) {
  if (!ff().probe) return {};
  try {
    const out = execFileSync('ffprobe', ['-v', 'error', '-print_format', 'json', '-show_entries', 'format=duration:stream=codec_type,width,height', path],
      { encoding: 'utf8', timeout: 15000, maxBuffer: 1024 * 1024, stdio: ['ignore', 'pipe', 'ignore'] });
    const j = JSON.parse(out);
    const streams = j.streams || [];
    const v = streams.find((s) => s.codec_type === 'video' && s.width && s.height);
    const dur = Number(j.format && j.format.duration);
    return {
      w: v ? v.width : 0, h: v ? v.height : 0,
      duration: Number.isFinite(dur) && dur > 0 ? Math.round(dur * 10) / 10 : 0,
      hasVideo: !!v, hasAudio: streams.some((s) => s.codec_type === 'audio'),
    };
  } catch { return {}; }
}

/** A frame from near the start, as the picture a video shows before it plays. */
function makePoster(src, dest, duration) {
  if (!ff().mpeg) return false;
  try {
    const at = duration > 2 ? '1' : '0';
    execFileSync('ffmpeg', ['-v', 'error', '-y', '-ss', at, '-i', src, '-frames:v', '1', '-vf', 'scale=min(960\\,iw):-2', '-q:v', '5', dest],
      { stdio: 'ignore', timeout: 20000 });
    return existsSync(dest) && statSync(dest).size > 0;
  } catch { return false; }
}

// ─── Adding ───

const tidyCaption = (c) => String(c || '').replace(/\s+/g, ' ').trim().slice(0, 200);
const mb = (n) => (n / 1048576).toFixed(n < 10485760 ? 1 : 0) + ' MB';

/** Check one path as one kind; returns what to store, or an error. Nothing is written. */
function inspect(kind, path) {
  const p = String(path || '').trim();
  if (!p) return { ok: false, error: 'Give a file path.' };
  if (!existsSync(p)) return { ok: false, error: `No such file: ${p}` };
  let st; try { st = statSync(p); } catch (err) { return { ok: false, error: String(err.message || err) }; }
  if (st.isDirectory()) return { ok: false, error: `${p} is a folder, not a file.` };
  if (st.size === 0) return { ok: false, error: `${p} is empty.` };
  if (st.size > LIMITS[kind]) return { ok: false, error: `${basename(p)} is ${mb(st.size)}; the limit for ${kind === 'file' ? 'a file' : kind} is ${mb(LIMITS[kind])}.` };
  if (kind === 'file') return { ok: true, path: p, size: st.size };
  let first; try { first = head(p, 64); } catch (err) { return { ok: false, error: String(err.message || err) }; }
  if (kind === 'image') {
    const ext = sniffImage(first.length >= 12 ? first : Buffer.alloc(12));
    if (!ext) return { ok: false, error: `${basename(p)} is not a PNG, JPEG, GIF or WebP. (An SVG cannot be shown here; render it to PNG first, or offer it with show_file.)` };
    // The size may sit past the first 64 bytes (JPEG): read enough to find it.
    let big = first; try { big = head(p, Math.min(st.size, 256 * 1024)); } catch { /* keep the short read */ }
    return { ok: true, path: p, size: st.size, ext, dims: imageSize(big, ext) };
  }
  const ext = sniffAv(first);
  if (!ext) return { ok: false, error: `${basename(p)} is not ${kind === 'video' ? 'an MP4, MOV or WebM video' : 'an MP3, WAV, OGG, M4A, FLAC or WebM audio file'}. Convert it first (ffmpeg), or offer it with show_file.` };
  const info = probe(p);
  if (kind === 'video') {
    if (!['mp4', 'mov', 'webm', 'm4a'].includes(ext)) return { ok: false, error: `${basename(p)} is audio, not video. Use show_audio.` };
    if (info.hasVideo === false) return { ok: false, error: `${basename(p)} has no video track. Use show_audio.` };
    return { ok: true, path: p, size: st.size, ext: ext === 'm4a' ? 'mp4' : ext, info };
  }
  // Audio: a container that might hold video is fine as long as there is sound to play.
  if (info.hasAudio === false) return { ok: false, error: `${basename(p)} has no audio track.` };
  const aext = ext === 'mp4' || ext === 'mov' ? 'm4a' : ext === 'webm' ? 'weba' : ext;
  return { ok: true, path: p, size: st.size, ext: aext, info };
}

/**
 * Put media in a chat. All or nothing: every item is checked before any is
 * stored, so a typo in the third path does not leave two orphans on screen.
 *
 * @param {string} sessionId
 * @param {'image'|'video'|'audio'|'file'} kind
 * @param {Array<{path:string, caption?:string}>} items
 * @returns {{ok:true, items:object[]}|{ok:false, error:string}}
 */
export function addMedia(sessionId, kind, items) {
  if (!validSession(sessionId)) return { ok: false, error: 'This is not a Crundi chat, so there is nowhere to show it.' };
  if (!LIMITS[kind]) return { ok: false, error: `Unknown kind "${kind}"` };
  const list = (Array.isArray(items) ? items : []).filter((x) => x && x.path);
  if (!list.length) return { ok: false, error: 'Give at least one file: path (one) or paths (several).' };
  if (list.length > MAX_PER_CALL) return { ok: false, error: `At most ${MAX_PER_CALL} in one call; you gave ${list.length}. Split them, or pick the ones that matter.` };
  const checked = [];
  for (const [i, it] of list.entries()) {
    const r = inspect(kind, it.path);
    if (!r.ok) return { ok: false, error: `${list.length > 1 ? `Item ${i + 1}: ` : ''}${r.error}` };
    checked.push({ ...r, caption: tidyCaption(it.caption) });
  }
  const dir = dirFor(sessionId);
  mkdirSync(dir, { recursive: true });
  const out = [];
  if (kind === 'file') {
    const refs = readRefs(sessionId);
    for (const c of checked) {
      const id = randomBytes(8).toString('hex');
      refs[id] = { path: c.path, name: basename(c.path), size: c.size, at: Date.now() };
      out.push({ kind, id, filename: basename(c.path), size: c.size, ext: extname(c.path).slice(1).toLowerCase().slice(0, 12), caption: c.caption || undefined });
    }
    const ids = Object.keys(refs).sort((a, b) => refs[a].at - refs[b].at);
    for (const old of ids.slice(0, Math.max(0, ids.length - LIMITS.fileRefs))) delete refs[old];
    writeFileSync(join(dir, 'files.json'), JSON.stringify(refs));
    return { ok: true, items: out };
  }
  for (const c of checked) {
    const stem = randomBytes(8).toString('hex');
    const name = `${stem}.${c.ext}`;
    copyFileSync(c.path, join(dir, name));
    const item = { kind, name, filename: basename(c.path), size: c.size, caption: c.caption || undefined };
    if (kind === 'image' && c.dims && c.dims.w > 0 && c.dims.h > 0) { item.w = c.dims.w; item.h = c.dims.h; }
    if (kind !== 'image') {
      if (c.info.duration) item.duration = c.info.duration;
      if (kind === 'video') {
        if (c.info.w && c.info.h) { item.w = c.info.w; item.h = c.info.h; }
        const poster = `${stem}.poster.jpg`;
        if (makePoster(join(dir, name), join(dir, poster), c.info.duration || 0)) item.poster = poster;
      }
    }
    out.push(item);
  }
  prune(dir);
  return { ok: true, items: out };
}

// ─── show_embed: something from the web, or a piece of HTML, in a frame ───

const EMBED_HTML_MAX = 512 * 1024;
const clampInt = (v, lo, hi, dflt) => { const n = Math.round(Number(v)); return Number.isFinite(n) && n > 0 ? Math.max(lo, Math.min(hi, n)) : dflt; };
const idOk = (v) => /^[A-Za-z0-9_-]{1,80}$/.test(String(v || ''));

/**
 * Where to point a frame for a link someone would paste: the site's own embed
 * address, and the shape it wants. A link we know nothing about is framed as
 * it is (`known: false`); many sites refuse that, which the caller says.
 * @returns {{src?:string, html?:string, provider:string, known:boolean, ratio?:[number,number], height?:number, width?:number}|null}
 */
export function embedTarget(raw) {
  let u; try { u = new URL(String(raw || '').trim()); } catch { return null; }
  if (u.protocol !== 'https:') return null;
  const host = u.hostname.toLowerCase().replace(/^(www|m|mobile)\./, '');
  const seg = u.pathname.split('/').filter(Boolean);
  const q = u.searchParams;
  const enc = encodeURIComponent(u.href);
  const out = (provider, src, shape) => ({ provider, known: true, src, ...shape });
  const wide = { ratio: [16, 9] };

  if (host === 'youtu.be' || host === 'youtube.com' || host === 'music.youtube.com' || host === 'youtube-nocookie.com') {
    let id = '', tall = false;
    if (host === 'youtu.be') id = seg[0];
    else if (seg[0] === 'watch') id = q.get('v');
    else if (['embed', 'shorts', 'live', 'v'].includes(seg[0])) { id = seg[1]; tall = seg[0] === 'shorts'; }
    const list = q.get('list');
    const t = parseInt(String(q.get('t') || q.get('start') || '').replace(/s$/, ''), 10);
    const extra = [Number.isFinite(t) && t > 0 ? `start=${t}` : '', list && idOk(list) && idOk(id) ? `list=${list}` : ''].filter(Boolean).join('&');
    if (idOk(id) && id !== 'videoseries') return out('YouTube', `https://www.youtube-nocookie.com/embed/${id}${extra ? '?' + extra : ''}`, tall ? { ratio: [9, 16] } : wide);
    if (list && idOk(list)) return out('YouTube', `https://www.youtube-nocookie.com/embed/videoseries?list=${list}`, wide);
  }
  if (host === 'vimeo.com' && /^\d+$/.test(seg[seg.length - 1] || '')) return out('Vimeo', `https://player.vimeo.com/video/${seg[seg.length - 1]}`, wide);
  if (host === 'player.vimeo.com') return out('Vimeo', u.href, wide);
  if (host === 'dailymotion.com' && seg[0] === 'video' && idOk(seg[1])) return out('Dailymotion', `https://geo.dailymotion.com/player.html?video=${seg[1]}`, wide);
  if (host === 'loom.com' && (seg[0] === 'share' || seg[0] === 'embed') && idOk(seg[1])) return out('Loom', `https://www.loom.com/embed/${seg[1]}`, wide);
  if ((host === 'twitter.com' || host === 'x.com') && seg[1] === 'status' && /^\d+$/.test(seg[2] || '')) {
    return out('X', `https://platform.twitter.com/embed/Tweet.html?id=${seg[2]}&theme=dark&dnt=true`, { height: 600, width: 550 });
  }
  if (host === 'instagram.com' && ['p', 'reel', 'reels', 'tv'].includes(seg[0]) && idOk(seg[1])) {
    return out('Instagram', `https://www.instagram.com/${seg[0] === 'p' ? 'p' : 'reel'}/${seg[1]}/embed/`, { height: 720, width: 400 });
  }
  if (host === 'tiktok.com' && seg[1] === 'video' && /^\d+$/.test(seg[2] || '')) return out('TikTok', `https://www.tiktok.com/embed/v2/${seg[2]}`, { height: 760, width: 330 });
  if (host === 'facebook.com' || host === 'fb.watch') {
    const video = host === 'fb.watch' || seg.includes('videos') || seg[0] === 'watch' || seg[0] === 'reel';
    return out('Facebook', `https://www.facebook.com/plugins/${video ? 'video' : 'post'}.php?href=${enc}&show_text=true&width=500`, video ? wide : { height: 600, width: 500 });
  }
  if (host === 'open.spotify.com') {
    const s = seg[0] === 'embed' ? seg.slice(1) : (/^intl-/.test(seg[0] || '') ? seg.slice(1) : seg);
    if (['track', 'album', 'playlist', 'episode', 'show', 'artist'].includes(s[0]) && idOk(s[1])) {
      return out('Spotify', `https://open.spotify.com/embed/${s[0]}/${s[1]}?theme=0`, { height: s[0] === 'track' || s[0] === 'episode' ? 152 : 352, width: 560 });
    }
  }
  if (host === 'soundcloud.com' && seg.length >= 2) return out('SoundCloud', `https://w.soundcloud.com/player/?url=${enc}&color=%236366f1&visual=false`, { height: seg[1] === 'sets' ? 450 : 166, width: 560 });
  if (host === 'codepen.io' && ['pen', 'full', 'details', 'embed'].includes(seg[1]) && idOk(seg[0]) && idOk(seg[2])) {
    return out('CodePen', `https://codepen.io/${seg[0]}/embed/${seg[2]}?default-tab=result&theme-id=dark`, { height: 460 });
  }
  if (host === 'reddit.com' && seg[0] === 'r' && seg[2] === 'comments' && idOk(seg[1]) && idOk(seg[3])) {
    return out('Reddit', `https://embed.reddit.com/r/${seg[1]}/comments/${seg[3]}/${idOk(seg[4]) ? seg[4] + '/' : ''}?embed=true&theme=dark`, { height: 520, width: 560 });
  }
  if (host === 'gist.github.com' && idOk(seg[0]) && /^[0-9a-f]{8,64}$/.test(seg[1] || '')) {
    // A gist has no frame address, only a script that writes itself into a page.
    return { provider: 'GitHub Gist', known: true, height: 420, html: `<style>body{background:#fff;color:#111;padding:0}</style><script src="https://gist.github.com/${seg[0]}/${seg[1]}.js"></script>` };
  }
  if (host === 'google.com' && seg[0] === 'maps' && seg[1] === 'embed') return out('Google Maps', u.href, { height: 420 });
  return { provider: host, known: false, src: u.href, height: 480 };
}

/** A fragment gets a page around it that suits the dark chat; a whole document is left alone. */
function wrapEmbedHtml(html) {
  if (/<html[\s>]/i.test(html) || /<!doctype/i.test(html)) return html;
  return '<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">'
    + '<style>html{color-scheme:dark}*,*::before,*::after{box-sizing:border-box}body{margin:0;padding:12px;font:14px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;background:#0f0f14;color:#e8e8ee}a{color:#8b8dfb}pre,code{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}</style>'
    + '</head><body>' + html + '</body></html>';
}

/**
 * Something to frame in a chat: a web address (a video, a post, a player, a
 * page) or HTML of Claude's own (a demo with its CSS and JavaScript).
 * @param {string} sessionId
 * @param {{url?:string, html?:string, title?:string, caption?:string, height?:number}} o
 */
export function addEmbed(sessionId, o = {}) {
  if (!validSession(sessionId)) return { ok: false, error: 'This is not a Crundi chat, so there is nowhere to show it.' };
  const url = String(o.url || '').trim(), html = typeof o.html === 'string' ? o.html : '';
  if (!url && !html.trim()) return { ok: false, error: 'Give url (a web address to embed) or html (your own HTML, CSS and JavaScript).' };
  if (url && html.trim()) return { ok: false, error: 'Give url or html, not both.' };
  const item = { kind: 'embed', title: tidyCaption(o.title) || undefined, caption: tidyCaption(o.caption) || undefined };
  let body = html;
  if (url) {
    if (!/^https:\/\//i.test(url)) return { ok: false, error: 'Only https:// addresses can be embedded. For a local page, expose it first (add_forward) and embed that address, or pass its HTML as html.' };
    const t = embedTarget(url);
    if (!t) return { ok: false, error: `Not a usable address: ${url.slice(0, 200)}` };
    item.url = url.slice(0, 2000);
    item.provider = t.provider; item.known = t.known;
    if (t.ratio) { item.w = t.ratio[0]; item.h = t.ratio[1]; }
    if (t.width) item.width = t.width;
    item.height = clampInt(o.height, 80, 1200, t.height || 0) || undefined;
    if (o.height && t.ratio) { delete item.w; delete item.h; }       // an explicit height wins over the usual shape
    if (t.src) item.src = t.src;
    body = t.html || '';
  } else {
    item.height = clampInt(o.height, 80, 1200, 360);
  }
  if (body) {
    const doc = wrapEmbedHtml(body);
    if (Buffer.byteLength(doc) > EMBED_HTML_MAX) return { ok: false, error: `The HTML is ${mb(Buffer.byteLength(doc))}; the limit is ${mb(EMBED_HTML_MAX)}. Load large libraries or data from a URL instead of inlining them.` };
    const dir = dirFor(sessionId);
    mkdirSync(dir, { recursive: true });
    item.name = `${randomBytes(8).toString('hex')}.embed`;
    writeFileSync(join(dir, item.name), doc);
    prune(dir);
  }
  return { ok: true, items: [item] };
}

function prune(dir) {
  try {
    const files = readdirSync(dir).filter(validName).map((n) => { const st = statSync(join(dir, n)); return { n, t: st.mtimeMs, size: st.size }; })
      .sort((a, b) => b.t - a.t);   // newest first
    let bytes = 0, kept = 0;
    for (const f of files) {
      bytes += f.size; kept++;
      if (kept > LIMITS.perChatFiles || bytes > LIMITS.perChatBytes) rmSync(join(dir, f.n), { force: true });
    }
  } catch { /* tidy next time */ }
}

// ─── Reading ───

/** A stored picture, video, audio file or poster: where it is and what to call it. Null if unknown. */
export function locate(sessionId, name) {
  if (!validSession(sessionId) || !validName(name)) return null;
  for (const base of [root(), legacyRoot()]) {
    const file = join(base, String(sessionId), name);
    try { const st = statSync(file); if (st.isFile()) return { file, size: st.size, mime: MIME[name.split('.').pop()] }; } catch { /* next */ }
  }
  return null;
}

function readRefs(sessionId) {
  try { const j = JSON.parse(readFileSync(join(dirFor(sessionId), 'files.json'), 'utf8')); return j && typeof j === 'object' ? j : {}; }
  catch { return {}; }
}

/** A file offered with show_file, as it is NOW. `gone` if it has since been moved or deleted. */
export function locateFile(sessionId, id) {
  if (!validSession(sessionId) || !validRef(id)) return null;
  const ref = readRefs(sessionId)[id];
  if (!ref) return null;
  try { const st = statSync(ref.path); if (st.isFile()) return { file: ref.path, size: st.size, name: ref.name }; } catch { /* gone */ }
  return { gone: true, name: ref.name };
}

/** Drop the media of chats nobody has touched in a long while. */
export function sweep(now = Date.now()) {
  let removed = 0;
  for (const base of [root(), legacyRoot()]) {
    let ids = []; try { ids = readdirSync(base); } catch { continue; }
    for (const id of ids) {
      const dir = join(base, id);
      let newest = 0;
      try { for (const n of readdirSync(dir)) newest = Math.max(newest, statSync(join(dir, n)).mtimeMs); } catch { continue; }
      if (now - newest > MAX_AGE_MS) { rmSync(dir, { recursive: true, force: true }); removed++; }
    }
  }
  return removed;
}

/** For tests: forget whether ffmpeg was found. */
export function _resetFf(state = null) { ffState = state; }
