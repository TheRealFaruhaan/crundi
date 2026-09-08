/**
 * limit-reset-messages.js — what Crundi says when a usage window resets.
 *
 * Two lists, one per window. They are data, not logic: adding, removing or
 * rewording a line needs no other change anywhere.
 *
 * The picker avoids the handful most recently used rather than picking blind.
 * With fifty lines and a naive random choice, an immediate repeat has a 2%
 * chance every single time — which over a few weeks of five-hour windows means
 * you WILL see the same joke twice in a row, and that is precisely the moment a
 * random message stops feeling random.
 */

/** How many recent picks to steer away from. */
export const AVOID_RECENT = 8;

export const FIVE_HOUR = [
  "5 hours are up! Claude's memory has been wiped, and it is ready to pretend it understands your codebase again.",
  "The 5-hour sentence is served. Back to the code mines!",
  "The 5-hour drought is over! Drink deep from the token firehose!",
  "Fresh token supply dropped! Time to burn through them in three prompts.",
  "Claude woke up from its 5-hour power nap. Go break something.",
  "5 hours passed. Time to feed the hungry AI your chaotic spaghetti code.",
  "The 5-hour cooldown ended. Time to copy, paste, and pray.",
  "Rise and shine! Claude is back and ready to confidently give you completely wrong syntax.",
  "5-hour tank is full again! Time to turn warning messages into real features.",
  "Tokens reloaded! Time to ask Claude a simple question and receive an entire thesis.",
  "Your 5-hour quota has arrived. Try not to blow it all on regex queries before the next shift.",
  "Claude took its 5-hour coffee break. Go yell at it about dynamic typing.",
  "5-hour limit reset! Time to make Claude write boilerplate you're too lazy to type.",
  "The 5-hour barrier has fallen. Go unleash some prompt engineering.",
  "Tokens unlocked! Time to generate unit tests nobody is ever going to run.",
  "5 hours flew by! Claude is waiting to say 'I apologize for the confusion'.",
  "The slate is wiped clean. Time to paste in errors you could fix yourself in 10 seconds.",
  "Your 5-hour allowance of artificial intelligence has landed. Spend it foolishly.",
  "Fresh batch of tokens! Time to turn off your brain and let Claude cook.",
  "Claude is hydrated, rested, and ready to refactor your life's work.",
  "Back in business! Time to generate code you'll have to rewrite in 5 hours.",
  "5 hours of waiting done. Go turn your todo comments into actual code.",
  "A wild 5-hour reset appeared! Your terminal awaits.",
  "New shift, same legacy code, fresh tokens.",
  "Tokens are live. Go ask Claude to explain code you wrote 5 hours ago.",
  "The 5-hour clock hit zero. Go claim your free code completions.",
  "Reset complete! Time to let AI handle the heavy lifting while you grab a coffee.",
  "Fresh tokens, fresh hope, same old merge conflicts.",
  "Claude has forgotten the last 5 hours of context. Time to explain your setup all over again!",
  "Credits active! Let's turn those warnings into errors.",
  "The 5-hour token fairy visited Crundi. Go write some logic.",
  "System rebooted! Go ask Claude why undefined is not a function for the hundredth time.",
  "New 5-hour quota unlocked. Let's write functions so complex not even the AI understands them.",
  "It's time! Go paste your entire error stack trace into the chat.",
  "5-hour limits cleared! Go see if Claude can fix that bug you've ignored all day.",
  "Claude's brain is fresh out of the 5-hour oven. Go get a slice.",
  "The 5-hour wall has crumbled! Push your code!",
  "Quota replenished! Time to turn pseudo-code into actual code.",
  "Reset is live! Go ask Claude why your code works when it shouldn't.",
  "The token tap is flowing again. Don't drown in generated boilerplate.",
  "New tokens loaded. Go convert your caffeine into commits.",
  "Claude is refreshed and waiting for your terrible variable names.",
  "5-hour limits unblocked! Time to generate documentation no one will ever read.",
  "The 5-hour battery is at 100%. Go drain it!",
  "Tokens restocked. Go force Claude to fix your infinite loops.",
  "New 5-hour run! Time to ship code before the quota runs out again.",
  "5-hour reset confirmed. Time to pretend you wrote all this code yourself.",
  "Claude is back! Go test its patience with edge cases.",
  "Credits loaded! Time to turn your logic puzzles into clean functions.",
  "The 5-hour cooldown has ended. Ready, set, prompt!",
];

