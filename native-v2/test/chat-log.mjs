// The chat log's features (F75, F76, F65; PLAN.md Chat: log), end to end on
// a stub server (tools/stub-server.sh: its chats run tools/claude-stub.mjs,
// which thinks, calls tools, asks questions and permissions, proposes a
// plan and offers a suggestion on cue). Server-side things the stub cannot
// make (agents, the goal, a recap, media, a factory card) are played into
// the chat as the live link would bring them (the log's `chat-event`).
// Pictures go to native-v2/screens/chat-log/.
//
//   native-v2/tools/stub-server.sh 4884 &
//   PATH=/tmp/v2-shim:$PATH CRUNDI_TEST_SERVER=http://localhost:4884 CRUNDI_TEST_PORT=4889 \
//     NATIVE_BIN=$HOME/.cache/crundi-v2-chat-target/debug/crundi-v2 \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/chat-log.mjs
//   kill $(cat /tmp/crundi-devtest-4884/server.pid)

import * as t from './lib.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const T = await t.begin('chat-log');
const snap = (f) => T.snap('chat-log', f);
const log = async () => (await T.state()).log || {};
const untilLog = (what, test, ms) => T.until(what, (s) => test(s.log || {}), ms);
const ev = (e) => T.ui('chat-event', JSON.stringify(e));

// ─── F76: the resume picker is the launcher's chooser ───
// An earlier conversation of the project (a transcript where the stub's
// HOME keeps them), so the picker has something to offer.
const port = new URL(T.SERVER).port;
const enc = T.projDir.replace(/[^a-zA-Z0-9]/g, '-');
const tdir = join(`/tmp/crundi-devtest-${port}-home/.claude/projects`, enc);
mkdirSync(tdir, { recursive: true });
writeFileSync(join(tdir, '0b8a2c1e-1111-4222-8333-944455556666.jsonl'), JSON.stringify({ type: 'ai-title', aiTitle: 'Docs rewrite' }) + '\n' + JSON.stringify({ type: 'user', message: { role: 'user', content: 'rewrite the docs' } }) + '\n');
await T.ui('resume-open');
let s = await untilLog('the picker opens with the earlier conversation', (l) => l.resume?.picker?.rows?.length === 1);
t.check('its heading is the web\'s', s?.log?.resume?.picker?.heading === 'Open UI Mode', s?.log?.resume);
t.check('the row is the transcript\'s title', s?.log?.resume?.picker?.rows?.[0]?.title === 'Docs rewrite', s?.log?.resume);
await t.sleep(400);
await snap('resume-picker');
await T.ui('resume-new');
s = await untilLog('New session answers "new"', (l) => l.resume?.chosen?.mode === 'new' && !l.resume.picker);

// A chat to work in.
await T.ui('new-chat');
await T.until('a chat pane', (st) => st.chat && st.panes?.length > 0, 30000);
await T.until('the chat is idle', (st) => st.chatState === 'idle', 30000);

// ─── 2.14 / 2.15: thinking, tool calls grouped under their summary line ───
await T.ui('send', 'use tools please');
s = await untilLog('a thinking block streams, open', (l) => l.rows?.some((r) => r.kind === 'thinking' && r.status === 'streaming' && r.open), 30000);
await snap('thinking-streaming');
s = await untilLog('three calls fold under one summary line', (l) => l.rows?.filter((r) => r.kind === 'tool' && r.tool?.groupSummary).length === 3, 30000);
await T.until('the reply has come and the chat is idle', (st) => st.chatState === 'idle' && st.log?.rows?.some((r) => r.kind === 'assistant'), 30000);
await t.sleep(600);
let l = await log();
const tools = l.rows.filter((r) => r.kind === 'tool');
const head = tools.find((r) => r.tool.group === r.id);
t.check('the group\'s head is its first call', !!head && head.id === tools[0].id, tools);
t.check('the head counts the group\'s calls', head?.groupCount === 3, head);
t.check('folded: the other calls take no room', tools.filter((r) => r !== head).every((r) => r.h === 0), tools.map((r) => r.h));
await snap('tools-group-folded');
await T.ui('tool-toggle', 'g:' + head.tool.group);
await t.sleep(400);
l = await log();
t.check('open: every call is a card', l.rows.filter((r) => r.kind === 'tool').every((r) => r.h > 0 && r.groupOpen), l.rows.filter((r) => r.kind === 'tool'));
await T.ui('tool-toggle', tools[0].id);
await t.sleep(400);
await snap('tools-group-open-card-open');
await T.ui('tool-toggle', tools[0].id);
await T.ui('tool-toggle', 'g:' + head.tool.group);
s = await untilLog('the thinking block folds 5 s after it finished', (l2) => l2.rows?.some((r) => r.kind === 'thinking' && !r.open), 15000);
const th = (await log()).rows.find((r) => r.kind === 'thinking');
await T.ui('think-toggle', th.id);
await untilLog('the reader opens it again (the disclosure)', (l2) => l2.rows?.find((r) => r.id === th.id)?.open);
await t.sleep(300);
await snap('thinking-open');
await T.ui('think-toggle', th.id);

