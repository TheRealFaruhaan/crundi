/**
 * voice/actions.js: what the voice assistant may change (Phase 4).
 *
 * Every tool here changes something, so every one goes through the same gate
 * (gated()):
 *
 *   - the phone is locked: nothing runs; the owner is asked to unlock (spec §4);
 *   - the action needs a yes (the rules below): it is not run. It is parked
 *     with an id and the question, and the model is told to ask. It runs only
 *     through confirm_action, and only once the owner has said something since
 *     (a later turn), so the model can never approve its own action;
 *   - otherwise it runs.
 *
 * Which need a yes:
 *   D15  a message into a chat: sent at once when the owner clearly asked for it
 *        (intent 'explicit'); read back first when the model decided to send
 *        it itself (intent 'inferred'). A long message is read back as one line.
 *   D24  closing a chat; D31 parking one.
 *   D22  creating a project (a new folder).
 *   D32  every setting change, the voice's own included.
 *   D45  creating, changing or deleting a schedule (it acts later, unattended).
 *        Deleting anything (cards, notes, nodes), and moving a card to To do,
 *        which starts a factory run.
 *
 * Never here: typing into a terminal (D21), keys and critical settings (D17),
 * secret values (D16), scheduled shell commands (they would be terminal input
 * by another name).
 */

import { join } from 'node:path';

export const ACTION_TOOL_NAMES = [
  'send_to_chat', 'start_chat', 'close_chat', 'park_chat', 'resume_chat', 'watch_chat', 'unwatch_chat',
  'add_kanban_task', 'update_kanban_task', 'move_kanban_task', 'delete_kanban_task', 'add_kanban_todo', 'check_kanban_todo',
  'create_note', 'append_to_note', 'delete_note',
  'add_mindmap_node', 'update_mindmap_node', 'delete_mindmap_node',
  'create_project',
  'list_chat_schedules', 'add_chat_schedule', 'delete_chat_schedule',
  'add_schedule', 'update_schedule', 'set_schedule_enabled', 'delete_schedule',
  'set_voice_setting', 'set_background_listening',
  'confirm_action', 'cancel_action', 'close_voice',
];

/** D6: how long background "Hey Cru" listening may be turned on for by voice (minutes). */
export const LISTEN_MINUTES = [15, 30, 45, 60, 120];

/** Settings voice may change (each confirmed, D32). Keys and critical settings are not among them (D17). */
export const VOICE_SETTINGS = ['model', 'ttsVoice', 'speechRate', 'monthlyLimitUsd', 'deepgramLimitUsd', 'pauseMs', 'billingDay', 'deepgramBillingDay'];

const PENDING_MS = 10 * 60 * 1000;
const STATUSES = ['backlog', 'todo', 'in_progress', 'done'];

/**
 * Actions waiting for the owner's yes. Pure apart from the clock, so it can be
 * tested: an action proposed in turn N can only be taken in turn N+1 or later.
 */
export function createPending({ now = () => Date.now() } = {}) {
  const items = new Map();
  let turn = 0;
  let seq = 0;
  const sweep = () => { for (const [id, p] of items) if (now() - p.at > PENDING_MS) items.delete(id); };
  return {
    /** The owner has said something: a new turn begins. */
    newTurn() { turn++; sweep(); },
    get turn() { return turn; },
    propose(tool, input, question) {
      sweep();
      const id = `a${++seq}`;
      items.set(id, { id, tool, input, question, turn, at: now() });
      return id;
    },
    /** The action, if the owner has spoken since it was proposed; else why not. */
    take(id) {
      sweep();
      const p = items.get(String(id || ''));
      if (!p) return { error: 'No action is waiting with that id (it may have expired). Ask the owner again.' };
      if (p.turn >= turn) return { error: 'The owner has not answered yet. Ask the question and wait for their reply; never confirm on their behalf.' };
      items.delete(p.id);
      return { action: p };
    },
    cancel(id) { return items.delete(String(id || '')); },
    list() { sweep(); return [...items.values()].map(({ id, tool, question }) => ({ id, tool, question })); },
  };
}

// ─── schema helpers ───
const str = (description, extra = {}) => ({ type: 'string', description, ...extra });
const num = (description) => ({ type: 'number', description });
const bool = (description) => ({ type: 'boolean', description });
const obj = (properties = {}, required = []) => ({ type: 'object', properties, required, additionalProperties: false });

