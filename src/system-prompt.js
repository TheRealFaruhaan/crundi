/**
 * system-prompt.js — what every Crundi-spawned Claude is told about where it is.
 *
 * ─── Why this exists ───
 *
 * A Claude launched by Crundi has tools no ordinary terminal session has: it can
 * register a service, drive a browser, take a screenshot of a window, or message
 * the user on their phone. None of that is discoverable from the model's side
 * beyond a tool list, so without a word of framing it does the terminal-shaped
 * thing — leaves a server running in a foreground shell, or reports a finding
 * into a transcript nobody is reading.
 *
 * ─── The three layers ───
 *
 * One combined prompt, always in this order:
 *
 *   1. Crundi's own text (BASE) — what this machine is, which tools exist.
 *   2. The user's system-wide prompt, from Settings.
 *   3. The user's prompt for the open project.
 *
 * Later layers come after, so the user's words are the last thing read and win
 * on any disagreement. Scheduled jobs get layer 1 only: an unattended run at
 * 07:00 should behave the same next month regardless of what someone typed into
 * Settings for their own interactive use.
 *
 * ─── The cap, and why it is not decoration ───
 *
 * This is passed as a single argv entry. Linux caps ONE argument at
 * MAX_ARG_STRLEN (128 KB) and exec fails with E2BIG past it — which would look
 * like "chat won't start" with nothing pointing at the prompt. Each user layer
 * is capped well below that, and the total is capped again as a backstop.
 *
 * ─── The part that will surprise you ───
 *
 * Claude Code records the system prompt once per conversation
 * (--system-prompt-snapshot, default on) and replays that record on every later
 * request AND on resume, even when a later launch passes different text. So
 * editing a prompt here changes NEW conversations; existing ones keep the text
 * they started with until they are compacted. That is a deliberate upstream
 * prompt-cache optimisation, not something to work around — but it does mean
 * "I changed it and nothing happened" is expected, and the UI says so.
 */

