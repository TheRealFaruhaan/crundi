/**
 * Which device a voice request came from (the owner, 11 Nov: an unrecognised
 * "No." at 17:55 on 10 Nov had nothing to say where it was heard).
 *
 * Worked out on the server from what every client already sends, so clients
 * released before this need no change: the caller's kind (client-version.js
 * identifyClient: the User-Agent or X-Crundi-Client), the `where` it gives
 * /api/voice/turn ("side key", "Cru", "Cru bubble", "web (host)", with
 * ", phone locked" when it is) and, for a browser, its User-Agent.
 *
 * The label goes into Cru's context line ("from: the PC app") and with the
 * turn into memory ("Owner (PC app): …"), so memory_search can answer
 * "where did that come from?" later.
 */

/** "Chrome on Windows", "Safari on iPhone", or '' when the User-Agent says nothing useful. */
export function browserOf(ua) {
  const s = String(ua || '');
  if (!s) return '';
  const browser = /\bEdg\//.test(s) ? 'Edge'
    : /\bOPR\//.test(s) ? 'Opera'
      : /\bFirefox\//.test(s) ? 'Firefox'
        : /\bChrome\//.test(s) ? 'Chrome'
          : /\bSafari\//.test(s) ? 'Safari' : '';
  const os = /\bAndroid\b/.test(s) ? 'Android'
    : /\biPhone\b/.test(s) ? 'iPhone'
      : /\biPad\b/.test(s) ? 'iPad'
        : /\bWindows\b/.test(s) ? 'Windows'
          : /\bMac OS X\b|\bMacintosh\b/.test(s) ? 'Mac'
            : /\bCrOS\b/.test(s) ? 'ChromeOS'
              : /\bLinux\b/.test(s) ? 'Linux' : '';
  if (browser && os) return `${browser} on ${os}`;
  return browser || os;
}

/**
 * @param {object} o
 * @param {string} [o.kind]   identifyClient(req).kind: 'android', 'ios', 'desktop', 'web' or ''
 * @param {string} [o.where]  what the client sent as `where`
 * @param {string} [o.ua]     the User-Agent
 * @returns {{ id: string, label: string }}  id: android | android-overlay | android-bubble | ios | pc-app | web | unknown
 */
export function voiceDevice({ kind = '', where = '', ua = '' } = {}) {
  const w = String(where || '').toLowerCase();
  const web = /^web\b/.test(w);
  if (kind === 'android') {
    if (/side key/.test(w)) return { id: 'android-overlay', label: 'Android phone, the side-key assistant overlay' };
    if (/bubble/.test(w)) return { id: 'android-bubble', label: 'Android phone, the Cru bubble' };
    if (web) return { id: 'android', label: 'Android phone, a page in the Crundi app' };
    return { id: 'android', label: 'Android phone, the Cru screen in the app' };
  }
  if (kind === 'ios') return { id: 'ios', label: 'iPhone app' };
  if (kind === 'desktop') return { id: 'pc-app', label: 'PC app' };
  if (kind === 'web' || web) {
    const b = browserOf(ua);
    return { id: 'web', label: `web browser${b ? `, ${b}` : ''}` };
  }
  return { id: 'unknown', label: 'an unknown device' };
}
