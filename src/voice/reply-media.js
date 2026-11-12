/**
 * voice/reply-media.js — things Cru puts inside its replies in the chat view
 * (capability 'voice-reply-media'): pictures, video, audio, files to
 * download, live frames (a link, or Cru's own HTML: a chart, a demo) and
 * cards (a panel of tables, lists, progress, chips… drawn in Crundi's style).
 *
 * The same storage and the same shapes as a Claude chat's show_* tools
 * (chat-media.js), under one media "session" of Cru's own, so the pages draw
 * them with the chat panel's own code (CrundiWidgets.replyMedia on the web,
 * MediaGroup on Android). What a turn showed travels with its reply
 * ({ media: [...] } on /api/voice/turn, on a report, and on each turn of
 * /api/voice/history), in the order it was shown.
 *
 * Unlike the live screen (screen.js), which is one panel that Cru keeps
 * changing, these stay in the conversation where they were shown.
 *
 * Item shapes (as chat-media.js answers them, plus the card):
 *   { kind: 'image'|'video'|'audio', name, filename, size, caption?, w?, h?, duration?, poster? }
 *   { kind: 'file', id, filename, size, ext, caption? }
 *   { kind: 'embed', name? (stored HTML) | src (an https frame), url?, title?, caption?, provider?, w?, h?, width?, height? }
 *   { kind: 'card', title, blocks: Block[] }          blocks as screen.js cleanBlock
 *
 * Files are read as the Crundi user, so Cru may only show what is inside a
 * project's folder, the system's temporary folder, or Cru's own folder
 * (attachments, notes). Anything else is refused, as read_file refuses it.
 */

