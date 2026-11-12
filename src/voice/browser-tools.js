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
 * Reading (browser_tabs, browser_look, browser_inspect, browser_marks) is in
 * Cru's tool list. Between them and browser_do, Cru can do everything a
 * chat's browser_* tools can on a drawer tab.
 * Acting (browser_go, browser_do, browser_mark) goes through actions.js's
 * gate as "safe" actions: they run when asked, but never while the phone is
 * locked or when the voice was not recognised as the owner's.
 *
 * Everything goes through the same browser_* handlers as the chats' MCP tools
 * (deps.call → mcpDispatch with _cru set), so it reaches the drawer over the
 * client's own connection, wherever the client runs. With no desktop app
 * connected, every tool answers at once with a plain reason.
 */

import { clickablesScript, formatClickables, findScript } from '../browser-find.js';

export const CRU_BROWSER_READ_NAMES = ['browser_tabs', 'browser_look', 'browser_inspect', 'browser_marks'];
export const CRU_BROWSER_ACTION_NAMES = ['browser_go', 'browser_do', 'browser_mark'];

const obj = (properties = {}, required = []) => ({ type: 'object', properties, required });
const str = (description, extra = {}) => ({ type: 'string', description, ...extra });
const num = (description) => ({ type: 'number', description });
const parseJson = (v) => { try { return JSON.parse(v); } catch { return null; } };

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
      description: 'Look at a drawer tab: its text, address and title, what can be clicked on it (each with its role, text and x,y in page pixels, the open dialog\'s first when one is open), and (screenshot, default true) a picture of what it shows. Waking a sleeping tab is fine; an unloaded one loads again.',
      input_schema: obj({ key: str('Drawer tab key, from browser_tabs'), screenshot: { type: 'boolean', description: 'Include a picture (default true)' } }, ['key']),
      run: async ({ key, screenshot = true }) => {
        needKey(key);
        const page = await call('browser_read_page', { key });
        if (!page || page.ok === false) return page;
        const cl = await call('browser_eval', { key, code: clickablesScript(60) });
        const clickable = cl && cl.ok !== false ? formatClickables(parseJson(cl.result)) : '';
        const content = [{ type: 'text', text: `${page.title || ''}\n${page.url || ''}\n\n${String(page.text || '').slice(0, 6000)}`
          + (clickable ? `\n\nClickable (browser_do click by text, or at x,y):\n${clickable}` : '') }];
        if (screenshot !== false) {
          const shot = await call('browser_screenshot', { key });
          if (shot && shot.ok !== false && shot.data) content.push({ type: 'image', data: shot.data, mimeType: 'image/png' });
        }
        return { content };
      },
    },
    {
      name: 'browser_inspect',
      description: 'Read more of a drawer tab, as a chat\'s browser tools can: console (its log), network (the requests recorded since browser_do record_network start), cookies, source (the HTML), snapshot (the accessibility tree), elements (inputs, links and buttons with their attributes) or clickables (what can be clicked, with x,y).',
      input_schema: obj({ key: str('Drawer tab key'), what: str('What to read', { enum: ['console', 'network', 'cookies', 'source', 'snapshot', 'elements', 'clickables'] }), selector: str('For elements: a CSS selector to list instead') }, ['key', 'what']),
      run: async ({ key, what, selector }) => {
        needKey(key);
        const cut = (v, n) => (typeof v === 'string' && v.length > n ? v.slice(0, n) + '\n[...cut]' : v);
        switch (what) {
          case 'console': { const r = await call('browser_console', { key }); return r && r.logs ? { ok: true, total: r.total, logs: r.logs.slice(-60) } : r; }
          case 'network': { const r = await call('browser_network', { key, action: 'log' }); return r && Array.isArray(r.requests) ? { ...r, requests: r.requests.slice(-50) } : r; }
          case 'cookies': return call('browser_cookies', { key, action: 'get' });
          case 'source': { const r = await call('browser_view_source', { key }); return r && r.html ? { ...r, html: cut(r.html, 20000) } : r; }
          case 'snapshot': return call('browser_snapshot', { key, maxLength: 20000 });
          case 'elements': { const r = await call('browser_elements', { key, selector }); return r && Array.isArray(r.elements) ? { ...r, elements: r.elements.slice(0, 120) } : r; }
          case 'clickables': { const r = await call('browser_eval', { key, code: clickablesScript(120) }); return r && r.ok !== false ? { ok: true, text: formatClickables(parseJson(r.result)) } : r; }
          default: return { ok: false, error: 'what must be console, network, cookies, source, snapshot, elements or clickables' };
        }
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
      description: 'Use a drawer tab, as a chat can. click: by visible text (with role, and within a CSS selector; an open dialog is searched first), by selector, or at x,y in page pixels; it says what it clicked, so check it is what you meant. double_click, right_click, hover: the same targets. drag: from a target to toX,toY. mouse_down / mouse_move / mouse_up: press, move holding, release. type (text, into selector), fill (selector, text), select (an option of a <select>: selector, text), scroll (y, or selector), wait (for selector), back, forward, touch (gesture tap, double_tap, long_press, swipe, pinch at x,y), resize (width, height), eval (code: JavaScript in the page), cookie (cookie_action get, set, delete, clear), record_network (network_action start, stop, clear; read it with browser_inspect), network (switch the tab to client, server or homepool:<network>), close (the tab). The owner sees your cursor when you use the mouse.',
      input_schema: obj({
        key: str('Drawer tab key'),
        op: str('What to do', { enum: ['click', 'double_click', 'right_click', 'hover', 'drag', 'mouse_down', 'mouse_move', 'mouse_up', 'type', 'fill', 'select', 'scroll', 'wait', 'back', 'forward', 'touch', 'resize', 'eval', 'cookie', 'record_network', 'network', 'close'] }),
        selector: str('CSS selector'), text: str('Visible text to click (click and the mouse ops); for type, fill and select the text to put in'),
        role: str('With a text target: button, link, checkbox, radio, tab, menuitem, option, textbox'), within: str('With a text target: search inside this CSS selector'), nth: num('Which match, when several (0 = first)'),
        x: num('Page x'), y: num('Page y (for scroll: pixels down)'), toX: num('drag: release at x'), toY: num('drag: release at y'),
        gesture: str('For touch', { enum: ['tap', 'double_tap', 'long_press', 'swipe', 'pinch'] }), direction: str('For a swipe instead of toX/toY', { enum: ['up', 'down', 'left', 'right'] }), scale: num('For a pinch'),
        duration: num('ms for a drag, mouse_move or touch'), width: num('For resize'), height: num('For resize'), timeout: num('For wait, ms'),
        code: str('For eval'), cookie_action: str('For cookie', { enum: ['get', 'set', 'delete', 'clear'] }), cookie: { type: 'object', description: 'For cookie set/delete: { name, value, url, domain, path, ... }' },
        network_action: str('For record_network', { enum: ['start', 'stop', 'clear'] }), network: str('For network'),
      }, ['key', 'op']),
      safe: true,
      check: (i) => { needKey(i.key); },
      run: async (i) => {
        const key = i.key;
        // A point for the mouse ops: x,y as given, or where a text / selector target is.
        const where = async () => {
          if (i.x != null && i.y != null) return { x: Number(i.x), y: Number(i.y) };
          if (!i.text && !i.selector && !i.role) throw new Error('Give a target: text, selector, or x and y');
          const r = await call('browser_eval', { key, code: findScript({ selector: i.selector, text: i.text, role: i.role, within: i.within, nth: i.nth }) });
          const f = r && r.ok !== false ? parseJson(r.result) : null;
          if (!f || !f.ok) throw new Error((f && f.error) || (r && r.error) || 'Could not find it on the page');
          return { x: f.x, y: f.y, target: { role: f.role, text: f.text, tag: f.tag } };
        };
        const mouse = async (action, extra = {}) => {
          const p = await where();
          const r = await call('browser_mouse', { key, action, x: p.x, y: p.y, ...extra });
          return r && r.ok !== false && p.target ? { ...r, target: p.target } : r;
        };
        switch (i.op) {
          case 'click': return call('browser_click', { key, selector: i.selector, text: i.text, role: i.role, within: i.within, nth: i.nth, x: i.x, y: i.y });
          case 'double_click': return mouse('double_click');
          case 'right_click': return mouse('click', { button: 'right' });
          case 'hover': return mouse('move');
          case 'drag': {
            if (i.toX == null || i.toY == null) return { ok: false, error: 'drag needs toX and toY' };
            return mouse('drag', { toX: i.toX, toY: i.toY, duration: i.duration });
          }
          case 'mouse_down': return mouse('down');
          case 'mouse_move': return mouse('move', { duration: i.duration });
          case 'mouse_up': return call('browser_mouse', { key, action: 'up', ...(i.x != null && i.y != null ? { x: i.x, y: i.y } : {}) });
          case 'type': return call('browser_type', { key, text: String(i.text || ''), selector: i.selector });
          case 'fill': return call('browser_fill', { key, selector: i.selector, value: String(i.text || '') });
          case 'select': return call('browser_select', { key, selector: i.selector, value: String(i.text || '') });
          case 'scroll': return call('browser_scroll', { key, y: Number(i.y) || 0, selector: i.selector });
          case 'wait': return call('browser_wait', { key, selector: i.selector, timeout: i.timeout });
          case 'back': return call('browser_go_back', { key });
          case 'forward': return call('browser_go_forward', { key });
          case 'touch': return call('browser_touch', { key, action: i.gesture || 'tap', x: i.x, y: i.y, toX: i.toX, toY: i.toY, direction: i.direction, scale: i.scale, duration: i.duration });
          case 'resize': return call('browser_resize', { key, width: i.width, height: i.height });
          case 'eval': return call('browser_eval', { key, code: String(i.code || '') });
          case 'cookie': return call('browser_cookies', { key, action: i.cookie_action || 'get', cookie: i.cookie });
          case 'record_network': return call('browser_network', { key, action: i.network_action || 'start' });
          case 'network': return call('browser_tab_network', { key, network: String(i.network || '') });
          case 'close': return call('browser_close', { key });
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
