// Notes (run by test/planning.mjs), ported from native-gpui/test/planning/notes.mjs:
// the Notes tab (page list + the page), the editor's blocks and keys, the
// menus, the format bar, the block selection, saving and live changes, and
// Notes panes in the workbench (the picker, one page one pane). With v2's
// checks: a long page draws only the blocks on screen, and the idle tab
// draws no frames.

const N = (p) => (p.areas?.planning ?? p.planningArea)?.notes;
const at = (p, key) => N(p)?.places?.find((x) => x.key === key);
const tabPage = (p) => at(p, '');
const blocks = (p, key = '') => at(p, key)?.blocks || [];
const renders = (p, v) => p.renders?.[v] ?? 0;

export async function run(T, t) {
  // On a failed wait, what the screen held (the harness keeps it short).
  const until = T.until.bind(T);
  T.until = async (what, test, ms) => {
    const r = await until(what, test, ms);
    if (!r) { const p = await T.props(); console.log('   notes:', JSON.stringify({ ...N(p), places: undefined, tab: undefined }).slice(0, 1500), JSON.stringify(N(p)?.places?.map((x) => ({ ...x, blocks: x.blocks.map((b) => b.kind + ':' + b.src.slice(0, 20)) }))).slice(0, 2500), 'toast:', p.toast); }
    return r;
  };
  const snap = async (name) => { await t.sleep(300); await T.snap('planning', name); };
  const api = (body) => T.api.http('POST', '/api/notes', { project: 'demo', clientId: 'test-other', ...body });
  const page = async (id) => (await T.api.http('GET', `/api/notes/page?project=demo&id=${id}`)).body?.page;

  // ── The tab, empty ──
  await T.ui('tab', 'notes');
  await T.until('the tab with no pages says how to start', (p) => N(p)?.tab?.project && N(p).tab.listMsg === 'No pages yet. Start one with New page.'
    && tabPage(p)?.mode === 'empty' && tabPage(p).msg === 'No pages yet. Start one with New page.');
  await snap('notes-empty');

  // ── Pages from the server arrive live; the newest opens ──
  const ideas = (await api({ action: 'create', title: 'Ideas', blocks: [{ type: 'p', text: 'A first thought' }] })).body.page;
  await t.sleep(30);
  const plan = (await api({ action: 'create', title: 'Plan', blocks: [
    { type: 'h1', text: 'The plan' },
    { type: 'p', text: 'Some <b>bold</b>, <i>italic</i>, <u>under</u>, <s>gone</s>, <code>code</code> and <a href="https://example.com">a link</a>.' },
    { type: 'todo', text: 'Done already', checked: true },
    { type: 'todo', text: 'Still to do' },
    { type: 'bullet', text: 'A point' },
    { type: 'number', text: 'First' },
    { type: 'number', text: 'Second' },
    { type: 'quote', text: 'Said someone' },
    { type: 'code', text: 'const x = 1;\nconsole.log(x);', lang: 'javascript' },
    { type: 'table', rows: [['Name', 'Value'], ['a', '1']], header: true },
    { type: 'divider' },
    { type: 'p', text: '' },
  ] })).body.page;
  await T.until('the list shows both pages, newest first; the first to arrive opened', (p) => N(p).tab.items.map((i) => i.title).join() === 'Plan,Ideas'
    && tabPage(p).mode === 'editor' && tabPage(p).title === 'Ideas' && N(p).tab.items[1].cur);
  await T.act('notes-pick', '', plan.id);
  await T.until('a click on a page opens it', (p) => tabPage(p).title === 'Plan' && blocks(p).length === 12 && N(p).tab.items[0].cur);
  let p = await T.props();
  const b = blocks(p);
  t.check('the source of formatted text', b[1].src === 'Some **bold**, *italic*, __under__, ~~gone~~, `code` and [a link](https://example.com).', b[1].src);
  t.check('numbered items count', b[5].num === '1.' && b[6].num === '2.', [b[5].num, b[6].num]);
  t.check('code keeps its language', b[8].lang === 'JavaScript', b[8]);
  t.check('the table and its header row', b[9].header && b[9].rows.length === 2 && b[9].rows[0][0] === 'Name', b[9]);
  t.check('the item says when it was edited', /^Edited /.test(N(p).tab.items[0].meta), N(p).tab.items[0]);
  t.check('a saved page says Saved', tabPage(p).saveText === 'Saved' && tabPage(p).save === 'ok', tabPage(p));
  await snap('notes-tab');

  // ── Typing saves (700 ms later), as HTML ──
  await T.act('notes-type', '', '11', 'Hello **world**');
  await T.until('typing marks the page as being edited', (p) => tabPage(p).saveText === 'Editing…' || tabPage(p).saveText === 'Saving…' || tabPage(p).saveText === 'Saved');
  await T.until('then it saves', (p) => tabPage(p).saveText === 'Saved' && !N(p).tabPage?.dirty);
  let srv = await page(plan.id);
  t.check('the server has it as HTML', srv.blocks[11].text === 'Hello <strong>world</strong>', srv.blocks[11]);

  // ── Markdown shortcuts and Enter ──
  await T.act('notes-type', '', '11', '## ');
  await T.until('"## " makes a heading', (p) => blocks(p)[11].kind === 'h2' && blocks(p)[11].src === '');
  await T.act('notes-type', '', '11', 'Next steps');
  await T.act('notes-add-below', '', '11');
  await T.until('+ adds a line and opens the slash menu', (p) => N(p).float === 'slash' && N(p).menu?.label === 'Turn this into, or add' && blocks(p).length === 13
    && N(p).menu.items.map((i) => i.name).join() === 'Text,Heading 1,Heading 2,Heading 3,To-do,Bulleted list,Numbered list,Quote,Code,Table,Divider');
  await snap('notes-slash');
  await T.act('notes-menu-pick', 'todo');
  await T.until('the slash menu turns it into a to-do', (p) => blocks(p)[12].kind === 'todo' && !N(p).menu);
  await T.act('notes-type', '', '12', 'abcdef');
  await T.act('notes-key', '', '12', 'enter', '3', '3', 'true', 'true');
  await T.until('Enter splits a to-do into two', (p) => blocks(p)[12].src === 'abc' && blocks(p)[13].src === 'def' && blocks(p)[13].kind === 'todo');
  await T.act('notes-key', '', '13', 'backspace', '0', '0', 'true', 'true');
  await T.until('Backspace at the start of a to-do makes it text', (p) => blocks(p)[13].kind === 'p');
  await T.act('notes-key', '', '13', 'backspace', '0', '0', 'true', 'true');
  await T.until('then joins it to the line above', (p) => blocks(p)[12].src === 'abcdef' && blocks(p).length === 13);
  // "/" typed: the menu filters as you type.
  await T.act('notes-type', '', '12', 'abcdef /', '8');
  await T.until('"/" after a space opens the slash menu', (p) => N(p).float === 'slash');
  await T.act('notes-type', '', '12', 'abcdef /co', '10');
  await T.until('"/co" filters the slash menu', (p) => N(p).float === 'slash' && N(p).menu?.label === 'Blocks matching “co”' && N(p).menu.items.map((i) => i.id).join() === 'code');
  await T.act('notes-menu-key', 'escape');
  await T.until('Escape closes it', (p) => !N(p).menu);
  await T.act('notes-type', '', '12', 'abcdef');

  // ── The format bar: bold over a selection, a link ──
  await T.act('notes-fmt', '', '12', '0', '3');
  await T.until('selected text shows the format bar', (p) => N(p).float === 'fmt' && N(p).fmt && !N(p).fmt.link);
  await snap('notes-format');
  await T.act('notes-fmt-act', 'bold');
  await T.until('B makes it bold', (p) => blocks(p)[12].src === '**abc**def');
  await T.act('notes-fmt', '', '12', '7', '10');
  await T.act('notes-fmt-act', 'link');
  await T.until('Link asks for the address', (p) => N(p).float === 'link' && N(p).fmt?.link);
  await T.act('notes-link', 'example.org');
  await T.until('the link is applied (https added)', (p) => blocks(p)[12].src === '**abc**[def](https://example.org)');

  // ── The grip's menu ──
  await T.act('notes-grip', '', '4');
  await T.until('the block menu: Turn into, then move, duplicate, delete', (p) => N(p).float === 'block' && N(p).menu?.label === 'Turn into'
    && N(p).menu.items.map((i) => i.name).slice(-4).join() === 'Move up,Move down,Duplicate,Delete' && N(p).menu.items.at(-1).danger
    && !N(p).menu.items.some((i) => i.name === 'Bulleted list') && blocks(p)[4].menuOpen);
  await snap('notes-block-menu');
  await T.act('notes-menu-pick', 'dup');
  await T.until('Duplicate copies the block below', (p) => blocks(p)[5].src === 'A point' && blocks(p)[5].kind === 'bullet' && blocks(p).length === 14);
  await T.act('notes-grip', '', '5');
  await T.act('notes-menu-pick', 'del');
  await T.until('Delete removes it', (p) => blocks(p).length === 13 && blocks(p)[5].kind === 'number');
  await T.act('notes-move', '', '7', '0');
  await T.until('a block moved by its grip lands where dropped', (p) => blocks(p)[0].kind === 'quote' && blocks(p)[1].kind === 'h1');
  await T.act('notes-move', '', '0', '8');
  await T.until('and back', (p) => blocks(p)[7].kind === 'quote');

  // ── The block selection ──
  await T.act('notes-select', '', '2', '4');
  await T.until('three blocks selected, the bar says so', (p) => tabPage(p).selCount === 3 && blocks(p).filter((x) => x.sel).length === 3);
  await snap('notes-selection');
  await T.act('notes-sel-act', '', 'copy');
  await T.until('Copy puts them on the clipboard as Markdown', (p) => (p.clipboard ?? N(p).clipboard) === '- [x] Done already\n- [ ] Still to do\n- A point');
  await T.act('notes-sel-key', '', 'escape');
  await T.until('Escape ends the selection', (p) => tabPage(p).selCount === 0);

  // ── Tables and code ──
  await T.act('notes-table', '', '9', 'add-col');
  await T.until('+ Column adds a column', (p) => blocks(p)[9].rows[0].length === 3);
  await T.act('notes-cell', '', '9', '1', '2', 'two');
  await T.until('a cell takes text', (p) => blocks(p)[9].rows[1][2] === 'two');
  await T.act('notes-table', '', '9', 'header');
  await T.until('Header row toggles', (p) => !blocks(p)[9].header);
  await T.act('notes-code', '', '8', 'lang');
  await T.until('the language list, the current one ticked', (p) => N(p).float === 'lang' && N(p).menu?.queryOn && N(p).menu.items.find((i) => i.check)?.name === 'JavaScript');
  await snap('notes-lang');
  await T.act('notes-menu-query', 'pyt');
  await T.until('it filters', (p) => N(p).menu?.items.map((i) => i.name).join() === 'Python');
  await T.act('notes-menu-key', 'enter');
  await T.until('Enter picks it', (p) => blocks(p)[8].lang === 'Python' && !N(p).menu);
  await T.act('notes-code', '', '8', 'wrap');
  await T.until('wrap long lines', (p) => blocks(p)[8].wrap);
  await T.until('all of it saved', (p) => tabPage(p).saveText === 'Saved');
  srv = await page(plan.id);
  t.check('the server has the code language and the table', srv.blocks[8].lang === 'python' && srv.blocks[8].wrap && srv.blocks[9].rows[1][2] === 'two' && !srv.blocks[9].header, [srv.blocks[8], srv.blocks[9]]);

  // ── The text size (all notes) ──
  await T.act('notes-font', '1');
  await T.act('notes-font', '1');
  await T.until('A+ makes the text larger', (p) => N(p).font === 17 && N(p).scale > 1.1);
  await T.act('notes-font', '0');
  await T.until('reset', (p) => N(p).font === 15);

  // ── Live: a save from elsewhere reloads the page ──
  srv = await page(plan.id);
  await api({ action: 'save', id: plan.id, title: 'Plan, renamed', blocks: srv.blocks, baseVersion: srv.version });
  await T.until('a change from elsewhere shows live', (p) => tabPage(p).title === 'Plan, renamed' && N(p).tab.items.some((i) => i.title === 'Plan, renamed'));

  // ── Trash ──
  await T.act('notes-item-menu', ideas.id);
  await T.until('a page\'s menu: Move to trash', (p) => N(p).float === 'item' && N(p).menu?.items[0]?.name === 'Move to trash');
  await snap('notes-item-menu');
  await T.act('notes-menu-pick', 'trash');
  await T.until('it leaves the list', (p) => N(p).tab.items.length === 1);
  await T.act('notes-view', 'trash');
  await T.until('the Trash lists it', (p) => N(p).tab.view === 'trash' && N(p).tab.items.map((i) => i.title).join() === 'Ideas');
  await snap('notes-trash');
  await T.act('notes-trash', 'restore', ideas.id);
  await T.until('Restore brings it back', (p) => N(p).tab.listMsg === 'The trash is empty.');
  await T.act('notes-view', 'pages');
  await T.until('both pages again', (p) => N(p).tab.items.length === 2);

  // ── A long page: only the blocks on screen are drawn ──
  const long = (await api({ action: 'create', title: 'Long', blocks: Array.from({ length: 400 }, (_, i) => ({ type: 'p', text: `Line ${i + 1}` })) })).body.page;
  await T.until('the long page arrives', (p) => N(p).tab.items.length === 3);
  await T.act('notes-pick', '', long.id);
  await T.until('it opens, all 400 blocks in the model', (p) => tabPage(p).title === 'Long' && blocks(p).length === 400 && tabPage(p).saveText === 'Saved');
  // (Another page opened: the last one's caret went with it, nothing blinks.
  // Settled first: the live events of the page's arrival are handled.)
  await t.sleep(2500);
  const rb = (await T.props()).renders;
  const idle = T.frames.mark();
  await t.sleep(1500);
  const ra = (await T.props()).renders;
  const notesDrawn = ['notes', 'notes-block', 'notes-side', 'notes-bar'].filter((k) => (ra[k] ?? 0) !== (rb[k] ?? 0));
  t.check('the idle Notes tab draws no frames and renders none of its views', T.frames.since(idle).frames === 0 && notesDrawn.length === 0, { f: T.frames.since(idle), notesDrawn, changed: Object.keys(ra).filter((k) => ra[k] !== (rb[k] ?? 0)), toast: (await T.props()).toast });
  const r0 = renders(await T.props(), 'notes-block');
  await T.act('notes-type', '', '0', 'Line one');
  await T.until('an edit to the first block', (p) => blocks(p)[0].src === 'Line one');
  await t.sleep(400);
  const r1 = renders(await T.props(), 'notes-block');
  t.check('an edit redraws only the blocks on screen (not all 400)', r1 - r0 > 0 && r1 - r0 < 120, { r0, r1 });
  await T.until('the edit saved', (p) => tabPage(p).saveText === 'Saved');
  await api({ action: 'delete', id: long.id });
  await api({ action: 'purge', id: long.id });
  await T.until('the long page goes', (p) => N(p).tab.items.length === 2);

  // ── New page from the tab ──
  await T.act('notes-new', '');
  await T.until('+ New page opens an empty page, the caret in its title', (p) => N(p).tab.items.length === 3 && tabPage(p).title === '' && tabPage(p).focus === 'title' && blocks(p).length === 1);
  await T.act('notes-title', '', 'Fresh');
  await T.until('its title goes into the list', (p) => N(p).tab.items.some((i) => i.title === 'Fresh' && i.cur));
  await T.until('and is saved', (p) => tabPage(p).saveText === 'Saved');

  // ── Notes panes: the picker, then a page; one page, one pane ──
  await T.ui('add-pane', 'notes');
  await T.until('a Notes pane', (p) => p.panes?.includes('panel:notes:1'));
  await T.ui('tab', 'workbench');
  await T.until('leaving the tab lets its page go; the pane shows the picker', (p) => !N(p).tab.open && at(p, 'panel:notes:1')?.mode === 'picker'
    && at(p, 'panel:notes:1').items.length === 3 && at(p, 'panel:notes:1').headTitle === 'Notes');
  await snap('notes-picker');
  await T.act('notes-pick', 'panel:notes:1', ideas.id);
  await T.until('picking opens the page in the pane', (p) => at(p, 'panel:notes:1')?.mode === 'editor' && at(p, 'panel:notes:1').title === 'Ideas'
    && at(p, 'panel:notes:1').headTitle === 'Ideas' && blocks(p, 'panel:notes:1')[0]?.src === 'A first thought');
  await T.until('the pane header shows the page\'s title', (p) => p.heads?.['panel:notes:1'] === 'Ideas');
  await snap('notes-pane');
  await T.ui('add-pane', 'notes');
  await T.until('a second pane: the page open in the first is busy', (p) => at(p, 'panel:notes:2')?.items.find((i) => i.id === ideas.id)?.busy
    && at(p, 'panel:notes:2').items.find((i) => i.id === ideas.id).meta === 'Open in another pane');
  await T.act('notes-pick', 'panel:notes:2', ideas.id);
  await T.until('it cannot be opened twice', (p) => p.toast === 'That page is open in another pane' && at(p, 'panel:notes:2').mode === 'picker');
  await T.act('notes-pages', 'panel:notes:1');
  await T.until('Pages takes the first pane back to its picker, freeing the page', (p) => at(p, 'panel:notes:1').mode === 'picker'
    && !at(p, 'panel:notes:2').items.find((i) => i.id === ideas.id)?.busy);
  await T.ui('tab', 'notes');
  await T.until('back on the tab, the last page opens again', (p) => N(p).tab.open && tabPage(p).mode === 'editor' && tabPage(p).title === 'Fresh');
}
