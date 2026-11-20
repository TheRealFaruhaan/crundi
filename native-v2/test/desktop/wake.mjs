// ─── "Hey Cru": trained from five recordings, listening only when it may, the owner's voice opens Cru ───
// Ported from native-gpui/test/desktop.mjs ("Hey Cru"). The models: CRUNDI_WAKE_MODELS, a folder
// kept between runs (the app downloads into it the first time; native-gpui's suite used
// /tmp/vw-install when there); without it only the first check runs. v2: the power and the lock are read only while listening is on (no pump
// otherwise).
import { writeFileSync, existsSync } from 'node:fs';

export async function run(T, t) {
  const D = T.D;
  const W = (p) => D(p).wake;
  const fx = (n) => new URL(`../../../native-gpui/test/fixtures/heycru/${n}.wav`, import.meta.url).pathname;
  const models = !!process.env.CRUNDI_WAKE_MODELS;
  // The owner's voice check needs the speaker model (downloaded with the keyword model, kept only
  // when its checksum matches): without it any voice's "Hey Cru" wakes, as the app says.
  const voiceModel = models && existsSync(`${process.env.CRUNDI_WAKE_MODELS}/speaker/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx`);
  // A second of silence, as a recording.
  const silent = '/tmp/crundi-wake-silent.wav';
  { const n = 16000, b = Buffer.alloc(44 + n * 2); b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVEfmt ', 8); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(16000, 24); b.writeUInt32LE(32000, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write('data', 36); b.writeUInt32LE(n * 2, 40); writeFileSync(silent, b); }
  let p = await T.state();
  t.check('off and untrained at first', !W(p).enabled && !W(p).trained, W(p));
  // Training fetches the models first (engine::install, ~40 MB): never on every run. Without a
  // models folder for the run (CRUNDI_WAKE_MODELS, kept between runs), the rest is skipped.
  if (!models) { console.log('skip: "Hey Cru" training and listening need CRUNDI_WAKE_MODELS=<a folder kept between runs> (the app downloads into it once)'); return; }
  // Settings › Voice assistant › On this device (the Operations area's page, when there).
  const toDevice = async () => { await T.ui('ops:settings-open', 'voice'); await t.sleep(600); for (let i = 0; i < 5; i++) await T.mouse('wheel', 800, 500, { dy: -300 }); await t.sleep(600); };
  await T.ui('tab', 'settings');
  await toDevice();
  await T.snap('desktop', 'settings-voice-device');
  await T.act('wake-train-start');
  await T.until('Train: five recordings, the first', (p) => W(p).training && W(p).trainPos === 0);
  await T.act('wake-take', silent);
  await T.until('a silent take is refused, as the web says', (p) => W(p).trainBad && /Nothing was heard\. Speak a little closer\. Record it again\./.test(W(p).trainMsg) && W(p).trainPos === 0);
  for (let i = 1; i <= 2; i++) await T.act('wake-take', fx(`draco-train${i}`));
  await T.until('…two kept', (p) => W(p).trainPos === 2 && W(p).trainMsg === 'Got it.');
  await toDevice();
  await T.snap('desktop', 'wake-training');
  for (let i = 3; i <= 5; i++) await T.act('wake-take', fx(`draco-train${i}`));
  p = await T.until('…the fifth trains it', (p) => W(p).trained && !W(p).training, 180000);
  if (!p) return;
  if (voiceModel) t.check('…with the owner\'s voice check', W(p).voiceCheck === 'on', W(p));
  else t.check('…without the voice model: the voice check off, and it says so', W(p).voiceCheck === 'off' && /voice model is missing/.test(W(p).trainMsg), W(p));
  await toDevice();
  await T.snap('desktop', 'wake-trained');
  {
    // v2: trained but off, nothing reads the power or the lock: the pump sleeps.
    await T.ui('tab', 'inbox');
    await t.sleep(1500);
    const a = D(await T.state()).pumps;
    await t.sleep(3000);
    const b = D(await T.state()).pumps;
    t.check('v2: off, no deadline: the area\'s pump sleeps', a === b, { a, b });
  }
  await T.act('wake-cond', 'false', 'false');
  await T.act('wake-set', 'true');
  await T.until('on: listening (on AC, unlocked, Cru closed)', (p) => W(p).state === 'listening', 60000);
  {
    // v2: on, the conditions are read every 2 s (a pump each, no frame).
    const a = D(await T.state()).pumps; const m = T.frames.mark();
    await t.sleep(4500);
    const b = D(await T.state()).pumps;
    t.check(`v2: listening, the conditions read every 2 s: ${b - a} pumps in 4.5 s`, b - a >= 2 && b - a <= 4, { a, b });
    t.check('…and nothing draws', T.frames.since(m).frames === 0, T.frames.since(m));
  }
  await T.act('wake-cond', 'true', 'false');
  await T.until('on battery: paused', (p) => W(p).state === 'on-battery' && W(p).label === 'Paused: on battery');
  await T.act('wake-cond', 'false', 'true');
  await T.until('screen locked: paused', (p) => W(p).state === 'paused' && W(p).label === 'Paused: screen locked');
  await T.act('wake-cond', 'false', 'false');
  await T.until('…listening again', (p) => W(p).state === 'listening', 30000);
  await T.ui('tab', 'workbench');
  await T.act('wake-hear', fx('draco-neg-heyyou'));
  await t.sleep(4000);
  t.check('a decoy ("hey you") does not open Cru', !D(await T.state()).cru.open);
  if (voiceModel) {
    await T.act('wake-hear', fx('arcas-pos-heycru'));
    await t.sleep(4000);
    t.check('another voice\'s "Hey Cru" does not open Cru', !D(await T.state()).cru.open, W(await T.state()).log);
  }
  await T.act('wake-hear', fx('draco-pos-heycru'));
  await T.until('the owner\'s "Hey Cru" opens Cru', (p) => D(p).cru.open && W(p).lastHeard?.keyword === 'HEY_CRU', 15000);
  await T.until('…and it pauses while Cru\'s screen is open', (p) => W(p).state === 'voice-open');
  await T.act('cru');
  await T.until('Cru closed: listening again', (p) => !D(p).cru.open && W(p).state === 'listening', 30000);
  await T.act('wake-set', 'false');
  await T.act('wake-forget');
  await T.until('Forget asks first', (p) => /Forget your/.test(p.dialog?.title || ''));
  await T.ui('dialog-answer', 'true');
  await T.until('…and forgets: off, untrained', (p) => !W(p).trained && !W(p).enabled);
  await T.act('wake-cond', '', '');
}
