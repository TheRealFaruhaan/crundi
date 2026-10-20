/**
 * factory-prompts.js — what each person in a factory is told. Pure.
 *
 * Two layers, on purpose:
 *   - the system prompt (systemPromptFor): who they are, the persona, and the
 *     standing rules. It survives compaction, and an employee cannot mistake
 *     it for the request it was given.
 *   - one message per step (stepMessage): the task, what came before, notes,
 *     and exactly what the handoff must contain, as a tree of the questions
 *     that apply depending on the answers.
 *
 * Every factory message starts with "[Factory]" so the Manager, who also
 * talks to the person in the same chat, can tell the two apart.
 */

import { MANAGER_ID } from './factory-model.js';

const TOOL = 'factory_handoff (mcp__crundi__factory_handoff)';

/**
 * @param {object} o
 * @param {object} o.factory        the run's snapshot
 * @param {object} o.employee       roster entry
 * @param {string} o.project
 * @param {object} o.paths          { worktree, branch, parentBranch, notesFile, sharedDir }
 * @param {string} [o.managerMode]  'normal' | 'bypass' (the run's current setting)
 */
export function systemPromptFor({ factory, employee, project, paths, managerMode = 'normal' }) {
  const isManager = employee.id === MANAGER_ID;
  const persona = (employee.persona || '').trim();
  const lines = [];
  if (isManager) {
    lines.push(
      `[Crundi Factory "${factory.name}": you are the Manager]`,
      `You lead a team of AI employees working on one task in the project "${project}". The person who started this run reads this chat and may talk to you here at any time.`,
    );
  } else {
    lines.push(
      `[Crundi Factory "${factory.name}": you are ${employee.name}]`,
      `You are one employee in an automated team (a "factory") working on one task in the project "${project}".`,
    );
  }
  if (persona) lines.push('', 'Your persona, written by the person who set up this factory. Read it and act as it describes:', '---', persona, '---');

  lines.push('', 'How this works:');
  if (isManager) {
    lines.push(
      `- You route the work by calling ${TOOL} with your answers to the flow's questions. Crundi does the routing from your answers; you never hand work to someone directly.`,
      '- Messages starting with "[Factory]" come from Crundi, not from the person: they tell you what your team did and what you must decide. Answer them with a handoff.',
      '- Messages without that prefix are the person talking to you. Answer them normally. If you need them to decide something, ask in this chat and end your turn; the run waits for them.',
      `- Your team works in ${paths.worktree} on the branch ${paths.branch}, cut from ${paths.parentBranch}. Read it to judge their work. Do not do their work yourself unless the person asks you to.`,
      `- When the whole task is done and you are satisfied with it, call ${TOOL} with finish: true and a summary of what was done.`,
      '- Never commit, merge, push or reset with git yourself. Crundi commits each step and merges the result when the person agrees.',
      '',
      'Permissions:',
      '- Your team edits files in the run\'s worktrees freely. Anything else they want to do that needs permission (shell commands, web access, reading elsewhere) comes to you as a "[Factory] Permission request" message. Answer with factory_permission (mcp__crundi__factory_permission). Your decision is final: the person is not asked.',
      '- Allow what the task genuinely needs; deny what is risky, destructive or off-task, and say why so they can find another way. For a safe command they will need again (tests, builds, linters), add a rule such as "Bash(npm test:*)" so it is allowed for the rest of the run without asking.',
      '- Some things are refused before they reach you and cannot be allowed: sudo, restarting Crundi, pushing or changing remotes, the server\'s ports (80, 443, 8888, 22, 53), and writing outside the run\'s folders.',
      managerMode === 'bypass'
        ? '- The person launched you with bypass permissions: they trust you. Your own actions are not checked, and you should answer your team\'s requests promptly and generously within the task, without asking the person.'
        : '- Your own actions ask the person in this chat as usual.',
    );
    const team = factory.roster.filter(r => r.id !== MANAGER_ID);
    if (team.length) {
      lines.push('', 'Your team:');
      for (const r of team) lines.push(`- ${r.name}: ${oneLine(r.persona, 160) || 'no description'}`);
    }
  } else {
    lines.push(
      '- You are given one step at a time. Each step says what to do, what came before, and what you must decide at the end.',
      `- When the step is done, call ${TOOL} exactly once. That ends your step: stop there, with no further tool calls and no long reply.`,
      `- If you cannot do the step (blocked, unclear, missing access, out of your depth), call ${TOOL} with escalate set to the reason instead. The Manager picks it up.`,
      '- Nobody is watching your turn. Do not ask the person questions; escalate instead.',
      '- Work only in the folder the step names. The project\'s own checkout is off limits.',
      '- Do not commit, branch, merge, push or reset with git. Crundi commits your work when you hand off.',
    );
  }
  if (paths.notesFile) lines.push(`- Your private notes file: ${paths.notesFile}. Keep anything you want to remember between steps there.`);
  if (paths.sharedDir) lines.push(`- The team's shared folder: ${paths.sharedDir} (for example plan.md). Everyone can read and write it.`);
  return lines.join('\n');
}

