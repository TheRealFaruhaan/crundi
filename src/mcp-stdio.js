#!/usr/bin/env node
/**
 * mcp-stdio.js — Standalone MCP server for Claude Code CLI sessions.
 *
 * Runs as a child process of Claude Code CLI via stdio transport.
 * Communicates with the main Crundi server via HTTP for Telegram
 * notifications and service management.
 *
 * Environment variables (set by Crundi when spawning Claude terminals):
 *   CRUNDI_API_URL  — http://localhost:<port> of the Crundi webapp
 *   CRUNDI_API_KEY  — internal API key for auth
 *   CRUNDI_PROJECT  — project alias (for context in notifications)
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { readFileSync, existsSync } from 'node:fs';
import http from 'node:http';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

const API_URL = process.env.CRUNDI_API_URL || 'http://localhost:8888';
const API_KEY = process.env.CRUNDI_API_KEY || '';
const PROJECT = process.env.CRUNDI_PROJECT || '';

// ─── HTTP client for Crundi webapp ───

function apiCall(tool, args = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL('/api/mcp/call', API_URL);
    const body = JSON.stringify({ tool, args });
    const req = http.request(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Api-Key': API_KEY,
        'Content-Length': Buffer.byteLength(body),
      },
    }, (res) => {
      let data = '';
      res.on('data', (c) => { data += c; });
      res.on('end', () => {
        try { resolve(JSON.parse(data)); }
        catch { resolve({ ok: false, error: 'Invalid response' }); }
      });
    });
    req.on('error', (err) => reject(err));
    req.write(body);
    req.end();
  });
}

// ─── Tool definition helpers ───

const keyOnlyBrowser = (name, description) => ({
  name, description,
  inputSchema: {
    type: 'object',
    properties: { key: { type: 'string', description: 'Browser instance key (alias:name)' } },
    required: ['key'],
  },
});

// ─── Tool definitions ───

const TOOLS = [
  // Telegram notification tools
  {
    name: 'send_message_to_user',
    description: 'Send a Telegram notification to the user. Use this to notify the user when you finish a task, when you need their input, or for important mid-turn status updates. The user may be away from the terminal, so always send a notification when your work is complete.',
    inputSchema: { type: 'object', properties: { message: { type: 'string', description: 'The message to send' } }, required: ['message'] },
  },
  {
    name: 'send_photo_to_user',
    description: 'Send a photo/image to the user via Telegram.',
    inputSchema: { type: 'object', properties: { path: { type: 'string', description: 'Absolute path to the image file' } }, required: ['path'] },
  },
  {
    name: 'send_file_to_user',
    description: 'Share a file with the user via a download link.',
    inputSchema: { type: 'object', properties: { path: { type: 'string', description: 'Absolute path to the file' } }, required: ['path'] },
  },

  // Service tools
  { name: 'list_services', description: 'List all registered services and their status.', inputSchema: { type: 'object', properties: {} } },
  {
    name: 'register_service', description: 'Register a new background service.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string', description: 'Service name' },
        command: { type: 'string', description: 'Shell command to run' },
        cwd: { type: 'string', description: 'Working directory' },
        stopCommand: { type: 'string', description: 'Optional command to stop the service' },
      },
      required: ['name', 'command'],
    },
  },
  { name: 'start_service', description: 'Start a registered service.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Service key (alias::name)' } }, required: ['key'] } },
  { name: 'stop_service', description: 'Stop a running service.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Service key' } }, required: ['key'] } },
  { name: 'restart_service', description: 'Restart a service.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Service key' } }, required: ['key'] } },
  { name: 'delete_service', description: 'Delete a registered service.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Service key' } }, required: ['key'] } },
  { name: 'get_service_logs', description: 'Get recent log output from a service.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Service key' }, lines: { type: 'number', description: 'Number of log lines (default 50)' } }, required: ['key'] } },

  // Tunnel tools
  { name: 'enable_tunnel', description: 'Start a Cloudflare tunnel for a local port.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Tunnel key' }, port: { type: 'number', description: 'Local port to tunnel' } }, required: ['key', 'port'] } },
  { name: 'disable_tunnel', description: 'Stop a Cloudflare tunnel.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Tunnel key' } }, required: ['key'] } },

  // Syntax check
  { name: 'syntax_check', description: 'Run syntax/compile checks on files. Auto-detects language from extension.', inputSchema: { type: 'object', properties: { files: { type: 'array', items: { type: 'string' }, description: 'Array of absolute file paths to check' } }, required: ['files'] } },

  // Browser tools
  {
    name: 'browser_open', description: 'Open a hidden browser at a URL. Up to 5 per project. Requires Electron.',
    inputSchema: {
      type: 'object',
      properties: {
        url: { type: 'string', description: 'URL to open' },
        name: { type: 'string', description: 'Instance name (default: "default")' },
        width: { type: 'number', description: 'Viewport width (default: 1280, max: 1920)' },
        height: { type: 'number', description: 'Viewport height (default: 720, max: 1080)' },
      },
      required: ['url'],
    },
  },
  keyOnlyBrowser('browser_close', 'Close a browser instance.'),
  { name: 'browser_navigate', description: 'Navigate a browser to a different URL.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, url: { type: 'string', description: 'URL to navigate to' } }, required: ['key', 'url'] } },
  keyOnlyBrowser('browser_screenshot', 'Capture the browser viewport as a PNG image (returned for analysis).'),
  keyOnlyBrowser('browser_read_page', "Get the page's cleaned HTML, title, URL, and recent console logs."),
  keyOnlyBrowser('browser_view_source', 'Get the raw live HTML source of the page.'),
  keyOnlyBrowser('browser_go_back', 'Navigate back in browser history.'),
  keyOnlyBrowser('browser_go_forward', 'Navigate forward in browser history.'),
  { name: 'browser_click', description: 'Click an element by CSS selector.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, selector: { type: 'string', description: 'CSS selector' } }, required: ['key', 'selector'] } },
  { name: 'browser_type', description: 'Type text, optionally into a specific element.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, text: { type: 'string', description: 'Text to type' }, selector: { type: 'string', description: 'CSS selector (optional)' } }, required: ['key', 'text'] } },
  { name: 'browser_fill', description: 'Fill an input field with a value (clears first).', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, selector: { type: 'string', description: 'CSS selector' }, value: { type: 'string', description: 'Value to fill' } }, required: ['key', 'selector', 'value'] } },
  { name: 'browser_select', description: 'Select an option in a <select> element.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, selector: { type: 'string', description: 'CSS selector' }, value: { type: 'string', description: 'Value to select' } }, required: ['key', 'selector', 'value'] } },
  { name: 'browser_eval', description: 'Run arbitrary JavaScript in the page and return the result.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, code: { type: 'string', description: 'JS code to evaluate' } }, required: ['key', 'code'] } },
  { name: 'browser_mouse', description: 'Send a mouse event (click/move) by coordinate.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, x: { type: 'number' }, y: { type: 'number' }, action: { type: 'string', description: '"click" or "move"' } }, required: ['key', 'x', 'y'] } },
  { name: 'browser_resize', description: 'Change the browser viewport size (max 1920x1080).', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, width: { type: 'number' }, height: { type: 'number' } }, required: ['key', 'width', 'height'] } },
  { name: 'browser_scroll', description: 'Scroll the page or a specific element.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, x: { type: 'number' }, y: { type: 'number' }, selector: { type: 'string' } }, required: ['key'] } },
  { name: 'browser_wait', description: 'Wait for an element to appear in the page.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, selector: { type: 'string', description: 'CSS selector to wait for' }, timeout: { type: 'number', description: 'Max wait ms (default: 30000)' } }, required: ['key', 'selector'] } },
  { name: 'browser_console', description: 'Read (and optionally clear) captured console log entries.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, clear: { type: 'boolean' }, start: { type: 'number' }, end: { type: 'number' }, countOnly: { type: 'boolean' } }, required: ['key'] } },
  { name: 'browser_network', description: 'Record and inspect network calls. Actions: start, stop, log, clear.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, action: { type: 'string', description: '"start", "stop", "log", or "clear"' }, start: { type: 'number' }, end: { type: 'number' }, countOnly: { type: 'boolean' } }, required: ['key', 'action'] } },
  { name: 'browser_cookies', description: 'Get, set, or delete cookies.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, action: { type: 'string', description: '"get", "set", or "delete"' }, cookie: { type: 'object' }, filter: { type: 'object' }, url: { type: 'string' } }, required: ['key'] } },
  { name: 'browser_snapshot', description: 'Get an accessibility tree snapshot of the page.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, maxDepth: { type: 'number' }, maxLength: { type: 'number' } }, required: ['key'] } },
  { name: 'browser_elements', description: 'List interactive elements on the page.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, selector: { type: 'string' }, start: { type: 'number' }, end: { type: 'number' }, countOnly: { type: 'boolean' } }, required: ['key'] } },
  { name: 'browser_pdf', description: 'Export the current page as a PDF.', inputSchema: { type: 'object', properties: { key: { type: 'string', description: 'Browser instance key' }, landscape: { type: 'boolean' } }, required: ['key'] } },
  { name: 'browser_list', description: 'List all open browser instances.', inputSchema: { type: 'object', properties: {} } },

  // Screenshot / display tools
  { name: 'list_windows', description: 'List all open windows across virtual desktops.', inputSchema: { type: 'object', properties: {} } },
  { name: 'list_displays', description: 'List all connected displays/monitors.', inputSchema: { type: 'object', properties: {} } },
  { name: 'capture_window', description: 'Capture a screenshot of a specific window (returned as image).', inputSchema: { type: 'object', properties: { windowId: { type: 'number', description: 'Window ID (hwnd) from list_windows' } }, required: ['windowId'] } },
  { name: 'capture_display', description: 'Capture a screenshot of a display/monitor (returned as image).', inputSchema: { type: 'object', properties: { display: { type: 'number', description: '1-based display index (default: 1)' } } } },
  { name: 'send_window_screenshot_to_user', description: 'Capture a window screenshot and send it to the user via Telegram.', inputSchema: { type: 'object', properties: { windowId: { type: 'number', description: 'Window ID (hwnd)' } }, required: ['windowId'] } },
  { name: 'send_display_screenshot_to_user', description: 'Capture a display screenshot and send it to the user via Telegram.', inputSchema: { type: 'object', properties: { display: { type: 'number', description: '1-based display index (default: 1)' } } } },

  // User terminal tools (non-Claude PTY instances)
  { name: 'spawn_terminal', description: 'Create a named terminal. Requires Electron.', inputSchema: { type: 'object', properties: { name: { type: 'string', description: 'Terminal name' }, command: { type: 'string', description: 'Initial command' }, cwd: { type: 'string', description: 'Working directory' } }, required: ['name'] } },
  { name: 'terminal_input', description: 'Send commands/keystrokes to a named terminal.', inputSchema: { type: 'object', properties: { name: { type: 'string', description: 'Terminal name' }, input: { type: 'string', description: 'Text to send' } }, required: ['name', 'input'] } },
  { name: 'terminal_output', description: 'Read buffered output from a named terminal.', inputSchema: { type: 'object', properties: { name: { type: 'string', description: 'Terminal name' }, start: { type: 'number' }, end: { type: 'number' }, countOnly: { type: 'boolean' } }, required: ['name'] } },
  { name: 'terminal_wait', description: 'Block until a pattern appears in terminal output or timeout.', inputSchema: { type: 'object', properties: { name: { type: 'string', description: 'Terminal name' }, pattern: { type: 'string', description: 'Pattern to wait for' }, timeout: { type: 'number', description: 'Max wait ms (default: 30000)' } }, required: ['name', 'pattern'] } },
  { name: 'close_terminal', description: 'Kill a named terminal and remove it.', inputSchema: { type: 'object', properties: { name: { type: 'string', description: 'Terminal name' } }, required: ['name'] } },
  { name: 'list_terminals', description: 'List all open named terminals with their status.', inputSchema: { type: 'object', properties: {} } },

  // RDP tools
  { name: 'disconnect_rdp', description: 'Disconnect RDP session (keeps GUI alive for screenshots).', inputSchema: { type: 'object', properties: {} } },

  // ─── Kanban tools (project-scoped) ───
  { name: 'kanban_list', description: 'List the Kanban board for the current project: task cards grouped by status, each with its todo checklist. Statuses: backlog, todo, in_progress, done. Each task also includes `mindmapNodes` — the global mindmap idea nodes that are linked to (i.e. brainstorm) that task, so you can see which tasks have been expanded in the mindmap.', inputSchema: { type: 'object', properties: { includeDeleted: { type: 'boolean', description: 'Include soft-deleted tasks/todos (default false)' } } } },
  { name: 'kanban_get_task', description: 'Get ONE task by id with full detail — its todos, plus the mindmap nodes linked to it. Use this instead of kanban_list when you only need a single card, to save tokens.', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, includeDeleted: { type: 'boolean', description: 'Include soft-deleted todos (default false)' } }, required: ['taskId'] } },
  { name: 'kanban_list_column', description: 'List lightweight task summaries (id, title, todo counts, linked-mindmap count) for ONE status column only — backlog | todo | in_progress | done. Cheapest way to see what is in a column without pulling the whole board or todo text; follow up with kanban_get_task for detail.', inputSchema: { type: 'object', properties: { status: { type: 'string', description: 'backlog | todo | in_progress | done' }, includeDeleted: { type: 'boolean' } }, required: ['status'] } },
  { name: 'kanban_add_task', description: 'Add a task card to the Kanban board.', inputSchema: { type: 'object', properties: { title: { type: 'string', description: 'Task title' }, description: { type: 'string', description: 'Optional details' }, status: { type: 'string', description: 'backlog | todo | in_progress | done (default todo)' }, todos: { type: 'array', items: { type: 'string' }, description: 'Optional initial todo checklist items' } }, required: ['title'] } },
  { name: 'kanban_update_task', description: 'Update a task\'s title, description, and/or status (move between columns).', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, title: { type: 'string' }, description: { type: 'string' }, status: { type: 'string', description: 'backlog | todo | in_progress | done' } }, required: ['taskId'] } },
  { name: 'kanban_move_task', description: 'Move/reorder a task: set its status column and optionally its position WITHIN that column. index is 0-based among the column\'s tasks (0 = top). Use this to reorder cards too.', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, status: { type: 'string', description: 'backlog | todo | in_progress | done' }, index: { type: 'number', description: 'Optional 0-based position within the target column' } }, required: ['taskId', 'status'] } },
  { name: 'kanban_delete_task', description: 'Soft-delete a task (recoverable — it moves to the trash, never truly removed).', inputSchema: { type: 'object', properties: { taskId: { type: 'string' } }, required: ['taskId'] } },
  { name: 'kanban_restore_task', description: 'Restore a previously deleted task.', inputSchema: { type: 'object', properties: { taskId: { type: 'string' } }, required: ['taskId'] } },
  { name: 'kanban_add_todo', description: 'Add a todo checklist item to a task.', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, text: { type: 'string' } }, required: ['taskId', 'text'] } },
  { name: 'kanban_update_todo', description: 'Update a todo\'s text and/or checked state.', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, todoId: { type: 'string' }, text: { type: 'string' }, done: { type: 'boolean' } }, required: ['taskId', 'todoId'] } },
  { name: 'kanban_delete_todo', description: 'Soft-delete a todo (recoverable).', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, todoId: { type: 'string' } }, required: ['taskId', 'todoId'] } },
  { name: 'kanban_restore_todo', description: 'Restore a previously deleted todo.', inputSchema: { type: 'object', properties: { taskId: { type: 'string' }, todoId: { type: 'string' } }, required: ['taskId', 'todoId'] } },
  { name: 'kanban_history', description: 'Get the full immutable change history for this project\'s Kanban board.', inputSchema: { type: 'object', properties: {} } },

  // ─── Claude usage (real, account-wide limits) ───
  { name: 'get_usage', description: 'Get the user\'s REAL Claude account usage (the same numbers as the /usage command): 5-hour rolling window and weekly (7-day) utilization as percentages, plus reset times and subscription/rate-limit tier. Account-wide, not just this app. Use this to check how close the user is to their limits.', inputSchema: { type: 'object', properties: { force: { type: 'boolean', description: 'Bypass the 60s cache and fetch fresh' } } } },

  // ─── Mindmap tools (brainstorming map; scoped to THIS project) ───
  { name: 'mindmap_list', description: 'List the mindmap nodes scoped to THIS project ONLY (nodes created here, nodes linked to this project\'s Kanban tasks, and their sub-ideas). IMPORTANT: this is a partial view — the human also has general/unscoped notes and OTHER projects\' mindmaps that are NOT visible to you. So if the user refers to a mind-map idea you can\'t find here, do NOT assume it doesn\'t exist: tell them it isn\'t linked to this project and may be a general note or live under another project, and ask them to link/scope it to this project (via the node\'s "Link / scope" in the web UI, or mindmap_link_node) so you can see it. PERFORMANCE: this can be large. For big maps prefer mindmap_search (find by keyword) or mindmap_get_children (walk level-by-level); or pass compact:true here for a note-less skeleton (id, text, parentId, noteCount, hasChildren, link) and then drill with mindmap_get_subtree. Full (non-compact) node: id, text, notes (LIST of strings), parentId (null=root), and `linkedTaskInfo` (linked task\'s project/title/status/todo progress; dangling links marked missing).', inputSchema: { type: 'object', properties: { compact: { type: 'boolean', description: 'Return a note-less skeleton (much smaller) instead of full nodes' } } } },
  { name: 'mindmap_search', description: 'Find mindmap nodes in THIS project by keyword (matches node text and notes). Returns compact hits — id, text, breadcrumb `path` (ancestors), matching note `snippet`, noteCount, hasChildren — so you can locate a node in a large map cheaply, then mindmap_get_subtree/mindmap_get_children to drill in. Same scope caveat as mindmap_list: only this project\'s nodes are searchable.', inputSchema: { type: 'object', properties: { query: { type: 'string', description: 'Keyword(s) to match in node text/notes' }, limit: { type: 'number', description: 'Max results (default 30, max 100)' } }, required: ['query'] } },
  { name: 'mindmap_get_subtree', description: 'Get one node and ALL of its descendants (within THIS project only) as a nested tree, each with its linked-task info and notes list. Use instead of mindmap_list to fetch a single branch and save tokens. Errors if the node is not in this project — note that general/unscoped or other-project nodes are invisible to you, so a "not found" may just mean the user needs to link/scope it to this project first.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Node id to start from' } }, required: ['id'] } },
  { name: 'mindmap_get_children', description: 'Get only the DIRECT children of a node within THIS project (one level, no descendants); omit id to get this project\'s root nodes. Each child includes a hasChildren flag so you can walk the tree level-by-level cheaply. Remember this view excludes general/unscoped and other-project nodes — if something the user mentions is missing, ask them to link/scope it to this project.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Parent node id; omit/empty for root nodes' } } } },
  { name: 'mindmap_get_ancestors', description: 'Get a node\'s ancestor chain up to the root, in order (root → … → parent), plus the node itself and its depth. Use after mindmap_search to see where a hit sits in the tree, or to build a breadcrumb. Scoped to this project.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Node id whose ancestors you want' } }, required: ['id'] } },
  { name: 'mindmap_add_node', description: 'Add a mindmap idea node, automatically scoped to THIS project (even when not linked to a Kanban task). Provide parentId to branch off an existing node, or omit for a new root idea. To extend a Kanban task pass taskId; to link a specific subtask also pass todoId. A child strictly inherits its parent\'s project — linking it to another project\'s task is rejected. Notes are a LIST: pass `notes` (array of strings).', inputSchema: { type: 'object', properties: { text: { type: 'string', description: 'Node text' }, parentId: { type: 'string', description: 'Parent node id (omit for a root idea)' }, notes: { type: 'array', items: { type: 'string' }, description: 'Optional list of note strings' }, taskId: { type: 'string', description: 'Optional Kanban task id to link this node to' }, todoId: { type: 'string', description: 'Optional todo (subtask) id within the task to link to' }, project: { type: 'string', description: 'Project alias for the linked task (defaults to current project)' } }, required: ['text'] } },
  { name: 'mindmap_update_node', description: 'Update a mindmap node\'s text and/or its full notes list. Pass `notes` (array of strings) to REPLACE the whole list. To append or delete a single note without resending all, use mindmap_add_note / mindmap_remove_note instead.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, text: { type: 'string' }, notes: { type: 'array', items: { type: 'string' }, description: 'Replaces the entire notes list' } }, required: ['id'] } },
  { name: 'mindmap_add_note', description: 'Append one note to a mindmap node\'s notes list.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, text: { type: 'string', description: 'Note text to append' } }, required: ['id', 'text'] } },
  { name: 'mindmap_remove_note', description: 'Remove the note at the given 0-based index from a mindmap node\'s notes list.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, index: { type: 'number', description: '0-based index of the note to remove' } }, required: ['id', 'index'] } },
  { name: 'mindmap_move_node', description: 'Move/reorder a node: reparent it (parentId null/empty = root) and optionally set its position among its new siblings via index (0-based, 0 = first). Use this to reorder branches too. Rejects moves that would create a cycle.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, parentId: { type: 'string', description: 'New parent id, or null/empty for root' }, index: { type: 'number', description: 'Optional 0-based position among siblings' } }, required: ['id'] } },
  { name: 'mindmap_link_node', description: 'Link a mindmap node to a Kanban task, or to a specific subtask by also passing todoId.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, taskId: { type: 'string' }, todoId: { type: 'string', description: 'Optional todo (subtask) id within the task' }, project: { type: 'string', description: 'Project alias (defaults to current project)' } }, required: ['id', 'taskId'] } },
  { name: 'mindmap_unlink_node', description: 'Remove a node\'s link to a Kanban task (the node stays as a free-standing idea).', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },
  { name: 'mindmap_delete_node', description: 'Delete a node and all of its descendant nodes.', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },

  // ─── Schedule tools (strictly scoped to THIS project — you cannot target another) ───
  { name: 'schedule_list', description: 'List scheduled tasks for THIS project. Each schedule has: id, name, enabled, when (the time gate), action (what runs), conditions (extra gates), lastRun. Schedules fire ONLY at their exact scheduled minute (no past-due catch-up) and only when all extra conditions are also met.', inputSchema: { type: 'object', properties: {} } },
  { name: 'schedule_get', description: 'Get one scheduled task by id (must belong to this project).', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },
  { name: 'schedule_add', description: 'Create a scheduled task in THIS project (project is fixed — not settable). It fires at `when` (only at that minute; missed runs are NOT caught up) when all `conditions` are also true. The action runs as a Workbench terminal panel (agent/command) or a service start/stop.', inputSchema: { type: 'object', properties: {
    name: { type: 'string', description: 'Schedule name' },
    enabled: { type: 'boolean', description: 'Default true' },
    when: { type: 'object', description: 'Time gate (mandatory). One-time: {mode:"once", date:"YYYY-MM-DD", time:"HH:MM"}. Recurring: {mode:"recurring", days:[0..6] (0=Sun), time:"HH:MM"}. Time is the server local clock, 24h.', properties: { mode: { type: 'string', description: 'once | recurring' }, time: { type: 'string', description: 'HH:MM (24h)' }, date: { type: 'string', description: 'YYYY-MM-DD (one-time only)' }, days: { type: 'array', items: { type: 'number' }, description: '0=Sun..6=Sat (recurring only)' } }, required: ['mode', 'time'] },
    action: { type: 'object', description: 'What to run. Agent: {kind:"agent", agent:"claude", mode:"normal"|"skip", model?:"opus|sonnet|haiku|fable|mythos", effort?:"low|medium|high|xhigh|max", session:"new"|"continue"|"resume", sessionId?, prompt}. Slash commands like /frontend-design or /loop are allowed in the prompt. Command: {kind:"command", command}. Service: {kind:"service", serviceKey, op:"start"|"stop"|"toggle"}.', properties: { kind: { type: 'string', description: 'agent | command | service' }, agent: { type: 'string', description: 'claude (only option for now)' }, mode: { type: 'string', description: 'normal | skip (agent)' }, model: { type: 'string' }, effort: { type: 'string' }, session: { type: 'string', description: 'new | continue | resume (agent)' }, sessionId: { type: 'string', description: 'for session=resume' }, prompt: { type: 'string', description: 'agent prompt' }, command: { type: 'string', description: 'for kind=command' }, serviceKey: { type: 'string', description: 'for kind=service' }, op: { type: 'string', description: 'start | stop | toggle (service)' } }, required: ['kind'] },
    conditions: { type: 'array', description: 'Optional extra gates (ALL must be true at the scheduled minute). Items: {type:"usage", metric:"5h"|"week", op:"below"|"above"|"equal", value} | {type:"terminals", op, value} (open Claude terminals in this project) | {type:"kanban", taskId, todoId?, status} (task column status, or subtask status "done"|"pending" when todoId given).', items: { type: 'object' } },
  }, required: ['name', 'when', 'action'] } },
  { name: 'schedule_update', description: 'Update a scheduled task (must belong to this project). Pass only the fields to change (name/enabled/when/action/conditions). when/action/conditions REPLACE the existing value when provided.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, name: { type: 'string' }, enabled: { type: 'boolean' }, when: { type: 'object' }, action: { type: 'object' }, conditions: { type: 'array', items: { type: 'object' } } }, required: ['id'] } },
  { name: 'schedule_set_enabled', description: 'Enable or disable a scheduled task (must belong to this project).', inputSchema: { type: 'object', properties: { id: { type: 'string' }, enabled: { type: 'boolean' } }, required: ['id', 'enabled'] } },
  { name: 'schedule_delete', description: 'Delete a scheduled task (must belong to this project).', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },

  // ─── Media tools (attached files — strictly scoped to THIS project) ───
  { name: 'media_list', description: 'List media files (images, PDFs, video/audio, or any file) attached within THIS project. Each item includes: id, originalName, mime, kind (image|pdf|video|audio|other), size, `path` (the absolute on-disk path so you can open/read the file yourself), `link` (what it is attached to: {type:"task",taskId} | {type:"todo",taskId,todoId} | {type:"node",nodeId} | null for unlinked) and `linkStatus` (alive|deleted|none). Use the `path` to inspect a file directly.', inputSchema: { type: 'object', properties: {} } },
  { name: 'media_get', description: 'Get one media item by id (must belong to this project), including its absolute on-disk `path`.', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },
  { name: 'media_add_path', description: 'Attach a file that already exists on disk as a media item in THIS project (the file is copied into Crundi\'s media store). Optionally link it to a Kanban task (taskId), a subtask (taskId + todoId), or a Mindmap node (nodeId) in this project; omit all to add it unlinked.', inputSchema: { type: 'object', properties: { path: { type: 'string', description: 'Absolute path to the source file' }, name: { type: 'string', description: 'Optional display name (defaults to the file name)' }, taskId: { type: 'string', description: 'Optional Kanban task id to attach to' }, todoId: { type: 'string', description: 'Optional subtask id (requires taskId)' }, nodeId: { type: 'string', description: 'Optional Mindmap node id to attach to' } }, required: ['path'] } },
  { name: 'media_delete', description: 'Delete a media item by id (must belong to this project). Removes the stored file too.', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },

  // ─── Secrets tools (global) ───
  { name: 'secret_search', description: 'Search the global secrets store by name and description (case-insensitive). Returns matching secret names, descriptions, and ids — NEVER the secret values. No approval needed. Use this to discover which secret to request.', inputSchema: { type: 'object', properties: { query: { type: 'string', description: 'Search text matched against name and description. Empty returns all.' } } } },
  { name: 'secret_get', description: 'Request the decrypted value of a secret. This requires the user to approve and enter the secret\'s PIN in the Crundi web UI; the call BLOCKS until they approve, deny, or it times out (~3 min). The user is notified via Telegram. Identify the secret by id (preferred) or exact name. Always provide a clear reason.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Secret id (from secret_search) — preferred' }, name: { type: 'string', description: 'Exact secret name (used if id not given)' }, reason: { type: 'string', description: 'Why you need it — shown to the user on the approval request' } } } },
];

// ─── Tool handler ───

const ALIAS_TOOLS = new Set(['browser_open', 'browser_list', 'register_service', 'spawn_terminal', 'list_terminals', 'terminal_input', 'terminal_output', 'terminal_wait', 'close_terminal',
  'kanban_list', 'kanban_get_task', 'kanban_list_column', 'kanban_add_task', 'kanban_update_task', 'kanban_move_task', 'kanban_delete_task', 'kanban_restore_task',
  'kanban_add_todo', 'kanban_update_todo', 'kanban_delete_todo', 'kanban_restore_todo', 'kanban_history',
  'mindmap_list', 'mindmap_search', 'mindmap_get_subtree', 'mindmap_get_children', 'mindmap_get_ancestors', 'mindmap_add_node', 'mindmap_link_node',
  'schedule_list', 'schedule_get', 'schedule_add', 'schedule_update', 'schedule_set_enabled', 'schedule_delete',
  'media_list', 'media_get', 'media_add_path', 'media_delete',
  'secret_get']);
const IMAGE_TOOLS = new Set(['browser_screenshot', 'capture_window', 'capture_display']);

// Tools backed by Windows-only mechanisms — don't advertise them off Windows so
// the agent never attempts a call that can only return "unsupported". The server
// runs on the same host as Crundi, so process.platform is the host platform.
const WINDOWS_ONLY_TOOLS = new Set(['disconnect_rdp']);
const ADVERTISED_TOOLS = process.platform === 'win32'
  ? TOOLS
  : TOOLS.filter(t => !WINDOWS_ONLY_TOOLS.has(t.name));

async function handleToolCall(name, args) {
  if (name === 'syntax_check') return runSyntaxCheck(args.files || []);

  if (ALIAS_TOOLS.has(name) && !args.alias) args.alias = PROJECT;

  const result = await apiCall(name, args);

  if (IMAGE_TOOLS.has(name) && result.ok && result.data) {
    return { content: [{ type: 'image', data: result.data, mimeType: 'image/png' }] };
  }

  if (!result.ok && result.error) {
    return { content: [{ type: 'text', text: `Error: ${result.error}` }], isError: true };
  }
  return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
}

// ─── Syntax check (local) ───

function runSyntaxCheck(files) {
  const { execFileSync } = require('child_process');
  const results = [];
  for (const file of files) {
    if (!existsSync(file)) { results.push(`[FAIL] ${file}\nFile not found`); continue; }
    const ext = file.substring(file.lastIndexOf('.')).toLowerCase();
    let cmd, cmdArgs;
    switch (ext) {
      case '.js': case '.mjs': case '.cjs':
        cmd = 'node'; cmdArgs = ['--check', file]; break;
      case '.py':
        cmd = 'python'; cmdArgs = ['-m', 'py_compile', file]; break;
      case '.json':
        try { JSON.parse(readFileSync(file, 'utf-8')); results.push(`[PASS] ${file}\nOK`); } catch (e) { results.push(`[FAIL] ${file}\n${e.message}`); }
        continue;
      default:
        results.push(`[SKIP] ${file}\nNo checker for ${ext}`); continue;
    }
    try {
      execFileSync(cmd, cmdArgs, { timeout: 10000, encoding: 'utf-8' });
      results.push(`[PASS] ${file}\nOK`);
    } catch (err) {
      results.push(`[FAIL] ${file}\n${err.stderr || err.message}`);
    }
  }
  const passed = results.filter(r => r.startsWith('[PASS]')).length;
  const failed = results.filter(r => r.startsWith('[FAIL]')).length;
  return { content: [{ type: 'text', text: `${passed} passed, ${failed} failed out of ${files.length} file(s)\n\n${results.join('\n\n')}` }] };
}

// ─── Server setup ───

const server = new Server(
  { name: 'crundi', version: '5.0.0' },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: ADVERTISED_TOOLS };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  try {
    return await handleToolCall(name, args || {});
  } catch (err) {
    return { content: [{ type: 'text', text: `Error calling ${name}: ${err.message}` }], isError: true };
  }
});

// ─── Start ───

const transport = new StdioServerTransport();
await server.connect(transport);
