#!/usr/bin/env node
/**
 * test-voice-turns.mjs — the owner's turns on /api/voice/turn (voice/turns.js):
 * a resend of the same turn gets its result (never a second run, never
 * "busy"), and a turn that finds Cru busy waits for it. Pure node.
 * Run: node scripts/test-voice-turns.mjs
 */
const { createTurnGate, BUSY_ERROR } = await import('../src/voice/turns.js');

let failed = 0;
const ok = (c, m) => { if (c) console.log('PASS  ' + m); else { failed++; console.log('FAIL  ' + m); } };

// A pretend Cru: one turn at a time, as agent.js and agent-cli.js are.
function cru(ms = 50) {
  let busy = false; const ran = [];
  return {
    ran,
    turn: async (text) => {
      if (busy) return { ok: false, error: BUSY_ERROR };
      busy = true; ran.push(text);
      try { await new Promise((r) => setTimeout(r, ms)); return { ok: true, speak: `done: ${text}` }; } finally { busy = false; }
    },
  };
}

{
  // 10 Nov: "Yes" sent, the same request again 13 s into the turn.
  const c = cru(80), g = createTurnGate({ pollMs: 10 });
  const first = g.run('req-1', () => c.turn('Yes'));
  await new Promise((r) => setTimeout(r, 20));
  const again = g.run('req-1', () => c.turn('Yes'));
  const [a, b] = await Promise.all([first, again]);
  ok(a.ok && b.ok && b.speak === 'done: Yes' && b.repeated, 'a resend of the same turn gets its answer, not "busy"');
  ok(c.ran.length === 1, 'and the turn runs once (the yes is acted on once)');
  const later = await g.run('req-1', () => c.turn('Yes'));
  ok(later.ok && c.ran.length === 1, 'a resend after it finished: the same answer, still once');
}
{
  const c = cru(60), g = createTurnGate({ pollMs: 10 });
  const one = g.run('a', () => c.turn('first'));
  await new Promise((r) => setTimeout(r, 10));
  const two = await g.run('b', () => c.turn('second'));
  await one;
  ok(two.ok && two.speak === 'done: second' && c.ran.join() === 'first,second', 'a new turn while Cru is busy waits its turn, then runs');
}
{
  const c = cru(60), g = createTurnGate({ pollMs: 10 });
  const one = c.turn('wake-up');   // a turn of Cru's own, not through the gate
  await new Promise((r) => setTimeout(r, 10));
  const r = await g.run('', () => c.turn('owner'));
  await one;
  ok(r.ok && c.ran.join() === 'wake-up,owner', 'also behind a wake-up or a report (no requestId: an older client)');
}
{
  let t = 0; const c = { turn: async () => ({ ok: false, error: BUSY_ERROR }) };
  const g = createTurnGate({ now: () => t, sleep: async () => { t += 1000; }, waitMs: 5000 });
  const r = await g.run('x', () => c.turn());
  ok(!r.ok && r.error === BUSY_ERROR && t >= 5000, 'it waits only so long, then says so');
}
{
  const g = createTurnGate();
  const r = await g.run('e', async () => ({ ok: false, error: 'Something else' }));
  ok(r.error === 'Something else', 'other errors come back at once');
}

console.log(failed ? `\n${failed} FAILED` : '\nAll turn checks passed');
process.exit(failed ? 1 : 0);
