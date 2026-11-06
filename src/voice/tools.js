/**
 * voice/tools.js — what the voice assistant can look at (Phase 1: read only).
 *
 * Every tool here READS. Nothing in this file writes a file, a setting, a
 * board or a chat; the write tools of Phase 4 arrive as a separate, reviewed
 * list. scripts/test-voice.mjs pins the names below, so adding one is a
 * deliberate change, not an accident.
 *
 * Two things are never handed to the model:
 *   - secret VALUES (D16): secrets are listed by name and description only;
 *   - credentials inside settings: any setting whose name says token, key,
 *     secret, password or hash is reported as "set" or "not set".
 *
 * Files are read inside a project's folder only. Paths that climb out of it
 * ("..", absolute paths, symlinks pointing elsewhere) are refused.
 *
 * The tools get what they need through `deps`, built in webapp.js from the
 * live chat and terminal managers and the stores, so this file can be tested
 * without a server.
 */

import { readdirSync, statSync, readFileSync, realpathSync, existsSync } from 'node:fs';
import { join, resolve, relative, isAbsolute, sep } from 'node:path';

export const READ_ONLY_TOOL_NAMES = [
  'list_projects', 'list_folder', 'read_file', 'search_files',
  'get_settings', 'get_voice_spend',
  'list_services', 'get_service_logs',
  'list_notes', 'read_note',
  'get_kanban', 'get_kanban_task',
  'mindmap_overview', 'mindmap_search',
  'list_schedules', 'get_usage', 'list_secrets',
  'list_panels', 'read_chat', 'read_terminal',
  'list_containers', 'get_container_logs',
];

const MAX_FILE_CHARS = 20000;
const MAX_LIST = 200;
const SKIP_DIRS = new Set(['.git', 'node_modules', '.next', 'dist', 'build', '.venv', 'venv', '__pycache__', '.gradle', '.idea', 'target']);
const CREDENTIAL_NAME = /token|key|secret|password|passwd|hash|totp|credential/i;

// ─── helpers ───

/** A setting's value as the model may see it. */
export function redactSettings(obj) {
  if (Array.isArray(obj)) return obj.map(redactSettings);
  if (!obj || typeof obj !== 'object') return obj;
  const out = {};
  for (const [k, v] of Object.entries(obj)) {
    if (CREDENTIAL_NAME.test(k) && (typeof v === 'string' || typeof v === 'number')) out[k] = v === '' || v == null ? 'not set' : 'set';
    else out[k] = redactSettings(v);
  }
  return out;
}

/**
 * Resolve `p` inside the project folder `root`, or throw. Refuses absolute
 * paths, "..", and symlinks whose target is outside the folder.
 */
export function insideProject(root, p = '') {
  const want = String(p || '').replace(/\\/g, '/');
  if (isAbsolute(want) || /^[a-zA-Z]:/.test(want)) throw new Error('Use a path inside the project, not an absolute path');
  const base = resolve(root);
  const full = resolve(base, want || '.');
  const rel = relative(base, full);
  if (rel === '..' || rel.startsWith('..' + sep) || isAbsolute(rel)) throw new Error('That path is outside the project');
  if (existsSync(full)) {
    const real = realpathSync(full);
    const realBase = realpathSync(base);
    if (real !== realBase && !real.startsWith(realBase + sep)) throw new Error('That path leads outside the project');
  }
  return full;
}

function looksBinary(buf) {
  const n = Math.min(buf.length, 4000);
  for (let i = 0; i < n; i++) if (buf[i] === 0) return true;
  return false;
}

/**
 * A board as the model needs it: one short line per live task and the counts
 * per column. No descriptions, history or deleted tasks, which on a busy board
 * run far past what a tool result may hold (get_kanban_task has the detail).
 */
export function compactBoard(board) {
  const tasks = (Array.isArray(board?.tasks) ? board.tasks : []).filter(t => !t.deleted).map(t => {
    const live = (t.todos || []).filter(td => !td.deleted);
    return { id: t.id, title: t.title, status: t.status, todosDone: live.filter(td => td.done).length, todosTotal: live.length };
  });
  const counts = { total: tasks.length };
  for (const st of board?.statuses || ['backlog', 'todo', 'in_progress', 'done']) counts[st] = tasks.filter(t => t.status === st).length;
  return { counts, tasks };
}

/** Terminal output without colour codes and cursor moves. */
export function stripAnsi(s) {
  return String(s || '')
    .replace(/\x1b\][^\x07\x1b]*(\x07|\x1b\\)/g, '')       // OSC
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '')                // CSI
    .replace(/\x1b[@-Z\\-_]/g, '')                           // other escapes
    .replace(/\r(?!\n)/g, '\n')
    .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, '');
}