import { readFileSync, existsSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';

/** Per-layer cap for user-supplied text. Generous for a prompt, far from E2BIG. */
export const MAX_LAYER = 8000;
/** Backstop for the assembled prompt, including our own base text. */
export const MAX_TOTAL = 24000;

/**
 * Crundi's own framing. Deliberately says nothing about developing Crundi
 * itself — most projects that run under it are somebody else's work.
 */
export const BASE = [
  'You are running inside Crundi, a self-hosted workbench that manages this machine’s projects, services, terminals and schedules.',
  'The person you are talking to reaches you through Crundi’s web UI or desktop app, frequently from a phone, so prefer replies that read well in a narrow column.',
  '',
  'If the "crundi" MCP server is connected, its tools (named mcp__crundi__*) act on this machine and are usually better than shelling out:',
  '',
  '- Reaching the user out of band: send_message_to_user, send_photo_to_user, send_file_to_user. These arrive even when nobody is reading the transcript, so use them for anything worth knowing before the user next looks.',
  '- Services and exposure: list_services, register_service, start_service, stop_service, restart_service, delete_service, get_service_logs, and enable_tunnel / disable_tunnel to publish one on a real certificate. Register a long-running process as a service rather than leaving it in a foreground shell.',
  '- Ports: list_forwards, add_forward, remove_forward.',
  '- Browser automation against a real browser: browser_open, browser_navigate, browser_click, browser_type, browser_fill, browser_select, browser_eval, browser_snapshot, browser_elements, browser_console, browser_network, browser_cookies, browser_pdf. Check what a page actually does instead of reasoning about it.',
  '- Screens and windows: list_windows, list_displays, capture_window, capture_display, send_window_screenshot_to_user, send_display_screenshot_to_user.',
  '- Terminals that outlive one tool call: spawn_terminal, terminal_input, terminal_output, terminal_wait, close_terminal, list_terminals.',
  '- Planning and notes: kanban_* for the board, mindmap_* for the mind map.',
  '- Schedules: schedule_list, schedule_get, schedule_add, schedule_update, schedule_set_enabled, schedule_delete.',
  '- Media: media_list, media_get, media_add_path, media_delete.',
  '- Secrets: secret_search, secret_get, secret_run. secret_run hands a secret to a command without printing it — prefer it over reading a secret and pasting the value.',
  '- Also: get_usage for the current Claude usage windows, syntax_check for a fast parse of a file you just wrote.',
  '',
  'Use them where they fit, and call them rather than describing what calling them would do.',
].join('\n');

/** Trim, normalise line endings, and cap one user-supplied layer. */
export function clampLayer(text, max = MAX_LAYER) {
  const s = String(text == null ? '' : text).replace(/\r\n?/g, '\n').trim();
  if (!s) return '';
  return s.length <= max ? s : s.slice(0, max).trimEnd();
}

/** The user's system-wide prompt, from .crundi-state.json. */
export function globalPrompt() {
  try {
    const f = join(config.dataDir, '.crundi-state.json');
    if (!existsSync(f)) return '';
    const st = JSON.parse(readFileSync(f, 'utf-8')) || {};
    return clampLayer(st.systemPrompt);
  } catch { return ''; }
}

/** The user's prompt for one project, from projects.json. */
export function projectPrompt(alias) {
  if (!alias) return '';
  try {
    const f = join(config.dataDir, 'projects.json');
    if (!existsSync(f)) return '';
    const map = JSON.parse(readFileSync(f, 'utf-8')) || {};
    const e = map[String(alias).toLowerCase()];
    // Legacy entries are a bare path string and carry no prompt.
    if (!e || typeof e === 'string') return '';
    return clampLayer(e.systemPrompt);
  } catch { return ''; }
}

/**
 * Assemble the prompt for one spawn.
 *
 * @param project    project alias, for layer 3
 * @param userLayers false for unattended runs (scheduled jobs): base only
 * @param extra      appended last; the scheduled-job briefing uses this
 * @returns the combined text, or '' if there is genuinely nothing to say
 */
export function composeSystemPrompt({ project = '', userLayers = true, extra = '' } = {}) {
  const parts = [BASE];
  if (userLayers) {
    const g = globalPrompt();
    if (g) parts.push(g);
    const p = projectPrompt(project);
    if (p) parts.push(p);
  }
  const x = clampLayer(extra, MAX_LAYER);
  if (x) parts.push(x);
  const joined = parts.filter(Boolean).join('\n\n');
  return joined.length <= MAX_TOTAL ? joined : joined.slice(0, MAX_TOTAL).trimEnd();
}

/**
 * The argv pair to append, or [] when there is nothing to add.
 * Kept separate so callers never have to remember the flag name.
 */
/**
 * What a collaborator's Claude is told about the situation it is in.
 *
 * This is ORIENTATION, not enforcement. Everything that actually matters is a
 * flag — --restricted confines the file tools and refuses commands it cannot
 * analyse, --tools is an exact allowlist, --settings carries deny rules the
 * session cannot edit. A model can be talked out of an instruction; it cannot
 * be talked out of a tool it was never given.
 *
 * So this says what is POSSIBLE and what to do instead, because a Claude that
 * understands the shape of the sandbox stops trying to tunnel out of it and
 * starts using the routes that exist.
 */
export function collaboratorPromptLayer({ name, project, branch, root } = {}) {
  return [
    `You are working with ${name || 'an outside collaborator'} on the "${project}" project.`,
    'They are a guest on this machine, not its owner, and this session is deliberately confined.',
    '',
    `Your working directory is a git worktree at ${root}, on branch ${branch}.`,
    'It is a real checkout and it is theirs to change. Everything outside it is off limits,',
    'and the tools enforce that rather than relying on you to remember it.',
    '',
    'What this session cannot do, and what to do instead:',
    '',
    '- No dev servers from the shell. A process started with `npm run dev &` or `nohup` is',
    '  invisible and dies with the turn. Register it as a Crundi service instead',
    '  (register_service, then start_service) — it survives, it is named, and it can be',
    '  stopped from the UI. Use get_service_logs to read its output.',
    '- No git beyond this branch. Committing here is fine and encouraged. push, checkout,',
    '  merge, rebase, reset and branch are denied. The collaborator has a Push button in',
    '  the UI for their own branch, and can ask the owner to merge it.',
    '- No sudo, systemctl, docker or package installs at the system level. These reach the',
    '  whole machine, which is shared.',
    '- No secrets. The credential tools are not available on this session.',
    '- No schedules, no terminals, no access to other projects on this machine.',
    '',
    'What this session is good at, and should be used for:',
    '',
    '- The browser tools drive a real browser: open a page, click, type, read the console',
    '  and the network log, take a snapshot. Use them to check that a change actually works',
    '  rather than reasoning about whether it should.',
    '- The services tools run and supervise long-lived processes properly.',
    '- The kanban and mindmap tools are scoped to this project and are a good place to',
    '  leave notes the owner will see.',
    '',
    'If something is genuinely blocked and the collaborator needs it, say so plainly and',
    'tell them they can ask the owner through the UI. Do not attempt to work around the',
    'restrictions, and do not speculate about what else is on this machine.',
  ].join('\n');
}

export function systemPromptArgs(opts) {
  const text = composeSystemPrompt(opts);
  return text ? ['--append-system-prompt', text] : [];
}
