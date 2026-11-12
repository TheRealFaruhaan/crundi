/**
 * voice/kid-workshop.js — the games workshop: on a night after kid mode was
 * used, while nobody is using Crundi, the owner's own Claude Code (claude -p,
 * JSON mode, as agent-cli.js runs it; never the Agent SDK) improves one or two
 * games in the library and makes one new learning game.
 *
 * When (shouldRun): the setting is on (Settings › Kid mode, "Improve games
 * overnight", default on), it is inside the window (default 01:00-05:00,
 * local time), kid mode is off, nothing has been used for 30 minutes, it has
 * not run yet tonight, and kid mode WAS used since the last run and within
 * the 24 hours before the window opened. Never before kid mode is first used.
 *
 * How: one run, at most 20 minutes and 60 turns, in the library folder, with
 * Read/Write/Edit/Glob/Grep only. The play log and the child's levels are
 * restored afterwards whatever it did; every changed or new game is checked
 * (kid-games.js afterWorkshop) before it is live.
 */

import { spawn as nodeSpawn } from 'node:child_process';
import { existsSync, readFileSync, mkdirSync, copyFileSync, readdirSync, statSync, appendFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writePrivateFile } from '../private-fs.js';
import { BUILTIN_DIR } from './kid-games.js';

const REPO = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const RUN_MS = 20 * 60 * 1000;
export const IDLE_MS = 30 * 60 * 1000;
const DAY = 24 * 3600 * 1000;

const minutes = (hhmm) => { const m = /^(\d{1,2}):(\d{2})$/.exec(String(hhmm || '')); return m ? Math.min(23, Number(m[1])) * 60 + Math.min(59, Number(m[2])) : null; };
const pad = (n) => String(n).padStart(2, '0');
const localDay = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** Is [now] inside from-to (local time; a window may cross midnight)? */
export function inWindow(now, from = '01:00', to = '05:00') {
  const f = minutes(from), t = minutes(to);
  if (f == null || t == null || f === t) return false;
  const d = new Date(now), m = d.getHours() * 60 + d.getMinutes();
  return f < t ? m >= f && m < t : m >= f || m < t;
}

/** When tonight's window opened (ms), for [now] inside it. */
export function windowStart(now, from = '01:00') {
  const f = minutes(from) ?? 60;
  const d = new Date(now);
  const s = new Date(d.getFullYear(), d.getMonth(), d.getDate(), Math.floor(f / 60), f % 60, 0, 0);
  if (s.getTime() > now) s.setDate(s.getDate() - 1);
  return s.getTime();
}

/**
 * Should the workshop start now? Pure: everything comes in.
 * @returns {{run:boolean, why:string, night?:string}}
 */
export function shouldRun({ now = Date.now(), enabled = true, from = '01:00', to = '05:00', kidOn = false, lastActivity = 0, lastRunNight = '', lastRunAt = 0, lastKidUse = 0, running = false } = {}) {
  if (!enabled) return { run: false, why: 'switched off' };
  if (running) return { run: false, why: 'already running' };
  if (!inWindow(now, from, to)) return { run: false, why: 'outside the window' };
  const start = windowStart(now, from);
  const night = localDay(new Date(start));
  if (lastRunNight === night) return { run: false, why: 'already ran tonight', night };
  if (kidOn) return { run: false, why: 'kid mode is on', night };
  if (now - lastActivity < IDLE_MS) return { run: false, why: 'Crundi is in use', night };
  if (!lastKidUse) return { run: false, why: 'kid mode has never been used', night };
  if (lastKidUse <= lastRunAt || lastKidUse < start - DAY) return { run: false, why: 'kid mode was not used since the last run', night };
  return { run: true, why: 'kid mode was used', night };
}

