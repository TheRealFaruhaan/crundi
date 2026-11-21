import { join } from 'node:path';
import { mkdirSync, writeFileSync } from 'node:fs';
// Kanban and the card dialog (run by test/planning.mjs), ported from
// native-gpui/test/planning/kanban.mjs, with v2's checks: a column's list
// draws only the cards on screen, a scroll redraws that column alone, the
// idle board draws no frames.

let T;
const PA = (p) => p.planningArea ?? {};
const K = (p) => PA(p).kanban;
const A = (p) => PA(p).ask;
const modal = (p) => !!PA(p).pointerModal;
const menu = (p) => PA(p).menu;
const dialog = (p) => PA(p).dialog ?? p.dialog;
const answerDialog = (ok, value = '') => T.ui('dialog-answer', ok ? 'true' : 'false', value);
const pick = (id) => T.ui('menu-pick', id);
const col = (p, st) => K(p)?.columns?.find((c) => c.status === st && !c.other);
const cardBy = (p, title) => K(p)?.columns?.flatMap((c) => c.cards.map((x) => ({ ...x, status: c.status }))).find((x) => x.title === title);
const kpost = (body) => T.api.http('POST', '/api/kanban', { project: 'demo', ...body });
const renders = (p, v) => p.renders?.[v] ?? 0;

