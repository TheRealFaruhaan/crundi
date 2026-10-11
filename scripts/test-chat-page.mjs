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
ok(scripts.length === 4 && scripts.every((m) => /\bsrc="\/vendor\/[a-z-]+\.js\?v=tag-[a-z-]+\.js"/.test(m[1]) && m[2] === ''), 'every script is a file, with a version on its address; none is written into the page');
ok(html.indexOf('claude-chat.js') < html.indexOf('crundi-chat-page.js'), 'the chat component loads before the page that hosts it');
ok(!/\son[a-z]+=/.test(html.replace(/<style>[\s\S]*?<\/style>/, '')), 'and no handler is written into the markup');
for (const id of ['cp-back', 'cp-mark', 'cp-title', 'cp-proj', 'cp-more', 'cp-chat', 'cp-note', 'cp-menu']) ok(html.includes(`id="${id}"`), 'the page has its ' + id);
ok(getChatPageHtml({ version: '"><script>x</script>', rootCss: '', tag: () => '"><b>' }).match(/<script\b/g).length === 4, 'nothing passed in can add markup');

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
ok(server.includes("mark: markOf(s.project, s.title)") && server.includes("mark: markOf(term.project || '', term.title)") && !server.includes('mark: chatMark('), 'the chat list and a chat\'s notifications carry the same mark');
ok(/function markOf\(project, title\) \{\s*const mark = chatMark\(/.test(server) && server.includes('return icon ? { ...mark, icon } : mark;'), 'a mark always has its letters and colour, with the project\'s icon added where it has one');
ok(js.includes("m.icon ? markIcon(m.icon) : ''") && js.includes('mark.textContent = m.initials;'), 'the compact chat draws the icon, and the letters until it has it or when it cannot');
ok(/const about = chat \? \{ chat, url: `\/chat\/\$\{term\.id\}` \} : \{\};/.test(server), 'a chat\'s notifications say which chat, and point at its page');
const cv = await import('../src/client-version.js');
ok(cv.isGated('/chat/abc') === false, 'a client too old for the server can still load the page that tells it so');
// What Claude shows in a chat is drawn the same in both pages.
const widgets = readFileSync(join(root, 'app', 'vendor', 'crundi-widgets.js'), 'utf8');
ok(widgets.includes('function toolMedia(sessionId, entry)') && widgets.includes('toolMedia: toolMedia'), 'pictures, files and frames Claude shows are parsed in one shared place');
ok(main.includes('return window.CrundiWidgets.toolMedia(t.id, entry);') && !main.includes('const EMBED = /^[0-9a-f]{16}[.]embed$/'), 'the workbench uses it, and keeps no copy of its own');
ok(html.includes('/vendor/crundi-widgets.js') && js.includes('toolExtra: toolExtra') && js.includes('W.toolMedia(sid, entry)'), 'the compact chat loads it and draws them too: a file offered there used to be invisible');
ok(widgets.includes("cfg.fileIcon(it.filename || 'file') : IC.file") && /file: svg\(/.test(widgets), 'a file offered in the compact chat is drawn as a file, not as the panels icon');
ok(!/handleEvent\(d\);\s*dock\(\)/.test(js), 'a panel event is not docked from stale data before the fresh list arrives');
ok(js.includes('return r === undefined ? true : r'), 'a call the app answered with nothing still counts as done, so the page does not do it again');
ok(js.includes("bridge('download', abs") && js.includes("a.download = String(o.name || 'download')"), 'a file is saved by the phone inside the app, and by a plain link in a browser');

// A chat that is being read loses its notification.
ok(main.includes('window.CrundiNative.seen(String(id))') && /shown \/ Math\.min\(r\.height, window\.innerHeight\) >= 0\.4/.test(main), 'the workbench tells the app which chats are really on screen, measured, not assumed');
ok(js.includes("bridge('seen', sid)"), 'and so does the compact chat');

// A secret Claude asks for can be answered from the compact chat.
ok(html.includes('id="cp-ask"') && js.includes("apiFetch('/api/secrets')") && js.includes("action: 'approve', reqId: r.id, pin: v") && js.includes("action: 'deny', reqId: r.id"), 'the compact chat shows a secret request and can approve it with the PIN, or deny it');
ok(js.includes("r.sessionId ? r.sessionId === sid : (!r.project || r.project === meta.project)"), 'it shows the requests of its own chat, and ones no chat is named for');
ok(/if \(sig === askShown\) return;/.test(js), 'and does not rebuild the card under someone typing a PIN');

// The rest of what the workbench gives a chat.
ok(js.includes('W.shotsNode(meta.project, id)') && js.includes('W.inlineNode(meta.project, id)') && js.includes('window.CrundiWidgets.setProject(meta.project'), 'panels Claude places in the chat are drawn in the compact chat too');
ok(html.includes('data-do="plan"') && js.includes("if ('plan' in patch) hasPlan = !!patch.plan;") && js.includes('view.togglePlan()'), 'the plan can be shown, when there is one');
ok(html.includes('data-do="rename"') && js.includes("'/rename'") && !/window\.(prompt|confirm|alert)\(/.test(js), 'and the chat can be renamed, without a dialog a web view in an app would not show');
ok(js.includes("openMain('/chat/' + encodeURIComponent(sid))"), '"Open in Crundi" names the chat in a way the app can act on');

// A panel docked to the chat, and the icons everything is drawn with.
ok(/\.ic\{width:1em;height:1em;/.test(html), 'the compact page sizes icons as the main page does: without it a panel\'s icon filled the bubble');
ok(js.includes("window.CrundiWidgets.syncDock($('cp-body'), sid)") && /#cp-body\{[^}]*display:flex;flex-direction:column/.test(html), 'a panel docked to the chat sits above the conversation, as in the workbench');
ok(js.includes("new EventSource('/api/events?token='") && js.includes("es.addEventListener('widget'") && js.includes("es.addEventListener('secret-requests'"), 'it listens to the server for panel updates and for secret requests as they are raised');

// Going to a chat without loading the page again.
ok(main.includes('window.crundiOpenChat = (sid) =>') && /if \(!done && \+\+tries < 24\) setTimeout\(attempt, 250\);/.test(main), 'the app can ask the loaded page for a chat, and the page keeps trying while its chats are still arriving');

// The phone app's two extras in a chat's menu.
ok(main.includes('data-action="chat-bubble"') && main.includes("case 'chat-bubble':"), 'a chat\'s menu can offer Bubble, and it is wired');
ok(/NATIVE_CAN\.indexOf\('bubble'\) >= 0\) h \+=/.test(main) && main.includes('window.CrundiNative.can'), 'but only where the app round the page says it can: in a browser it is not drawn');
ok(!/chat-pip|openPip|cp-pip|crundiSetPip/.test(main + html + js), 'the floating window is gone: bubbles are the one way a chat floats');
const check = readFileSync(join(root, 'scripts', 'check-syntax.mjs'), 'utf8');
ok(check.includes("'crundi-chat-page.js'"), 'the page\'s script is in the syntax check');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
