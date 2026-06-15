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
  { name: 'mindmap_list', description: 'List the mindmap nodes scoped to THIS project (nodes you created here, anything linked to this project\'s Kanban tasks, and their sub-ideas). The human can see all projects\' nodes plus their own general notes in the web UI, but you only see this project\'s. Each node has id, text, note, parentId (null=root), and `linkedTaskInfo` — if the node links to a Kanban task, this holds that task\'s live project, title, description, status, deleted flag, and todo progress (todosDone/todosTotal); a dangling link is marked missing.', inputSchema: { type: 'object', properties: {} } },
  { name: 'mindmap_get_subtree', description: 'Get one node and ALL of its descendants (within this project) as a nested tree, each with its linked-task info. Use instead of mindmap_list to fetch a single branch and save tokens. Errors if the node is not in this project.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Node id to start from' } }, required: ['id'] } },
  { name: 'mindmap_get_children', description: 'Get only the DIRECT children of a node within this project (one level, no descendants); omit id to get this project\'s root nodes. Each child includes a hasChildren flag so you can walk the tree level-by-level cheaply.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Parent node id; omit/empty for root nodes' } } } },
  { name: 'mindmap_add_node', description: 'Add a mindmap idea node, automatically scoped to THIS project (even when not linked to a Kanban task). Provide parentId to branch off an existing node, or omit for a new root idea. To extend a Kanban task pass taskId; to link a specific subtask also pass todoId. A child strictly inherits its parent\'s project — linking it to another project\'s task is rejected.', inputSchema: { type: 'object', properties: { text: { type: 'string', description: 'Node text' }, parentId: { type: 'string', description: 'Parent node id (omit for a root idea)' }, note: { type: 'string', description: 'Optional longer note' }, taskId: { type: 'string', description: 'Optional Kanban task id to link this node to' }, todoId: { type: 'string', description: 'Optional todo (subtask) id within the task to link to' }, project: { type: 'string', description: 'Project alias for the linked task (defaults to current project)' } }, required: ['text'] } },
  { name: 'mindmap_update_node', description: 'Update a mindmap node\'s text and/or note.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, text: { type: 'string' }, note: { type: 'string' } }, required: ['id'] } },
  { name: 'mindmap_move_node', description: 'Move/reorder a node: reparent it (parentId null/empty = root) and optionally set its position among its new siblings via index (0-based, 0 = first). Use this to reorder branches too. Rejects moves that would create a cycle.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, parentId: { type: 'string', description: 'New parent id, or null/empty for root' }, index: { type: 'number', description: 'Optional 0-based position among siblings' } }, required: ['id'] } },
  { name: 'mindmap_link_node', description: 'Link a mindmap node to a Kanban task, or to a specific subtask by also passing todoId.', inputSchema: { type: 'object', properties: { id: { type: 'string' }, taskId: { type: 'string' }, todoId: { type: 'string', description: 'Optional todo (subtask) id within the task' }, project: { type: 'string', description: 'Project alias (defaults to current project)' } }, required: ['id', 'taskId'] } },
  { name: 'mindmap_unlink_node', description: 'Remove a node\'s link to a Kanban task (the node stays as a free-standing idea).', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },
  { name: 'mindmap_delete_node', description: 'Delete a node and all of its descendant nodes.', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] } },

  // ─── Secrets tools (global) ───
  { name: 'secret_search', description: 'Search the global secrets store by name and description (case-insensitive). Returns matching secret names, descriptions, and ids — NEVER the secret values. No approval needed. Use this to discover which secret to request.', inputSchema: { type: 'object', properties: { query: { type: 'string', description: 'Search text matched against name and description. Empty returns all.' } } } },
  { name: 'secret_get', description: 'Request the decrypted value of a secret. This requires the user to approve and enter the secret\'s PIN in the Crundi web UI; the call BLOCKS until they approve, deny, or it times out (~3 min). The user is notified via Telegram. Identify the secret by id (preferred) or exact name. Always provide a clear reason.', inputSchema: { type: 'object', properties: { id: { type: 'string', description: 'Secret id (from secret_search) — preferred' }, name: { type: 'string', description: 'Exact secret name (used if id not given)' }, reason: { type: 'string', description: 'Why you need it — shown to the user on the approval request' } } } },
];

// ─── Tool handler ───

const ALIAS_TOOLS = new Set(['browser_open', 'browser_list', 'register_service', 'spawn_terminal', 'list_terminals', 'terminal_input', 'terminal_output', 'terminal_wait', 'close_terminal',
  'kanban_list', 'kanban_get_task', 'kanban_list_column', 'kanban_add_task', 'kanban_update_task', 'kanban_move_task', 'kanban_delete_task', 'kanban_restore_task',
  'kanban_add_todo', 'kanban_update_todo', 'kanban_delete_todo', 'kanban_restore_todo', 'kanban_history',
  'mindmap_list', 'mindmap_get_subtree', 'mindmap_get_children', 'mindmap_add_node', 'mindmap_link_node',
  'secret_get']);
const IMAGE_TOOLS = new Set(['browser_screenshot', 'capture_window', 'capture_display']);

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
  return { tools: TOOLS };
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
