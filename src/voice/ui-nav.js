/**
 * voice/ui-nav.js — Cru moves the owner's Crundi page and points at things on
 * it (capability 'voice-ui-nav'). The owner's card (10 Nov): when he talks to
 * Cru from the PC app or the web, Cru can take him to a place in Crundi and
 * show him something there. Desktop and web only: the phone has no such page.
 *
 * Which page: every web/PC page has an id of its own (crundi-voice.js sends it
 * as `page` with each turn, its pageId). The latest turn from a web or PC page
 * makes that page the target; a turn from the phone makes the phone the
 * target, and then the tool says so instead of moving a PC nobody is looking
 * at. A target older than TARGET_MS is forgotten.
 *
 * How: the move goes out as the owner-only SSE event 'cru-ui'
 * ({ id, page, project?, tab?, chat?, card?, node?, file?, widget?, highlight?, note? });
 * every owner page gets it and only the page with that id acts. That page
 * answers POST /api/voice/ui/ack { id, ok, error?, found? }, so Cru learns
 * whether the thing it pointed at was there. No answer within ACK_MS: the
 * page is shut or asleep, and Cru is told that.
 */

export const UI_NAV_TOOL_NAMES = ['show_in_crundi'];
/** The tabs of the workbench page (webapp-html.js data-tab / data-panel). */
export const UI_TABS = ['workbench', 'git', 'files', 'kanban', 'notes', 'mindmap', 'media', 'widgets', 'factories', 'schedule', 'services', 'docker', 'browsers', 'info', 'secrets', 'settings', 'inbox'];
/** Devices (voice/device.js ids) whose page Cru can move. */
const PAGE_DEVICES = new Set(['web', 'pc-app']);
const PHONE_DEVICES = new Set(['android', 'android-overlay', 'android-bubble', 'ios']);
export const TARGET_MS = 30 * 60 * 1000;
export const ACK_MS = 4000;

const clip = (s, n) => String(s == null ? '' : s).replace(/\s+/g, ' ').trim().slice(0, n);

/** Where the owner last talked to Cru from: a page id (web/PC) or the phone. */
export function createUiTarget({ now = () => Date.now() } = {}) {
  let t = null;
  return {
    /** A turn came in. [page] from the web/PC voice screen; [deviceId] from voice/device.js. */
    note({ page = '', deviceId = '', device = '' } = {}) {
      const p = clip(page, 80);
      if (p && PAGE_DEVICES.has(deviceId)) t = { page: p, device, at: now() };
      else if (PHONE_DEVICES.has(deviceId)) t = { phone: true, device, at: now() };
      // A turn with no page from an unknown client changes nothing.
    },
    get() { return t && now() - t.at <= TARGET_MS ? t : null; },
    clear() { t = null; },
  };
}

/** Acks from the page, by move id. */
export function createAcks({ timeoutMs = ACK_MS } = {}) {
  const waiting = new Map();
  return {
    wait(id) {
      return new Promise((resolve) => {
        const timer = setTimeout(() => { waiting.delete(id); resolve(null); }, timeoutMs);
        if (timer.unref) timer.unref();
        waiting.set(id, (r) => { clearTimeout(timer); waiting.delete(id); resolve(r); });
      });
    },
    /** The page's answer; false when nobody was waiting for that id. */
    answer(id, r) { const f = waiting.get(String(id || '')); if (!f) return false; f(r || {}); return true; },
  };
}

/**
 * The tool.
 * @param {object} o
 * @param {{get: Function}} o.target       createUiTarget()
 * @param {{wait: Function}} o.acks        createAcks()
 * @param {(msg: object) => void} o.send   to the owner's pages (SSE 'cru-ui')
 * @param {() => {alias: string, name?: string}[]} o.projects
 * @param {() => {id: string, project: string, title?: string}[]} [o.chats]
 */
