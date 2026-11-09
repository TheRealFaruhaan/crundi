/**
 * voice/attachments.js — files the owner gives Cru (10 Nov 2026, card
 * 5556940dd1f3615d): photos, media, documents, anything, attached on the web,
 * the PC (or dropped on Cru) and the phone (camera, gallery, files, share).
 *
 * Kept in <dataDir>/voice-memory/attachments/<id>/ (private to this user),
 * with a meta.json; an image may carry a smaller preview made by the client
 * (preview.jpg), which is what Cru looks at. Kept 14 days, 1 GB at most.
 *
 * Cru sees a line in the turn for each ([The owner attached: …]) and has two
 * read-only tools (open_attachment, list_attachments); send_to_chat and
 * start_chat take attachment ids and pass the files on as a chat attachment.
 */

import { mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync, statSync, existsSync, copyFileSync } from 'node:fs';
import { join, extname, basename } from 'node:path';
import { randomBytes } from 'node:crypto';
import { execFile } from 'node:child_process';

export const MAX_FILE = 50 * 1024 * 1024;
export const MAX_PREVIEW = 4 * 1024 * 1024;
const KEEP_MS = 14 * 86400000;
const MAX_TOTAL = 1024 * 1024 * 1024;
const TEXT_MAX = 40000;
const IMAGE_TYPES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);
const TEXTY = /^(text\/|application\/(json|xml|javascript|x-yaml|yaml|x-sh|sql|csv|x-ndjson))/;
const TEXT_EXT = new Set(['.txt', '.md', '.json', '.csv', '.tsv', '.log', '.js', '.mjs', '.ts', '.tsx', '.jsx', '.py', '.kt', '.java', '.go', '.rs', '.rb', '.php', '.html', '.css', '.scss', '.xml', '.yml', '.yaml', '.toml', '.ini', '.env.example', '.sh', '.sql', '.c', '.h', '.cpp', '.cs', '.swift', '.vue', '.svelte', '.srt', '.vtt']);

