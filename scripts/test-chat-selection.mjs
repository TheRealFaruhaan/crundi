// A text selection in a chat stays in the chat.
//
// On a phone: select text, scroll until the start of the selection is above
// the log, drag the end handle. The browser stretches the selection between
// two screen points, and the hidden end's handle is pinned to the edge of the
// log - so the selection jumped to the page header above the chat. And a
// handle dragged to the edge of the log did not scroll it.
//
// The behaviour itself needs a browser (it was driven in headless Chrome by
// reproducing what the phone does to the selection). These checks hold the
// wiring in place, so the pieces cannot be removed one at a time unnoticed.
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const chat = readFileSync(join(root, 'app', 'vendor', 'claude-chat.js'), 'utf8').replace(/\r\n/g, '\n');
let failed = 0;
const ok = (cond, name) => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}`); if (!cond) failed++; };

ok(/html\.cc-selecting \*:not\(\.cc-sel-path\):not\(\.cc-sel-host\):not\(\.cc-sel-host \*\)\{-webkit-user-select:none!important;user-select:none!important\}/.test(chat),
  'while a chat holds the selection, the rest of the page is unselectable');
ok(/n\.classList\.add\('cc-sel-path'\)/.test(chat) && /top\.classList\.add\('cc-selecting'\)/.test(chat),
  'the log\'s ancestors are exempted, or the log would inherit the ban');
ok(/document\.addEventListener\('selectionchange', onSelectionChange\)/.test(chat), 'selection changes are watched');
ok(/var fixS = !selSame\(s, selSaved\.s\) && selHiddenAbove\(selSaved\.s\);/.test(chat)
  && /var fixE = !selSame\(e, selSaved\.e\) && selHiddenBelow\(selSaved\.e\);/.test(chat),
  'an end that is scrolled out of sight is put back if it moves');
ok(/var direct = selTouches > 0 \|\| selMouse \|\|/.test(chat) && /if \(direct\) \{/.test(chat),
  'but not while a finger or the mouse is making the selection directly');
ok(/selAtLogEdge\(s, false\) && selAtLogEdge\(e, true\)/.test(chat), '"Select all" is allowed to move both ends');
ok(/if \(!selMouse && movedS !== movedE\) selAutoScroll\(/.test(chat) && /log\.scrollBy\(\{ top: step, behavior: 'smooth' \}\)/.test(chat)
  && /log\.scrollBy\(\{ top: -step, behavior: 'smooth' \}\)/.test(chat),
  'a handle dragged to the top or bottom edge scrolls the log');
const dispose = chat.slice(chat.indexOf('function selDispose()'), chat.indexOf('function selDispose()') + 700);
ok(/removeEventListener\('selectionchange'/.test(dispose) && /selEnd\(\);/.test(dispose) && /\n\s+selDispose\(\);\n\s+stopTicker\(\);/.test(chat),
  'closing a chat removes the listeners and gives the page back');

console.log(failed ? `\n${failed} selection check(s) failed` : '\nAll chat selection checks passed');
process.exit(failed ? 1 : 0);
