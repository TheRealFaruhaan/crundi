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
  // The real "+" (F56): the pointer on the line shows its "+"; a click adds a
  // line under it and opens the slash menu there, under the new line.
  const p0 = await T.props();
  const b11 = blocks(p0)[11];
  const box11 = tabPage(p0).boxes.find((x) => x.id === b11.id);
  const pg = tabPage(p0).pageBox;
  await T.mouse('move', pg.x + pg.w / 2, box11.y + box11.h / 2);
  const shown = await T.until('the pointer on a line shows its "+"', (p) => tabPage(p).plus.some((x) => x.id === b11.id));
  const plus = tabPage(shown).plus.find((x) => x.id === b11.id);
  await T.mouse('move', plus.x, plus.y);
  await T.mouse('click', plus.x, plus.y);
  const slash = await T.until('+ adds a line and opens the slash menu', (p) => N(p).float === 'slash' && N(p).menu?.label === 'Turn this into, or add' && blocks(p).length === 13
    && N(p).menu.items.map((i) => i.name).join() === 'Text,Heading 1,Heading 2,Heading 3,To-do,Bulleted list,Numbered list,Quote,Code,Table,Divider');
  t.check('the slash menu opens under the new line, by the words (not at the window\'s corner)',
    N(slash).menu.y >= box11.y + box11.h && N(slash).menu.y < box11.y + box11.h + 120 && N(slash).menu.x > pg.x && N(slash).menu.x < pg.x + pg.w / 2,
    { menu: { x: N(slash).menu.x, y: N(slash).menu.y }, line: box11, page: pg });
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

  // ── Editing as a person types (F119): the words formatted, never the source ──
  const E = (p) => tabPage(p)?.edit || {};
  // Where a block's words start: right of its "+" (shown with the pointer on it).
  const wordsAt = async (i) => {
    const p0 = await T.props();
    const id = blocks(p0)[i].id;
    const bx = tabPage(p0).boxes.find((x) => x.id === id);
    await T.mouse('move', tabPage(p0).pageBox.x + tabPage(p0).pageBox.w / 2, bx.y + bx.h / 2);
    const s1 = await T.until(`the pointer on block ${i} shows its "+"`, (p) => tabPage(p).plus.some((x) => x.id === id));
    const pl = tabPage(s1).plus.find((x) => x.id === id);
    return { id, x: pl.x + 40, y: bx.y + bx.h / 2, box: bx };
  };
  const plainOf1 = 'Some bold, italic, under, gone, code and a link.';
  let w1 = await wordsAt(1);
  await T.mouse('click', w1.x + 60, w1.y);
  let e1 = await T.until('a click on formatted text puts the caret in it', (p) => E(p).on && E(p).id === w1.id);
  t.check('the line being edited shows its words, formatted, not its Markdown source', E(e1).text === plainOf1 && E(e1).bold >= 1 && E(e1).decos >= 6, E(e1));
  t.check('the caret lands where the click was, not at the end', E(e1).a > 0 && E(e1).a < plainOf1.length && E(e1).a === E(e1).b, E(e1));
  await snap('notes-editing');
  await T.key('End');
  await T.key('!');
  await T.until('typed text goes into the words; the source follows', (p) => E(p).text === plainOf1 + '!' && blocks(p)[1].src.endsWith('[a link](https://example.com).!'));
  await T.key('Backspace');
  await T.until('Backspace takes it out again', (p) => blocks(p)[1].src.endsWith('[a link](https://example.com).'));
  // Ctrl+B over a selection, then typing at the bold word's end stays bold.
  await T.key('Home');
  for (let i = 0; i < 4; i++) await T.key('Right', ['shift']);
  await T.key('b', ['ctrl']);
  await T.until('Ctrl+B makes the selected words bold (a format toggle, not marks typed)', (p) => blocks(p)[1].src.startsWith('**Some** **bold**') && E(p).text === plainOf1);
  await T.key('Right');
  await T.key('X');
  await T.until('typing at the end of bold words continues them bold', (p) => blocks(p)[1].src.startsWith('**SomeX** **bold**') && E(p).text.startsWith('SomeX bold'));
  await T.key('Backspace');
  await T.key('Home');
  for (let i = 0; i < 4; i++) await T.key('Right', ['shift']);
  await T.key('b', ['ctrl']);
  await T.until('Ctrl+B again takes the bold off', (p) => blocks(p)[1].src.startsWith('Some **bold**,'));
  // A code block is coloured while it is typed in.
  const w8 = await wordsAt(8);
  await T.mouse('click', w8.x + 30, w8.y + 6);
  const e8 = await T.until('a click on code puts the caret in it', (p) => E(p).on && E(p).id === w8.id);
  t.check('the code being edited is coloured by its language', E(e8).text === blocks(e8)[8].src && E(e8).colours >= 2 && E(e8).bold === 0, E(e8));
  await T.key('End', ['ctrl']);
  await T.key(' # hi');
  await T.until('typed code is coloured as it is typed (a new comment)', (p) => blocks(p)[8].src.endsWith(' # hi') && E(p).colours > E(e8).colours);
  await snap('notes-code-editing');

  // ── A drag across a block's words, the caret elsewhere, selects them (F162) ──
  {
    const w7 = await wordsAt(7);
    t.check('the block to drag across has no caret', E(await T.props()).id !== w7.id, E(await T.props()));
    await T.mouse('down', w7.x + 30, w7.y);
    for (let i = 1; i <= 8; i++) await T.mouse('move', w7.x + 30 + 6 * i, w7.y, { buttons: ['left'] });
    await T.until('the words dragged across show selected as the press moves', (p) => N(p).tsel?.id === w7.id && N(p).tsel.b > N(p).tsel.a);
    await T.mouse('up', w7.x + 78, w7.y);
    const g = await T.until('let go, the block has the caret and the words stay selected', (p) => E(p).on && E(p).id === w7.id && E(p).b > E(p).a && !N(p).tsel);
    // F187: the web's mouseup runs maybeFormatBar, so the dragged words show the format bar.
    const fb = await T.until('…and the format bar opens over the dragged words', (p) => N(p).float === 'fmt' && N(p).fmt && !N(p).fmt.link);
    t.check('the bar sits over the dragged words (its middle, its top)', fb && N(fb).fmt.x > w7.x + 20 && N(fb).fmt.x < w7.x + 100 && Math.abs(N(fb).fmt.y - w7.box.y) < w7.box.h + 8, fb && { fmt: N(fb).fmt, w7, g: g && E(g) });
    await T.key('Escape');
    // A double-click on a word: the format bar over it, not at the window's corner.
    const w1b = await wordsAt(1);
    await T.mouse('click', w1b.x + 60, w1b.y);
    await T.until('the caret in the second block', (p) => E(p).on && E(p).id === w1b.id);
    await t.sleep(150);
    await T.mouse('dblclick', w1b.x + 60, w1b.y);
    const f = await T.until('a double-clicked word shows the format bar', (p) => N(p).float === 'fmt' && N(p).fmt && E(p).b > E(p).a);
    t.check('the format bar opens over the word (its middle, its top)', f && N(f).fmt.x > w1b.x && N(f).fmt.x < w1b.x + 140 && Math.abs(N(f).fmt.y - w1b.box.y) < w1b.box.h + 8, f && { fmt: N(f).fmt, w1b });
    await T.key('Escape');
  }

  // ── Lists nest (the owner's: Tab / Shift+Tab, Enter goes on or steps out) ──
  {
    const p0 = await T.props();
    const n0 = blocks(p0).length;
    const last = tabPage(p0).boxes.find((x) => x.id === blocks(p0)[n0 - 1].id);
    await T.mouse('click', tabPage(p0).pageBox.x + tabPage(p0).pageBox.w / 2, last.y + last.h + 40);
    await T.until('a click under the page adds a line with the caret', (p) => blocks(p).length === n0 + 1 && E(p).on && E(p).id === blocks(p)[n0].id);
    await T.key('- ');
    await T.until('"- " makes a bulleted list', (p) => blocks(p)[n0].kind === 'bullet');
    await T.key('one');
    await T.key('Return');
    await T.key('Tab');
    await T.until('Tab nests the next item one level', (p) => blocks(p)[n0 + 1]?.kind === 'bullet' && blocks(p)[n0 + 1].level === 1);
    await T.key('two');
    await T.key('Return');
    await T.until('Enter goes on at the same level', (p) => blocks(p)[n0 + 2]?.kind === 'bullet' && blocks(p)[n0 + 2].level === 1);
    await T.key('Tab');
    await T.key('Tab');
    await T.until('an item nests at most one level below the one above it', (p) => blocks(p)[n0 + 2].level === 2);
    await T.key('three');
    await T.key('Return');
    await T.key('Return');
    await T.until('Enter on an empty nested item steps out one level', (p) => blocks(p)[n0 + 3]?.kind === 'bullet' && blocks(p)[n0 + 3].level === 1);
    await T.key('Tab', ['shift']);
    await T.until('Shift+Tab steps out too', (p) => blocks(p)[n0 + 3].level === 0);
    await T.key('Return');
    await T.until('and Enter on an empty item at the left ends the list', (p) => blocks(p)[n0 + 3].kind === 'p' && blocks(p).length === n0 + 4);
    let q = await T.props();
    t.check('one, two, three nest 0, 1, 2', blocks(q).slice(n0, n0 + 3).map((b) => `${b.src}:${b.level}`).join() === 'one:0,two:1,three:2', blocks(q).slice(n0));
    await snap('notes-nested');
    await T.act('notes-select', '', String(n0), String(n0 + 2));
    await T.act('notes-sel-act', '', 'copy');
    await T.until('nested items copy as nested Markdown', (p) => (p.clipboard ?? N(p).clipboard) === '- one\n  - two\n    - three');
    await T.act('notes-sel-key', '', 'escape');
    await T.until('the nesting is saved', (p) => tabPage(p).saveText === 'Saved' && !N(p).tabPage?.dirty);
    const sv = await page(plan.id);
    t.check('the server keeps each item\'s level', sv.blocks[n0 + 1].level === 1 && sv.blocks[n0 + 2].level === 2 && sv.blocks[n0].level === undefined, sv.blocks.slice(n0));

    // ── Block keys: Alt+Up/Down move, Ctrl+D duplicates, Ctrl+Shift+K deletes ──
    const w = await wordsAt(n0 + 1);
    await T.mouse('click', w.x + 200, w.y);
    await T.until('the caret in "two"', (p) => E(p).on && E(p).id === w.id);
    // A block goes with its children ("three" under "two"; the owner's 20 Nov decision).
    await T.key('Down', ['alt']);
    await T.until('Alt+Down moves the block and its children down past the next block, the caret with it', (p) => blocks(p)[n0 + 1].src === '' && blocks(p)[n0 + 2].id === w.id
      && blocks(p)[n0 + 3].src === 'three' && blocks(p)[n0 + 3].level === 2 && E(p).id === w.id && E(p).on);
    await T.key('Up', ['alt']);
    await T.until('Alt+Up moves them back', (p) => blocks(p)[n0 + 1].id === w.id && blocks(p)[n0 + 2].src === 'three');
    await T.key('d', ['ctrl']);
    q = await T.until('Ctrl+D duplicates it with its children below them, the caret in the copy', (p) => blocks(p).length === n0 + 6 && blocks(p)[n0 + 3].src === 'two' && blocks(p)[n0 + 3].level === 1
      && blocks(p)[n0 + 4].src === 'three' && blocks(p)[n0 + 4].level === 2 && blocks(p)[n0 + 3].id !== w.id && E(p).id === blocks(p)[n0 + 3].id);
    await T.key('k', ['ctrl', 'shift']);
    await T.until('Ctrl+Shift+K deletes the block with the caret, its children with it', (p) => blocks(p).length === n0 + 4 && blocks(p)[n0 + 2].src === 'three' && blocks(p)[n0 + 3].src === '');
    await T.act('notes-select', '', String(n0), String(n0 + 1));
    await T.until('a selected block\'s children are selected with it', (p) => tabPage(p).selCount === 3 && blocks(p)[n0 + 2].sel);
    await T.act('notes-sel-key', '', 'alt-down');
    await T.until('Alt+Down moves selected blocks together, still selected', (p) => blocks(p)[n0].src === '' && blocks(p)[n0 + 1].src === 'one' && blocks(p)[n0 + 2].src === 'two' && blocks(p)[n0 + 3].src === 'three' && tabPage(p).selCount === 3);
    await T.act('notes-sel-key', '', 'ctrl-d');
    await T.until('Ctrl+D copies selected blocks, the copies selected', (p) => blocks(p).length === n0 + 7 && blocks(p)[n0 + 4].src === 'one' && blocks(p)[n0 + 5].src === 'two' && tabPage(p).selCount === 3 && blocks(p)[n0 + 4].sel);
    await T.act('notes-sel-key', '', 'ctrl-shift-k');
    await T.until('Ctrl+Shift+K deletes them', (p) => blocks(p).length === n0 + 4 && tabPage(p).selCount === 0);
    // A paragraph nests too: the empty line, moved under "three", one in.
    await T.act('notes-type', '', String(n0), 'a note');
    await T.act('notes-key', '', String(n0), 'alt-down', '0', '0', 'true', 'true');
    await T.until('Alt+Down takes the line past "one" and its children', (p) => blocks(p)[n0 + 3].src === 'a note' && blocks(p)[n0].src === 'one');
    await T.act('notes-key', '', String(n0 + 3), 'tab', '0', '0', 'true', 'true');
    await T.until('Tab nests a paragraph', (p) => blocks(p)[n0 + 3].kind === 'p' && blocks(p)[n0 + 3].level === 1);
    await T.act('notes-key', '', String(n0 + 3), 'tab', '0', '0', 'true', 'true');
    await T.act('notes-key', '', String(n0 + 3), 'tab', '0', '0', 'true', 'true');
    await T.until('…as deep as one below the block above', (p) => blocks(p)[n0 + 3].level === 3);
    await T.act('notes-key', '', String(n0 + 3), 'tab', '0', '0', 'true', 'true');
    q = await T.props();
    t.check('…and no deeper', blocks(q)[n0 + 3].level === 3, blocks(q)[n0 + 3]);
    await T.act('notes-key', '', String(n0 + 3), 'backspace', '0', '0', 'true', 'true');
    await T.until('Backspace at its start steps it out one level', (p) => blocks(p)[n0 + 3].level === 2 && blocks(p)[n0 + 3].src === 'a note');
    // Folding: "one"'s children hide behind its fold, and show again.
    q = await T.props();
    t.check('a block with children says so (its fold shows)', blocks(q)[n0].kids && !blocks(q)[n0 + 3].kids, blocks(q).slice(n0));
    await T.act('notes-fold', '', String(n0));
    await T.until('the fold hides the children', (p) => blocks(p)[n0].folded && blocks(p).length === n0 + 1 && N(p).tabPage.blocks.length === n0 + 4);
    await snap('notes-folded');
    await T.until('the fold is saved', (p) => tabPage(p).saveText === 'Saved' && !N(p).tabPage?.dirty);
    const sf = await page(plan.id);
    t.check('the server keeps "folded", and the paragraph\'s level', sf.blocks[n0].folded === true && sf.blocks[n0 + 3].level === 2 && sf.blocks[n0 + 3].type === 'p', sf.blocks.slice(n0));
    await T.act('notes-fold', '', String(n0));
    await T.until('again: they show', (p) => !blocks(p)[n0].folded && blocks(p).length === n0 + 4);
    await T.until('all of it saved', (p) => tabPage(p).saveText === 'Saved');
  }

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