/** One line for a message read back (D15): short ones whole, long ones summed up. */
export function readBack(text) {
  const t = String(text || '').replace(/\s+/g, ' ').trim();
  const words = t ? t.split(' ').length : 0;
  if (t.length <= 140) return `"${t}"`;
  return `"${t.slice(0, 110).replace(/\s+\S*$/, '')}…" (about ${words} words)`;
}

/**
 * @param deps  from webapp.js: chats(), sendMessage(id, text), createChat(project, opts),
 *              closeChat(id), parkChat(id), resumeChat(id), listProjects(), projectsDir(),
 *              registerProject(alias, path, name), kanban, notes, mindmap, chatSchedule,
 *              schedules (schedule-store), services(), voiceSettings, onSettingsChanged(),
 *              broadcast(kind), watch(id, title, project)
 * @param ctx   { pending: createPending(), locked: () => boolean }
 */
export function makeActionTools(deps, ctx) {
  const project = (alias) => {
    const a = String(alias || '').toLowerCase();
    const p = deps.listProjects().find(x => x.alias === a);
    if (!p) throw new Error(`No project called "${alias}". Use list_projects.`);
    return p;
  };
  const chat = (ref) => {
    const r = String(ref || '').trim().toLowerCase();
    const all = deps.chats();
    const hit = all.find(c => c.id === ref) || all.find(c => String(c.title || '').toLowerCase() === r)
      || all.filter(c => String(c.title || '').toLowerCase().includes(r))[0];
    if (!hit) throw new Error(`No open chat matches "${ref}". Use list_panels for the chats and their ids.`);
    return hit;
  };
  const chatName = (c) => `the "${c.title || 'untitled'}" chat in ${c.project}`;

  /** Each action: schema, an optional check (fails early, before asking), the yes rule, and what it does. */
  const defs = [
    {
      name: 'send_to_chat',
      description: 'Send a message into an open Claude Code chat, as the owner typing it. intent: "explicit" when the owner clearly asked you to send, tell, type or reply something; "inferred" when you decided to send it on your own reading (it is then read back to the owner first). Chats only: you can never type into a terminal.',
      input_schema: obj({ chat: str('The chat: its id or title (see list_panels)'), text: str('The message, exactly as it should be sent'), intent: str('explicit or inferred', { enum: ['explicit', 'inferred'] }) }, ['chat', 'text', 'intent']),
      check: (i) => { chat(i.chat); if (!String(i.text || '').trim()) throw new Error('Nothing to send'); },
      confirm: (i) => i.intent === 'explicit' ? null : `Send ${readBack(i.text)} to ${chatName(chat(i.chat))}?`,
      run: (i) => {
        const c = chat(i.chat); const r = deps.sendMessage(c.id, String(i.text));
        if (r && r.ok === false) return r;
        // What Cru sets going, it reports back on.
        deps.watch?.(c.id, c.title, c.project, { auto: true });
        return { ok: true, sent: true, chat: c.title, project: c.project, watching: true };
      },
    },
    {
      name: 'start_chat',
      description: 'Start a new Claude Code chat in a project, with the project\'s default model and mode unless the owner said otherwise, optionally with a first message (D15 rules apply to it, as send_to_chat).',
      input_schema: obj({ project: str('Project alias'), title: str('Optional title'), message: str('Optional first message'), intent: str('explicit or inferred, for the first message', { enum: ['explicit', 'inferred'] }), model: str('Optional model, only if the owner named one') }, ['project']),
      check: (i) => project(i.project),
      confirm: (i) => i.message && i.intent !== 'explicit' ? `Start a chat in ${project(i.project).alias} with ${readBack(i.message)}?` : null,
      run: async (i) => {
        const p = project(i.project);
        const r = await deps.createChat(p.alias, { title: i.title || '', model: i.model || '' });
        if (!r || !r.ok) return r || { ok: false, error: 'The chat could not be started' };
        if (i.message) { deps.sendMessage(r.id, String(i.message)); deps.watch?.(r.id, i.title || '', p.alias, { auto: true }); }
        return { ok: true, started: true, chat: r.id, project: p.alias, sent: !!i.message, watching: !!i.message };
      },
    },
    {
      name: 'close_chat',
      description: 'Close an open chat, as closing it in the app (its pane goes). Always asks the owner first.',
      input_schema: obj({ chat: str('The chat: its id or title') }, ['chat']),
      check: (i) => chat(i.chat),
      confirm: (i) => { const c = chat(i.chat); return `Close ${chatName(c)}? It is ${c.status || 'open'}.`; },
      run: async (i) => { const c = chat(i.chat); const r = await deps.closeChat(c.id); return r && r.ok === false ? r : { ok: true, closed: c.title }; },
    },
    {
      name: 'park_chat',
      description: 'Park (sleep) an open chat: it stops, and keeps its place with a Resume button. Always asks the owner first.',
      input_schema: obj({ chat: str('The chat: its id or title') }, ['chat']),
      check: (i) => chat(i.chat),
      confirm: (i) => `Park ${chatName(chat(i.chat))}?`,
      run: (i) => { const c = chat(i.chat); return deps.parkChat(c.id); },
    },
    {
      name: 'resume_chat',
      description: 'Resume a parked chat in place.',
      input_schema: obj({ chat: str('The parked chat\'s id') }, ['chat']),
      run: async (i) => deps.resumeChat(String(i.chat)),
    },
    {
      name: 'watch_chat',
      description: 'Report on a chat when it finishes its turn or waits on the owner: said aloud if a voice screen is open (or on the PC), else a phone notification. follow: true keeps reporting every turn until unwatch_chat; otherwise just the current turn. Chats you send to or start are watched for their turn already.',
      input_schema: obj({ chat: str('The chat: its id or title'), follow: { type: 'boolean', description: 'Keep reporting every turn, until told to stop' } }, ['chat']),
      check: (i) => chat(i.chat),
      run: (i) => { const c = chat(i.chat); deps.watch(c.id, c.title, c.project, { follow: !!i.follow }); return { ok: true, watching: c.title, follow: !!i.follow }; },
    },
    {
      name: 'unwatch_chat',
      description: 'Stop reporting on a chat.',
      input_schema: obj({ chat: str('The chat: its id or title') }, ['chat']),
      run: (i) => { const c = chat(i.chat); return { ok: true, stopped: deps.unwatch ? deps.unwatch(c.id) : false }; },
    },

    // ─── kanban (D13) ───
    {
      name: 'add_kanban_task',
      description: 'Add a card to a project\'s kanban board. New cards always go to Backlog (To do starts a factory run).',
      input_schema: obj({ project: str('Project alias'), title: str('Card title'), description: str('Optional description') }, ['project', 'title']),
      check: (i) => project(i.project),
      run: (i) => deps.kanban.addTask(project(i.project).alias, { title: String(i.title), description: String(i.description || ''), status: 'backlog' }),
    },
    {
      name: 'update_kanban_task',
      description: 'Change a card\'s title or description.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Card id'), title: str('New title'), description: str('New description') }, ['project', 'taskId']),
      check: (i) => project(i.project),
      run: (i) => deps.kanban.updateTask(project(i.project).alias, String(i.taskId), Object.fromEntries(Object.entries({ title: i.title, description: i.description }).filter(([, v]) => v !== undefined))),
    },
    {
      name: 'move_kanban_task',
      description: 'Move a card to another column: backlog, todo, in_progress or done. Moving to todo starts a factory run, so that asks the owner first.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Card id'), status: str('Column', { enum: STATUSES }) }, ['project', 'taskId', 'status']),
      check: (i) => { project(i.project); if (!STATUSES.includes(i.status)) throw new Error('Unknown column'); },
      confirm: (i) => i.status === 'todo' ? `Moving it to To do starts a factory run on it. Move it?` : null,
      run: (i) => deps.kanban.moveTask(project(i.project).alias, String(i.taskId), i.status),
    },
    {
      name: 'delete_kanban_task',
      description: 'Delete a card (it can be restored). Asks the owner first.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Card id') }, ['project', 'taskId']),
      check: (i) => { const t = deps.kanban.getTask(project(i.project).alias, String(i.taskId)); if (!t || t.ok === false) throw new Error('No such card'); },
      confirm: (i) => { const t = deps.kanban.getTask(project(i.project).alias, String(i.taskId)); return `Delete the card "${t?.task?.title || t?.title || i.taskId}"?`; },
      run: (i) => deps.kanban.deleteTask(project(i.project).alias, String(i.taskId)),
    },
    {
      name: 'add_kanban_todo',
      description: 'Add a todo to a card.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Card id'), text: str('The todo') }, ['project', 'taskId', 'text']),
      check: (i) => project(i.project),
      run: (i) => deps.kanban.addTodo(project(i.project).alias, String(i.taskId), String(i.text)),
    },
    {
      name: 'check_kanban_todo',
      description: 'Tick a card\'s todo done, or untick it.',
      input_schema: obj({ project: str('Project alias'), taskId: str('Card id'), todoId: str('Todo id'), done: bool('Done or not') }, ['project', 'taskId', 'todoId', 'done']),
      check: (i) => project(i.project),
      run: (i) => deps.kanban.updateTodo(project(i.project).alias, String(i.taskId), String(i.todoId), { done: !!i.done }),
    },

    // ─── notes (D13) ───
    {
      name: 'create_note',
      description: 'Create a note page in a project. Text becomes paragraphs (one per line).',
      input_schema: obj({ project: str('Project alias'), title: str('Page title'), text: str('The note\'s text') }, ['project', 'title']),
      check: (i) => project(i.project),
      run: (i) => deps.notes.createPage(project(i.project).alias, { title: String(i.title), blocks: paragraphs(i.text) }),
    },
    {
      name: 'append_to_note',
      description: 'Add text to the end of a note page.',
      input_schema: obj({ project: str('Project alias'), id: str('Page id (read_note / list_notes)'), text: str('Text to add') }, ['project', 'id', 'text']),
      check: (i) => { if (!deps.notes.getPage(project(i.project).alias, String(i.id))) throw new Error('No such page'); },
      run: (i) => {
        const a = project(i.project).alias;
        const page = deps.notes.getPage(a, String(i.id));
        return deps.notes.savePage(a, page.id, { blocks: [...(page.blocks || []), ...paragraphs(i.text)], baseVersion: page.version });
      },
    },
    {
      name: 'delete_note',
      description: 'Delete a note page (it can be restored). Asks the owner first.',
      input_schema: obj({ project: str('Project alias'), id: str('Page id') }, ['project', 'id']),
      check: (i) => { if (!deps.notes.getPage(project(i.project).alias, String(i.id))) throw new Error('No such page'); },
      confirm: (i) => `Delete the note "${deps.notes.getPage(project(i.project).alias, String(i.id))?.title || i.id}"?`,
      run: (i) => deps.notes.deletePage(project(i.project).alias, String(i.id)),
    },

    // ─── mindmap (D13) ───
    {
      name: 'add_mindmap_node',
      description: 'Add a node to the mindmap, under a parent node or at the top, optionally in a project.',
      input_schema: obj({ text: str('The node\'s text'), parentId: str('Optional parent node id'), project: str('Optional project alias') }, ['text']),
      run: (i) => deps.mindmap.addNode({ text: String(i.text), parentId: i.parentId || null, project: i.project ? project(i.project).alias : undefined }),
    },
    {
      name: 'update_mindmap_node',
      description: 'Change a mindmap node\'s text.',
      input_schema: obj({ id: str('Node id'), text: str('New text') }, ['id', 'text']),
      run: (i) => deps.mindmap.updateNode(String(i.id), { text: String(i.text) }),
    },
    {
      name: 'delete_mindmap_node',
      description: 'Delete a mindmap node and what is under it (it can be restored). Asks the owner first.',
      input_schema: obj({ id: str('Node id') }, ['id']),
      check: (i) => { if (!deps.mindmap.getNode(String(i.id))) throw new Error('No such node'); },
      confirm: (i) => `Delete the mindmap node "${deps.mindmap.getNode(String(i.id))?.text || i.id}" and what is under it?`,
      run: (i) => deps.mindmap.deleteNode(String(i.id)),
    },

    // ─── projects (D22) ───
    {
      name: 'create_project',
      description: 'Create a new project: a new folder in the projects folder, registered in Crundi. Asks the owner first.',
      input_schema: obj({ alias: str('Short name: lowercase letters, digits and dashes'), name: str('Optional display name') }, ['alias']),
      check: (i) => {
        if (!/^[a-z0-9][a-z0-9-]{0,40}$/.test(String(i.alias || ''))) throw new Error('The name must be lowercase letters, digits and dashes');
        if (deps.listProjects().some(p => p.alias === i.alias)) throw new Error(`There is already a project called ${i.alias}`);
        if (!deps.projectsDir()) throw new Error('No projects folder is set in Crundi\'s settings, so there is nowhere to create it');
      },
      confirm: (i) => `Create a new project "${i.name || i.alias}", a folder ${i.alias} in the projects folder?`,
      run: (i) => deps.registerProject(String(i.alias), join(deps.projectsDir(), String(i.alias)), i.name ? String(i.name) : undefined),
    },

    // ─── chat schedules: a message into a chat later (D45) ───
    {
      name: 'list_chat_schedules',
      description: 'The messages scheduled to be sent into chats in a project.',
      input_schema: obj({ project: str('Project alias') }, ['project']),
      read: true,
      run: (i) => deps.chatSchedule.list(project(i.project).alias, ''),
    },
    {
      name: 'add_chat_schedule',
      description: 'Schedule a message to be sent into a chat: at a time (ISO date-time, the owner\'s local time unless they said otherwise), or when the chat\'s current turn ends ("turn-end"), or when the usage limit resets ("limit-reset"). Asks the owner first.',
      input_schema: obj({ project: str('Project alias'), chat: str('Optional chat id or title; without one, a new chat'), text: str('The message'), trigger: str('time, turn-end or limit-reset', { enum: ['time', 'turn-end', 'limit-reset'] }), at: str('For time: the ISO date-time') }, ['project', 'text', 'trigger']),
      check: (i) => { project(i.project); if (i.chat) chat(i.chat); if (i.trigger === 'time' && !Date.parse(i.at)) throw new Error('A time is needed, as an ISO date-time'); },
      confirm: (i) => {
        const when = i.trigger === 'time' ? `at ${new Date(Date.parse(i.at)).toLocaleString('en-GB', { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}` : i.trigger === 'turn-end' ? 'when its current turn ends' : 'when the usage limit resets';
        return `Schedule ${readBack(i.text)} into ${i.chat ? chatName(chat(i.chat)) : `a new chat in ${i.project}`}, ${when}?`;
      },
      run: (i) => deps.chatSchedule.add({ project: project(i.project).alias, text: String(i.text), trigger: i.trigger === 'time' ? { type: 'time', at: i.at } : { type: i.trigger }, chatId: i.chat ? chat(i.chat).id : '' }),
    },
    {
      name: 'delete_chat_schedule',
      description: 'Cancel a scheduled chat message. Asks the owner first.',
      input_schema: obj({ id: str('Its id (list_chat_schedules)') }, ['id']),
      confirm: (i) => `Cancel the scheduled message ${i.id}?`,
      run: (i) => deps.chatSchedule.remove(String(i.id)),
    },

    // ─── scheduled tasks (D45): chat jobs and service start/stop only ───
    {
      name: 'add_schedule',
      description: 'Create a scheduled task in a project: a chat job (a prompt run in a new or resumed chat) or a service start/stop. when: once at date+time, or recurring at time on days (0=Sunday..6=Saturday; empty = every day). No shell commands. Asks the owner first.',
      input_schema: obj({
        project: str('Project alias'), name: str('A short name'),
        kind: str('chat or service', { enum: ['chat', 'service'] }),
        prompt: str('For chat: the prompt'), serviceKey: str('For service: the service key (list_services)'), op: str('For service: start, stop or toggle', { enum: ['start', 'stop', 'toggle'] }),
        mode: str('once or recurring', { enum: ['once', 'recurring'] }), time: str('HH:MM, 24-hour'), date: str('For once: YYYY-MM-DD'),
        days: { type: 'array', items: { type: 'integer', minimum: 0, maximum: 6 }, description: 'For recurring: days of the week' },
      }, ['project', 'name', 'kind', 'mode', 'time']),
      check: (i) => { project(i.project); if (i.kind === 'chat' && !String(i.prompt || '').trim()) throw new Error('A chat job needs a prompt'); if (i.kind === 'service' && !deps.services().some(s => s.key === i.serviceKey)) throw new Error('No such service'); },
      confirm: (i) => `Create the scheduled task "${i.name}": ${i.kind === 'chat' ? `run ${readBack(i.prompt)} in a chat` : `${i.op || 'start'} ${i.serviceKey}`}, ${i.mode === 'once' ? `once on ${i.date || 'today'} at ${i.time}` : `at ${i.time} ${i.days?.length ? 'on days ' + i.days.join(', ') : 'every day'}`}?`,
      run: (i) => deps.schedules.addSchedule(scheduleData(i, project(i.project).alias)),
    },
    {
      name: 'update_schedule',
      description: 'Change a scheduled task: its name, prompt, or when it runs. Asks the owner first.',
      input_schema: obj({ id: str('The schedule id (list_schedules)'), name: str('New name'), prompt: str('New prompt (chat jobs)'), mode: str('once or recurring', { enum: ['once', 'recurring'] }), time: str('HH:MM'), date: str('YYYY-MM-DD'), days: { type: 'array', items: { type: 'integer', minimum: 0, maximum: 6 } } }, ['id']),
      check: (i) => { const s = deps.schedules.getSchedule(String(i.id)); if (!s) throw new Error('No such schedule'); if (s.action?.kind === 'command') throw new Error('Shell-command schedules can only be changed in the app'); },
      confirm: (i) => `Change the scheduled task "${deps.schedules.getSchedule(String(i.id))?.name}"?`,
      run: (i) => {
        const s = deps.schedules.getSchedule(String(i.id));
        const when = { ...s.when, ...Object.fromEntries(Object.entries({ mode: i.mode, time: i.time, date: i.date, days: i.days }).filter(([, v]) => v !== undefined)) };
        const action = i.prompt !== undefined && s.action?.kind === 'chat' ? { ...s.action, prompt: String(i.prompt) } : s.action;
        return deps.schedules.updateSchedule(s.id, { ...s, name: i.name ?? s.name, when, action });
      },
    },
    {
      name: 'set_schedule_enabled',
      description: 'Turn a scheduled task on or off. Asks the owner first.',
      input_schema: obj({ id: str('The schedule id'), enabled: bool('On or off') }, ['id', 'enabled']),
      check: (i) => { if (!deps.schedules.getSchedule(String(i.id))) throw new Error('No such schedule'); },
      confirm: (i) => `Turn ${i.enabled ? 'on' : 'off'} the scheduled task "${deps.schedules.getSchedule(String(i.id))?.name}"?`,
      run: (i) => deps.schedules.setEnabled(String(i.id), !!i.enabled),
    },
    {
      name: 'delete_schedule',
      description: 'Delete a scheduled task. Asks the owner first.',
      input_schema: obj({ id: str('The schedule id') }, ['id']),
      check: (i) => { if (!deps.schedules.getSchedule(String(i.id))) throw new Error('No such schedule'); },
      confirm: (i) => `Delete the scheduled task "${deps.schedules.getSchedule(String(i.id))?.name}"?`,
      run: (i) => deps.schedules.deleteSchedule(String(i.id)),
    },

    // ─── the voice's own settings (D32: always asked) ───
    {
      name: 'set_voice_setting',
      description: `Change one of the voice assistant's own settings: ${VOICE_SETTINGS.join(', ')}. Always asks the owner first. Keys and every other setting are out of reach.`,
      input_schema: obj({ setting: str('Which', { enum: VOICE_SETTINGS }), value: str('The new value, as text (numbers as digits)') }, ['setting', 'value']),
      check: (i) => { if (!VOICE_SETTINGS.includes(i.setting)) throw new Error('That setting is not one voice may change'); },
      confirm: (i) => `Change ${i.setting} to ${i.value}?`,
      run: (i) => {
        const v = ['monthlyLimitUsd', 'deepgramLimitUsd', 'pauseMs', 'billingDay', 'deepgramBillingDay', 'speechRate'].includes(i.setting) ? Number(i.value) : String(i.value);
        const r = deps.voiceSettings.updateSettings({ [i.setting]: v });
        if (r.ok) deps.onSettingsChanged?.();
        return r;
      },
    },
    // ─── "Hey Crundi" in the background (Phase 6, D4–D7, D32) ───
    {
      name: 'set_background_listening',
      description: 'Turn background listening for "Hey Cru" on the phone on for a while (15, 30, 45, 60 or 120 minutes; 30 if the owner named no time; never unlimited), or off (minutes 0). Always asks the owner first. The phone does it when this answer reaches it.',
      input_schema: obj({ minutes: { type: 'integer', enum: [0, ...LISTEN_MINUTES], description: 'How long; 0 turns it off' } }, []),
      confirm: (i) => { const m = i.minutes === undefined ? 30 : i.minutes; return m === 0 ? 'Turn off background listening?' : `Background listening for ${m >= 60 ? (m / 60) + (m === 60 ? ' hour' : ' hours') : m + ' minutes'}?`; },
      run: (i) => {
        const m = i.minutes === undefined ? 30 : Number(i.minutes);
        if (m !== 0 && !LISTEN_MINUTES.includes(m)) return { ok: false, error: 'Choose 15, 30, 45, 60 or 120 minutes' };
        return { ok: true, phoneAction: { type: 'background-listening', minutes: m }, say: m ? `The phone will listen for "Hey Cru" for ${m} minutes.` : 'The phone will stop listening.' };
      },
    },
  ];

  const byName = new Map(defs.map(d => [d.name, d]));

  async function gated(def, input) {
    if (def.read) return def.run(input);
    if (ctx.locked()) return { done: false, needsUnlock: true, say: 'The phone is locked. Ask the owner to unlock it for this; nothing was changed.' };
    if (def.check) def.check(input);
    const q = def.confirm ? def.confirm(input) : null;
    if (q) {
      const id = ctx.pending.propose(def.name, input, q);
      return { done: false, needsConfirmation: true, id, ask: q, rule: 'Nothing has been done. Ask the owner this, in your own words, and stop. Only if the owner agrees in their next message, call confirm_action with this id; if they decline, call cancel_action.' };
    }
    const r = await def.run(input);
    if (r && r.ok !== false) deps.broadcast?.(def.name, input);
    return r;
  }

  const tools = defs.map(d => ({ name: d.name, description: d.description, input_schema: d.input_schema, run: (input) => gated(d, input || {}) }));
  tools.push({
    name: 'confirm_action',
    description: 'Carry out an action the owner has agreed to in their latest message. Only after asking and hearing yes; never on your own.',
    input_schema: obj({ id: str('The id the action gave') }, ['id']),
    run: async ({ id }) => {
      if (ctx.locked()) return { done: false, needsUnlock: true, say: 'The phone is locked. Ask the owner to unlock it; nothing was changed.' };
      const t = ctx.pending.take(id);
      if (t.error) return { done: false, error: t.error };
      const def = byName.get(t.action.tool);
      const r = await def.run(t.action.input);
      if (r && r.ok !== false) deps.broadcast?.(def.name, t.action.input);
      return r;
    },
  });
  tools.push({
    name: 'cancel_action',
    description: 'Drop an action the owner declined.',
    input_schema: obj({ id: str('The id the action gave') }, ['id']),
    run: ({ id }) => ({ ok: true, cancelled: ctx.pending.cancel(id) }),
  });
  // Closing the voice screen itself: only the window, nothing changes, so no yes is needed.
  tools.push({
    name: 'close_voice',
    description: 'Close this voice screen (the phone\'s voice screen or side-key overlay, or the voice dropdown on the web and PC) once your answer has been said. Use it when the owner says they are done: "close", "that\'s all", "goodbye", "go away", "thanks, that\'s it". Say a brief goodbye in your reply.',
    input_schema: obj({}),
    run: () => ({ ok: true, close: true, say: 'The voice screen closes after your reply.' }),
  });
  return tools;
}

function paragraphs(text) {
  return String(text || '').split(/\n+/).map(t => t.trim()).filter(Boolean).map(t => ({ type: 'p', text: t }));
}

function scheduleData(i, alias) {
  const action = i.kind === 'chat'
    ? { kind: 'chat', prompt: String(i.prompt || ''), session: 'new' }
    : { kind: 'service', serviceKey: String(i.serviceKey), op: i.op || 'start' };
  return {
    name: String(i.name).slice(0, 120), project: alias, enabled: true, action,
    when: { mode: i.mode === 'once' ? 'once' : 'recurring', time: String(i.time || '09:00'), date: String(i.date || ''), days: Array.isArray(i.days) ? i.days : [] },
  };
}
