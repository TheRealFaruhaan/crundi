/**
 * inbox-send.js — telling the owner something: the inbox first, then copies.
 *
 * Every message meant for the owner is written to Crundi's inbox (see
 * inbox-store.js) and THEN handed to the notification channels that are on.
 * The inbox does not depend on any of them, so with no Telegram, no phone and
 * no browser push the message is still there to read.
 *
 * Also the MCP tools send_message_to_user, send_photo_to_user and
 * send_file_to_user, which used to go straight to the Telegram bot and so went
 * nowhere without it.
 */

import { existsSync, statSync } from 'node:fs';
import { basename } from 'node:path';
import { decodeImage } from './image-input.js';

/**
 * @param {object} o
 * @param {object} o.inbox            createInboxStore()
 * @param {Function} o.deliver        (text, meta, opts) => Promise<[{ id, ok }]> (notify-channels deliver)
 * @param {Function} [o.labelOf]      channel id -> label, for the tools' notes
 */
export function createOwnerMessenger({ inbox, deliver, labelOf = (id) => id } = {}) {
  /**
   * Put one thing in the inbox and send copies out.
   *
   * kind/title/text/project/link/files: as inbox.add. chat: { id, title,
   * project, mark } when it is about a chat. channelText: what the channels
   * get (default title + text). meta: extra channel meta (tag, title, markdown,
   * prompt…). deliverOpts: { only, except } for notify-channels. copies:
   * false keeps it to the inbox.
   */
  async function tell({ kind = 'message', title = '', text = '', project = '', link = '', chat = null, files = [], channelText = null, meta = {}, deliverOpts = {}, copies = true, inboxLink = false, collapse = false, thread = '', cru = false } = {}) {
    const item = inbox.add({
      kind, title, text, link, files, collapse, thread, cru,
      project: project || chat?.project || '',
      chatId: chat?.id || '',
      chatTitle: chat?.title || '',
    });
    if (!copies) return { item, results: [] };
    const kept = (item.media || []).find((m) => m.url);
    const file = kept ? inbox.mediaPath(item.id, kept.name) : null;
    const attach = file ? { kind: kind === 'photo' ? 'photo' : 'file', path: file.path, name: file.name, size: file.size, mime: file.mime } : null;
    const out = channelText != null ? String(channelText) : [item.title, item.text].filter(Boolean).join('\n');
    let results = [];
    try {
      results = await deliver(out, {
        tag: kind,
        ...(chat && chat.id ? { chat, url: `/chat/${chat.id}` } : { url: `/#inbox/${item.id}` }),
        ...meta,
        inboxId: item.id,
        inboxThread: item.thread,
        ...(inboxLink ? { inboxLink: true } : {}),
        ...(attach ? { attach: { ...attach, ...(meta.attach || {}) } } : {}),
      }, deliverOpts) || [];
    } catch (err) {
      console.warn('[inbox] channel copies failed:', err.message);
    }
    return { item, results };
  }

  /** Fire and forget: whatever was being notified about must not wait. */
  function post(args) {
    tell(args).catch((err) => console.warn('[inbox] could not add:', err.message));
  }

  /** What the tool answers to Claude: where it went. */
  function note(results, extra = '') {
    const sent = (results || []).filter((r) => r.ok).map((r) => labelOf(r.id));
    const failed = (results || []).filter((r) => !r.ok).map((r) => labelOf(r.id));
    let s = 'Put in the person\'s Crundi inbox';
    s += sent.length ? `, and sent to: ${sent.join(', ')}.` : '. No other notification channel is on, so it waits there for them.';
    if (failed.length) s += ` Could not reach: ${failed.join(', ')}.`;
    return extra ? `${s} ${extra}` : s;
  }

  const delivered = (results) => ['inbox', ...(results || []).filter((r) => r.ok).map((r) => r.id)];

  // ─── The MCP tools ───

  async function sendMessage(args = {}, { chat = null } = {}) {
    const message = String(args.message || '').trim() || '(empty)';
    const { item, results } = await tell({ kind: 'message', text: message, chat, channelText: message, inboxLink: true });
    return { ok: true, inboxId: item.id, delivered: delivered(results), note: note(results) };
  }

  async function sendPhoto(args = {}, { chat = null } = {}) {
    const img = decodeImage({ path: args.path, data: args.data });
    if (!img.ok) return { ok: false, error: img.error };
    const caption = String(args.caption || '').slice(0, 1024);
    const name = args.path ? basename(String(args.path)) : img.filename;
    const { item, results } = await tell({
      kind: 'photo', title: caption.split('\n')[0] || 'A picture', text: caption.includes('\n') ? caption.slice(caption.indexOf('\n') + 1) : '',
      chat, files: [{ buffer: img.buffer, name: /\.[a-z0-9]+$/i.test(name) ? name : img.filename, mime: `image/${img.ext === 'jpg' ? 'jpeg' : img.ext}` }],
      channelText: caption ? caption : 'A picture, in Crundi\'s inbox.', inboxLink: true,
    });
    return { ok: true, inboxId: item.id, sent: img.ext, bytes: img.buffer.length, delivered: delivered(results), note: note(results) };
  }

  /** shareUrl(path) -> a 30-minute download link (or null). */
  async function sendFile(args = {}, { chat = null, shareUrl = () => null } = {}) {
    const filePath = String(args.path || '');
    if (!filePath || !existsSync(filePath)) return { ok: false, error: 'File not found' };
    let st;
    try { st = statSync(filePath); } catch (err) { return { ok: false, error: err.message }; }
    if (!st.isFile()) return { ok: false, error: `${filePath} is not a file` };
    const url = shareUrl(filePath) || null;
    const name = basename(filePath);
    const { item, results } = await tell({
      kind: 'file', title: name, chat, files: [{ path: filePath }],
      channelText: `${name}, in Crundi's inbox.`,
      meta: { attach: { shareUrl: url } }, inboxLink: true,
    });
    const kept = (item.media || []).some((m) => m.url);
    const extra = (kept ? '' : 'The file itself was too large to copy into the inbox. ')
      + (url ? `A download link valid for 30 minutes: ${url}` : '');
    return { ok: true, url, inboxId: item.id, delivered: delivered(results), note: note(results, extra.trim()) };
  }

  return { tell, post, sendMessage, sendPhoto, sendFile, note };
}

/**
 * Which notification events also land in the inbox, and as what. Events left
 * out are live state (a chat finished a turn, a service came up, a card moved):
 * the page shows those already, and an inbox of them would bury the rest.
 */
export const INBOX_EVENTS = Object.freeze({
  needsInput: 'question',
  secretRequest: 'permission',
  collabPermission: 'permission',
  scheduledChat: 'schedule',
  serviceDown: 'alert',
  forwardDown: 'alert',
  updateAvailable: 'system',
  limitReset: 'system',
  factory: 'report',
});