/**
 * The message for one step.
 * @param {object} o
 * @param {object} o.run          engine run
 * @param {object} o.effect       the activate effect
 * @param {object} o.req          requirements(run, tokenId)
 * @param {string} o.wsPath       folder to work in
 * @param {string} [o.notice]     anything extra the runner must say (a merge conflict)
 * @param {object} [o.recovery]   { reason, lastAction, status, diffStat, note } — this step was
 *                                interrupted and is being picked up again
 */
export function stepMessage({ run, effect, req, wsPath, notice = '', recovery = null }) {
  const f = run.snapshot;
  const node = f.nodes.find(n => n.id === effect.nodeId);
  const c = effect.context || {};
  const lines = [];
  const stepName = node ? (node.label || (node.type === 'manager' ? 'Manager' : node.type)) : 'step';
  if (recovery) {
    lines.push(`[Factory] You were interrupted: ${recovery.reason}`);
    lines.push(`You were on the step "${stepName}". Continue from where you stopped; the work you already did is still in the folder.`);
    if (recovery.lastAction) lines.push(`Your last completed handoff in this run: ${recovery.lastAction}`);
    if (recovery.status) lines.push('', 'The folder right now (git status, uncommitted changes are yours from before the interruption):', indent(recovery.status));
    if (recovery.diffStat) lines.push('', 'Changes in this run so far:', indent(recovery.diffStat));
    if (recovery.note) lines.push('', 'A note from the person:', indent(recovery.note));
    lines.push('', '— The step, as it was given —', '');
  }
  if (effect.mode === 'answer') lines.push(`[Factory] A decision for you: ${node?.label || node?.decision?.question || ''}`.trim());
  else lines.push(`[Factory] Your step: ${stepName}`);
  lines.push('', `Task: ${run.task || '(none given)'}`);
  if (wsPath) lines.push(`Work in: ${wsPath}`);
  if (c.escalation) lines.push('', `Escalated to you: ${c.escalation}`);
  if (notice) lines.push('', notice);
  if (c.from && c.summary) lines.push('', `From ${c.from.name}${c.from.label && c.from.label !== c.from.name ? ` (${c.from.label})` : ''}:`, indent(c.summary));
  if (c.message) lines.push('', 'Message for you:', indent(c.message));
  if (c.notes && c.notes.length) {
    lines.push('', 'Notes left for you:');
    for (const n of c.notes) lines.push(`- From ${n.from}${n.label ? ` (${n.label})` : ''}: ${n.text}`);
  }
  if (effect.mode === 'answer') lines.push('', 'Nothing else is asked of you here: look at whatever you need to decide, then hand off with your answer(s).');

  lines.push('', `When you are done, call ${TOOL} with:`);
  lines.push('- summary: what you did or decided, in a few sentences (required)');
  if (req && req.route && req.route.decision) {
    const tree = [];
    renderRoute(req.route, tree, '  ', false);
    lines.push('- answers:', ...tree);
  } else if (req && req.route) {
    // No decision at this step: say where the work goes, outside "answers",
    // or Claude invents an answer to fill the heading.
    const tree = [];
    describeNext(req.route, tree, '', '  ');
    lines.push(`- no answers are needed here; the work then ${tree[0]}`, ...tree.slice(1));
  }
  if (req && req.noteTargets && req.noteTargets.length) {
    lines.push('- notes (optional), delivered when the flow reaches them:');
    for (const n of req.noteTargets) lines.push(`    notes.${n.key}: for ${n.to}${n.label ? ` ("${n.label}")` : ''}`);
  }
  if (req && req.canFinish) lines.push('- finish: true, instead of answers, when the whole task is done');
  if (req && req.mode === 'work' && req.who !== MANAGER_ID) lines.push('- escalate: "<reason>", instead of everything else, if you cannot do this step');
  const ex = example(req);
  if (ex) lines.push('', `Example: ${JSON.stringify(ex)}`);
  return lines.join('\n');
}

/**
 * What the Manager is told first when a run is recovered: why it stopped,
 * where everyone was, and what is in the worktree.
 */