/** A safe file name, kept recognisable. */
export function safeName(name) {
  return basename(String(name || 'file')).replace(/[^\w.\- ]+/g, '_').replace(/\s+/g, ' ').trim().slice(-100) || 'file';
}
export function sizeLabel(n) {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} bytes`;
}
/** What kind of thing it is, for Cru and the tool. */
export function kindOf(mime, name) {
  const m = String(mime || '').toLowerCase(), e = extname(String(name || '')).toLowerCase();
  if (IMAGE_TYPES.has(m) || ['.png', '.jpg', '.jpeg', '.gif', '.webp'].includes(e)) return 'image';
  if (m === 'application/pdf' || e === '.pdf') return 'pdf';
  if (TEXTY.test(m) || TEXT_EXT.has(e)) return 'text';
  if (m.startsWith('video/')) return 'video';
  if (m.startsWith('audio/')) return 'audio';
  return 'file';
}

export function createAttachmentStore({ dir, now = () => Date.now() } = {}) {
  const metaOf = (id) => {
    if (!/^att_[a-f0-9]{8,16}$/.test(String(id || ''))) return null;
    try { return JSON.parse(readFileSync(join(dir, id, 'meta.json'), 'utf-8')); } catch { return null; }
  };
  function prune() {
    let all = [];
    try { all = readdirSync(dir).map(metaOf).filter(Boolean).sort((a, b) => b.at - a.at); } catch { return; }
    let total = 0;
    for (const m of all) {
      total += m.size + (m.previewSize || 0);
      if (now() - m.at > KEEP_MS || total > MAX_TOTAL) { try { rmSync(join(dir, m.id), { recursive: true, force: true }); } catch { /* gone */ } }
    }
  }
  return {
    dir,
    /** @returns meta { id, name, mime, kind, size, at, file, preview? } */
    add({ name, mime = '', data, preview = null, from = '' }) {
      if (!Buffer.isBuffer(data) || !data.length) throw new Error('The file is empty');
      if (data.length > MAX_FILE) throw new Error(`Files up to ${sizeLabel(MAX_FILE)}`);
      const id = `att_${randomBytes(6).toString('hex')}`;
      const n = safeName(name);
      const d = join(dir, id);
      mkdirSync(d, { recursive: true, mode: 0o700 });
      writeFileSync(join(d, n), data, { mode: 0o600 });
      const meta = { id, name: n, mime: String(mime || '').slice(0, 100), kind: kindOf(mime, n), size: data.length, at: now(), from: String(from || '').slice(0, 20) };
      if (preview && Buffer.isBuffer(preview) && preview.length <= MAX_PREVIEW) { writeFileSync(join(d, 'preview.jpg'), preview, { mode: 0o600 }); meta.previewSize = preview.length; }
      writeFileSync(join(d, 'meta.json'), JSON.stringify(meta), { mode: 0o600 });
      prune();
      return { ...meta, file: join(d, n) };
    },
    get(id) { const m = metaOf(id); return m ? { ...m, file: join(dir, m.id, m.name), previewFile: m.previewSize ? join(dir, m.id, 'preview.jpg') : '' } : null; },
    list(limit = 20) {
      try { return readdirSync(dir).map(metaOf).filter(Boolean).sort((a, b) => b.at - a.at).slice(0, limit); } catch { return []; }
    },
    prune,
  };
}

/** The line in the turn that tells Cru about an attachment. */
export function attachmentLine(m) {
  return `[The owner attached: "${m.name}" (${m.kind}${m.mime ? `, ${m.mime}` : ''}, ${sizeLabel(m.size)}), id ${m.id}. open_attachment to look at or read it; send_to_chat or start_chat with attachments to pass it on.]`;
}

function pdfText(file, pages = '') {
  return new Promise((done) => {
    const args = ['-layout', '-enc', 'UTF-8'];
    const m = String(pages || '').match(/^(\d+)(?:-(\d+))?$/);
    if (m) args.push('-f', m[1], '-l', m[2] || m[1]); else args.push('-l', '30');
    execFile('pdftotext', [...args, file, '-'], { timeout: 20000, maxBuffer: 8 * 1024 * 1024 }, (err, out) => done(err ? null : String(out)));
  });
}

/** Cru's read-only tools for attachments. */
export function attachmentToolDefs(store) {
  return [
    {
      name: 'open_attachment',
      description: 'Look at or read a file the owner attached (its id from the [The owner attached: …] line, or list_attachments): a picture is shown to you, text and code are read, a PDF\'s text is read (pages "3" or "2-5"; the first 30 by default). Video, audio and other files: only what they are (name, type, size); pass them on to a chat if they need work.',
      input_schema: { type: 'object', additionalProperties: false, properties: { id: { type: 'string', description: 'The attachment id (att_…)' }, pages: { type: 'string', description: 'PDF pages, like "3" or "2-5"' } }, required: ['id'] },
      run: async ({ id, pages } = {}) => {
        const m = store.get(id);
        if (!m) return { ok: false, error: `No attachment ${id} (they are kept 14 days). list_attachments shows the recent ones.` };
        const head = `${m.name} (${m.kind}, ${sizeLabel(m.size)}), attached ${new Date(m.at).toLocaleString('en-GB')}`;
        if (m.kind === 'image') {
          const f = m.previewFile || m.file;
          const size = statSync(f).size;
          if (size > MAX_PREVIEW) return { ok: false, error: `${head}: too large to look at (${sizeLabel(size)}). Pass it on to a chat instead.` };
          const mime = m.previewFile ? 'image/jpeg' : (IMAGE_TYPES.has(m.mime) ? m.mime : extname(m.name).toLowerCase() === '.png' ? 'image/png' : 'image/jpeg');
          return { content: [{ type: 'image', data: readFileSync(f).toString('base64'), mimeType: mime }, { type: 'text', text: head }] };
        }
        if (m.kind === 'text') {
          const t = readFileSync(m.file, 'utf-8');
          return { ok: true, about: head, text: t.length > TEXT_MAX ? t.slice(0, TEXT_MAX) + `\n…(cut: ${t.length} characters in all)` : t };
        }
        if (m.kind === 'pdf') {
          const t = await pdfText(m.file, pages);
          if (t == null) return { ok: true, about: head, note: 'Its text could not be read (a scanned PDF?). Pass it on to a chat to work with it.' };
          return { ok: true, about: head, pages: pages || '1-30', text: t.length > TEXT_MAX ? t.slice(0, TEXT_MAX) + '\n…(cut)' : t };
        }
        return { ok: true, about: head, note: `A ${m.kind} file: you cannot play or open it yourself. Say what it is, and pass it on to a chat if it needs work.` };
      },
    },
    {
      name: 'list_attachments',
      description: 'The files the owner attached lately (14 days), newest first: id, name, kind, size, when.',
      input_schema: { type: 'object', additionalProperties: false, properties: {} },
      run: () => ({ ok: true, attachments: store.list(20).map((m) => ({ id: m.id, name: m.name, kind: m.kind, size: sizeLabel(m.size), at: new Date(m.at).toISOString() })) }),
    },
  ];
}
export const ATTACHMENT_TOOL_NAMES = ['open_attachment', 'list_attachments'];

/** Copy attachments into a project's attachments folder; the lines to add to a chat message. */
export function passOn(store, ids = [], projectDir, ensureDir) {
  const lines = [];
  for (const id of [].concat(ids || []).slice(0, 10)) {
    const m = store.get(String(id));
    if (!m) throw new Error(`No attachment ${id}`);
    const d = ensureDir(projectDir);
    const to = join(d, `${Date.now()}-${m.name}`);
    copyFileSync(m.file, to);
    lines.push(`[File Path: ${to}]`);
  }
  return lines;
}
