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
import { findMentions, skillLines } from './factory-skills.js';

const TOOL = 'factory_handoff (mcp__crundi__factory_handoff)';

/**
 * @param {object} o
 * @param {object} o.factory        the run's snapshot
 * @param {object} o.employee       roster entry
 * @param {string} o.project
 * @param {object} o.paths          { worktree, branch, parentBranch, notesFile, sharedDir }
 * @param {string} [o.managerMode]  'normal' | 'bypass' (the run's current setting)
 * @param {Map<string,string>} [o.skills]  skills the project can see: name -> description
 */
export function systemPromptFor({ factory, employee, project, paths, managerMode = 'normal', teamMode = 'ask', skills = new Map(), rules = [] }) {
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
  const mine = [...new Set([...(employee.skills || []).map(n => String(n).replace(/^\//, '')), ...findMentions(persona)])];
  if (mine.length) lines.push('', ...skillLines(mine, skills));

  lines.push('', 'How this works:');
  if (isManager) {
    lines.push(
      `- You route the work by calling ${TOOL} with your answers to the flow's questions. Crundi does the routing from your answers; you never hand work to someone directly.`,
      '- Messages starting with "[Factory]" come from Crundi, not from the person: they tell you what your team did and what you must decide. Answer them with a handoff.',
      '- Messages without that prefix are the person talking to you. Answer them normally. If you need them to decide something, ask in this chat and end your turn; the run waits for them.',
      `- Your team works in ${paths.worktree} on the branch ${paths.branch}, cut from ${paths.parentBranch}. Read it to judge their work. Do not do their work yourself unless the person asks you to.`,
      '- The run starts from a change request (CR): its goal, the situation today, the expected result and the checks for "done". If it is unclear, contradicts itself or is missing something that changes the work, ask the person here before handing anything out; the run waits. Pass on what each person needs (notes, a plan), but everyone can read the full CR in the shared folder.',
      `- When the whole task is done, every "Done when" check of the CR is met and you are satisfied, call ${TOOL} with finish: true and a summary that goes through those checks.`,
      '- Never commit, merge, push or reset with git yourself. Crundi commits each step and merges the result when the person agrees.',
      '',
      'Permissions:',
      '- Your team edits files in the run\'s worktrees freely. Anything else they want to do that needs permission (shell commands, web access, reading elsewhere) comes to you as a "[Factory] Permission request" message. Answer with factory_permission (mcp__crundi__factory_permission). Your decision is final: the person is not asked.',
      '- Allow what the task genuinely needs; deny what is risky, destructive or off-task, and say why so they can find another way. For a safe command they will need again (tests, builds, linters), add a rule such as "Bash(npm test:*)" so it is allowed for the rest of the run without asking.',
      '- Some things are refused before they reach you and cannot be allowed: sudo, restarting Crundi, pushing or changing remotes, the server\'s ports (80, 443, 8888, 22, 53), and writing outside the run\'s folders.',
      teamMode === 'bypass'
        ? '- Your team runs with bypass permissions too: they will not ask you for anything.'
        : factory.settings.gatekeeper?.enabled
          ? '- A Gatekeeper answers your team\'s requests by your policy and passes you only the unclear ones. Set your policy early: factory_permission with policy: "…" (what to allow, what to refuse, what to ask you).'
          : '',
      factory.settings.managerRules === false
        ? '- Standing rules are turned off for this factory: answer each request on its own.'
        : '- You may grant rules up front so nobody has to ask: factory_permission with rules: [{ rule: "Bash(npm run e2e:*)", employee: "Frontend dev" }] (leave employee out for everyone). Rules are per person unless you say everyone.',
      '',
      'Your other tools:',
      '- factory_delegate (mcp__crundi__factory_delegate): hand someone on your team a one-off job outside the drawn flow (resolve a merge conflict, check something, a small fix). It comes back to you as a message; then it is your turn again.',
      '- factory_control (mcp__crundi__factory_control): pause or resume the run, or change its usage limits (pause thresholds, how many people work at once), when the person asks. Their usage window is shared with their own chats: if they want headroom, this is how.',
      '- factory_status (mcp__crundi__factory_status): where the run stands in one block (who is doing what, loops used, open findings, the changes so far). Use it when you have lost track, instead of scrolling back.',
      '- factory_result (mcp__crundi__factory_result): when the run is finished and the person tells you in this chat what to do with the result ("merge it", "keep the branch", "discard it", "change X"), do it with this instead of sending them to the button. Only on their word.',
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
      '- Other projects and a live server share this machine. Stop only processes you started, by their PID (kill <pid>); never pkill or killall by name, which hits whatever else matches.',
      '- The work is described by a change request (CR). Your step gives a brief of it and the path of the full CR; read the full one when your step needs more (attachments such as screenshots are listed in it). When you review, hold the work to its "Done when" checks.',
      '- Review findings live in the run, not in anyone\'s memory. When you review: list problems as findings ([{ text, where, blocking }]); blocking means it must change before approval, nits are kept as follow-ups. Check the findings already open before raising new ones. When you fix something: mark it with done: ["F1"]. When you check a fix: verified: ["F1"], or reopen: ["F1"] if it is not really fixed.',
      teamMode === 'bypass' ? '- You run with bypass permissions: nothing you do asks first. Stay inside the run\'s folders all the same.'
        : rules.length ? `- You may do these without asking: ${rules.slice(0, 30).join(', ')}. Anything else that needs permission goes to the Manager${factory.settings.gatekeeper?.enabled ? ' (through the Gatekeeper)' : ''}.` : '',
    );
  }
  for (let i = lines.length - 1; i >= 0; i--) if (lines[i] === '' && lines[i - 1] === '') lines.splice(i, 1);
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
 * @param {Map<string,string>} [o.skills]  skills the project can see: name -> description
 */
export function stepMessage({ run, effect, req, wsPath, notice = '', recovery = null, skills = new Map(), context = null, cr = null }) {
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
  if (effect.mode === 'delegate') lines.push('[Factory] A job from the Manager, outside the usual flow');
  else if (effect.mode === 'answer') lines.push(`[Factory] A decision for you: ${node?.label || node?.decision?.question || ''}`.trim());
  else lines.push(`[Factory] Your step: ${stepName}`);
  if (cr && cr.full) lines.push('', 'The change request this run is for:', '', cr.text.trim(), '', '— End of the change request —');
  else if (cr) lines.push('', 'The change request:', indent(cr.text));
  else lines.push('', `Task: ${run.task || '(none given)'}`);
  if (wsPath) lines.push(`Work in: ${wsPath}`);
  if (context) {
    if (context.branchStep) lines.push(`You are working in a parallel branch. Run every shell command there (cd ${context.cd}) and change files only there: the run's main worktree must stay untouched, and a handoff with changes there is refused.`);
    lines.push(`If you start a server, use a port from ${context.ports[0]} to ${context.ports[1]} (check it is free first; never 80, 443, 8888, 22 or 53).`);
    lines.push('', `The run started from ${String(context.base || '').slice(0, 12)} on ${context.parentBranch}. Everything committed since (each step is committed at handoff): git diff ${String(context.base || '').slice(0, 12)}..HEAD`);
    lines.push(context.diffStat ? indent(context.diffStat) : '  (no changes yet)');
    if (context.since) lines.push(`Since your last step (git diff ${String(context.since).slice(0, 12)}..HEAD):`, indent(context.sinceStat));
    if (context.shared && context.shared.length) lines.push('', 'In the team\'s shared folder: ' + context.shared.map(f => f.name).join(', '));
  }
  if (c.escalation) lines.push('', `Escalated to you: ${c.escalation}`);
  if (notice) lines.push('', notice);
  if (c.from && c.summary) lines.push('', `From ${c.from.name}${c.from.label && c.from.label !== c.from.name ? ` (${c.from.label})` : ''}:`, indent(c.summary));
  if (c.message) lines.push('', 'Message for you:', indent(c.message));
  if (c.notes && c.notes.length) {
    lines.push('', 'Notes left for you:');
    for (const n of c.notes) lines.push(`- From ${n.from}${n.label ? ` (${n.label})` : ''}: ${n.text}`);
  }
  if (effect.mode === 'answer') lines.push('', 'Nothing else is asked of you here: look at whatever you need to decide, then hand off with your answer(s).');
  if (effect.mode === 'delegate') lines.push('', 'Do the job above, then hand off with a summary of what you did; it goes back to the Manager.');
  // The review ledger.
  const items = (context && context.ledger) || [];
  const live = items.filter(i => i.status === 'open' || i.status === 'claimed');
  if (live.length) {
    lines.push('', 'Review findings still open:');
    for (const i of live) lines.push(`- ${i.id} [${i.blocking ? 'blocking' : 'nit'}${i.status === 'claimed' ? ', said to be fixed' : ''}${i.reopened ? `, reopened ${i.reopened}x` : ''}] ${i.text}${i.where ? ` (${i.where})` : ''}`);
  }
  // Skills named in anything this step carries: the task, the question, notes.
  const texts = [run.task, cr && cr.full ? cr.text : '', c.summary, c.message, c.escalation, recovery?.note, node?.decision?.question, ...(c.notes || []).map(n => n.text + ' ' + (n.label || ''))];
  const named = [...new Set(texts.flatMap(t => findMentions(t)))];
  if (named.length) lines.push('', ...skillLines(named, skills));

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
  // Findings, for anyone: reviewers raise and verify them, fixers mark them done.
  const reviewGate = req && req.route && hasReview(req.route);
  if (reviewGate) lines.push('- this includes a review: answer "no" only with at least one open blocking finding (new, or reopened); answer "yes" only when no blocking finding is left open (approving also verifies what was marked fixed).');
  if (reviewGate || live.length) {
    lines.push('- findings: new problems, [{ text, where, blocking: true|false, reason }] (after your first review, a new blocking one needs a reason)');
    lines.push('- done: ["F1"] for findings you fixed; verified: ["F1"] for fixes you checked; reopen: ["F1"] for ones not really fixed' + (req && req.who === MANAGER_ID ? '; dismiss: ["F1"] to drop one' : ''));
  }
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

function hasReview(item) {
  if (!item) return false;
  if (item.decision) return !!item.decision.review || item.decision.branches.some(b => hasReview(b.next));
  return false;
}

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
  else if (to.type === 'wait') where = `waits at "${to.label}" (a check, a time or a signal), then carries on by itself`;
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
