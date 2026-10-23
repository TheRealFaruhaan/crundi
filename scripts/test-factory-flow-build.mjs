#!/usr/bin/env node
/**
 * test-factory-flow-build.mjs — the factory editor bundle is current.
 *
 * app/vendor/factory-flow.js is built (tools/factory-flow) and committed. It
 * carries its own copy of the validator and the engine, so if those change
 * without a rebuild the editor would disagree with the server about what is
 * valid. The build stamps a hash of every file it was built from; this check
 * recomputes it.
 *
 * Run: node scripts/test-factory-flow-build.mjs
 */
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sourceHash } from '../tools/factory-flow/src-hash.mjs';

const root = join(fileURLToPath(new URL('.', import.meta.url)), '..');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const js = join(root, 'app', 'vendor', 'factory-flow.js');
const css = join(root, 'app', 'vendor', 'factory-flow.css');
ok(existsSync(js) && existsSync(css), 'the editor bundle and its stylesheet exist');
const text = existsSync(js) ? readFileSync(js, 'utf8') : '';
ok(/CrundiFactoryFlow/.test(text) && /mountEditor/.test(text), 'it defines window.CrundiFactoryFlow.mountEditor');
const hash = sourceHash();
ok(text.includes(hash), `it was built from the current sources (${hash}); if not: cd tools/factory-flow && npm install --include=dev && npm run build`);
process.exit(failed ? 1 : 0);