// ─── 2.16: the copy button on code and links, the link menu, a path's hover ───
// (The fold above moves the rows: read where the texts are once it is drawn.)
await t.sleep(800);
const lt = await T.ui('log-texts');
const reply = lt.texts.find((x) => x.text.includes('README.md') && x.text.includes('main.rs'));
t.check('the reply is on screen', !!reply, lt.texts.map((x) => x.text));
if (reply) {
  // The pointer over the path `src/main.rs`: the button after it, the hover colour.
  const at = (frac) => ({ x: reply.x + reply.w * frac, y: reply.y + reply.h / 2 });
  let hit = null;
  for (let f = 0.05; f < 1 && !hit; f += 0.02) {
    const p = at(f);
    await T.mouse('move', p.x, p.y);
    await t.sleep(60);
    const c = (await log()).copy;
    if (c && c.what === 'path') hit = { ...p, c };
  }
  t.check('over a path: the copy button says "Copy path"', !!hit, (await log()).copy);
  t.check('the path takes the hover colour', !!(await log()).hot);
  if (hit) {
    await snap('copy-button-path');
    await T.ui('copy-press');
    const lt2 = await T.ui('log-texts');
    t.check('Copy copies the path, "Copied"', lt2.clipboard === 'src/main.rs' && (await log()).toast === 'Copied', { clip: lt2.clipboard, toast: (await log()).toast });
    t.check('the button turns into a tick', (await log()).copy?.done === true);
  }
  // The web link: the right-click menu.
  let link = null;
  for (let f = 0.05; f < 1 && !link; f += 0.02) {
    const p = at(f);
    await T.mouse('move', p.x, p.y);
    await t.sleep(50);
    const c = (await log()).copy;
    if (c && c.what === 'link') link = p;
  }
  t.check('over a web link: "Copy link"', !!link);
  if (link) {
    await T.ui('log-context', String(link.x), String(link.y));
    l = await log();
    t.check('a right-click on it: Open in browser, Copy link', JSON.stringify(l.menu?.items?.map((i) => i.label)) === '["Open in browser","Copy link"]', l.menu);
    await t.sleep(300);
    await snap('link-menu');
    await T.ui('menu-pick', 'copy');
    const lt3 = await T.ui('log-texts');
    t.check('Copy link: the address, "Link copied"', lt3.clipboard === 'https://example.com/docs' && (await log()).toast === 'Link copied', { clip: lt3.clipboard });
  }
  await T.mouse('move', 5, 5);
}

// ─── 2.13: a question card; a permission card ───
await T.ui('send', 'ask me something');
s = await untilLog('a question card, open', (l2) => l2.question?.open, 30000);
await t.sleep(300);
await snap('question-open');
await T.ui('q-pick', '0', '1');
s = await untilLog('picked: Submit is possible', (l2) => l2.question?.can);
await T.ui('q-submit');
s = await untilLog('answered: its tag', (l2) => l2.question?.status === 'allowed' && l2.question?.tags?.includes('SQLite'), 10000);
await T.until('Claude carries on', (st) => st.entries?.some((e) => e.text === 'Thanks, carrying on.') && st.chatState === 'idle', 30000);
await snap('question-answered');
await T.ui('send', 'need permission for this');
s = await untilLog('a permission card, pending', (l2) => l2.rows?.some((r) => r.kind === 'permission' && r.status === 'pending'), 30000);
await t.sleep(300);
await snap('permission-pending');
await T.ui('perm-allow');
await untilLog('allowed', (l2) => l2.rows?.some((r) => r.kind === 'permission' && r.status !== 'pending'), 10000);
await T.until('it carries on', (st) => st.chatState === 'idle', 30000);