import { realpathSync, existsSync } from 'node:fs';
import { resolve, relative, isAbsolute, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { cleanBlock } from './screen.js';

/** Cru's media "session" in chat-media.js: where its pictures and files are kept. */
export const CRU_MEDIA_SESSION = 'cru-voice';
export const REPLY_MEDIA_TOOL_NAMES = ['show_image', 'show_video', 'show_audio', 'show_file', 'show_embed', 'show_card'];
/** At most this many items in one reply. */
export const MAX_REPLY_MEDIA = 24;
const MAX_CARD_BLOCKS = 40;
const MAX_CARD_BYTES = 60 * 1024;

const realOr = (p) => { try { return realpathSync(p); } catch { return resolve(p); } };

/** [p] (absolute) when it is inside one of [roots]; throws otherwise. */
export function allowedPath(p, roots) {
  const want = String(p || '').trim();
  if (!want) throw new Error('Give the file\'s path');
  if (!isAbsolute(want)) throw new Error('Give an absolute path');
  const full = existsSync(want) ? realOr(want) : resolve(want);
  for (const r of roots) {
    if (!r) continue;
    const base = realOr(r);
    const rel = relative(base, full);
    if (rel === '' || (!rel.startsWith('..' + sep) && rel !== '..' && !isAbsolute(rel))) return full;
  }
  throw new Error('Cru can only show files inside a project\'s folder, the temporary folder or its own folder');
}

/** A card: a title and blocks, checked as the live screen checks them. */
export function cleanCard({ title = '', blocks = [] } = {}) {
  const list = Array.isArray(blocks) ? blocks : [];
  if (list.length > MAX_CARD_BLOCKS) throw new Error(`At most ${MAX_CARD_BLOCKS} blocks in one card`);
  const b = list.map((x) => cleanBlock(x));
  const t = String(title || '').slice(0, 200);
  if (!b.length && !t.trim()) throw new Error('Nothing to show: give a title and blocks');
  const card = { kind: 'card', title: t, blocks: b };
  if (JSON.stringify(card).length > MAX_CARD_BYTES) throw new Error('That card is too big; show less in it, or split it');
  return card;
}

/**
 * Cru's show_* tools.
 * @param {object} o
 * @param {{addMedia: Function, addEmbed: Function}} o.media   chat-media.js
 * @param {() => string[]} o.roots   folders Cru may show files from
 * @param {string} [o.session]
 */
export function makeReplyMediaTools({ media, roots = () => [], session = CRU_MEDIA_SESSION } = {}) {
  const fileProps = {
    path: { type: 'string', description: 'Absolute path of one file' },
    paths: { type: 'array', items: { type: 'string' }, description: 'Several files at once (up to 8), shown together' },
    caption: { type: 'string', description: 'Optional: a line under it' },
  };
  const fileTool = (kind, description) => ({
    name: `show_${kind}`,
    description,
    input_schema: { type: 'object', additionalProperties: false, properties: fileProps },
    run: (input = {}) => {
      const list = [];
      if (input.path) list.push({ path: String(input.path), caption: input.caption });
      for (const p of Array.isArray(input.paths) ? input.paths : []) list.push({ path: String((p && p.path) || p || ''), caption: (p && p.caption) || undefined });
      if (!list.length) return { ok: false, error: 'Give path (one file) or paths (several).' };
      const allowed = roots();
      for (const it of list) it.path = allowedPath(it.path, allowed);
      const r = media.addMedia(session, kind, list);
      if (!r.ok) return r;
      return { ok: true, shown: r.items.length, note: 'It is in your reply in the chat view, under what you write. Do not describe it at length; say what to look at. It is not said aloud.', media: r.items };
    },
  });
  return [
    fileTool('image', 'Put a picture in your reply (the chat view): a screenshot, a photo, a chart you rendered to PNG. It stays in the conversation where you showed it; the owner can tap it to enlarge it. PNG, JPEG, GIF or WebP, from a project\'s folder, the temporary folder or your own folder.'),
    fileTool('video', 'Put a video player in your reply (the chat view): a recording, a render. MP4, MOV or WebM.'),
    fileTool('audio', 'Put an audio player in your reply (the chat view): a recording, a voice-over, music. MP3, WAV, OGG, M4A, FLAC or WebM.'),
    fileTool('file', 'Offer a file for the owner to download, in your reply (the chat view): an export, a report, an archive, a log. It downloads the file as it is when he takes it, so do not delete or move it.'),
    {
      name: 'show_embed',
      description: 'Something live in your reply (the chat view), in a sandboxed frame. url: an https link worth seeing in place (YouTube, Vimeo, X, Instagram, TikTok, Spotify, SoundCloud, CodePen, Google Maps… are turned into their proper embeds; any other page is framed as it is, if the site allows it). Or html: your own HTML with its CSS and JavaScript, for a chart, a diagram, an interactive table, a small demo; it may load libraries from a CDN (Chart.js, D3…), cannot read files or call Crundi, so put its data inside it. Use it for a one-off thing in the conversation; the live screen (show_screen) is for a panel you keep changing.',
      input_schema: {
        type: 'object', additionalProperties: false,
        properties: {
          url: { type: 'string', description: 'An https address (the ordinary link, not an embed link)' },
          html: { type: 'string', description: 'Your own HTML (a fragment gets a dark page around it)' },
          title: { type: 'string' },
          caption: { type: 'string' },
          height: { type: 'number', description: 'Optional: the frame\'s height in pixels (80 to 1200), for html' },
        },
      },
      run: (input = {}) => {
        const r = media.addEmbed(session, { url: input.url, html: input.html, title: input.title, caption: input.caption, height: input.height });
        if (!r.ok) return r;
        return { ok: true, shown: 1, note: 'It is in your reply in the chat view, under what you write. Say what it is in a line.', media: r.items };
      },
    },
    {
      name: 'show_card',
      description: 'A card in your reply (the chat view), drawn in Crundi\'s style: a title and blocks, for data better read than heard: a table, a list, a checklist, progress bars, status chips, headings and text, a picture. It stays in the conversation where you showed it (unlike the live screen, which you keep changing). Several cards in one reply are fine.',
      input_schema: {
        type: 'object', additionalProperties: false,
        properties: {
          title: { type: 'string' },
          blocks: {
            type: 'array',
            items: { type: 'object', description: 'A block: { type: heading|text|list|checklist|table|progress|image|chips, ... } — heading/text: text; list: items[string], ordered?; checklist: items[{text,done}]; table: rows[[cell, ...]], headers?; progress: label, value 0-100; image: url (https or data:image), alt; chips: items[{text, tone: ok|warn|bad|info|muted}]' },
          },
        },
        required: ['blocks'],
      },
      run: (input = {}) => {
        const card = cleanCard(input);
        return { ok: true, shown: 1, note: 'It is in your reply in the chat view, under what you write. Keep the reply itself short.', media: [card] };
      },
    },
  ];
}
