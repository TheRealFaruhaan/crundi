// T13 (owner): a Notes code block that doesn't wrap scrolls sideways (the
// T7 shared scroll: it claims a sideways scroll it can use), keeps its long
// lines while edited with the caret in view, and its wrap toggle (a real
// click on its button) is saved as the web saves it (wrap: true, else no key).

const N = (p) => (p.areas?.planning ?? p.planningArea)?.notes;
const tabPage = (p) => N(p)?.places?.find((x) => x.key === '');
const blocks = (p) => tabPage(p)?.blocks || [];
const E = (p) => tabPage(p)?.edit || {};

export async function run(T, t) {
  const api = (body) => T.api.http('POST', '/api/notes', { project: 'demo', clientId: 'test-other', ...body });
  const page = async (id) => (await T.api.http('GET', `/api/notes/page?project=demo&id=${id}`)).body?.page;
  const long = 'const answer = ' + Array.from({ length: 40 }, (_, i) => `part${i}`).join(' + ') + '; // the end';
  await T.ui('tab', 'notes');
  const pg = (await api({ action: 'create', title: 'Long code', blocks: [{ type: 'p', text: 'Above' }, { type: 'code', text: `${long}\nshort();`, lang: 'javascript' }] })).body.page;
  await T.act('notes-pick', '', pg.id);
  let p = await T.until('the page with a long code line opens', (p) => tabPage(p)?.title === 'Long code' && blocks(p)[1]?.kind === 'code', 8000);
  const id = blocks(p)[1].id;
  const code = (p) => E(p).code?.[id];
  p = await T.until('T13: not wrapped, the code block can scroll sideways', (p) => code(p)?.max > 100, 5000);
  t.check('T13: the block is not wrapped (the default)', !blocks(p)[1].wrap, blocks(p)[1]);
  const bx = tabPage(p).boxes.find((x) => x.id === id);
  const c0 = code(p);
  const at = [c0.left + c0.w / 2, bx.y + bx.h - 24];
  // A sideways scroll over it (wheel steps, then a touchpad's): the block scrolls.
  await T.mouse('wheel', at[0], at[1], { dx: -120 });
  p = await T.until('T13: a sideways scroll over the block scrolls its long line', (p) => code(p)?.x > 60, 3000);
  const x1 = code(p)?.x ?? 0;
  await T.mouse('wheel', at[0], at[1], { dy: -120, mods: ['shift'] });
  p = await T.until('T13: …Shift and the wheel too', (p) => code(p)?.x > x1 + 60, 3000);
  const x2 = code(p)?.x ?? 0;
  await T.mouse('wheel', at[0], at[1], { dy: 60 });
  p = await T.state();
  t.check('T13: …an up-and-down scroll leaves it where it is', Math.abs((code(p)?.x ?? 0) - x2) < 0.5, { x2, now: code(p) });
  // Back to its start.
  for (let i = 0; i < 20 && (code(await T.state())?.x ?? 0) > 0.5; i++) await T.mouse('wheel', at[0], at[1], { dx: 300 });

  // Editing keeps the mode: the box doesn't wrap; End takes the caret to the
  // line's end, scrolled into view.
  await T.mouse('click', c0.left + 40, bx.y + bx.h - 30);
  p = await T.until('T13: a click in the code puts the caret in it', (p) => E(p).on && E(p).id === id, 4000);
  t.check('T13: …the box keeps long lines (no soft wrap)', E(p).softWrap === false, E(p));
  await T.key('Up'); await T.key('Home'); await T.key('End');
  p = await T.until('T13: End goes to the long line\'s end', (p) => E(p).a === long.length, 3000);
  const cx = E(p).caretX;
  t.check('T13: …the caret stays in view (inside the block)', cx != null && cx >= c0.left - 1 && cx <= c0.left + c0.w + 1, { caretX: cx, block: c0 });
  await T.key('Escape');
  p = await T.until('(Escape leaves the box)', (p) => !E(p).on, 3000);
  t.check('T13: out of the code, the box wraps again (other blocks)', E(p).softWrap === true, E(p));

  // The wrap button (shown on hover, at the block's top right): a real click.
  const toggle = async () => {
    await T.mouse('move', c0.left + c0.w / 2, bx.y + bx.h / 2);
    await t.sleep(150);
    // Its head: right 6 px, padding 2, then lang (96), wrap (24), copy (24) side by side.
    const right = c0.left + c0.w;
    const p0 = await T.state();
    const top = (p0 && tabPage(p0).boxes.find((x) => x.id === id)?.y) ?? bx.y;
    await T.mouse('click', right - 6 - 2 - 24 - 12, top + 4 + 2 + 11);
  };
  await toggle();
  p = await T.until('T13: a click on Wrap long lines wraps the block', (p) => blocks(p)[1].wrap, 3000);
  p = await T.until('T13: …it no longer scrolls sideways', (p) => !(code(p)?.max > 1), 3000);
  await T.until('(saved)', (p) => tabPage(p).saveText === 'Saved', 8000);
  let srv = await page(pg.id);
  t.check('T13: saved as the web saves it: wrap: true', srv.blocks[1].wrap === true, srv.blocks[1]);
  await toggle();
  p = await T.until('T13: a second click scrolls instead', (p) => !blocks(p)[1].wrap && code(p)?.max > 100, 3000);
  await T.until('(saved)', (p) => tabPage(p).saveText === 'Saved', 8000);
  srv = await page(pg.id);
  t.check('T13: …saved with no wrap key, as the web', !('wrap' in srv.blocks[1]), srv.blocks[1]);
}
