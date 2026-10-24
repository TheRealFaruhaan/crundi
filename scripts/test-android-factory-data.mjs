// The Android factory editor reads its persona templates, permission presets
// and default change request form from a JSON file made from the server's
// modules (android/tools/gen-factory-data.mjs). This fails when the file is
// out of step with them, so the phone never offers something the web does not.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { DATA_PATH, editorData, text } from '../android/tools/gen-factory-data.mjs';

let have;
try { have = readFileSync(DATA_PATH, 'utf8'); } catch { assert.fail('missing ' + DATA_PATH + ': run node android/tools/gen-factory-data.mjs'); }
assert.equal(have, text(editorData()), 'factory-editor-data.json is out of step with src/: run node android/tools/gen-factory-data.mjs');
const d = JSON.parse(have);
assert.ok(d.personas.length >= 10, 'personas');
assert.ok(d.presets.some(p => p.key === 'tests'), 'presets');
console.log('android factory data: ok (' + d.personas.length + ' personas, ' + d.presets.length + ' presets)');
