// A project's mark: the small coloured tile that stands for it.
//
// Projects have names and nothing else to tell them apart by eye. Where one
// has to be recognised at a glance (the round head of a chat bubble on a
// phone, the header of the compact chat) it gets a tile: one or two letters
// on a colour, both worked out from the name, so the same project is always
// the same tile, on every device, with nothing to store or sync.
//
// Pure, so the server, the page and the phone app can each make the same one.
// The Kotlin copy in the app must give the same answers; test-project-mark.mjs
// pins the values it has to match.

/** "my-cool_project" -> "MC", "crundi" -> "C", "" -> "?". */
export function markInitials(name) {
  const words = String(name || '').split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  if (!words.length) return '?';
  const first = [...words[0]][0];
  const second = words.length > 1 ? [...words[1]][0] : '';
  return (first + second).toUpperCase();
}

/** A hue from 0 to 359, the same for the same name whatever its case. */
export function markHue(name) {
  // FNV-1a over the UTF-16 code units: small, stable, and easy to write again
  // in another language without getting a different number.
  let h = 0x811c9dc5;
  for (const ch of String(name || '').toLowerCase()) {
    for (let i = 0; i < ch.length; i++) { h ^= ch.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  }
  return h % 360;
}

function hslToHex(h, s, l) {
  const a = s * Math.min(l, 1 - l);
  const f = (n) => { const k = (n + h / 30) % 12; return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)))); };
  return '#' + [f(0), f(8), f(4)].map((x) => x.toString(16).padStart(2, '0')).join('');
}

// How bright a colour looks, 0 (black) to 1 (white), by the usual sRGB rule.
function luminance(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * @returns {{ initials: string, hue: number, color: string, colorDeep: string }}
 *   color is the tile, colorDeep the darker end of its gradient.
 *
 *   Not every hue is equally bright at the same lightness: a yellow or a
 *   green that is fine for blue is too pale for white letters. So the tile
 *   starts at one lightness and is stepped darker until white reads on it at
 *   3 to 1 or better. The steps are fixed (0.46, then down by 0.02), so
 *   another language walking the same steps lands on the same colour.
 */
export function projectMark(name) {
  const hue = markHue(name);
  let l = 0.46, color = hslToHex(hue, 0.58, l);
  for (let i = 0; i < 12 && 1.05 / (luminance(color) + 0.05) < 3; i++) { l = Math.round((l - 0.02) * 100) / 100; color = hslToHex(hue, 0.58, l); }
  return { initials: markInitials(name), hue, color, colorDeep: hslToHex(hue, 0.62, Math.round(l * 74) / 100) };
}

/**
 * A chat's mark: the project's colour with the CHAT's initials on it.
 *
 * Several chats in one project would otherwise wear the same tile, and a row
 * of identical bubble heads tells nobody which is which. The colour still
 * says which project; the letters say which chat. A chat nobody has named
 * yet ("Chat") falls back to the project's own letters.
 */
export function chatMark(project, title) {
  const base = projectMark(project);
  const t = String(title || '').trim();
  const named = t && t.toLowerCase() !== 'chat';
  const initials = named ? markInitials(t) : base.initials;
  return { ...base, initials: initials === '?' ? base.initials : initials };
}
