#!/usr/bin/env node
/**
 * test-voice-names.mjs — Crundi's names put back into what was heard
 * (src/voice/names.js), and plain English left alone.
 *
 * The first cases are what Deepgram really wrote for the owner's own voice
 * on 8 Nov 2026, with keyterms and without.
 * Run: node scripts/test-voice-names.mjs
 */
import { fixNames, nameList } from '../src/voice/names.js';
import { createAssembler } from '../src/voice/stream.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const projects = ['ai-thakey', 'baaru', 'crundi', 'crundi-voice', 'dhivo', 'dhivo-ad', 'dhivo-socials', 'faruhaan', 'homepool', 'jellyfin', 'maasinga', 'openhands', 'payment-gateway', 'playground', 'sam-cv', 'sentiq'];
const services = ['sam-cv', 'api-server', 'notes'];
const fix = (t) => fixNames(t, { projects, services });
const is = (heard, want) => { const got = fix(heard); ok(got === want, `"${heard}" -> "${got}"`); };

// The owner's recordings.
is('Hey, crew. Open the Kurony project and start a new chat.', 'Hey, Cru. Open the Crundi project and start a new chat.');
is('Hey, crew. Open the Crundi project and start a new chat.', 'Hey, Cru. Open the Crundi project and start a new chat.');
is('Restart the same cv service and show me the logs.', 'Restart the sam cv service and show me the logs.');
is('Restart the same CV service and show me the logs.', 'Restart the sam cv service and show me the logs.');
is('Move the Daibo Socials card to in progress.', 'Move the Dhivo socials card to in progress.');
is('A crew open the Cruny Project and start a new church.', 'A crew open the Crundi Project and start a new church.');
is('Tell Cloud to fix the Kotlin build.', 'Tell Claude to fix the Kotlin build.');
is('Is the deep gram spend over?', 'Is the Deepgram spend over?');
// Names said in pieces, or nearly.
is('Open the Senti Q project', 'Open the Sentiq project');
is('Open jelly fin', 'Open jellyfin');
is('Open the dibo ad', 'Open the dhivo ad');
is('Open the crandy voice chat', 'Open the crundi voice chat');
// Plain English stays as heard.
for (const t of ['The crew went to the same place.', 'Deploy it to the cloud.', 'Some CVs were sent.', 'I have a crunchy snack',
  'Look at the sentence again', 'The divorce was sad', 'She wrote a CV', 'Show me the country list.', 'Open my notes.',
  'I said the same thing twice.', 'Open the dhivo ad', 'Hey Cru, open the Crundi project.', 'Is it the same as before?']) is(t, t);
// Without a sam-cv, "same cv" is left alone.
ok(fixNames('Restart the same cv service', { projects: ['crundi'] }) === 'Restart the same cv service', 'no sam-cv: "same cv" stays');
ok(nameList({ projects: ['notes', 'payment-gateway'] }).every((n) => !/notes|payment/.test(n.text)), 'names made of English words are not matched by sound');

// The live stream puts them back too, in the words so far and the final words.
const asm = createAssembler({ names: nameList({ projects, services }) });
const said = (text, final) => asm.push({ type: 'Results', is_final: final, start: 0, duration: 1, channel: { alternatives: [{ transcript: text }] } });
const mid = said('Restart the same cv', false);
ok(mid && mid.text === 'Restart the sam cv', 'interim words are fixed');
said('Restart the same cv service.', true);
const end = asm.push({ type: 'UtteranceEnd' });
ok(end && end.type === 'utterance' && end.text === 'Restart the sam cv service.', 'the utterance is fixed');

console.log(failed ? `\n${failed} FAILED` : '\nAll name checks passed');
process.exit(failed ? 1 : 0);
