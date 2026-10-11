#!/usr/bin/env node
/**
 * test-chat-page.mjs — the compact chat at /chat/<id>, and the project mark.
 *
 * The page is what a chat bubble on a phone opens. It is small on purpose and
 * most of what can go wrong with it is structural: a script written into the
 * template (where a stray escape breaks it silently), a palette that drifts
 * from the main page's, a route that does not reach it, or a mark that comes
 * out differently on the phone than on the server.
 *
 * Run: node scripts/test-chat-page.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { getChatPageHtml } from '../src/chat-page.js';
import { projectMark, chatMark, markInitials, markHue } from '../src/project-mark.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── The mark ───
// These exact values are the contract with the phone app's own copy of the
// recipe. If one changes, every bubble head changes colour on every phone.
const PINNED = [
  ['crundi', 'C', 316, '#b93195', '#8c2170'],
  ['dhivo-socials', 'DS', 202, '#3187b9', '#21658c'],
  ['my_cool project', 'MC', 148, '#2da967', '#1e804c'],
  ['a', 'A', 340, '#b9315f', '#8c2145'],
  ['', '?', 61, '#979929', '#72741b'],
];
for (const [name, initials, hue, color, colorDeep] of PINNED) {
  const m = projectMark(name);
  ok(m.initials === initials && m.hue === hue && m.color === color && m.colorDeep === colorDeep, `the mark for ${JSON.stringify(name)} is ${initials} on ${color}`);
}
ok(JSON.stringify(projectMark('Crundi')) === JSON.stringify(projectMark('crundi')) && markHue('CRUNDI') === markHue('crundi'), 'the colour does not depend on how the name is capitalised');
ok(markInitials('日本語 プロジェクト') === '日プ' && markInitials('émile-zola') === 'ÉZ' && markInitials('  --  ') === '?', 'initials work for any script, and a name with no letters still gets a tile');
ok(markInitials('one two three four') === 'OT' && markInitials('x') === 'X', 'at most two letters');
{
  // White letters have to read on every tile there can be.
  const lum = (hex) => { const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4)); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
  let worst = 99;
  for (let i = 0; i < 400; i++) { const m = projectMark('project-' + i); worst = Math.min(worst, 1.05 / (lum(m.color) + 0.05)); }
  ok(worst >= 3, `white on the lightest tile there can be still reads (${worst.toFixed(2)} to 1)`);
}

// A chat wears its project's colour and its own letters.
{
  const a = chatMark('crundi', 'Fix login redirect'), b = chatMark('crundi', 'Add tests'), other = chatMark('dhivo-socials', 'Fix login redirect');
  ok(a.initials === 'FL' && b.initials === 'AT' && a.color === b.color && a.color === projectMark('crundi').color, 'two chats in one project share its colour and differ by their initials');
  ok(other.initials === 'FL' && other.color !== a.color, 'the same title in another project is another colour');
  ok(chatMark('crundi', 'Chat').initials === 'C' && chatMark('crundi', '').initials === 'C' && chatMark('crundi', '  ---  ').initials === 'C', 'a chat nobody has named yet wears the project\'s letters');
  ok(chatMark('crundi', 'Android app scoping').initials === 'AA', 'the letters come from the first two words of the title');
}

// ─── The page ───
const main = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
const rootCss = /:root\s*\{[^}]*\}/.exec(main)[0];
const html = getChatPageHtml({ version: '1.20.0-dev.1', rootCss, tag: (f) => 'tag-' + f });
ok(html.startsWith('<!DOCTYPE html>') && html.includes('data-version="1.20.0-dev.1"'), 'the page says which server version built it');
ok(html.includes('--bg-primary: #0a0a0f') && html.includes('--accent: #6366f1'), 'it carries the main page\'s palette, taken from the main page');
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
ok(scripts.length === 3 && scripts.every((m) => /\bsrc="\/vendor\/[a-z-]+\.js\?v=tag-[a-z-]+\.js"/.test(m[1]) && m[2] === ''), 'every script is a file, with a version on its address; none is written into the page');
ok(html.indexOf('claude-chat.js') < html.indexOf('crundi-chat-page.js'), 'the chat component loads before the page that hosts it');
ok(!/\son[a-z]+=/.test(html.replace(/<style>[\s\S]*?<\/style>/, '')), 'and no handler is written into the markup');
for (const id of ['cp-back', 'cp-mark', 'cp-title', 'cp-proj', 'cp-more', 'cp-chat', 'cp-note', 'cp-menu']) ok(html.includes(`id="${id}"`), 'the page has its ' + id);
ok(getChatPageHtml({ version: '"><script>x</script>', rootCss: '', tag: () => '"><b>' }).match(/<script\b/g).length === 3, 'nothing passed in can add markup');
ok(html.includes('body.cp-pip #cp-head') && html.includes('.cc-composer'), 'picture in picture hides the header and the message box');

// ─── Its script ───
const js = readFileSync(join(root, 'app', 'vendor', 'crundi-chat-page.js'), 'utf8');
ok(js.includes("window.CrundiChat.mount($('cp-chat')") && js.includes("type: 'presence'"), 'the script mounts the same chat component the workbench uses, and reports presence');
ok(js.includes("opts.headers['X-Crundi-Web'] = VERSION") && js.includes('res.status === 426'), 'it sends its version and handles being refused for it');
ok(/'crundi_token'/.test(js) && /'crundi_refresh'/.test(js), 'it uses the main page\'s sign-in, so a signed-in browser is signed in here');
for (const fn of ['session', 'saveSession', 'close', 'openMain', 'chatState']) ok(js.includes(`bridge('${fn}'`), `the app's bridge is asked for ${fn} by name, and is optional`);
ok(!/innerHTML\s*=\s*[^'"]/.test(js.replace(/el\.textContent = ''/g, '')), 'nothing from the server is written as markup');

// ─── Wired in ───
const server = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
const routeAt = server.indexOf('html(res, chatPageHtml())');
ok(routeAt > 0 && routeAt < server.indexOf('// ─── PWA: manifest + service worker ───'), 'the server serves the page at /chat/<id>, before anything that needs a login');
ok(/\^\\\/chat\\\/\[A-Za-z0-9_-\]\{1,80\}/.test(server), 'only for an address shaped like a chat id');
ok(server.includes("mark: chatMark(s.project, s.title)") && server.includes("mark: chatMark(term.project || '', term.title)"), 'the chat list and a chat\'s notifications carry the same mark');
ok(/const about = chat \? \{ chat, url: `\/chat\/\$\{term\.id\}` \} : \{\};/.test(server), 'a chat\'s notifications say which chat, and point at its page');
const cv = await import('../src/client-version.js');
ok(cv.isGated('/chat/abc') === false, 'a client too old for the server can still load the page that tells it so');
// The phone app's two extras in a chat's menu.
ok(main.includes('data-action="chat-bubble"') && main.includes('data-action="chat-pip"') && main.includes("case 'chat-bubble':") && main.includes("case 'chat-pip':"), 'a chat\'s menu can offer Bubble and Float, and both are wired');
ok(/NATIVE_CAN\.indexOf\('bubble'\) >= 0\) h \+=/.test(main) && /NATIVE_CAN\.indexOf\('pip'\) >= 0\) h \+=/.test(main) && main.includes('window.CrundiNative.can'), 'but only where the app round the page says it can: in a browser neither is drawn');
const check = readFileSync(join(root, 'scripts', 'check-syntax.mjs'), 'utf8');
ok(check.includes("'crundi-chat-page.js'"), 'the page\'s script is in the syntax check');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