// ─── 2.19: the plan; the goal bar ───
await T.ui('send', 'plan please');
s = await untilLog('a plan, proposed', (l2) => l2.goal?.plan === 'proposed', 30000);
await T.ui('plan-toggle');
await untilLog('the plan panel opens', (l2) => l2.goal?.planOpen);
await t.sleep(300);
await snap('plan-panel');
await T.ui('plan-toggle');
await T.ui('perm-allow');
await T.until('the plan was answered', (st) => st.chatState === 'idle', 30000);
await ev({ type: 'goal', goal: { condition: 'every test passes', verdicts: 1, looping: false } });
await ev({ type: 'entry', entry: { id: 'gv1', kind: 'goal-verdict', n: 1, text: 'Two tests still fail.' } });
s = await untilLog('the goal bar: its count and stopped line', (l2) => l2.goal?.on && l2.goal.count === '1 check' && l2.goal.stopped);
await t.sleep(300);
await snap('goal-bar-verdict');
await ev({ type: 'goal', goal: null });

// ─── 2.17: the suggestion, the recap ───
await T.ui('send', 'suggest next please');
s = await untilLog('a suggestion, in the dock', (l2) => l2.suggestion === 'Tell me more about it', 30000);
await ev({ type: 'recap', recap: { text: 'You asked for a plan; it was approved.', why: 'idle' } });
s = await untilLog('the recap, in the dock', (l2) => l2.recap?.why === 'idle' && l2.agentDock?.dockH > 0);
await t.sleep(300);
await snap('dock-recap-suggestion');
await T.ui('recap-dismiss');
await T.ui('sug-dismiss');
await untilLog('both gone, the dock with them', (l2) => !l2.recap && !l2.suggestion && l2.agentDock?.dockH === 0);

// ─── 2.18: an agent's row, the dock's badge, its list and panel ───
await ev({ type: 'entry', entry: { id: 'ag1', kind: 'tool', name: 'Task', toolUseId: 'tu-ag1', input: { description: 'Count the lines', subagent_type: 'general-purpose' }, status: 'running' } });
await ev({ type: 'agent', agent: { toolUseId: 'tu-ag1', kind: 'agent', subagentType: 'general-purpose', description: 'Count the lines', status: 'running', prompt: 'Count the lines in src.' } });
await ev({ type: 'agent-entry', toolUseId: 'tu-ag1', entry: { id: 'am1', kind: 'assistant-text', text: 'Counting **now**.' } });
s = await untilLog('the call is its agent\'s row', (l2) => l2.rows?.find((r) => r.id === 'ag1')?.agentRow);
t.check('the dock\'s agents badge', s?.log?.agentDock?.agents === 1 && s.log.agentDock.agentsRunning, s?.log?.agentDock);
await t.sleep(300);
await snap('agent-row-badge');
await T.ui('agent-badge', 'agents');
await untilLog('the agents list', (l2) => l2.agentDock?.sheet === 'agents' && l2.agentDock.rows.length === 1);
await snap('agent-list');
await T.ui('agent-open', 'tu-ag1', 'list');
await untilLog('the agent\'s panel, its transcript', (l2) => l2.agentDock?.sheet === 'panel' && l2.agentDock.entries === 1);
await t.sleep(300);
await snap('agent-panel');
await T.ui('agent-close');

// ─── 2.21: a side question ───
await T.ui('aside-ask', '/btw what are you working on?');
s = await untilLog('the side question is answered, on the side', (l2) => l2.asides?.[0]?.state === 'done' && l2.asides[0].text.includes('design.md'), 30000);
await t.sleep(300);
await snap('side-question');
await T.ui('aside-close', String(s?.log?.asides?.[0]?.id || 1));
await untilLog('× closes it', (l2) => l2.asides?.length === 0);