export function makeUiNavTools({ target, acks, send, projects = () => [], chats = () => [] }) {
  let seq = 0;
  return [{
    name: 'show_in_crundi',
    description: 'Take the owner\'s Crundi page (the PC app or the web, the one he is talking to you from) to a place and point at something there, when seeing it in Crundi itself helps more than a description: "show me the kanban", "where is that setting", "open that chat", "show me the file". '
      + 'Give any of: project (switches project), tab, chat (a chat\'s id from list_panels: opens it on the workbench), card (a kanban task id: scrolls to it and flashes it), node (a mindmap node id: scrolls to it, selects and flashes it), file (a path inside the project: opens it in the editor), widget (a panel id), '
      + 'and highlight: words that appear on the page (a button, a setting\'s label, a card\'s title, a service\'s name): it is scrolled into view and ringed, with your note beside it. '
      + 'A card or node is only shown, not opened: add open: true only when he asks for its details (a card\'s edit dialog, a node\'s note). '
      + 'Not on the phone: if the owner is on his phone it says so; use show_card or the live screen there. The answer says whether what you pointed at was found; say in a sentence where to look.',
    input_schema: {
      type: 'object', additionalProperties: false,
      properties: {
        project: { type: 'string', description: 'A project\'s alias or name' },
        tab: { type: 'string', enum: UI_TABS, description: 'The tab to show' },
        chat: { type: 'string', description: 'A chat\'s id (list_panels): opened on the workbench' },
        card: { type: 'string', description: 'A kanban task id: the board opens at it' },
        node: { type: 'string', description: 'A mindmap node id: the map scrolls to it, selects and flashes it' },
        open: { type: 'boolean', description: 'With card or node: also open it (the card\'s details, the node\'s note). Leave it out to just show it' },
        file: { type: 'string', description: 'A file inside the project, relative to its folder: opened in the editor' },
        widget: { type: 'string', description: 'A panel (widget) id: the Panels tab opens at it' },
        highlight: { type: 'string', description: 'Words shown on the page to ring: a button, a label, a title' },
        note: { type: 'string', description: 'Optional: a short line shown beside what you ringed' },
      },
    },
    run: async (input = {}) => {
      const t = target.get();
      if (!t) return { ok: false, error: 'No Crundi page to show it on: the owner has not talked to you from the PC app or the web in the last half hour. Say it, or use show_card.' };
      if (t.phone) return { ok: false, error: `The owner is on his phone (${t.device || 'the phone'}); moving Crundi's page works only when he talks to you from the PC app or the web. Use show_card or the live screen instead.` };
      const msg = { id: `ui${Date.now().toString(36)}${(++seq).toString(36)}`, page: t.page };
      let project = '';
      if (input.project != null && String(input.project).trim()) {
        const want = String(input.project).trim().toLowerCase();
        const list = projects();
        const p = list.find((x) => String(x.alias || '').toLowerCase() === want) || list.find((x) => String(x.name || '').toLowerCase() === want);
        if (!p) return { ok: false, error: `No project "${input.project}". Projects: ${list.map((x) => x.alias).join(', ')}` };
        project = p.alias;
      }
      if (input.chat != null && String(input.chat).trim()) {
        const id = String(input.chat).trim();
        const c = chats().find((x) => x.id === id);
        if (!c) return { ok: false, error: `No open chat with id "${id}" (list_panels gives the ids)` };
        if (project && c.project && c.project !== project) return { ok: false, error: `That chat is in ${c.project}, not ${project}` };
        msg.chat = id; project = c.project || project; msg.tab = 'workbench';
      }
      if (project) msg.project = project;
      if (input.tab != null && String(input.tab).trim()) {
        const tab = String(input.tab).trim().toLowerCase();
        if (!UI_TABS.includes(tab)) return { ok: false, error: `No tab "${input.tab}". Tabs: ${UI_TABS.join(', ')}` };
        if (msg.chat && tab !== 'workbench') return { ok: false, error: 'A chat opens on the workbench tab' };
        msg.tab = tab;
      }
      const one = (k, n, tab) => {
        if (input[k] == null || !String(input[k]).trim()) return;
        msg[k] = clip(input[k], n);
        if (msg.tab && msg.tab !== tab) throw new Error(`A ${k} is shown on the ${tab} tab`);
        msg.tab = tab;
      };
      try {
        one('card', 80, 'kanban');
        one('node', 80, 'mindmap');
        one('widget', 80, 'widgets');
        if (input.file != null && String(input.file).trim()) {
          const f = String(input.file).trim().replace(/\\/g, '/');
          if (f.startsWith('/') || /^[a-z]:/i.test(f) || f.split('/').includes('..')) return { ok: false, error: 'Give the file relative to the project\'s folder' };
          msg.file = f.slice(0, 500);
          if (msg.tab && msg.tab !== 'files') throw new Error('A file is shown on the files tab');
          msg.tab = 'files';
        }
      } catch (e) { return { ok: false, error: e.message }; }
      if (input.open === true) {
        if (!msg.card && !msg.node) return { ok: false, error: 'open goes with a card or a node' };
        msg.open = true;
      }
      if (input.highlight != null && String(input.highlight).trim()) msg.highlight = clip(input.highlight, 120);
      if (input.note != null && String(input.note).trim()) msg.note = clip(input.note, 300);
      if (!msg.project && !msg.tab && !msg.highlight) return { ok: false, error: 'Say where: a project, a tab, a chat, a card, a file, or words to highlight' };
      const answer = acks.wait(msg.id);
      send(msg);
      const r = await answer;
      if (!r) return { ok: true, unconfirmed: true, note: `Sent to the owner's page (${t.device || 'web'}), but it did not answer: it may be closed or asleep. Do not claim it is on his screen.` };
      if (r.ok === false) return { ok: false, error: `The page could not do it: ${clip(r.error, 300) || 'unknown reason'}` };
      const out = { ok: true, shownOn: t.device || 'web', note: 'It is on the owner\'s screen now. Say in a sentence where to look; do not describe the page.' };
      if (msg.highlight) out.found = r.found !== false;
      if (msg.highlight && r.found === false) out.note = `The page moved, but "${msg.highlight}" was not found on it to ring. Tell him where it should be, or try other words.`;
      if (r.where) out.now = clip(r.where, 200);
      return out;
    },
  }];
}