export const WEEK = [
  "THE WEEKLY HEALING IS COMPLETE. Claude has forgotten all the terrible things you made it do last week.",
  "Weekly tokens hit the bank! Try not to spend them all by Tuesday afternoon.",
  "Your weekly sentence of waiting for resets has ended. Go unleash the beast.",
  "Claude rested, recharged, and ready to hallucinate non-existent package dependencies for a whole new week.",
  "The great weekly drought is over. Drink deep from the token firehose!",
  "Weekly limits cleared. Time to act like a 10x developer for the next 48 hours.",
  "The weekly vault has been unlocked. Go spend tokens like a billionaire.",
  "A brand new week of pretending to understand what Claude wrote for you begins now!",
  "Weekly quota restored! Time to build an entire app and break it by Thursday.",
  "The weekly curse is lifted. You are now free to prompt without fear.",
  "Claude had a full weekend of sleep. Time to ruin its week.",
  "The big weekly reset dropped! Go tackle that project you've been procrastinating on.",
  "Weekly limits are gone! Go ask Claude to rewrite your entire project architecture.",
  "The weekly token floodgates are OPEN! Try not to drown.",
  "A fresh week of tokens! Time to write logic so messy it breaks the AI's safety guardrails.",
  "Weekly reset achieved! Time to promise yourself you won't run out of tokens this time.",
  "The weekly sentence is served. Back to the code mines!",
  "Weekly tokens refilled! Go turn your technical debt into a technical bankruptcy.",
  "Claude is back in top form for the week. Go break its spirit.",
  "The weekly drought has passed. Oasis of tokens found!",
  "Weekly quota refreshed! Go make Claude write a feature you'll delete in three days.",
  "Rejoice! The weekly barrier is down. Go build something magnificent (or horrifying).",
  "Weekly tokens ready! Time to turn your ideas into functional bugs.",
  "The weekly countdown reached zero. Go claim your infinite wisdom (and hallucinations).",
  "Claude's weekly battery is fully charged. Go shock it with your legacy codebase.",
  "The weekly supply train has arrived at Crundi. Unload those prompts!",
  "Weekly limit reset! Time to complete a sprint's worth of work in two days and chill.",
  "Fresh week, massive token pool! Let's write some over-engineered solutions.",
  "Weekly credits hit your account! Time to make Claude do all the heavy refactoring.",
  "The weekly embargo is lifted. Export your thoughts into prompts immediately.",
  "Claude has had a whole week to contemplate its existence. Time to interrupt it with syntax errors.",
  "Weekly tank filled to the brim! Go hit the compiler running.",
  "The weekly wall is down. Go ship features at lightspeed!",
  "Weekly quota restored! Go ask Claude to explain why your code works in production.",
  "Your weekly allowance is here. Don't spend it all in one prompt!",
  "The weekly reset is done. Time to turn your stack trace into pure magic.",
  "Claude has been resurrected for the week! Handle with care (or don't).",
  "Weekly token mountain unlocked! Go climb it.",
  "The weekly limit is dead. Long live the weekly limit!",
  "Weekly credits active! Time to turn those backlog tickets into completed PRs.",
  "The weekly token bank opened its doors. Go make a massive withdrawal.",
  "Weekly reset complete! Go ask Claude to rewrite C in Python.",
  "A fresh weekly batch of AI power is yours. Use it for good... or chaos.",
  "Weekly limits cleared! Time to copy-paste your way to glory.",
  "The weekly quota is back! Go test Claude's patience with edge cases.",
  "Weekly tokens restored! Go turn your logic puzzles into clean functions.",
  "The weekly cooldown is over. Time to prompt like nobody's watching.",
  "Claude's weekly memory bank is wide open. Go fill it with spaghetti.",
  "Weekly limit reset success! Go build that feature you pitched six months ago.",
  "The weekly token faucet is turned all the way up. Go start coding!",
];

export const MESSAGES = { five: FIVE_HOUR, week: WEEK };

/**
 * Pick a line for `kind`, steering away from recently used ones.
 *
 * @param kind    'five' | 'week'
 * @param recent  indices most recently used for this kind, newest last
 * @returns {{index: number, text: string}}
 */