export function recoveryOverview({ run, reason, tokens, recent, dropped, status, diffStat, note, wasFinishing }) {
  const lines = [`[Factory] This run is being recovered. It stopped because: ${reason}`, ''];
  if (wasFinishing) lines.push('You had already finished (finish: true). When everyone is settled, present the result to the person again.');
  if (tokens.length) {
    lines.push('Where the work stood:');
    for (const t of tokens) lines.push(`- ${t.employee || 'the person'}: ${t.what}`);
  }
  if (recent.length) {
    lines.push('', 'The last handoffs:');
    for (const h of recent) lines.push(`- ${h}`);
  }
  if (dropped.length) {
    lines.push('', 'Permission requests that were waiting were lost with the old sessions; the employees will ask again if they still need them:');
    for (const d of dropped) lines.push(`- ${d}`);
  }
  if (status) lines.push('', `The run worktree now (${run.id}):`, indent(status));
  if (diffStat) lines.push('', 'Changes in this run so far:', indent(diffStat));
  if (note) lines.push('', 'A note from the person:', indent(note));
  lines.push('', 'Everyone who was mid-step has been told to carry on. Nothing is needed from you unless a step of yours is listed below.');
  return lines.join('\n');
}

/** Sent once when an employee ends a turn without handing off. */
export function nudgeMessage() {
  return `[Factory] You ended your turn without calling ${TOOL}. If the step is done, hand off now. If you are stuck, hand off with escalate and the reason.`;
}

/** Sent to an employee whose step was cancelled (a fan-out was aborted). */
export function cancelMessage(reason) {
  return `[Factory] Your current step was cancelled: ${reason}\nStop working on it now and wait. Your changes so far are kept.`;
}

// ─── Rendering the question tree ───

function renderRoute(item, out, pad, top) {
  if (!item) return;
  if (item.decision) {
    const d = item.decision;
    const key = `answers.${d.id}`;
    let how;
    if (d.kind === 'yesno') how = '"yes" or "no"';
    else if (d.kind === 'one') how = 'one of ' + d.options.map(o => `"${o.id}" (${o.label})`).join(', ');
    else how = 'a list of any of ' + d.options.map(o => `"${o.id}" (${o.label})`).join(', ') + ', or [] for none';
    out.push(`${pad}${key}: ${d.question || d.label}  →  ${how}`);
    if (d.kind === 'many' && d.lastPicks) out.push(`${pad}  (last time: ${JSON.stringify(d.lastPicks)}; used again if you leave it out)`);
    for (const b of d.branches) {
      if (!b.next) continue;
      const lead = `${pad}  if ${d.kind === 'many' ? `you pick ${b.answer === 'none' ? 'none' : `"${b.answer}"`}` : `"${b.answer}"`}: `;
      describeNext(b.next, out, lead, pad + '    ');
    }
    return;
  }
  if (top) describeNext(item, out, `${pad}next: `, pad + '  ');
}

function describeNext(item, out, lead, pad) {
  const extra = [];
  if (item.requireNote) extra.push(`requires notes.${item.edgeId} ("${item.noteLabel || 'a note'}")`);
  if (item.maxRounds) extra.push(`round ${Math.min(item.used + 1, item.maxRounds)} of ${item.maxRounds}`);
  const tail = extra.length ? `  [${extra.join('; ')}]` : '';
  if (item.decision) {
    out.push(`${lead}then also answer:${tail}`);
    renderRoute(item, out, pad, false);
    return;
  }
  const to = item.to || {};
  let where;
  if (to.type === 'employee' || to.type === 'manager') where = `goes to ${to.name}`;
  else if (to.type === 'decision') where = `${to.answeredBy} is asked "${to.label}"`;
  else if (to.type === 'join') where = 'waits for the other branches';
  else if (to.type === 'ask') where = 'the person is asked';
  else where = `goes to ${to.label || to.type}`;
  out.push(`${lead}${where}${tail}`);
}

function example(req) {
  if (!req) return null;
  const ex = { summary: '…' };
  const ans = {};
  let item = req.route;
  while (item && item.decision) {
    const d = item.decision;
    const pick = d.kind === 'yesno' ? 'no' : d.kind === 'one' ? d.options[0]?.id : [d.options[0]?.id].filter(Boolean);
    ans[d.id] = pick;
    const h = Array.isArray(pick) ? pick[0] : pick;
    const b = d.branches.find(x => x.answer === h);
    item = b && b.next;
    if (item && item.requireNote) { ex.notes = { ...(ex.notes || {}), [item.edgeId]: '…' }; }
  }
  if (Object.keys(ans).length) ex.answers = ans;
  else if (req.canFinish && !req.route) ex.finish = true;
  return ex;
}

function oneLine(s, max) {
  const t = String(s || '').replace(/\s+/g, ' ').trim();
  return t.length > max ? t.slice(0, max - 1) + '…' : t;
}

function indent(s) {
  return String(s).split('\n').map(l => '  ' + l).join('\n');
}
