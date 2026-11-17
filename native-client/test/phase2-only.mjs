#!/usr/bin/env node
// Run some sections of phase2.mjs alone (its checks unchanged): the
// sections sit one after another in one file, so a section late in it
// otherwise runs only after every earlier one. This builds a script from
// phase2.mjs: its setup (sign-in, the project, the two chats), every
// top-level helper (function and arrow definitions, upper-case constants,
// constants from the environment or literals),
// and the chosen sections' blocks, then runs it.
//
//   PHASE2_ONLY=2.13,2.14 CRUNDI_TEST_SERVER=… NATIVE_BIN=… node native-client/test/phase2-only.mjs
//
// A section is named by its header ("// ─── 2.13 Questions ───" is 2.13;
// "2.17 Suggestions and recaps; 2.21 …" is 2.17). Sections without a block
// of their own (2.1-2.3, 2.5, 2.11, 2.12 run at the top level) can't be
// chosen; their helpers are kept. A section that reads a value an earlier
// section made at the top level fails with a ReferenceError: run that
// section with it, or in the full phase2.mjs.
import { readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const here = dirname(fileURLToPath(import.meta.url));
const want = (process.env.PHASE2_ONLY || '').split(',').map((s) => s.trim()).filter(Boolean);
if (!want.length) { console.error('Set PHASE2_ONLY (e.g. 2.13,2.14)'); process.exit(2); }
const lines = readFileSync(join(here, 'phase2.mjs'), 'utf8').split('\n');

// The setup: everything before the first section (2.1).
const first = lines.findIndex((l) => /^\/\/ ─── 2\.1 /.test(l));
const out = lines.slice(0, first);
// The GPUI client's per-chat fields (the log's, the composer's) are under
// props.log.panes[<chat>] and props.composer.panes[<chat>] until the
// workbench merges them into each pane (shared/requests.md): merged here.
const PROPS = "const props = () => d.send({ cmd: 'props' });";
const at = out.indexOf(PROPS);
if (at >= 0) out[at] = "const props = async () => { const p = await d.send({ cmd: 'props' }); for (const x of p.panes || []) { const id = (x.key || '').replace(/^live:/, ''); Object.assign(x, p.composer?.panes?.[id] || {}, p.log?.panes?.[id] || {}); } return p; };";

// Braces per line, outside strings and comments (good enough for this file).
const depthOf = (l) => {
  let d = 0, q = null;
  for (let i = 0; i < l.length; i++) {
    const c = l[i];
    if (q) { if (c === '\\') { i++; continue; } if (c === q) q = null; continue; }
    if (c === '/' && l[i + 1] === '/') break;
    if (c === "'" || c === '"' || c === '`') { q = c; continue; }
    if (c === '{' || c === '(' || c === '[') d++;
    if (c === '}' || c === ')' || c === ']') d--;
  }
  return d;
};
const isHelper = (l) => /^(async )?function /.test(l) || /^const \w+ = (async )?(\([^()]*\)|\w+) =>/.test(l) || /^const [A-Z][A-Z0-9_]* = /.test(l) || (/^const \w+ = (.*process\.env|['"`\d])/.test(l) && !/\bawait\b/.test(l)) || /^const \{ \w+(, \w+)* \} = await import\(/.test(l);

let section = null, chosen = false;
for (let i = first; i < lines.length; i++) {
  const l = lines[i];
  const h = /^\/\/ ─── (\S+) /.exec(l);
  if (h) { section = h[1]; chosen = want.includes(section); }
  // A section's block: from its "{" at the start of a line to the matching "}".
  if (l === '{') {
    let d = 0, j = i;
    for (; j < lines.length; j++) { d += depthOf(lines[j]); if (d === 0) break; }
    if (chosen) { out.push(`// ─── ${section} (phase2-only) ───`); out.push(...lines.slice(i, j + 1)); }
    i = j;
    continue;
  }
  // A top-level helper (kept whichever section it sits in).
  if (isHelper(l)) {
    let d = 0, j = i;
    for (; j < lines.length; j++) { d += depthOf(lines[j]); if (d <= 0 && /[;}]\s*$/.test(lines[j])) break; }
    out.push(...lines.slice(i, j + 1));
    i = j;
  }
}
out.push("console.log(failed ? `\\n${failed} check(s) FAILED` : '\\nall checks passed');", 'process.exit(failed ? 1 : 0);');

const tmp = join(here, `.phase2-only-${process.pid}.mjs`);
writeFileSync(tmp, out.join('\n'));
if (process.env.PHASE2_KEEP) { console.log(tmp); process.exit(0); }
const r = spawnSync(process.execPath, [tmp], { stdio: 'inherit', env: process.env });
try { unlinkSync(tmp); } catch { /* gone */ }
process.exit(r.status ?? 1);