export async function run(T_, t) {
  T = T_;
  // ── The empty board ──
  await T.ui('tab', 'kanban');
  await T.until('the board loads with four columns', (p) => K(p)?.state === 'ok' && K(p).columns.length === 4
    && K(p).columns.map((c) => c.label).join() === 'Backlog,To Do,In Progress,Done');
  await t.sleep(400);
  await T.snap('planning', 'kanban-empty');

  // The toolbar's + Add task: the card dialog with its Column choice.
  await T.act('kanban-act', 'add-task');
  await T.until('Add task asks for a title and a column', (p) => A(p)?.title === 'Add task' && A(p).label === 'Task title'
    && A(p).choices.join() === 'backlog,todo,in_progress,done' && A(p).ok === 'Add task' && modal(p));
  await T.snap('planning', 'kanban-add-task');
  await T.act('ask-answer', 'true', 'Write the plan', 'in_progress');
  await T.until('the task lands in In Progress', (p) => cardBy(p, 'Write the plan')?.status === 'in_progress');
  await T.act('kanban-act', 'add-task');
  await T.until('the column picked last is remembered', (p) => A(p)?.choice === 'in_progress');
  await T.act('ask-answer', 'false');
  await T.until('Cancel closes it', (p) => !A(p) && !modal(p));

  // "+ Add task" at a column's end: the blank card; Enter adds and stays open.
  await T.act('kanban-act', 'add-task-inline', '', 'backlog');
  await T.until('the blank card opens in Backlog', (p) => col(p, 'backlog')?.adding);
  await T.act('kanban-type-task', 'First card');
  await T.act('kanban-commit-task', 'First card', 'true');
  await T.until('Enter adds it, the blank card stays', (p) => cardBy(p, 'First card')?.status === 'backlog' && col(p, 'backlog').adding);
  await T.act('kanban-commit-task', 'Second card', 'false');
  await T.until('leaving it adds and closes', (p) => cardBy(p, 'Second card') && !col(p, 'backlog').adding);

  // Subtasks: the row opens on a card, Enter adds, the progress counts.
  const first = cardBy(await T.props(), 'First card');
  t.check('a card without subtasks is compact', first.compact, first);
  await T.act('kanban-act', 'add-subtask', first.id);
  await T.until('the new subtask row opens', (p) => cardBy(p, 'First card').adding && !cardBy(p, 'First card').compact);
  await T.act('kanban-commit-todo', 'Step one', 'true');
  await T.until('the subtask is added', (p) => cardBy(p, 'First card').total === 1);
  await T.act('kanban-commit-todo', 'Step two', 'false');
  await T.until('a second, and the row closes', (p) => cardBy(p, 'First card').total === 2 && !cardBy(p, 'First card').adding);
  const c1 = cardBy(await T.props(), 'First card');
  await T.act('kanban-toggle', c1.id, c1.todos[0].id, 'true');
  await T.until('ticking counts it done', (p) => cardBy(p, 'First card').done === 1);

  // The card's menu: its actions row.
  await T.act('kanban-act', 'card-menu', c1.id);
  await T.until('the menu opens the actions', (p) => cardBy(p, 'First card').menuOpen);
  await t.sleep(300);
  await T.snap('planning', 'kanban-board');

  // The subtask's "…": the shared menu (Attach media, Rename, Delete).
  await T.act('kanban-todo-menu', c1.id, c1.todos[1].id, '700', '300');
  await T.until('the subtask menu', (p) => menu(p)?.items?.map((i) => i.id).join() === 'media,rename,delete' && menu(p).items[2].danger);
  await t.sleep(300); // the menu's frame on screen before the snap (F45)
  await T.snap('planning', 'kanban-subtask-menu');
  await pick('rename');
  await T.until('Rename asks', (p) => dialog(p)?.title === 'Edit subtask' && dialog(p).value === 'Step two');
  await answerDialog(true, 'Step 2');
  await T.until('the subtask is renamed', (p) => cardBy(p, 'First card').todos[1].text === 'Step 2');
  // The subtask's Attach media: the web's openMediaModal, its media alone, uploads linked to it (F44).
  const MM = (p) => PA(p).mediaModal;
  const scratch = join(T.OUT, 'scratch'); mkdirSync(scratch, { recursive: true });
  const png = join(scratch, 'step.png');
  writeFileSync(png, Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==', 'base64'));
  await T.act('kanban-todo-menu', c1.id, c1.todos[1].id, '700', '300');
  await T.until('the menu again', (p) => !!menu(p));
  await pick('media');
  await T.until('Subtask media opens: attached here, nothing yet', (p) => MM(p)?.title === 'Subtask media' && MM(p).attachedHere && MM(p).loaded && MM(p).cards.length === 0 && /No media here yet/.test(MM(p).empty));
  await T.ui('workspace:pick-next', png);
  await T.act('media-modal-upload');
  await T.until('Upload links it to the subtask', (p) => MM(p)?.cards.length === 1 && MM(p).cards[0].link?.type === 'todo' && MM(p).cards[0].link.todoId === c1.todos[1].id && p.toast === 'Uploaded 1 file');
  await T.until('its picture loads', (p) => MM(p)?.cards[0].thumb);
  await t.sleep(300); await T.snap('planning', 'kanban-subtask-media');
  await T.act('media-modal-view', MM(await T.props()).cards[0].id);
  await T.until('View: the lightbox over it', (p) => MM(p)?.lightbox === 'step.png');
  await T.act('media-modal-lb-close');
  await T.until('the lightbox closes, the modal stays', (p) => MM(p) && !MM(p).lightbox);
  await T.act('media-modal-close');
  await T.until('Close; the board counts the subtask\'s media', (p) => !MM(p) && cardBy(p, 'First card').todos[1].media === 1);
  await T.act('kanban-act', 'media');
  await T.until('the toolbar\'s Media: the Kanban\'s, with the chips', (p) => MM(p)?.title === 'Media \u00b7 Kanban' && !MM(p).attachedHere && MM(p).kind === 'kanban' && MM(p).cards.some((c) => c.name === 'step.png'));
  await T.act('media-modal-close');
  // The paperclip's strip (.kc-media-pop, the web's placePop): under the
  // button, right edges together; it blocks nothing; Esc or a press
  // elsewhere closes it; a scroll of the board too.
  {
    const fc = cardBy(await T.props(), 'First card');
    const clipBox = (p) => K(p).boxes.controls.find((c) => c.id === fc.id && c.control === 'clip');
    const cb0 = clipBox(await T.props());
    t.check('the paperclip is on the card', !!cb0, K(await T.props()).boxes.controls.filter((c) => c.id === fc.id));
    if (cb0) {
      await T.mouse('move', cb0.x + cb0.w / 2, cb0.y + cb0.h / 2);
      await t.sleep(150);
      const cb = clipBox(await T.props()) || cb0;
      await T.mouse('click', cb.x + cb.w / 2, cb.y + cb.h / 2);
      const op = await T.until('a click on the paperclip opens its strip', (p) => K(p).clip?.task === fc.id);
      const pop = K(op).clip.pop;
      t.check('the strip is just under the paperclip, right edges together', Math.abs(pop.x + pop.w - (cb.x + cb.w)) < 1.5 && Math.abs(pop.y - (cb.y + cb.h + 4)) < 1.5, { pop, btn: cb });
      await T.key('Escape');
      await T.until('Esc closes it', (p) => !K(p).clip);
      await T.mouse('click', cb.x + cb.w / 2, cb.y + cb.h / 2);
      await T.until('the paperclip opens it again', (p) => K(p).clip?.task === fc.id);
      await T.mouse('click', cb.x + cb.w / 2, cb.y + cb.h / 2);
      await T.until('…and a second click on it closes it', (p) => !K(p).clip);
      await T.mouse('click', cb.x + cb.w / 2, cb.y + cb.h / 2);
      await T.until('open once more', (p) => K(p).clip?.task === fc.id);
      // A press on the board's toolbar goes through (nothing is blocked) and closes it.
      const p1 = await T.props();
      const col = K(p1).boxes.cols[0];
      await T.mouse('click', col.x + col.w / 2, col.y + 12);
      await T.until('a press elsewhere closes it', (p) => !K(p).clip);
    }
  }
  await T.until('closed', (p) => !MM(p));
  await T.act('kanban-todo-menu', c1.id, c1.todos[1].id, '700', '300');
  await T.until('the menu again', (p) => !!menu(p));
  await pick('delete');
  await T.until('the subtask is deleted', (p) => cardBy(p, 'First card').total === 1);

  // The card dialog: Edit (title and column, Next, then the description).
  await T.act('kanban-act', 'edit-task', c1.id);
  await T.until('Edit task: title and column', (p) => A(p)?.title === 'Edit task' && A(p).value === 'First card' && A(p).choice === 'backlog' && A(p).ok === 'Next');
  await T.act('ask-answer', 'true', 'First card, edited', 'todo');
  await T.until('then the description', (p) => A(p)?.label === 'Description' && A(p).multiline && A(p).ok === 'Save');
  await T.snap('planning', 'kanban-edit-description');
  await T.act('ask-answer', 'true', 'What it is about');
  await T.until('saved and moved to To Do', (p) => { const c = cardBy(p, 'First card, edited'); return c?.status === 'todo' && c.desc === 'What it is about'; });

  // Drag to move: a real mouse drag from Backlog's card to the Done column.
  await T.act('kanban-act', 'card-menu', c1.id);
  const second = cardBy(await T.props(), 'Second card');
  const boxes = await T.until('the boxes are measured', (p) => K(p).boxes.cards.some((b) => b.id === second.id) && K(p).boxes.cols.length === 4);
  const sb = K(boxes).boxes.cards.find((b) => b.id === second.id);
  const done = K(boxes).boxes.cols.find((b) => b.status === 'done');
  const sx = sb.x + sb.w / 2, sy = sb.y + 10;
  await T.mouse('move', sx, sy);
  await T.mouse('down', sx, sy);
  for (let i = 1; i <= 8; i++) { await T.mouse('move', sx + (done.x + done.w / 2 - sx) * i / 8, sy + (done.y + 40 - sy) * i / 8); await t.sleep(30); }
  await T.until('the drop line is in Done', (p) => K(p).drag?.id === second.id && K(p).drag.status === 'done');
  await T.snap('planning', 'kanban-drag');
  await T.mouse('up', done.x + done.w / 2, done.y + 40);
  await T.until('the card moved to Done', (p) => cardBy(p, 'Second card')?.status === 'done' && !K(p).drag);

  // F135: a card held near the wide board's right or left edge scrolls it
  // sideways (the web's wbScrollTick). A narrower window makes the four
  // columns wider than the board (not yet stacked).
  await T.d.send({ cmd: 'resize', w: 1060, h: 860 });
  const nb = await T.until('the board overflows sideways', (p) => { const cs = K(p)?.boxes?.cols ?? []; const d0 = cs.find((b) => b.status === 'done'); const b0 = cs.find((b) => b.status === 'backlog'); return d0 && b0 && cs.length === 4 && d0.x + d0.w > 1060 - 4 && b0.x > 0; });
  const bc = K(nb).boxes.cards.find((b) => b.status === 'backlog' && b.id !== second.id) ?? K(nb).boxes.cards.find((b) => b.status === 'todo');
  const back0 = K(nb).boxes.cols.find((b) => b.status === 'backlog').x;
  const ex = bc.x + bc.w / 2, ey = bc.y + 10;
  await T.mouse('move', ex, ey);
  await T.mouse('down', ex, ey);
  for (let i = 1; i <= 6; i++) { await T.mouse('move', ex + (1060 - 20 - ex) * i / 6, ey); await t.sleep(30); }
  const right = await T.until('held at the right edge, the board scrolls right', (p) => K(p).drag && K(p).boxes.cols.find((b) => b.status === 'backlog').x < back0 - 60);
  t.check('…and the card is still held', !!K(right).drag, K(right).drag);
  for (let i = 1; i <= 6; i++) { await T.mouse('move', 1060 - 20 - (1060 - 20 - (back0 - 10)) * i / 6, ey); await t.sleep(30); }
  await T.until('held at the left edge, it scrolls back', (p) => K(p).drag && Math.abs(K(p).boxes.cols.find((b) => b.status === 'backlog').x - back0) < 2);
  await T.mouse('move', ex, ey);
  await T.mouse('up', ex, ey);
  await T.until('dropped where it was', (p) => !K(p).drag);
  await T.d.send({ cmd: 'resize', w: 1400, h: 860 });

  // Live: a change from elsewhere shows without a reload.
  await kpost({ action: 'addTask', title: 'From the server', status: 'todo' });
  await T.until('a server-side change arrives live', (p) => cardBy(p, 'From the server')?.status === 'todo');

  // Delete, Trash, Restore; History.
  const srv = cardBy(await T.props(), 'From the server');
  await T.act('kanban-act', 'del-task', srv.id);
  await T.until('deleted, the Trash counts it', (p) => !cardBy(p, 'From the server') && K(p).trash === 1);
  await T.act('kanban-act', 'view-trash');
  await T.until('the Trash lists it and the deleted subtask', (p) => K(p).view === 'trash' && K(p).deletedTasks[0]?.text === 'From the server' && K(p).deletedTodos.some((x) => x.text === 'Step 2'));
  await t.sleep(300);
  await T.snap('planning', 'kanban-trash');
  await T.act('kanban-act', 'restore-task', srv.id);
  await T.until('Restore brings it back', (p) => K(p).trash === 0);
  await T.act('kanban-act', 'view-history');
  await T.until('History lists the changes, newest first', (p) => K(p).view === 'history' && K(p).history.length >= 5 && K(p).history[0].ts.includes('/'));
  await t.sleep(300);
  await T.snap('planning', 'kanban-history');
  await T.act('kanban-act', 'view-board');
  await T.until('back to the board', (p) => K(p).view === 'board' && cardBy(p, 'From the server'));

  // ── v2: a long column draws only what is on screen ──
  for (let i = 0; i < 60; i++) await kpost({ action: 'addTask', title: `Bulk ${String(i).padStart(2, '0')}`, status: 'backlog' });
  const many = await T.until('sixty more cards in Backlog', (p) => col(p, 'backlog')?.cards.length >= 60, 30000);
  await t.sleep(500);
  const drawn = (await T.props());
  const shown = K(drawn).boxes.cards.filter((b) => b.status === 'backlog').length;
  t.check('only the cards on screen are drawn', shown > 0 && shown < 30, { shown, total: many && col(many, 'backlog')?.cards.length });
  const bl = K(drawn).boxes.cols.find((b) => b.status === 'backlog');
  t.check('a long column is capped at the board', bl && bl.h < 900, bl);
  const before = drawn.renders || {};
  const m = T.frames.mark();
  for (let i = 0; i < 40; i++) { await T.mouse('wheel', bl.x + bl.w / 2, bl.y + bl.h / 2, { dy: -60 }); await t.sleep(8); }
  await t.sleep(300);
  const after = await T.props();
  const f = T.frames.since(m);
  console.log(`  kanban scroll frames: ${JSON.stringify(f)}`);
  t.check('a scroll redraws its column alone (no toolbar, no other view)', renders(after, 'kanban-toolbar') === (before['kanban-toolbar'] ?? 0) && renders(after, 'kanban-overlay') === (before['kanban-overlay'] ?? 0), { before, after: after.renders });
  const later = K(after).boxes.cards.filter((b) => b.status === 'backlog').map((b) => b.id);
  t.check('the column scrolled (other cards on screen)', later.length > 0 && later.join() !== K(drawn).boxes.cards.filter((b) => b.status === 'backlog').map((b) => b.id).join());
  const idle = T.frames.mark();
  await t.sleep(1500);
  t.check('the idle board draws no frames', T.frames.since(idle).frames === 0, T.frames.since(idle));

  // The panel in a workbench pane: the narrow board stacks its columns.
  await T.act('open-pane', 'kanban');
  await T.ui('tab', 'workbench');
  await T.until('a Kanban pane on the workbench', (p) => p.panes?.includes('panel:kanban') && p.tab === 'workbench');
  await t.sleep(600);
  await T.snap('planning', 'kanban-panel');
  await T.ui('tab', 'kanban');
}