/** What the run is asked to do. */
export function workshopPrompt({ hasSkill = false } = {}) {
  return `You are the games workshop for a young child's learning games, working alone overnight in this folder (the kid games library). Nobody will answer questions: decide, do the work, and finish.

Read first:
- _rules/README.md: the library's technical rules (file layout, meta.json, the kit, the message format). They are strict.
${hasSkill ? '- _rules/baby-games/SKILL.md (and its reference/ folder): how to design games for babies and young children by age band. Follow it.\n' : ''}- _rules/kit.js: the helpers every game gets (window.kid). Use them rather than writing your own.
- plays.jsonl: what the child played lately (time, finished or not, taps, correct and wrong answers, what was wrong).
- skills.json: the child's level per skill and what they are learning (items with low mastery) or have mastered.
- the games: each folder <id>/ has index.html and meta.json. _rules/trivia.base.json is the trivia bank; trivia.extra.json is where you may add to it.

Then do this:
1. Improve one or two existing games, aimed at what the child plays most and where they struggle: fix anything clunky, add variety or a gentler step where they get things wrong, more challenge where they find it easy, and use kid.init (focus, avoid, levels) so the game fits the child. Keep each game working as before for start_game's options.
2. Create ONE new basic learning game (letters, phonics, matching, counting, shapes, colours, or the world: animals, the body, nature, everyday things) aimed at what the child is learning now: a new folder <new-id>/ (lower-case letters, digits, dashes) with index.html and meta.json (title, skills from the taxonomy in the README, ageRange, options, version 1, createdBy "workshop").
3. Optionally add fresh instant-cheer lines to reflex.extra.json ({"events": {"correct": {"lines": [...]}, ...}}, the slots and rules in the README; short, warm, never shaming), so the child does not hear the same few.
4. Optionally add up to 30 new questions or fun facts to trivia.extra.json ({"items": [...]}, same fields as the base bank; choices must be pictures the kit draws; facts true, simple and kid-safe).
5. Bump "version" in meta.json for every game you change. Do not touch index.prev.html files, plays.jsonl, skills.json or progress.jsonl.
6. Add a short dated entry at the top of CHANGES.md: what you changed and why, in plain words for a parent.

Rules for every game (a game that breaks them is refused and never shown):
- One self-contained HTML fragment, at most 200 KB. No network of any kind: no URLs (except the SVG namespace), no fetch, XMLHttpRequest, WebSocket, import, <script src>, <link>, <iframe>, url() other than data:, no eval, no storage. No external assets; art is inline SVG or kid.pic / kid.shape. Never emoji.
- Big tap targets (at least 96 px for the youngest), bright colours on a dark background, very few words; everything important is also said aloud with kid.say.
- Always celebrate success (kid.correct), never shame (kid.wrong says try again kindly), give a hint before the answer (kid.hint), and never leave the child stuck. No scores, stars, timers or streaks on screen.
- Kid-safe content only. Report results with kid.ask(skill, item) before each question so the first answer is measured, and kid.done(summary) at the end.

When you are done, reply with two or three plain sentences saying what you changed.`;
}

/**
 * @param {object} o
 * @param o.lib            createGameLibrary()
 * @param o.dir            the library folder (cwd of the run)
 * @param o.settings()     { enabled, from, to }
 * @param o.kidOn()        is kid mode on
 * @param o.lastActivity() ms of the last use of Crundi (voice, chats)
 * @param o.claudeBin()    the claude executable, or null
 * @param o.render?(id)    check a game in the sandbox (headless browser), or none
 * @param o.onDone?(result)
 */
