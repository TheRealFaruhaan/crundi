// The web's Notes nesting and block keys (app/vendor/crundi-notes.js), in
// the real web UI in Electron on a stub server: run.sh starts all of it.
const { app, BrowserWindow, clipboard } = require('electron');
const fs = require('fs');
const S = 'http://localhost:' + (process.env.PORT || '4820');
app.setPath('userData', fs.mkdtempSync(require('os').tmpdir() + '/web-notes-ud-'));
const sess = JSON.parse(fs.readFileSync(process.env.SESS, 'utf8'));
const out = process.env.OUT;
let fails = 0;
const ok = (name, c, info) => { console.log((c ? 'ok   ' : 'FAIL ') + name + (c ? '' : '  ' + JSON.stringify(info))); if (!c) fails++; };
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
app.whenReady().then(async () => {
  const w = new BrowserWindow({ width: 1280, height: 860, show: true, webPreferences: { contextIsolation: false } });
  const js = (s) => w.webContents.executeJavaScript(s);
  await w.loadURL(S + '/');
  await js(`localStorage.setItem('crundi_token', ${JSON.stringify(sess.t)}); localStorage.setItem('crundi_refresh', ${JSON.stringify(sess.r)}); localStorage.setItem('crundi_notes_last_demo', ${JSON.stringify(sess.id)}); true`);
  await w.loadURL(S + '/');
  await wait(2500);
  // Dismiss What's new if it's up, pick the project, open the Notes tab.
  await js(`(function(){ var b=[...document.querySelectorAll('button')].find(x=>/^(Skip|Done)$/.test(x.textContent.trim())); if(b) b.click(); return true; })()`);
  await wait(400);
  await js(`(function(){ var p=[...document.querySelectorAll('[data-alias="demo"],[data-project="demo"]')][0]; if(p) p.click(); return !!p; })()`).then((r) => console.log('project click', r));
  await wait(800);
  for (let i = 0; i < 20; i++) { const sk = await js(`(function(){ var b=[...document.querySelectorAll('button')].find(x=>/^Skip/.test(x.textContent.trim())); if(b){ b.click(); return true;} return false; })()`); if (sk) { console.log('skipped tour'); break; } await wait(300); }
  await wait(500);
  await js(`document.querySelector('.tab-btn[data-tab="notes"]').click(); true`);
  await wait(1500);
  const state = () => js(`[...document.querySelectorAll('.nt-tabhost .nt-block')].map(b=>({t:b.dataset.type,l:+(b.dataset.level||0),x:(b.querySelector('.nt-text')||{}).textContent,m:(b.querySelector('.nt-mark')||{}).textContent||'',pl:getComputedStyle(b).paddingLeft}))`);
  let s = await state();
  console.log(JSON.stringify(s));
  ok('nested blocks load with their levels', s.map((b) => b.l).join() === '0,1,2,2,1,0,0', s);
  ok('each level 28px further in', s[1].pl === '28px' && s[2].pl === '56px', s);
  ok('numbers count per level', s[2].m === '1.' && s[3].m === '2.', s);
  await w.webContents.capturePage().then((i) => out && fs.writeFileSync(out + '/web-nested.png', i.toPNG()));
  const key = async (keyCode, modifiers = []) => { w.webContents.sendInputEvent({ type: 'keyDown', keyCode, modifiers }); w.webContents.sendInputEvent({ type: 'keyUp', keyCode, modifiers }); await wait(150); };
  const focusText = (i, where) => js(`(function(){ var t=document.querySelectorAll('.nt-tabhost .nt-block')[${i}].querySelector('.nt-text'); t.focus(); var r=document.createRange(); r.selectNodeContents(t); r.collapse(${where === 'start'}); var s=getSelection(); s.removeAllRanges(); s.addRange(r); return true; })()`);
  w.focus(); w.webContents.focus();
  // Tab: "Veg" (5) under the list goes one in; again: only one deeper than the item above.
  await focusText(5, 'end'); await key('Tab'); s = await state();
  ok('Tab nests a list item one level', s[5].l === 1, s[5]);
  await key('Tab'); s = await state();
  ok('Tab goes at most one below the item above', s[5].l === 2, s[5]);
  await key('Tab'); s = await state();
  ok('…and no further', s[5].l === 2, s[5]);
  await key('Tab', ['shift']); s = await state();
  ok('Shift+Tab steps out', s[5].l === 1, s[5]);
  // Enter at the end of a nested item: a new one at the same level; Enter on it empty: one out; again: ends the list.
  await key('Return'); s = await state();
  ok('Enter goes on at the same level', s[6] && s[6].t === 'bullet' && s[6].l === 1, s);
  await key('Return'); s = await state();
  ok('Enter on an empty nested item steps out one level', s[6].t === 'bullet' && s[6].l === 0, s[6]);
  await key('Return'); s = await state();
  ok('…and at level 0 ends the list', s[6].t === 'p' && s.length === 8, s);
  // Tab on a paragraph doesn't nest it.
  await focusText(6, 'end'); await key('Tab'); s = await state();
  ok('a paragraph never nests', s[6].l === 0, s[6]);
  // Alt+Up / Alt+Down move the block, caret kept.
  await focusText(3, 'end'); await key('Up', ['alt']); s = await state();
  ok('Alt+Up moves a block up', s[2].x === 'Green' && s[3].x === 'Red', s);
  ok('…numbers follow', s[2].m === '1.' && s[3].m === '2.', s);
  let act = await js(`document.activeElement && document.activeElement.textContent`);
  ok('…the caret stays in it', act === 'Green', act);
  await key('Down', ['alt']); s = await state();
  ok('Alt+Down moves it back', s[2].x === 'Red' && s[3].x === 'Green', s);
  // Ctrl+D duplicates, level kept, caret into the copy.
  await key('D', ['control']); s = await state();
  ok('Ctrl+D duplicates the block after it, level kept', s[4].x === 'Green' && s[4].l === 2 && s[4].m === '3.', s);
  act = await js(`(function(){ var a=document.activeElement; var b=a&&a.closest('.nt-block'); return [...document.querySelectorAll('.nt-tabhost .nt-block')].indexOf(b); })()`);
  ok('…the caret into the copy', act === 4, act);
  // Ctrl+Shift+K deletes it.
  await key('K', ['control', 'shift']); s = await state();
  ok('Ctrl+Shift+K deletes the block', s.length === 8 && s[4].x === 'Pear', s);
  // Turning a nested bullet into a to-do keeps its level; into a paragraph drops it.
  // Selection: Shift+Down from Apple selects blocks; Alt+Down moves them; Ctrl+D copies.
  await focusText(1, 'end'); await key('Down', ['shift']); await key('Down', ['shift']);
  let sel = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block.bsel')].map(b=>b.querySelector('.nt-text').textContent)`);
  ok('Shift+Down selects blocks', sel.join() === 'Apple,Red,Green', sel);
  await key('Down', ['alt']); s = await state();
  ok('Alt+Down moves the selection as one', s.map((b) => b.x).slice(1, 5).join() === 'Pear,Apple,Red,Green', s);
  sel = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block.bsel')].map(b=>b.querySelector('.nt-text').textContent)`);
  ok('…and it stays selected', sel.join() === 'Apple,Red,Green', sel);
  await key('Up', ['alt']);
  await key('D', ['control']); s = await state();
  sel = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block.bsel')].map(b=>b.dataset.id)`);
  ok('Ctrl+D on a selection copies it after, levels kept', s.map((b) => b.x + b.l).slice(1, 8).join() === 'Apple1,Red2,Green2,Apple1,Red2,Green2,Pear1', s);
  ok('…the copies selected', sel.length === 3 && !sel.includes('a2'), sel);
  await key('K', ['control', 'shift']); s = await state();
  ok('Ctrl+Shift+K deletes the selection', s.map((b) => b.x).slice(1, 5).join() === 'Apple,Red,Green,Pear', s);
  // Copy as Markdown: nested items indented, numbers per level.
  await focusText(0, 'end'); await key('A', ['control', 'shift']); await key('C', ['control']); await wait(300);
  const md = clipboard.readText();
  console.log(JSON.stringify(md));
  ok('Markdown indents nested items, numbers per level', md.startsWith('- Fruit\n  - Apple\n    1. Red\n    2. Green\n  - [x] Pear'), md);
  await key('Escape');
  // Turn into (the block menu): bullet to to-do keeps its level; to a paragraph drops it.
  await js(`(function(){ var b=document.querySelectorAll('.nt-tabhost .nt-block')[1]; b.querySelector('.nt-grip').click(); var x=[...document.querySelectorAll('.nt-menu button')].find(e=>/To-do/.test(e.textContent)); x.click(); return true; })()`);
  s = await state();
  ok('Turn into To-do keeps the level', s[1].t === 'todo' && s[1].l === 1, s[1]);
  await js(`(function(){ var b=document.querySelectorAll('.nt-tabhost .nt-block')[1]; b.querySelector('.nt-grip').click(); var x=[...document.querySelectorAll('.nt-menu button')].find(e=>/^.?Text$/.test(e.textContent.trim()) || /Text/.test(e.textContent)); x.click(); return true; })()`);
  s = await state();
  ok('Turn into Text drops it', s[1].t === 'p' && s[1].l === 0, s[1]);
  await js(`(function(){ var b=document.querySelectorAll('.nt-tabhost .nt-block')[1]; b.querySelector('.nt-grip').click(); var x=[...document.querySelectorAll('.nt-menu button')].find(e=>/Bulleted|Bullet/.test(e.textContent)); x.click(); return true; })()`);
  // A code block takes the block keys too (before CodeMirror's line moves).
  await js(`(function(){ var b=document.querySelectorAll('.nt-tabhost .nt-block')[6]; b.querySelector('.nt-grip').click(); var x=[...document.querySelectorAll('.nt-menu button')].find(e=>/Code/.test(e.textContent)); x.click(); return true; })()`);
  await wait(300);
  let before = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block')].map(b=>b.dataset.type)`);
  let ci = before.indexOf('code');
  await js(`(function(){ var b=document.querySelectorAll('.nt-tabhost .nt-block')[${ci}]; b._cm ? b._cm.view.focus() : 0; return true; })()`);
  await key('Up', ['alt']);
  let after = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block')].map(b=>b.dataset.type)`);
  ok('Alt+Up in a code block moves the block', after.indexOf('code') === ci - 1, { before, after });
  await key('D', ['control']);
  after = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block')].map(b=>b.dataset.type)`);
  ok('Ctrl+D in a code block duplicates it', after.filter((t) => t === 'code').length === 2, after);
  await key('K', ['control', 'shift']);
  after = await js(`[...document.querySelectorAll('.nt-tabhost .nt-block')].map(b=>b.dataset.type)`);
  ok('Ctrl+Shift+K in a code block deletes it', after.filter((t) => t === 'code').length === 1, after);
  s = await state();
  // Saved with levels.
  await wait(2500);
  const page = await js(`fetch('/api/notes/page?project=demo&id=${sess.id}',{headers:{Authorization:'Bearer ${sess.t}'}}).then(r=>r.json())`);
  const bl = page.page.blocks;
  ok('saved with "level" (never flattened)', bl.map((b) => b.level || 0).join() === s.map((b) => b.l).join(), bl);
  // Markdown of the page.
  await w.webContents.capturePage().then((i) => out && fs.writeFileSync(out + '/web-after.png', i.toPNG()));
  console.log(fails ? fails + ' failed' : 'all passed');
  app.exit(fails ? 1 : 0);
});