function obj(props = {}, required = []) {
  return { type: 'object', properties: props, required, additionalProperties: false };
}
const str = (description) => ({ type: 'string', description });
const num = (description) => ({ type: 'number', description });

// ─── the tools ───

/**
 * @param deps {
 *   listProjects(): {alias,name,path}[]
 *   settings(): object                      raw settings view (redacted here)
 *   voiceSpend(): object
 *   services(): object[]
 *   serviceLogs(key, lines): string[]|null
 *   notes: { listPages(alias), getPage(alias, id) }
 *   kanban: { getBoard(alias), getColumn(alias, status), getTask(alias, id) }
 *   mindmap: { getMindmap(project, {compact}), searchMindmap(q, project, limit) }
 *   schedules(project?): object[]
 *   usage(): Promise<object>
 *   secrets(): {name, description}[]
 *   chats(): object[]       terminals(): object[]
 *   chatHistory(id): {messages:[]}|null
 *   terminalScrollback(id): string
 *   docker?: { status(), listContainers({stats}), containerLogs(id, tail) }   (src/docker.js)
 * }
 */
export function makeTools(deps) {
  const project = (alias) => {
    const a = String(alias || '').toLowerCase();
    const p = deps.listProjects().find(x => x.alias === a);
    if (!p) throw new Error(`No project called "${alias}". Use list_projects.`);
    return p;
  };

  const tools = [
    {
      name: 'list_projects',
      description: 'List every project in Crundi: alias (used by other tools), display name and folder.',
      input_schema: obj(),
      run: () => deps.listProjects().map(p => ({ alias: p.alias, name: p.name, path: p.path })),
    },
    {
      name: 'list_folder',
      description: 'List the files and folders inside a project folder (or a subfolder of it).',
      input_schema: obj({ project: str('Project alias'), path: str('Folder inside the project; empty for the project root') }, ['project']),
      run: ({ project: alias, path = '' }) => {
        const root = project(alias).path;
        const dir = insideProject(root, path);
        const entries = readdirSync(dir, { withFileTypes: true })
          .filter(e => !e.name.startsWith('.git') || e.name === '.gitignore')
          .slice(0, MAX_LIST)
          .map(e => {
            let size;
            if (e.isFile()) { try { size = statSync(join(dir, e.name)).size; } catch { /* gone */ } }
            return { name: e.name, type: e.isDirectory() ? 'folder' : 'file', size };
          });
        return { path: relative(root, dir) || '.', entries };
      },
    },
    {
      name: 'read_file',
      description: `Read a text file inside a project. Long files are cut at ${MAX_FILE_CHARS} characters. Summarise rather than read code aloud.`,
      input_schema: obj({ project: str('Project alias'), path: str('File path inside the project') }, ['project', 'path']),
      run: ({ project: alias, path }) => {
        const file = insideProject(project(alias).path, path);
        const st = statSync(file);
        if (!st.isFile()) throw new Error('That is not a file');
        const buf = readFileSync(file);
        if (looksBinary(buf)) return { path, binary: true, size: st.size };
        const text = buf.toString('utf-8');
        return { path, size: st.size, truncated: text.length > MAX_FILE_CHARS, text: text.slice(0, MAX_FILE_CHARS) };
      },
    },
    {
      name: 'search_files',
      description: 'Find files in a project whose path contains the given words (case-insensitive). Skips .git, node_modules and build output.',
      input_schema: obj({ project: str('Project alias'), query: str('Words to look for in file paths') }, ['project', 'query']),
      run: ({ project: alias, query }) => {
        const root = project(alias).path;
        const words = String(query || '').toLowerCase().split(/\s+/).filter(Boolean);
        const hits = [];
        let seen = 0;
        const walk = (dir) => {
          if (hits.length >= 50 || seen > 20000) return;
          let entries;
          try { entries = readdirSync(dir, { withFileTypes: true }); } catch { return; }
          for (const e of entries) {
            if (hits.length >= 50) return;
            seen++;
            if (e.isDirectory()) { if (!SKIP_DIRS.has(e.name)) walk(join(dir, e.name)); continue; }
            const rel = relative(root, join(dir, e.name)).replace(/\\/g, '/');
            const low = rel.toLowerCase();
            if (words.every(w => low.includes(w))) hits.push(rel);
          }
        };
        walk(root);
        return { matches: hits };
      },
    },
    {
      name: 'get_settings',
      description: 'Read Crundi\'s settings. Credentials are shown only as "set" or "not set".',
      input_schema: obj(),
      run: () => redactSettings(deps.settings()),
    },
    {
      name: 'get_voice_spend',
      description: 'This month\'s spend on the voice assistant itself (Claude API and Deepgram), the monthly limit, and the model in use.',
      input_schema: obj(),
      run: () => deps.voiceSpend(),
    },
    {
      name: 'list_services',
      description: 'List background services: key (alias:name), project, status, CPU, memory, uptime.',
      input_schema: obj(),
      run: () => deps.services(),
    },
    {
      name: 'get_service_logs',
      description: 'The latest log lines of a running service.',
      input_schema: obj({ key: str('Service key, alias:name, from list_services'), lines: num('How many lines (default 30, max 200)') }, ['key']),
      run: ({ key, lines = 30 }) => {
        const out = deps.serviceLogs(String(key), Math.min(200, Math.max(1, Number(lines) || 30)));
        return out == null ? { key, running: false, lines: [] } : { key, running: true, lines: out.map(stripAnsi) };
      },
    },
    {
      name: 'list_notes',
      description: 'List the note pages of a project (title, id, preview).',
      input_schema: obj({ project: str('Project alias') }, ['project']),
      run: ({ project: alias }) => deps.notes.listPages(project(alias).alias),
    },
    {
      name: 'read_note',
      description: 'Read one note page of a project.',
      input_schema: obj({ project: str('Project alias'), id: str('Page id from list_notes') }, ['project', 'id']),
      run: ({ project: alias, id }) => deps.notes.getPage(project(alias).alias, String(id)),
    },
    {
      name: 'get_kanban',
      description: 'Kanban: one project\'s board (each task\'s id, title, status and todo progress, plus counts per column), or just one column. Leave project out for the task counts per column of every project. Statuses: backlog, todo, in_progress, done. Use get_kanban_task for a task\'s description and todos.',
      input_schema: obj({ project: str('Project alias; leave out for every project\'s counts'), status: str('Optional: only this column') }),
      run: ({ project: alias, status }) => {
        if (!alias) {
          return deps.listProjects()
            .map(p => ({ project: p.alias, ...compactBoard(deps.kanban.getBoard(p.alias)).counts }))
            .filter(c => c.total > 0)
            .sort((x, y) => y.total - x.total);
        }
        const a = project(alias).alias;
        return status ? deps.kanban.getColumn(a, String(status)) : compactBoard(deps.kanban.getBoard(a));
      },
    },
    {
      name: 'get_kanban_task',
      description: 'One kanban task with its todo list.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Task id') }, ['project', 'taskId']),
      run: ({ project: alias, taskId }) => deps.kanban.getTask(project(alias).alias, String(taskId)),
    },
    {
      name: 'mindmap_overview',
      description: 'The mindmap as a compact outline (no notes). Optionally only one project\'s part.',
      input_schema: obj({ project: str('Optional project alias') }),
      run: ({ project: alias } = {}) => deps.mindmap.getMindmap(alias ? project(alias).alias : null, { compact: true }),
    },
    {
      name: 'mindmap_search',
      description: 'Find mindmap ideas by keyword.',
      input_schema: obj({ query: str('Keywords'), project: str('Optional project alias') }, ['query']),
      run: ({ query, project: alias }) => deps.mindmap.searchMindmap(String(query), alias ? project(alias).alias : null, 30),
    },
    {
      name: 'list_schedules',
      description: 'Scheduled jobs (agent runs, commands, service start/stop), optionally for one project.',
      input_schema: obj({ project: str('Optional project alias') }),
      run: ({ project: alias } = {}) => redactSettings(deps.schedules(alias ? project(alias).alias : null)),
    },
    {
      name: 'get_usage',
      description: 'The owner\'s Claude subscription usage: the 5-hour and weekly windows as percentages, with reset times.',
      input_schema: obj(),
      run: () => deps.usage(),
    },
    {
      name: 'list_secrets',
      description: 'The names and descriptions of stored secrets. Values are never available to you.',
      input_schema: obj(),
      run: () => deps.secrets().map(s => ({ name: s.name, description: s.description || '' })),
    },
    {
      name: 'list_panels',
      description: 'The open chats (Claude Code conversations) and terminals, by project, with their state.',
      input_schema: obj(),
      run: () => ({
        chats: deps.chats().map(c => ({ id: c.id, project: c.project, title: c.title, status: c.status, state: c.agentState || null, model: c.model || '' })),
        terminals: deps.terminals().map(t => ({ id: t.id, project: t.project, title: t.title, status: t.status })),
      }),
    },
    {
      name: 'read_chat',
      description: 'The latest messages of an open chat: what the owner asked and what Claude answered, with tool use summarised.',
      input_schema: obj({ id: str('Chat id from list_panels'), messages: num('How many recent messages (default 12, max 40)') }, ['id']),
      run: ({ id, messages = 12 }) => {
        const h = deps.chatHistory(String(id));
        if (!h) throw new Error('No open chat with that id');
        const n = Math.min(40, Math.max(1, Number(messages) || 12));
        const out = [];
        for (const m of (h.messages || []).slice(-n * 3)) {
          if (m.kind === 'user') out.push({ from: 'Owner', text: clip(m.text) });
          else if (m.kind === 'assistant-text') out.push({ from: 'Claude', text: clip(m.text) });
          else if (m.kind === 'tool') out.push({ tool: m.name || m.tool || 'tool' });
          else if (m.kind === 'error') out.push({ error: clip(m.text || m.message) });
        }
        return { id, state: h.state, messages: out.slice(-n) };
      },
    },
    {
      name: 'read_terminal',
      description: 'The last lines a terminal printed. Read only: you cannot type into terminals.',
      input_schema: obj({ id: str('Terminal id from list_panels'), lines: num('How many lines (default 40, max 200)') }, ['id']),
      run: ({ id, lines = 40 }) => {
        const n = Math.min(200, Math.max(1, Number(lines) || 40));
        const text = stripAnsi(deps.terminalScrollback(String(id)));
        return { id, lines: text.split('\n').filter(l => l.trim()).slice(-n) };
      },
    },
    // Docker, read only and on demand: nothing is polled; a call reads Docker once.
    {
      name: 'list_containers',
      description: 'Docker containers on this server: name, image, state, health, compose project, published ports. With usage: true, also CPU %, memory and processes of each running one (takes a second or two; only when asked about load or usage). Read only.',
      input_schema: obj({ usage: { type: 'boolean', description: 'Also measure CPU and memory of running containers' }, all: { type: 'boolean', description: 'Include stopped containers (default true)' } }),
      run: async ({ usage = false, all = true } = {}) => {
        if (!deps.docker) return { error: 'Docker is not available here.' };
        const st = await deps.docker.status();
        if (!st || !st.installed) return { available: false, error: 'Docker is not installed on this server.' };
        if (!st.reachable) return { available: false, error: 'Docker is installed but not reachable: ' + (st.error || 'no answer') };
        let r = await deps.docker.listContainers({ stats: !!usage });
        // CPU needs two readings: the first only sets the baseline.
        if (usage && r.ok && r.containers.some(c => c.running && c.stats && c.stats.cpuPct == null)) {
          await new Promise(ok => setTimeout(ok, 1200));
          r = await deps.docker.listContainers({ stats: true });
        }
        if (!r.ok) return { error: r.error || 'Docker could not be read.' };
        const mb = (b) => b ? Math.round(b / 1048576) : 0;
        const list = r.containers.filter(c => all || c.running).map(c => ({
          name: c.name, image: c.image, state: c.state, status: c.status, health: c.health || undefined,
          project: c.project || undefined, service: c.service || undefined,
          ports: (c.ports || []).map(p => p.public ? `${p.public}->${p.private}` : String(p.private || '')).filter(Boolean),
          ...(c.stats ? { cpuPct: c.stats.cpuPct == null ? null : Math.round(c.stats.cpuPct * 10) / 10, memoryMB: mb(c.stats.memory), memoryLimitMB: mb(c.stats.memoryLimit), pids: c.stats.pids } : {}),
        }));
        return { running: list.filter(c => c.state === 'running').length, total: list.length, containers: list };
      },
    },
    {
      name: 'get_container_logs',
      description: 'The latest log lines of a Docker container. Read only.',
      input_schema: obj({ name: str('Container name or id, from list_containers'), lines: num('How many lines (default 30, max 200)') }, ['name']),
      run: async ({ name, lines = 30 }) => {
        if (!deps.docker) return { error: 'Docker is not available here.' };
        const n = Math.min(200, Math.max(1, Number(lines) || 30));
        const r = await deps.docker.containerLogs(String(name), n);
        if (!r.ok) return { error: r.error || 'No such container.' };
        return { name: r.name, lines: stripAnsi(r.text).split('\n').filter(l => l.trim()).slice(-n) };
      },
    },
  ];
  return tools;
}

function clip(s, n = 1500) {
  const t = String(s || '');
  return t.length > n ? t.slice(0, n) + '…' : t;
}
