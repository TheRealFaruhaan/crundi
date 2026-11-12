/**
 * browser-dispatch.js — the browser_* MCP tools, by name, onto browser.js.
 *
 * Shared by the chats' MCP calls (index.js mcpDispatch) and Cru's browser
 * tools (voice/browser-tools.js), so both reach the same handlers. The caller's
 * context (which project, whether it may use the drawer, homepool tabs, or is
 * Cru) rides in browser.js's callerContext, set here from the flags the
 * server put on the arguments; the flags are never passed on.
 */

import * as b from './browser.js';

export async function dispatchBrowserTool(tool, args = {}, { notifyEvent = () => {} } = {}) {
  const ctx = { alias: args.alias || '', noDrawer: !!args._noDrawer, hpAllowed: !!args._hpAllowed, cru: !!args._cru };
  const clean = { ...args };
  delete clean._noDrawer; delete clean._hpAllowed; delete clean._cru; delete clean.sessionId; delete clean.chatToken;
  return b.callerContext.run(ctx, () => browserTool(b, tool, clean, notifyEvent));
}

export async function browserTool(b, tool, args, notifyEvent = () => {}) {
    switch (tool) {
      // The same as browser_open on: "drawer", under a name a chat finds when it
      // looks for "a tab in the drawer" (one did not see it inside browser_open).
      case 'browser_drawer_open': {
        const r = await b.openBrowser({ ...args, on: 'drawer' });
        if (r && r.ok !== false) notifyEvent('browserLaunch', `🌐 Drawer tab opened${args.url ? `: ${args.url}` : ''}.`);
        return r;
      }
      case 'browser_open': {
        const r = await b.openBrowser(args);
        if (r && r.ok !== false) notifyEvent('browserLaunch', `🌐 Browser launched${args.key ? ` "${args.key}"` : ''}${args.url ? `: ${args.url}` : ''}.`);
        return r;
      }
      case 'browser_close': {
        const r = await b.closeBrowser(args.key);
        if (r && r.ok !== false) notifyEvent('browserStop', `🌐 Browser closed${args.key ? ` "${args.key}"` : ''}.`);
        return r;
      }
      case 'browser_navigate': return b.navigateBrowser(args.key, args.url);
      case 'browser_screenshot': {
        const r = await b.screenshotBrowser(args.key);
        return r; // data is already base64 from IPC
      }
      case 'browser_read_page': return b.readBrowserPage(args.key);
      case 'browser_view_source': return b.getBrowserSource(args.key);
      case 'browser_click': return b.clickBrowser(args.key, args.selector, { text: args.text, role: args.role, within: args.within, exact: args.exact, nth: args.nth, x: args.x, y: args.y, button: args.button });
      case 'browser_type': return b.typeBrowser(args.key, args.text, args.selector);
      case 'browser_fill': return b.fillBrowser(args.key, args.selector, args.value);
      case 'browser_select': return b.selectBrowser(args.key, args.selector, args.value, { label: args.label, index: args.index });
      case 'browser_eval': return b.evalBrowser(args.key, args.code);
      case 'browser_mouse': return b.mouseBrowser(args.key, args.x, args.y, args.action, { button: args.button, toX: args.toX, toY: args.toY, duration: args.duration });
      case 'browser_touch': return b.touchBrowser(args.key, { action: args.action, x: args.x, y: args.y, toX: args.toX, toY: args.toY, direction: args.direction, distance: args.distance, duration: args.duration, scale: args.scale, report: args.report });
      case 'browser_resize': return b.resizeBrowser(args.key, args.width, args.height);
      case 'browser_scroll': return b.scrollBrowser(args.key, { x: args.x, y: args.y, selector: args.selector });
      case 'browser_wait': return b.waitBrowser(args.key, { selector: args.selector, timeout: args.timeout });
      case 'browser_go_back': return b.goBackBrowser(args.key);
      case 'browser_go_forward': return b.goForwardBrowser(args.key);
      case 'browser_console': return b.getBrowserConsole(args.key, { clear: args.clear, start: args.start, end: args.end, countOnly: args.countOnly });
      case 'browser_network': return b.browserNetwork(args.key, args.action, { start: args.start, end: args.end, countOnly: args.countOnly });
      case 'browser_cookies': return b.cookiesBrowser(args.key, { action: args.action, cookie: args.cookie, filter: args.filter, url: args.url });
      case 'browser_snapshot': return b.snapshotBrowser(args.key, { maxDepth: args.maxDepth, maxLength: args.maxLength });
      case 'browser_elements': return b.elementsBrowser(args.key, { start: args.start, end: args.end, countOnly: args.countOnly, selector: args.selector });
      case 'browser_pdf': return b.pdfBrowser(args.key, { landscape: args.landscape });
      case 'browser_list':
      case 'list_browsers': {
        const all = await b.listAllBrowsers(args.alias);
        return { ok: true, ...all };
      }
      case 'browser_point': return b.pointDrawer(args.key, { x: args.x, y: args.y, selector: args.selector, label: args.label, show: args.show });
      case 'browser_annotate': return b.annotateDrawer(args.key, { shapes: args.shapes, clear: args.clear, show: args.show });
      case 'browser_annotations': return b.readDrawerAnnotations(args.key, { image: args.image });
      case 'browser_tab_network': return b.setDrawerNetwork(args.key, args.network);
      default:
        return { ok: false, error: `Unknown browser tool: ${tool}` };
    }
}