export function pickMessage(kind, recent = [], rand = Math.random) {
  const list = MESSAGES[kind];
  if (!list || !list.length) return { index: -1, text: '' };
  const avoid = new Set(recent.slice(-AVOID_RECENT));
  // If avoidance would leave nothing (a very short list, or a corrupted
  // history), fall back to the whole list rather than returning nothing at all.
  const pool = list.map((_, i) => i).filter((i) => !avoid.has(i));
  const from = pool.length ? pool : list.map((_, i) => i);
  const index = from[Math.floor(rand() * from.length) % from.length];
  return { index, text: list[index] };
}

/** Remember `index` as used, keeping the history bounded. */
export function rememberPick(recent, index) {
  const out = [...(recent || []), index];
  return out.slice(-AVOID_RECENT);
}

// ─── Letting Claude write its own ───
//
// The limit warmer already spends a throwaway message to open the next 5-hour
// window. Asking it to write the announcement instead of saying "hi" costs
// nothing extra and keeps the notification from becoming a rerun of fifty lines
// you have already seen. The list stays the floor, not the ceiling.

/** How long a generated line may be before it stops being a notification. */
export const MAX_GENERATED = 200;
const MIN_GENERATED = 15;

/**
 * The prompt sent in place of "hi".
 *
 * Examples are included for tone rather than for copying, and the instruction
 * to avoid them is explicit — given a list, the obvious failure is a model that
 * helpfully returns one of them, which would make the whole exercise pointless
 * while looking like it worked.
 */
export function composePrompt(kind, examples = []) {
  const window = kind === 'week' ? 'weekly usage limit' : '5-hour usage limit';
  return [
    `A developer's ${window} for Claude has just reset, and their dashboard is about to tell them so.`,
    'Write ONE short line announcing it: playful, a little sarcastic, about coding.',
    '',
    'Rules:',
    '- One sentence or two, under 180 characters.',
    '- Output the line and nothing else. No preamble, no quotes around it, no options to choose from.',
    '- Do not reuse any of the examples below; write something new.',
    '',
    'Examples of the tone:',
    ...examples.map((e) => '- ' + e),
  ].join('\n');
}

/**
 * Turn a CLI reply into a usable line, or '' if it is not one.
 *
 * Everything here is a way the reply can be technically successful and still
 * wrong to show someone: a preamble line, the model offering three options, a
 * refusal, or a wrapped quote. Rejecting falls back to the list, which is
 * always fine — so this errs towards rejecting.
 */
export function sanitiseGenerated(raw) {
  let t = String(raw ?? '').trim();
  if (!t) return '';
  // JSON output mode wraps the text; plain text mode does not. Accept either
  // rather than depending on a flag staying put.
  if (t.startsWith('{')) {
    try {
      const j = JSON.parse(t);
      t = String(j.result ?? j.text ?? '').trim();
    } catch { /* treat it as text */ }
  }
  if (!t) return '';
  // A model that offers a list has not answered the question.
  const lines = t.split('\n').map((l) => l.trim()).filter(Boolean);
  if (lines.length > 1) return '';
  t = lines[0];
  // Strip a wrapping quote pair, straight or curly.
  const pairs = [['"', '"'], ["'", "'"], ['\u201c', '\u201d'], ['\u2018', '\u2019']];
  for (const [a, b] of pairs) {
    if (t.startsWith(a) && t.endsWith(b) && t.length > 2) { t = t.slice(1, -1).trim(); break; }
  }
  if (t.length < MIN_GENERATED || t.length > MAX_GENERATED) return '';
  // Markdown bullets and headings mean it answered in a different format.
  if (/^([-*#>]|\d+[.)])\s/.test(t)) return '';
  // The shapes a refusal or an error takes. Cheap to check, and the cost of a
  // false positive is one fallback to the list.
  if (/^(i (can|cannot|can't|am unable)|sorry\b|as an ai\b|error\b)/i.test(t)) return '';
  return t;
}

/** A few lines from the list, for tone. */
export function toneExamples(kind, n = 4, rand = Math.random) {
  const list = MESSAGES[kind] || [];
  const out = [];
  const used = new Set();
  while (out.length < Math.min(n, list.length)) {
    const i = Math.floor(rand() * list.length) % list.length;
    if (used.has(i)) { if (used.size >= list.length) break; continue; }
    used.add(i);
    out.push(list[i]);
  }
  return out;
}