export function createWorkshop({ lib, dir, settings, kidOn, lastActivity, claudeBin, render = null, onDone = () => {}, spawn = nodeSpawn, now = () => Date.now(), runMs = RUN_MS, log = (m) => console.log(`[kid-workshop] ${m}`) }) {
  const stateFile = join(dir, 'workshop.json');
  let st = { lastRunNight: '', lastRunAt: 0, lastKidUse: 0, runs: [] };
  try { if (existsSync(stateFile)) st = { ...st, ...JSON.parse(readFileSync(stateFile, 'utf-8')) }; } catch { /* afresh */ }
  const save = () => { try { writePrivateFile(stateFile, JSON.stringify(st, null, 1)); } catch { /* next time */ } };
  let running = null;

  /** Kid mode was on, or a game was played: the next night may run. */
  function markKidUse(at = now()) { if (at - (st.lastKidUse || 0) > 60000) { st.lastKidUse = at; save(); } }

  function decide() {
    const s = settings();
    return shouldRun({ now: now(), enabled: s.enabled !== false, from: s.from, to: s.to, kidOn: !!kidOn(), lastActivity: lastActivity(), lastRunNight: st.lastRunNight, lastRunAt: st.lastRunAt, lastKidUse: st.lastKidUse, running: !!running });
  }

  /** What the run may read, put in _rules/ (the README, the kit, the bank, the baby-games skill). */
  function prepareRules() {
    const r = join(dir, '_rules');
    mkdirSync(r, { recursive: true, mode: 0o700 });
    copyFileSync(join(BUILTIN_DIR, 'README.md'), join(r, 'README.md'));
    copyFileSync(join(BUILTIN_DIR, 'kit.js'), join(r, 'kit.js'));
    copyFileSync(join(BUILTIN_DIR, 'trivia.json'), join(r, 'trivia.base.json'));
    const skill = join(REPO, 'skills', 'baby-games');
    if (!existsSync(join(skill, 'SKILL.md'))) return false;
    const copy = (from, to) => {
      mkdirSync(to, { recursive: true, mode: 0o700 });
      for (const e of readdirSync(from)) { const f = join(from, e); if (statSync(f).isDirectory()) copy(f, join(to, e)); else copyFileSync(f, join(to, e)); }
    };
    copy(skill, join(r, 'baby-games'));
    return true;
  }

  /** Start the run (the caller checked decide()). Resolves with the result. */
  async function run({ force = false } = {}) {
    if (running) return { ok: false, error: 'already running' };
    const d = decide();
    if (!force && !d.run) return { ok: false, skipped: d.why };
    const bin = claudeBin();
    if (!bin) return { ok: false, error: 'Claude Code (the claude command) is not installed' };
    const t0 = now();
    st.lastRunNight = d.night || st.lastRunNight; st.lastRunAt = t0; save();
    // What the run must never change: restored afterwards whatever it did.
    const keep = {};
    for (const f of ['plays.jsonl', 'skills.json', 'progress.jsonl', 'workshop.json']) { try { keep[f] = readFileSync(join(dir, f)); } catch { /* none */ } }
    const snap = lib.snapshot();
    const hasSkill = prepareRules();
    log('starting');
    running = runClaude(workshopPrompt({ hasSkill }), { ms: runMs, turns: 60 });
    const r = await running;
    running = null;
    for (const [f, buf] of Object.entries(keep)) { try { writePrivateFile(join(dir, f), buf); } catch { /* best effort */ } }
    let checked = { updated: [], created: [], failed: [] };
    try { checked = await lib.afterWorkshop(snap, { render }); } catch (e) { r.error = (r.error ? r.error + '; ' : '') + `checks: ${e.message}`; }
    const result = { at: new Date(t0).toISOString(), ms: now() - t0, ok: r.ok, summary: r.summary, error: r.error || '', costUsd: r.costUsd, ...checked };
    st = { ...st, ...readState(), lastRunNight: st.lastRunNight, lastRunAt: t0, runs: [result, ...(st.runs || [])].slice(0, 10) };
    save();
    try { appendFileSync(join(dir, 'workshop-log.jsonl'), JSON.stringify(result) + '\n', { mode: 0o600 }); } catch { /* best effort */ }
    log(`done: ${result.updated.length} updated, ${result.created.length} new, ${result.failed.length} refused${result.error ? ` (${result.error})` : ''}`);
    try { onDone(result); } catch { /* best effort */ }
    return result;
  }
  const readState = () => { try { return JSON.parse(readFileSync(stateFile, 'utf-8')); } catch { return {}; } };

  /**
   * One run of Claude Code in the library folder (stream-json, so its steps
   * can be followed): Read/Write/Edit/Glob/Grep only, none of the owner's
   * settings, MCP or hooks, the subscription paying. Stopped at [ms].
   */
  function runClaude(prompt, { ms, turns = 60, onStep = () => {} }) {
    const bin = claudeBin();
    const tools = 'Read,Write,Edit,Glob,Grep';
    const args = ['-p', prompt, '--output-format', 'stream-json', '--verbose', '--tools', tools, '--allowedTools', tools,
      '--permission-mode', 'acceptEdits', '--setting-sources', '', '--strict-mcp-config', '--disable-slash-commands', '--max-turns', String(turns)];
    const env = { ...process.env };
    delete env.ANTHROPIC_API_KEY;
    delete env.CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR;
    return new Promise((resolve) => {
      let buf = '', err = '', done = false, result = null;
      let p;
      try { p = spawn(bin, args, { cwd: dir, stdio: ['ignore', 'pipe', 'pipe'], env }); } catch (e) { resolve({ ok: false, error: e.message }); return; }
      const timer = setTimeout(() => { if (!done) { log('time is up: stopping it'); try { p.kill(); } catch { /* gone */ } } }, ms);
      timer.unref?.();
      p.stdout.on('data', (b) => {
        buf += b.toString();
        let i;
        while ((i = buf.indexOf('\n')) >= 0) {
          const l = buf.slice(0, i).trim(); buf = buf.slice(i + 1);
          if (!l) continue;
          let m; try { m = JSON.parse(l); } catch { continue; }
          if (m.type === 'result') result = m;
          else if (m.type === 'assistant' && Array.isArray(m.message?.content)) {
            for (const c of m.message.content) if (c.type === 'tool_use') { try { onStep({ tool: c.name, file: String((c.input && (c.input.file_path || c.input.path)) || '') }); } catch { /* the caller's */ } }
          }
        }
      });
      p.stderr.on('data', (b) => { err = (err + b.toString()).slice(-4000); });
      p.on('error', (e) => { if (done) return; done = true; clearTimeout(timer); resolve({ ok: false, error: e.message }); });
      p.on('exit', (code) => {
        if (done) return; done = true; clearTimeout(timer);
        resolve({ ok: code === 0 && !!result && !result.is_error, code, summary: String((result && result.result) || '').slice(0, 1500), costUsd: result && result.total_cost_usd, error: code === 0 ? '' : (err.trim().split('\n').pop() || `stopped (${code})`) });
      });
    });
  }

  /**
   * A game the child asked for, made now (kid mode): one simple game from the
   * skill's template, at most [ms] (3 minutes), checked like the workshop's,
   * then live. onStep(fraction, label) follows the run for the waiting scene.
   * @returns {Promise<{ok:boolean, id?:string, title?:string, error?:string}>}
   */
  async function buildLive({ idea, skill = '', age = '', onStep = () => {}, ms = 3 * 60 * 1000 } = {}) {
    if (running || building) return { ok: false, error: 'A game is already being made' };
    if (!claudeBin()) return { ok: false, error: 'Claude Code is not installed here' };
    const slug = String(idea || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 24) || 'game';
    const id = `live-${slug}-${Date.now().toString(36).slice(-4)}`;
    const keep = {};
    for (const f of ['plays.jsonl', 'skills.json', 'progress.jsonl', 'workshop.json']) { try { keep[f] = readFileSync(join(dir, f)); } catch { /* none */ } }
    const snap = lib.snapshot();
    const hasSkill = prepareRules();
    const prompt = `Make ONE simple learning game for a young child${age ? ` aged ${age}` : ''}, right now, about: ${String(idea).slice(0, 120)}${skill ? ` (skill: ${skill})` : ''}. A child is waiting, so be quick: at most a few files written, no long exploring.
Read _rules/README.md${hasSkill ? ' and _rules/baby-games/SKILL.md, then start from _rules/baby-games/reference/game-template.html' : ''}. Create the folder ${id}/ with index.html (one self-contained fragment, the rules in the README: no network, inline SVG art, big tap targets, the kit's kid.* helpers, kid.ask before each question, kid.done at the end, kid-safe) and meta.json (title, skills from the README's taxonomy, ageRange, options). Change nothing else. Reply with the game's title.`;
    building = true;
    let n = 0;
    try {
      const r = await runClaude(prompt, { ms, turns: 25, onStep: (st) => { n++; onStep(Math.min(0.9, 0.1 + n * 0.12), st.tool === 'Write' || st.tool === 'Edit' ? 'Drawing the pictures...' : 'Getting ideas...'); } });
      for (const [f, buf] of Object.entries(keep)) { try { writePrivateFile(join(dir, f), buf); } catch { /* best effort */ } }
      const checked = await lib.afterWorkshop(snap, { render });
      log(`live game "${idea}": ${checked.created.includes(id) ? 'ready' : 'refused'}${r.error ? ` (${r.error})` : ''}`);
      if (!checked.created.includes(id)) {
        const f = checked.failed.find((x) => x.id === id);
        return { ok: false, error: f ? `checks: ${f.problems[0]}` : (r.error || 'no game was made') };
      }
      const g = lib.get(id);
      return { ok: true, id, title: (g && g.title) || idea };
    } finally { building = false; }
  }
  let building = false;

  /** Called every few minutes. */
  function tick() { const d = decide(); if (d.run) run().catch((e) => log(`failed: ${e.message}`)); return d; }

  return { tick, run, decide, markKidUse, buildLive, get building() { return building; }, get state() { return { ...st, running: !!running || building }; } };
}
