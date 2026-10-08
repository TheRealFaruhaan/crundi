/**
 * chat-title.js — the title Claude gives its own chat.
 *
 * A chat starts as "Chat". Once the first request shows what the job is,
 * Claude calls rename_chat and the pane says what it is for, which matters
 * when four of them sit side by side on a phone. This is the one rule for
 * what such a title may be.
 */

export const MAX_AUTO_TITLE = 20;
/** The longer form, shown where a pane's header has the room for it. */
export const MAX_LONG_TITLE = 50;

function tidy(raw) {
  return String(raw == null ? '' : raw)
    .replace(/[\u0000-\u001f\u007f]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    // A title is not a sentence or a quotation: drop wrapping quotes and
    // closing punctuation, in whichever order they were stacked.
    .replace(/^["'`“”‘’\s]+|["'`“”‘’.:;,\s]+$/g, '');
}

/**
 * The short title: what every pane header and a phone shows.
 * @returns {{ok:true,title:string}|{ok:false,error:string}}
 */
export function cleanAutoTitle(raw) {
  const t = tidy(raw);
  if (!t) return { ok: false, error: 'Give a title: a few words naming the job.' };
  const length = [...t].length;
  if (length > MAX_AUTO_TITLE) {
    // Refused rather than cut: a title chopped mid-word reads worse than "Chat".
    return { ok: false, error: `"${t}" is ${length} characters; the limit is ${MAX_AUTO_TITLE}. Shorten it and call again.` };
  }
  return { ok: true, title: t };
}

/**
 * The optional long title. Empty is fine (the short one is used everywhere).
 * @returns {{ok:true,title:string}|{ok:false,error:string}}
 */
export function cleanLongTitle(raw) {
  const t = tidy(raw);
  if (!t) return { ok: true, title: '' };
  const length = [...t].length;
  if (length > MAX_LONG_TITLE) return { ok: false, error: `The long title is ${length} characters; the limit is ${MAX_LONG_TITLE}. Shorten it and call again.` };
  return { ok: true, title: t };
}
