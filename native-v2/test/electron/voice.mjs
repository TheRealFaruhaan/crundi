// T18: Settings › Cru › "Your voice" (the web's speaker section, capability
// 'voice-speaker'), with real clicks and keys against the test server's own
// voice check (onnxruntime-node in its node_modules; without it the section
// says the check is not available, and only that is checked). The microphone
// is a WAV of real speech (native-gpui/test/fixtures/heycru, joined), played
// through the same recorder and cleaning as a real one (CRUNDI_TEST_MIC_WAV).

import { readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** The fixtures joined (16 kHz mono PCM16): about 8 s of one voice. */
export function setup(ctx) {
  const dir = fileURLToPath(new URL('../../../native-gpui/test/fixtures/heycru/', import.meta.url));
  const names = ['draco-train1', 'draco-train2', 'draco-train3', 'draco-train4', 'draco-train5', 'draco-pos-heycru', 'draco-neg-heyyou'];
  const pcm = [];
  for (const n of names) {
    const b = readFileSync(join(dir, `${n}.wav`));
    let i = 12;
    while (i + 8 <= b.length) { const id = b.toString('ascii', i, i + 4), len = b.readUInt32LE(i + 4); if (id === 'data') { pcm.push(b.subarray(i + 8, i + 8 + len)); break; } i += 8 + len + (len & 1); }
    pcm.push(Buffer.alloc(16000 * 2 * 0.15));
  }
  const data = Buffer.concat(pcm);
  const h = Buffer.alloc(44);
  h.write('RIFF', 0); h.writeUInt32LE(36 + data.length, 4); h.write('WAVEfmt ', 8); h.writeUInt32LE(16, 16); h.writeUInt16LE(1, 20); h.writeUInt16LE(1, 22);
  h.writeUInt32LE(16000, 24); h.writeUInt32LE(32000, 28); h.writeUInt16LE(2, 32); h.writeUInt16LE(16, 34); h.write('data', 36); h.writeUInt32LE(data.length, 40);
  const f = join(tmpdir(), `crundi-v2-voice-${process.pid}.wav`);
  writeFileSync(f, Buffer.concat([h, data]));
  process.env.CRUNDI_TEST_MIC_WAV = f;
  ctx.drop.push(f);
}

export async function run(T, t) {
  const S = (p) => p.areas?.ops?.settings?.voice?.speaker || {};
  const at = async (words) => { const b = await T.ui('find', words); return b.ok ? { x: b.x + b.w / 2, y: b.y + b.h / 2 } : null; };
  const click = async (words) => { const p = await at(words); t.check(`"${words}" is on screen`, !!p); if (p) await T.mouse('click', p.x, p.y); return !!p; };
  const server = async () => (await T.api.http('GET', '/api/voice/speaker')).body || {};

  await T.ui('ops:settings-open', 'voice');
  let p = await T.until('Settings › Cru: "Your voice" is there (the server has voice-speaker)', (p) => S(p).shown, 20000);
  if (!p) return;
  await t.sleep(400);
  if (!await click('Your voice')) return;
  p = await T.until('…clicked open', (p) => S(p).open, 5000);
  const avail = (await server()).available === true;
  if (!avail) {
    t.check('…without onnxruntime-node: "not available on this server"', !!await at('The voice check is not available on this server'));
    return;
  }
  t.check('…"Not enrolled yet: Cru acts on any voice"', !!await at('Not enrolled yet'));
  await T.snap('electron', 'voice-speaker');
  if (!await click('Enrol my voice')) return;
  await T.until('enrolling: phrase 1', (p) => S(p).active && S(p).pos === 0 && !S(p).words, 5000);
  t.check('…the first phrase\'s line', !!await at('Phrase 1 of'));
  // Each phrase: Record, about 9 s of speech, Stop; the server checks it.
  for (let i = 0; i < 6; i++) {
    const before = S(await T.state()).results.length;
    if (!await click('Record')) break;
    await T.until(`phrase ${i + 1}: recording ("Listening…")`, (p) => S(p).recording, 5000);
    await t.sleep(9000);
    if (!await click('Stop')) break;
    p = await T.until(`…phrase ${i + 1} checked by the server`, (p) => S(p).results.length > before && !S(p).busy, 180000);
    if (!p) break;
    const r = S(p).results[S(p).results.length - 1];
    console.log(`  phrase ${i + 1}: ${r.accepted ? 'accepted' : 'refused: ' + r.problem}`);
    if (S(p).results.filter((x) => x.accepted).length >= 5) break;
  }
  p = await T.state();
  t.check('five phrases accepted', S(p).results.filter((x) => x.accepted).length >= 5, S(p).results);
  if (!await click('Finish now')) return;
  p = await T.until('…finished: enrolled (then the optional short replies, or done)', (p) => S(p).enrolled && !S(p).busy, 60000);
  if (S(await T.state()).active) await click('Not now');
  await T.until('…the section back to its status', (p) => !S(p).active, 5000);
  t.check('the server has the voice enrolled', (await server()).enrolled === true);
  t.check('…"Enrolled" on screen', !!await at('Enrolled'));
  // Only act on my voice.
  if (await click('Only act on my voice')) await T.until('"Only act on my voice" on: the server keeps it', (p) => S(p).ownerOnly, 8000);
  t.check('…the server says ownerOnly', (await T.api.http('GET', '/api/voice/settings')).body?.ownerOnly === true);
  // Forget my voice: asked first, Enter says yes.
  if (await click('Forget my voice')) {
    await t.sleep(500);
    await T.key('Enter');
    await T.until('forgotten: not enrolled', (p) => !S(p).enrolled && /forgotten/.test(S(p).msg), 10000);
    t.check('the server has no voice', (await server()).enrolled === false);
  }
  await T.ui('ops:settings-open', 'voice', 'false');
}
