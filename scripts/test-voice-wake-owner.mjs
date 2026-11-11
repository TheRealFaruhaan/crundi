// "Hey Cru" for the owner only (app/voice-wake/owner.js, worker.js ownerChecked),
// with a fake voice model: no download, no onnxruntime needed.
//   node scripts/test-voice-wake-owner.mjs
import { createOwnerCheck, validVoice, ownerScore, ownerVerdict, OWNER_THRESHOLD, OWNER_K, VOICE_MODEL } from '../app/voice-wake/owner.js';
import { ownerChecked } from '../app/voice-wake/worker.js';
import { MODEL } from '../src/voice/speaker.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// A fake model: the clip's first sample picks a "voice" direction, so 0.5 = the owner, 0.9 = someone else.
const dir = (seed) => { const v = new Float32Array(MODEL.dim); for (let i = 0; i < v.length; i++) v[i] = Math.sin(seed * 7.3 + i * 0.91); return v; };
const voices = { 0.5: dir(1), 0.9: dir(2) };
const embedder = { state: 'ready', async load() {}, async embed(c) { const base = voices[c[0]] || dir(3); const n = Float32Array.from(base, (x, i) => x + 0.15 * Math.sin(i * c[1])); return n; } };
const clip = (who, wobble, seconds = 0.8) => { const a = new Float32Array(Math.round(16000 * seconds)); a[0] = who; a[1] = wobble; return a; };
const owner = createOwnerCheck('/nowhere', { embedder });

const voice = await owner.enrol([1, 2, 3, 4, 5].map((w) => clip(0.5, w)));
ok(validVoice(voice) && voice.model === VOICE_MODEL && voice.clips.length === 5, 'the five takes become the profile\'s voice');
ok(JSON.stringify(voice).length < 12000 && validVoice(JSON.parse(JSON.stringify(voice))), 'small, and it survives profile.json');
ok(!validVoice({ ...voice, model: 'other' }) && !validVoice({ ...voice, clips: voice.clips.slice(0, OWNER_K - 1) }) && !validVoice(null), 'another model, too few takes or none: no voice');

const mine = await owner.check(voice, clip(0.5, 9)), theirs = await owner.check(voice, clip(0.9, 9));
ok(mine.ok === true && mine.score >= OWNER_THRESHOLD, 'the owner\'s "Hey Cru" passes (' + mine.score + ')');
ok(theirs.ok === false && theirs.score < OWNER_THRESHOLD, 'another voice\'s is refused (' + theirs.score + ')');
ok((await owner.check(voice, clip(0.5, 9, 0.2))).ok === null, 'too short to tell: no verdict');
ok((await owner.check(null, clip(0.5, 9))).ok === null, 'no voice in the profile: no verdict');
const broken = createOwnerCheck('/nowhere', { embedder: { state: 'unavailable', async load() { throw new Error('offline'); }, async embed() { throw new Error('offline'); } } });
ok((await broken.check(voice, clip(0.9, 9))).ok === null && (await broken.load()) === false, 'the model not to be had: no verdict (the wake stands, as before)');
ok(ownerVerdict(voice, await embedder.embed(clip(0.5, 4)), 0.8) === true && ownerVerdict(voice, await embedder.embed(clip(0.9, 4)), 0.8) === false && ownerVerdict(voice, await embedder.embed(clip(0.5, 4)), 0.1) === null, 'ownerVerdict agrees');
ok(Math.abs(ownerScore(voice, await embedder.embed(clip(0.5, 4))) - mine.score) < 0.2, 'scores are on one scale');

// What the spotter process posts for a wake.
const profile = { voice };
const wakeOf = (who) => ({ ok: true, keyword: 'HEY_CRU', score: 2, via: 'phrase', clip: clip(who, 7) });
ok((await ownerChecked(wakeOf(0.5), profile, owner)).type === 'detected', 'the owner: woken');
const other = await ownerChecked(wakeOf(0.9), profile, owner);
ok(other.type === 'rejected' && /owner/.test(other.why) && typeof other.voice === 'number', 'someone else saying Hey Cru: refused, and why');
ok((await ownerChecked(wakeOf(0.9), {}, owner)).voice === 'off', 'a profile from before the voice check: woken as before');
ok((await ownerChecked({ ok: false, keyword: 'HEY_CRU', score: 4, via: 'model' }, profile, owner)).type === 'rejected', 'a wake the phrase or model refused stays refused');
ok(/no verdict/.test((await ownerChecked(wakeOf(0.9), profile, broken)).voice), 'the model unavailable: woken, said so');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
