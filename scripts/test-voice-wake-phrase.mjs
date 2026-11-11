// The PC's "Hey Cru" phrase match (app/voice-wake/enroll.js matchPhrase), with
// no model: synthetic takes (scripts/wake-fixture.mjs signal), a spotter that
// hears nothing in training (a voice the model barely hears), and the model's
// second opinion given by hand.
//   node scripts/test-voice-wake-phrase.mjs
import * as E from '../app/voice-wake/enroll.js';
import { signal } from './wake-fixture.mjs';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

const takes = [1, 2, 3, 4, 5].map((i) => signal(10 + i, 140 + i, 0.95 + i * 0.02));
const deafSpotter = () => ({ accept: () => null, reset() {}, free() {} });
const r = E.train(takes, deafSpotter);
ok(r.ok && r.profile.modelHeard === 0, 'a voice the model never heard in training still trains');
const p = r.profile, owner = signal(30, 143, 1.0);

// 12 Nov (the owner, PC): a "hmm hmm" in his own voice woke Cru. A burst that sounds like his
// takes but in which the model hears no word must not wake, whatever the profile.
ok(!E.matchPhrase(p, owner, () => []).ok && E.matchPhrase(p, owner, () => []).why === 'no word', 'no word heard in it: no wake, however close it sounds');
ok(E.matchPhrase(p, owner, () => [E.WAKE]).ok, '"Hey Cru" heard, if faintly: wakes');
ok(!E.matchPhrase(p, owner, () => ['NOT_YOU']).ok, 'a decoy heard instead: no wake');
ok(!E.matchPhrase(p, owner, () => [E.WAKE, 'NOT_YOU']).ok, 'a voice the model hears poorly: a decoy beside "Hey Cru" still refuses it');
ok(E.matchPhrase({ ...p, modelHeard: 5 }, owner, () => [E.WAKE, 'NOT_YOU']).ok, 'a voice the model hears well: "Hey Cru" heard is enough');
ok(!E.matchPhrase({ ...p, modelHeard: 5 }, owner, () => []).ok, 'and it too needs the word');
ok(!E.matchPhrase(p, signal(31, 260, 0.7), () => [E.WAKE]).ok, 'a very different voice does not wake, word or not');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
