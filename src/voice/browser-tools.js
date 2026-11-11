/**
 * voice/browser-tools.js — Cru and the PC client's browser drawer.
 *
 * The drawer's tabs belong to projects, and a project's chats see only their
 * own. Cru is the owner's assistant across every project, so it sees and uses
 * all of them: the tabs of every project, on whichever network each is on,
 * homepool included (the "chats ask first" rule is for chats, not for Cru).
 * Cru cannot make or change the PC browser key; only the owner does, in the
 * drawer or the Homepool tab.
 *
 * Reading (browser_tabs, browser_look, browser_marks) is in Cru's tool list.
 * Acting (browser_go, browser_do, browser_mark) goes through actions.js's
 * gate as "safe" actions: they run when asked, but never while the phone is
 * locked or when the voice was not recognised as the owner's.
 *
 * Everything goes through the same browser_* handlers as the chats' MCP tools
 * (deps.call → mcpDispatch with _cru set), so it reaches the drawer over the
 * client's own connection, wherever the client runs. With no desktop app
 * connected, every tool answers at once with a plain reason.
 */

export const CRU_BROWSER_READ_NAMES = ['browser_tabs', 'browser_look', 'browser_marks'];
export const CRU_BROWSER_ACTION_NAMES = ['browser_go', 'browser_do', 'browser_mark'];

const obj = (properties = {}, required = []) => ({ type: 'object', properties, required });
const str = (description, extra = {}) => ({ type: 'string', description, ...extra });
const num = (description) => ({ type: 'number', description });

/** deps.call(tool, args) → the browser_* tool's result (mcpDispatch). */
export function makeCruBrowserTools(deps) {
  const call = async (tool, args) => {
    try { return await deps.call(tool, { ...args, _cru: true }); }
    catch (err) { return { ok: false, error: err.message }; }
  };
  const needKey = (key) => {
    if (!/^drawer:[A-Za-z0-9_-]{4,64}$/.test(String(key || ''))) throw new Error('key must be a drawer tab key (drawer:<id>) from browser_tabs');
  };

  const read = [
    {
      name: 'browser_tabs',
      description: 'The tabs open in the browser drawer of the owner\'s PC app, in every project: key (drawer:<id>), project, title, address, network (client: the PC\'s own; server: the server\'s; homepool:<network> and its exit), whether he is looking at it, asleep or unloaded. Says so when no PC app with the drawer is connected.',
      input_schema: obj({ project: str('Only this project\'s tabs (an alias); omit for all') }),
      run: async ({ project } = {}) => {
        const r = await call('browser_list', project ? { alias: String(project) } : {});
        if (!r || r.ok === false) return r;
        const tabs = (r.browsers || []).filter((b) => b.on === 'drawer');
        return { ok: true, drawer: r.drawer, tabs };
      },
    },
    {
      name: 'browser_look',
      description: 'Look at a drawer tab: its text, address and title, and (screenshot, default true) a picture of what it shows. Waking a sleeping tab is fine; an unloaded one loads again.',
      input_schema: obj({ key: str('Drawer tab key, from browser_tabs'), screenshot: { type: 'boolean', description: 'Include a picture (default true)' } }, ['key']),
      run: async ({ key, screenshot = true }) => {
        needKey(key);
        const page = await call('browser_read_page', { key });
        if (!page || page.ok === false) return page;
        const content = [{ type: 'text', text: `${page.title || ''}\n${page.url || ''}\n\n${String(page.text || '').slice(0, 6000)}` }];
        if (screenshot !== false) {
          const shot = await call('browser_screenshot', { key });
          if (shot && shot.ok !== false && shot.data) content.push({ type: 'image', data: shot.data, mimeType: 'image/png' });
        }
        return { content };
      },
    },
    {
      name: 'browser_marks',
      description: 'What is marked on a drawer tab: the owner\'s annotations (selected elements with their notes and CSS selectors, drawings with their notes) and any from chats, with a picture of the page with the marks.',
      input_schema: obj({ key: str('Drawer tab key, from browser_tabs') }, ['key']),
      run: async ({ key }) => { needKey(key); return call('browser_annotations', { key }); },
    },
  ];

  const actions = [
    {
      name: 'browser_go',
      description: 'Open an address in the browser drawer: in a tab (key), or in a new tab of a project (project, and network: "client" the PC\'s own, "server" the server\'s, "homepool:<network>"). show: bring the drawer up on it (default true). Do it when the owner asks.',
      input_schema: obj({ url: str('Full address, http(s)'), key: str('A tab to open it in (drawer:<id>)'), project: str('For a new tab: its project'), network: str('For a new tab: client (default), server or homepool:<network>'), show: { type: 'boolean', description: 'Show it to the owner (default true)' } }, ['url']),
      safe: true,
      check: (i) => { if (!/^https?:\/\//i.test(String(i.url || ''))) throw new Error('url must be a full http(s) address'); if (!i.key && !i.project) throw new Error('Give a tab key, or a project for a new tab'); if (i.key) needKey(i.key); },
      run: async (i) => {
        if (i.key) return call('browser_navigate', { key: i.key, url: i.url });
        return call('browser_open', { alias: String(i.project).toLowerCase(), url: i.url, on: 'drawer', network: i.network || 'client', show: i.show !== false });
      },
    },
    {
      name: 'browser_do',
      description: 'Use a drawer tab: click (selector), type (text, into selector if given), fill (selector, text), scroll (y, or selector), back, forward, or network (switch the tab to client, server or homepool:<network>).',
      input_schema: obj({
        key: str('Drawer tab key'), op: str('What to do', { enum: ['click', 'type', 'fill', 'scroll', 'back', 'forward', 'network'] }),
        selector: str('CSS selector'), text: str('For type and fill'), y: num('For scroll: pixels down'), network: str('For network'),
      }, ['key', 'op']),
      safe: true,
      check: (i) => { needKey(i.key); },
      run: async (i) => {
        switch (i.op) {
          case 'click': return call('browser_click', { key: i.key, selector: i.selector });
          case 'type': return call('browser_type', { key: i.key, text: String(i.text || ''), selector: i.selector });
          case 'fill': return call('browser_fill', { key: i.key, selector: i.selector, value: String(i.text || '') });
          case 'scroll': return call('browser_scroll', { key: i.key, y: Number(i.y) || 0, selector: i.selector });
          case 'back': return call('browser_go_back', { key: i.key });
          case 'forward': return call('browser_go_forward', { key: i.key });
          case 'network': return call('browser_tab_network', { key: i.key, network: String(i.network || '') });
          default: return { ok: false, error: 'Unknown op' };
        }
      },
    },
    {
      name: 'browser_mark',
      description: 'Point at something on a drawer tab where the owner sees it: a pin (x, y in page pixels, or a selector) with a label; clear: true removes your marks. Shows the drawer on that tab.',
      input_schema: obj({ key: str('Drawer tab key'), selector: str('CSS selector to point at'), x: num('Page x'), y: num('Page y'), label: str('Text by the pin'), clear: { type: 'boolean', description: 'Remove your marks instead' } }, ['key']),
      safe: true,
      check: (i) => { needKey(i.key); if (!i.clear && !i.selector && (i.x == null || i.y == null)) throw new Error('Give a selector, or x and y'); },
      run: (i) => (i.clear ? call('browser_annotate', { key: i.key, shapes: [], clear: true })
        : call('browser_point', { key: i.key, selector: i.selector, x: i.x, y: i.y, label: i.label })),
    },
  ];

  return { read, actions };
}