// ─── 2.16m: media under a call; 2.20: a factory's result card ───
await ev({ type: 'entry', entry: { id: 'med1', kind: 'tool', name: 'mcp__crundi__show_file', toolUseId: 'tu-m1', input: {}, status: 'done', result: JSON.stringify({ items: [{ kind: 'file', id: '0123456789abcdef', filename: 'export.csv', size: 12, caption: 'The export' }, { kind: 'image', name: '00112233445566aa.png', w: 1200, h: 700 }] }) } });
s = await untilLog('the call shows its media: a file card, a picture\'s box', (l2) => l2.rows?.find((r) => r.id === 'med1')?.media?.items?.length === 2);
const media = s?.log?.rows?.find((r) => r.id === 'med1')?.media;
t.check('the file\'s line', media?.items?.[0]?.line === '12 B · The export', media);
await untilLog('a picture no longer stored: gone', (l2) => l2.media?.pics?.['00112233445566aa.png']?.state === 'gone', 10000);
await T.ui('log-jump');
await t.sleep(400);
await snap('media-file-gone-picture');
await ev({ type: 'entry', entry: { id: 'fr1', kind: 'factory-result', runId: 'run1', state: 'ask', factory: 'Docs', branch: 'factory/docs', summary: '## Done\n\nThe **site** is rewritten.', files: [{ path: 'docs/a.md', add: 10, del: 2 }], add: 10, del: 2 } });
s = await untilLog('a factory\'s result card', (l2) => l2.fr?.cards?.length === 1);
await T.ui('fr-toggle', 'fr1', 'files');
await untilLog('its files open', (l2) => l2.fr?.cards?.[0]?.filesOpen);
await T.ui('log-jump');
await t.sleep(400);
await snap('factory-card');

// ─── "Go to the newest message", the streaming caret ───
await T.ui('send', 'count to three hundred please');
await untilLog('a long reply streaming', (l2) => l2.rows?.some((r) => r.kind === 'assistant' && r.status === 'streaming'), 30000);
await t.sleep(800);
await snap('streaming-caret');
await t.sleep(500);
await snap('streaming-caret-2');
await T.ui('log-scroll', '-4000');
s = await untilLog('scrolled up: "Go to the newest message" shows', (l2) => l2.jump?.shown && !l2.following, 10000);
await T.until('the reply ends', (st) => st.chatState === 'idle', 120000);
// Sent from elsewhere (another client), so the log stays scrolled up: a
// message sent from this box goes to the end, as the web's does.
await T.api.http('POST', `/api/ui-sessions/${(await T.state()).chat}/send`, { text: 'one more' });
await untilLog('something arrived below: the button\'s dot', (l2) => l2.jump?.new, 30000);
await snap('jump-button-new');
await T.ui('log-jump');
await untilLog('back at the end, following', (l2) => l2.following && !l2.jump?.shown, 10000);
await T.until('idle', (st) => st.chatState === 'idle', 60000);

// ─── 2.23: a parked pane ───
const chat = (await T.state()).chat;
const pk = await T.api.http('POST', `/api/panes/${chat}/park`, {});
t.check('the server parks the chat', pk.body?.ok, pk.body);
s = await untilLog('the parked card, the composer hidden', (l2) => l2.parked?.why && l2.parked.hidesComposer, 20000);
await t.sleep(500);
await snap('parked-card');
await T.ui('peek', 'on');
await untilLog('Peek: the conversation and its composer', (l2) => l2.parked?.peek && !l2.parked.hidesComposer);
await snap('parked-peek');
await T.ui('peek', 'off');
await T.ui('park-resume', 'resume');
await untilLog('Resume: the chat is live again', (l2) => l2.parked === null || l2.parked === undefined, 30000);

// ─── Idle: nothing animates (the caret's blink stops with the stream) ───
// (Resumed: its history comes again, the head and the composer settle.)
await T.until('the resumed chat is idle', (st) => st.chatState === 'idle', 30000);
await t.sleep(5000);
const idleFrom = T.frames.mark();
const idleR0 = (await T.state()).renders || {};
await t.sleep(4000);
const idle = T.frames.since(idleFrom);
const idleR1 = (await T.state()).renders || {};
t.check('idle: no frames for 4 s', idle.frames === 0, { idle, drew: Object.fromEntries(Object.keys(idleR1).filter((k) => idleR1[k] !== (idleR0[k] || 0)).map((k) => [k, idleR1[k] - (idleR0[k] || 0)])) });
await T.end();
